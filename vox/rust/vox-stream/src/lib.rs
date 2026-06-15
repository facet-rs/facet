//! Byte-stream transport for vox.
//!
//! Implements [`Link`] over any `AsyncRead + AsyncWrite`
//! pair (TCP, Unix sockets, stdio) using 4-byte little-endian length-prefix
//! framing.

use std::io;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader, BufWriter};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use vox_types::{Backing, Link, LinkRx, LinkTx};

#[cfg(not(target_arch = "wasm32"))]
use vox_core::{Attachment, LinkSource};

#[cfg(unix)]
mod fd_link;
#[cfg(unix)]
pub use fd_link::{FdStreamLink, FdStreamLinkRx, FdStreamLinkTx};

// ---------------------------------------------------------------------------
// Link prologue
// ---------------------------------------------------------------------------
//
// The first bytes on every vox byte-stream connection, sent once before any framed
// message. vox's *application* handshake is versioned and evolvable, but the framing layer
// underneath it historically had no magic or version — so a framing change (e.g. growing
// the header for fd-passing) failed silently as "link closed during transport prologue"
// instead of a clear error. This prologue gives the framing layer its own magic + version
// + capability flags, so a mismatch fails loudly and immediately, and the fd-capable header
// difference becomes negotiated rather than hard-coded per transport.

/// Magic that opens every vox link: ASCII `VOXL`.
pub(crate) const LINK_MAGIC: [u8; 4] = *b"VOXL";
/// Framing-layer version (independent of the application handshake version).
pub(crate) const LINK_VERSION: u8 = 1;
/// Flag bit: frames on this link carry the `[u32 fd_count]` field and may pass fds.
pub(crate) const LINK_FLAG_FD_CAPABLE: u8 = 0x01;
/// magic(4) + version(1) + flags(1).
pub(crate) const LINK_PROLOGUE_LEN: usize = 6;

/// Build the prologue bytes for a link with the given fd capability.
pub(crate) fn link_prologue(fd_capable: bool) -> [u8; LINK_PROLOGUE_LEN] {
    let flags = if fd_capable { LINK_FLAG_FD_CAPABLE } else { 0 };
    [
        LINK_MAGIC[0],
        LINK_MAGIC[1],
        LINK_MAGIC[2],
        LINK_MAGIC[3],
        LINK_VERSION,
        flags,
    ]
}

/// Validate a peer's prologue, checking magic, version, and that its fd capability matches
/// this link's. A mismatch is a hard, descriptive error rather than a silent mis-frame.
pub(crate) fn validate_link_prologue(
    buf: &[u8; LINK_PROLOGUE_LEN],
    expect_fd_capable: bool,
) -> io::Result<()> {
    if buf[..4] != LINK_MAGIC {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "bad vox link magic: expected {LINK_MAGIC:?}, got {:?}",
                &buf[..4]
            ),
        ));
    }
    if buf[4] != LINK_VERSION {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "unsupported vox link version {}: this build speaks {LINK_VERSION}",
                buf[4]
            ),
        ));
    }
    let peer_fd_capable = buf[5] & LINK_FLAG_FD_CAPABLE != 0;
    if peer_fd_capable != expect_fd_capable {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "vox link fd-capability mismatch: peer={peer_fd_capable}, local={expect_fd_capable}"
            ),
        ));
    }
    Ok(())
}

/// A [`Link`] over a byte stream with length-prefix framing.
///
/// Wraps an `AsyncRead + AsyncWrite` pair. Each message is framed as
/// `[len: u32 LE][payload bytes]`.
// r[impl transport.stream]
// r[impl transport.stream.kinds]
pub struct StreamLink<R, W> {
    reader: R,
    writer: W,
}

impl<R, W> StreamLink<R, W> {
    /// Construct from separate read and write halves.
    pub fn new(reader: R, writer: W) -> Self {
        Self { reader, writer }
    }
}

impl StreamLink<tokio::net::tcp::OwnedReadHalf, tokio::net::tcp::OwnedWriteHalf> {
    /// Wrap a [`TcpStream`](tokio::net::TcpStream).
    pub fn tcp(stream: tokio::net::TcpStream) -> Self {
        let (r, w) = stream.into_split();
        Self::new(r, w)
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub struct TcpLinkSource {
    addr: String,
    nodelay: bool,
    resolve_timeout: std::time::Duration,
    connect_timeout: std::time::Duration,
}

/// Default DNS resolution timeout.
const DEFAULT_RESOLVE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
/// Default TCP connect timeout.
const DEFAULT_CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

#[cfg(not(target_arch = "wasm32"))]
pub fn tcp_link_source(addr: impl Into<String>) -> TcpLinkSource {
    TcpLinkSource::new(addr)
}

#[cfg(not(target_arch = "wasm32"))]
impl TcpLinkSource {
    pub fn new(addr: impl Into<String>) -> Self {
        Self {
            addr: addr.into(),
            nodelay: true,
            resolve_timeout: DEFAULT_RESOLVE_TIMEOUT,
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
        }
    }

    pub fn nodelay(mut self, nodelay: bool) -> Self {
        self.nodelay = nodelay;
        self
    }

    pub fn resolve_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.resolve_timeout = timeout;
        self
    }

    pub fn connect_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.connect_timeout = timeout;
        self
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl LinkSource for TcpLinkSource {
    type Link = StreamLink<tokio::net::tcp::OwnedReadHalf, tokio::net::tcp::OwnedWriteHalf>;

    async fn next_link(&mut self) -> io::Result<Attachment<Self::Link>> {
        let addr = self.addr.clone();
        let resolve_fut = tokio::task::spawn_blocking(move || {
            use std::net::ToSocketAddrs;
            addr.to_socket_addrs()?.next().ok_or_else(|| {
                io::Error::new(io::ErrorKind::AddrNotAvailable, "no addresses found")
            })
        });
        let resolved = tokio::time::timeout(self.resolve_timeout, resolve_fut)
            .await
            .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "DNS resolution timed out"))?
            .map_err(io::Error::other)??;

        let stream = tokio::time::timeout(
            self.connect_timeout,
            tokio::net::TcpStream::connect(resolved),
        )
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "TCP connect timed out"))??;
        stream.set_nodelay(self.nodelay)?;
        Ok(Attachment::initiator(StreamLink::tcp(stream)))
    }
}

impl StreamLink<tokio::io::Stdin, tokio::io::Stdout> {
    /// Wrap stdio (stdin for reading, stdout for writing).
    pub fn stdio() -> Self {
        Self::new(tokio::io::stdin(), tokio::io::stdout())
    }
}

#[cfg(unix)]
impl StreamLink<tokio::net::unix::OwnedReadHalf, tokio::net::unix::OwnedWriteHalf> {
    /// Wrap a [`UnixStream`](tokio::net::UnixStream).
    pub fn unix(stream: tokio::net::UnixStream) -> Self {
        let (r, w) = stream.into_split();
        Self::new(r, w)
    }
}

#[cfg(windows)]
impl
    StreamLink<
        tokio::io::ReadHalf<tokio::net::windows::named_pipe::NamedPipeClient>,
        tokio::io::WriteHalf<tokio::net::windows::named_pipe::NamedPipeClient>,
    >
{
    /// Wrap a Windows named pipe client.
    pub fn named_pipe_client(pipe: tokio::net::windows::named_pipe::NamedPipeClient) -> Self {
        let (r, w) = tokio::io::split(pipe);
        Self::new(r, w)
    }
}

impl<R, W> Link for StreamLink<R, W>
where
    R: AsyncRead + Send + Unpin + 'static,
    W: AsyncWrite + Send + Unpin + 'static,
{
    type Tx = StreamLinkTx;
    type Rx = StreamLinkRx<BufReader<R>>;

    fn split(self) -> (Self::Tx, Self::Rx) {
        let (tx_chan, mut rx_chan) = mpsc::channel::<Vec<u8>>(128);
        let (read_tx, read_rx) = mpsc::channel::<io::Result<Option<Backing>>>(128);
        let mut reader = BufReader::new(self.reader);
        let mut writer = BufWriter::new(self.writer);

        let reader_task = tokio::spawn(async move {
            // Validate the peer's link prologue before any framing.
            let mut prologue = [0u8; LINK_PROLOGUE_LEN];
            match read_frame_exact(&mut reader, &mut prologue, "link prologue").await {
                Ok(ReadExactOutcome::Complete) => {
                    if let Err(error) = validate_link_prologue(&prologue, false) {
                        let _ = read_tx.send(Err(error)).await;
                        return;
                    }
                }
                Ok(ReadExactOutcome::CleanEof) => {
                    let _ = read_tx.send(Ok(None)).await;
                    return;
                }
                Err(error) => {
                    let _ = read_tx.send(Err(error)).await;
                    return;
                }
            }
            loop {
                let mut len_buf = [0u8; 4];
                match read_frame_exact(&mut reader, &mut len_buf, "frame header").await {
                    Ok(ReadExactOutcome::Complete) => {}
                    Ok(ReadExactOutcome::CleanEof) => {
                        let _ = read_tx.send(Ok(None)).await;
                        break;
                    }
                    Err(error) => {
                        let _ = read_tx.send(Err(error)).await;
                        break;
                    }
                }

                let len = u32::from_le_bytes(len_buf) as usize;
                let mut buf = vec![0u8; len];
                if let Err(error) = read_frame_exact(&mut reader, &mut buf, "frame body").await {
                    let _ = read_tx.send(Err(error)).await;
                    break;
                }

                if read_tx
                    .send(Ok(Some(Backing::Boxed(buf.into_boxed_slice()))))
                    .await
                    .is_err()
                {
                    break;
                }
            }
        });

        let writer_task = tokio::spawn(async move {
            // Announce the link prologue before any framed message, and flush so the peer
            // can validate immediately rather than blocking until the first real frame.
            writer.write_all(&link_prologue(false)).await?;
            writer.flush().await?;
            while let Some(bytes) = rx_chan.recv().await {
                let len = frame_len_prefix(bytes.len())?;
                writer.write_all(&len).await?;
                writer.write_all(&bytes).await?;
                // Drain any already-queued messages before flushing,
                // so bursts coalesce into fewer syscalls.
                while let Ok(bytes) = rx_chan.try_recv() {
                    let len = frame_len_prefix(bytes.len())?;
                    writer.write_all(&len).await?;
                    writer.write_all(&bytes).await?;
                }
                writer.flush().await?;
            }
            writer.shutdown().await?;
            Ok(())
        });

        (
            StreamLinkTx {
                tx: tx_chan,
                writer_task,
            },
            StreamLinkRx {
                rx: read_rx,
                reader_task,
                _phantom: std::marker::PhantomData,
            },
        )
    }
}

// ---------------------------------------------------------------------------
// Tx
// ---------------------------------------------------------------------------

/// Sending half of a [`StreamLink`].
pub struct StreamLinkTx {
    tx: mpsc::Sender<Vec<u8>>,
    writer_task: JoinHandle<io::Result<()>>,
}

impl LinkTx for StreamLinkTx {
    async fn send(&self, bytes: Vec<u8>) -> io::Result<()> {
        let _ = frame_len_prefix(bytes.len())?;
        // r[impl link.tx.send]
        // r[impl link.tx.cancel-safe]
        // r[impl link.message.empty]
        let permit = self.tx.clone().reserve_owned().await.map_err(|_| {
            io::Error::new(io::ErrorKind::ConnectionReset, "stream writer task stopped")
        })?;
        drop(permit.send(bytes));
        Ok(())
    }

    async fn close(self) -> io::Result<()> {
        drop(self.tx);
        self.writer_task.await.map_err(io::Error::other)?
    }
}

// r[impl link.tx.alloc.limits]
fn frame_len_prefix(len: usize) -> io::Result<[u8; 4]> {
    let len = u32::try_from(len).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "link payload exceeds 4GiB length-prefix limit",
        )
    })?;
    Ok(len.to_le_bytes())
}

// ---------------------------------------------------------------------------
// Rx
// ---------------------------------------------------------------------------

/// Receiving half of a [`StreamLink`].
pub struct StreamLinkRx<R> {
    rx: mpsc::Receiver<io::Result<Option<Backing>>>,
    reader_task: JoinHandle<()>,
    _phantom: std::marker::PhantomData<fn(R)>,
}

impl<R: Send + 'static> LinkRx for StreamLinkRx<R> {
    type Error = io::Error;

    // r[impl rpc.transport.stream.cancel-safe-recv]
    async fn recv(&mut self) -> io::Result<Option<Backing>> {
        match self.rx.recv().await {
            Some(result) => result,
            None => Ok(None),
        }
    }
}

impl<R> Drop for StreamLinkRx<R> {
    fn drop(&mut self) {
        self.reader_task.abort();
    }
}

enum ReadExactOutcome {
    Complete,
    CleanEof,
}

async fn read_frame_exact<R>(
    reader: &mut R,
    buf: &mut [u8],
    part: &'static str,
) -> io::Result<ReadExactOutcome>
where
    R: AsyncRead + Unpin,
{
    let mut read = 0;
    while read < buf.len() {
        let n = reader.read(&mut buf[read..]).await?;
        if n == 0 {
            if read == 0 && part == "frame header" {
                return Ok(ReadExactOutcome::CleanEof);
            }
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                format!("stream ended after {read} of {} {part} bytes", buf.len()),
            ));
        }
        read += n;
    }
    Ok(ReadExactOutcome::Complete)
}

// ---------------------------------------------------------------------------
// LocalLink
// ---------------------------------------------------------------------------

#[cfg(windows)]
type BoxReader = Box<dyn AsyncRead + Send + Unpin>;
#[cfg(windows)]
type BoxWriter = Box<dyn AsyncWrite + Send + Unpin>;

/// Raw local IPC stream.
#[cfg(unix)]
pub type LocalStream = tokio::net::UnixStream;

/// Raw local IPC stream.
#[cfg(windows)]
pub type LocalStream = tokio::net::windows::named_pipe::NamedPipeClient;

/// Raw server stream returned from [`LocalListener::accept`].
#[cfg(unix)]
pub type LocalServerStream = LocalStream;

/// Raw server stream returned from [`LocalListener::accept`].
#[cfg(windows)]
pub type LocalServerStream = tokio::net::windows::named_pipe::NamedPipeServer;

/// Raw local IPC listener.
///
/// This is a thin cross-platform wrapper:
/// - Unix: a Unix domain socket listener
/// - Windows: a named pipe acceptor with one pending server instance
pub struct LocalListener {
    #[cfg(unix)]
    inner: tokio::net::UnixListener,
    #[cfg(windows)]
    pipe_name: String,
    #[cfg(windows)]
    next_server: tokio::net::windows::named_pipe::NamedPipeServer,
}

impl LocalListener {
    /// Bind to a local endpoint.
    #[cfg(unix)]
    pub fn bind(path: impl AsRef<std::path::Path>) -> io::Result<Self> {
        let inner = tokio::net::UnixListener::bind(path)?;
        Ok(Self { inner })
    }

    /// Bind to a local endpoint.
    #[cfg(windows)]
    pub fn bind(pipe_name: impl Into<String>) -> io::Result<Self> {
        use tokio::net::windows::named_pipe::ServerOptions;

        let pipe_name = pipe_name.into();
        let next_server = ServerOptions::new().create(&pipe_name)?;
        Ok(Self {
            pipe_name,
            next_server,
        })
    }

    /// Accept a new incoming raw stream.
    #[cfg(unix)]
    pub async fn accept(&self) -> io::Result<LocalServerStream> {
        tracing::trace!("vox-stream local listener waiting for raw stream");
        let (stream, _addr) = self.inner.accept().await?;
        tracing::debug!("vox-stream local listener accepted raw stream");
        Ok(stream)
    }

    /// Accept a new incoming raw stream.
    #[cfg(windows)]
    pub async fn accept(&mut self) -> io::Result<LocalServerStream> {
        use tokio::net::windows::named_pipe::ServerOptions;

        self.next_server.connect().await?;
        let connected = std::mem::replace(
            &mut self.next_server,
            ServerOptions::new().create(&self.pipe_name)?,
        );
        Ok(connected)
    }
}

/// Connect to a local endpoint and return a raw stream.
#[cfg(unix)]
pub async fn connect(path: impl AsRef<std::path::Path>) -> io::Result<LocalStream> {
    let path = path.as_ref();
    tracing::debug!(path = %path.display(), "vox-stream local connect starting");
    let stream = tokio::net::UnixStream::connect(path).await;
    match &stream {
        Ok(_) => tracing::debug!(path = %path.display(), "vox-stream local connect succeeded"),
        Err(error) => tracing::debug!(
            path = %path.display(),
            ?error,
            "vox-stream local connect failed"
        ),
    }
    stream
}

/// Connect to a local endpoint and return a raw stream.
#[cfg(windows)]
pub async fn connect(pipe_name: impl AsRef<str>) -> io::Result<LocalStream> {
    let pipe_name = pipe_name.as_ref();
    loop {
        match tokio::net::windows::named_pipe::ClientOptions::new().open(pipe_name) {
            Ok(client) => return Ok(client),
            Err(e) if e.raw_os_error() == Some(231) => {
                vox_rt::time::sleep(std::time::Duration::from_millis(50)).await;
            }
            Err(e) => return Err(e),
        }
    }
}

/// Check if a local endpoint exists.
#[cfg(unix)]
pub fn endpoint_exists(path: impl AsRef<std::path::Path>) -> bool {
    path.as_ref().exists()
}

/// Check if a local endpoint exists.
#[cfg(windows)]
pub fn endpoint_exists(pipe_name: impl AsRef<str>) -> bool {
    match tokio::net::windows::named_pipe::ClientOptions::new().open(pipe_name.as_ref()) {
        Ok(_) => true,
        Err(e) => e.raw_os_error() == Some(231),
    }
}

/// Remove a local endpoint.
#[cfg(unix)]
pub fn remove_endpoint(path: impl AsRef<std::path::Path>) -> io::Result<()> {
    std::fs::remove_file(path)
}

/// Remove a local endpoint.
#[cfg(windows)]
pub fn remove_endpoint(_pipe_name: impl AsRef<str>) -> io::Result<()> {
    Ok(())
}

/// Convert a conceptual endpoint path into a platform-appropriate local endpoint.
#[cfg(windows)]
pub fn path_to_pipe_name(path: impl AsRef<std::path::Path>) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    path.as_ref().hash(&mut hasher);
    let hash = hasher.finish();
    format!(r"\\.\pipe\vox-{:016x}", hash)
}

/// On Unix this is the identity mapping for compatibility with Windows naming.
#[cfg(unix)]
pub fn path_to_pipe_name(path: impl AsRef<std::path::Path>) -> std::path::PathBuf {
    path.as_ref().to_path_buf()
}

/// Platform-agnostic local IPC link.
///
/// Uses Unix domain sockets on Linux/macOS, named pipes on Windows.
/// Addresses are strings: a socket path on Unix, a named pipe path on Windows
/// (e.g. `\\.\pipe\my-service`).
///
/// On Unix the link is fd-capable: methods that return `vox::Fd` work, because
/// the inner transport is [`FdStreamLink`] (one `sendmsg`/`recvmsg` per frame
/// with descriptors riding in `SCM_RIGHTS`). On Windows it is a plain named-pipe
/// stream — `vox::Fd` is not deliverable there, and a service that returns one
/// will error at send time as on any non-fd transport.
// r[impl transport.stream.local]
#[cfg(unix)]
pub struct LocalLink {
    inner: FdStreamLink,
}

#[cfg(windows)]
pub struct LocalLink {
    inner: StreamLink<BoxReader, BoxWriter>,
}

impl LocalLink {
    /// Connect to a local endpoint by address.
    #[cfg(unix)]
    pub async fn connect(addr: &str) -> io::Result<Self> {
        Ok(Self {
            inner: FdStreamLink::connect(addr).await?,
        })
    }

    /// Connect to a local endpoint by address.
    #[cfg(windows)]
    pub async fn connect(addr: &str) -> io::Result<Self> {
        let pipe = connect(addr).await?;
        let (r, w) = tokio::io::split(pipe);
        Ok(Self {
            inner: StreamLink::new(Box::new(r), Box::new(w)),
        })
    }
}

#[cfg(unix)]
impl Link for LocalLink {
    type Tx = FdStreamLinkTx;
    type Rx = FdStreamLinkRx;

    fn split(self) -> (Self::Tx, Self::Rx) {
        self.inner.split()
    }
}

#[cfg(windows)]
impl Link for LocalLink {
    type Tx = StreamLinkTx;
    type Rx = StreamLinkRx<BufReader<BoxReader>>;

    fn split(self) -> (Self::Tx, Self::Rx) {
        self.inner.split()
    }
}

/// Reconnecting source for [`LocalLink`] attachments.
///
/// Each call to `next_link` connects to the same local endpoint and yields an
/// initiator attachment.
// r[impl transport.stream.local]
pub struct LocalLinkSource {
    addr: String,
}

pub fn local_link_source(addr: impl Into<String>) -> LocalLinkSource {
    LocalLinkSource::new(addr)
}

impl LocalLinkSource {
    pub fn new(addr: impl Into<String>) -> Self {
        Self { addr: addr.into() }
    }
}

impl LinkSource for LocalLinkSource {
    type Link = LocalLink;

    async fn next_link(&mut self) -> io::Result<Attachment<Self::Link>> {
        let link = LocalLink::connect(&self.addr).await?;
        Ok(Attachment::initiator(link))
    }
}

// ---------------------------------------------------------------------------
// LocalLinkAcceptor
// ---------------------------------------------------------------------------

/// Accepts incoming [`LocalLink`] connections.
// r[impl transport.stream.local]
pub struct LocalLinkAcceptor {
    #[cfg(unix)]
    listener: tokio::net::UnixListener,
    /// On Windows, named pipes don't have a persistent listener object — each
    /// server instance accepts exactly one connection. We keep the current
    /// pending instance here, protected by a Mutex so `accept` can take `&self`.
    #[cfg(windows)]
    addr: String,
    #[cfg(windows)]
    pending: vox_rt::sync::Mutex<tokio::net::windows::named_pipe::NamedPipeServer>,
}

/// A held file lock that keeps a [`LocalLinkAcceptor`] exclusively owned.
///
/// The lock is released when this guard is dropped (or the process exits/crashes).
#[cfg(unix)]
pub struct LocalListenerLock {
    _file: std::fs::File,
    lock_path: std::path::PathBuf,
}

#[cfg(unix)]
impl Drop for LocalListenerLock {
    fn drop(&mut self) {
        // Best-effort cleanup of the lock file.
        let _ = std::fs::remove_file(&self.lock_path);
    }
}

/// Result of attempting to acquire a local listener lock.
#[cfg(unix)]
pub enum LocalLockOutcome {
    /// Lock acquired — we own the socket path. Bind and serve.
    Acquired(LocalListenerLock),
    /// Lock held by another process. The socket path may have a live server.
    /// Caller should health-check before giving up.
    Held,
}

/// Try to acquire an exclusive flock on `{addr}.lock`.
///
/// Returns [`LocalLockOutcome::Acquired`] with a guard if we got the lock,
/// or [`LocalLockOutcome::Held`] if another process holds it.
#[cfg(unix)]
pub fn try_local_lock(addr: &str) -> io::Result<LocalLockOutcome> {
    use std::os::unix::io::AsRawFd;

    let lock_path = std::path::PathBuf::from(format!("{addr}.lock"));

    let lock_file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)?;

    let rc = unsafe { libc::flock(lock_file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if rc != 0 {
        let err = io::Error::last_os_error();
        if err.kind() == io::ErrorKind::WouldBlock {
            return Ok(LocalLockOutcome::Held);
        }
        return Err(err);
    }

    Ok(LocalLockOutcome::Acquired(LocalListenerLock {
        _file: lock_file,
        lock_path,
    }))
}

impl LocalLinkAcceptor {
    /// Bind to a local address with exclusive file locking.
    ///
    /// Acquires an exclusive flock on `{addr}.lock`. If the lock is already
    /// held, another process is alive and serving — returns `AddrInUse`.
    /// If acquired, removes any stale socket file and binds.
    ///
    /// For more control (e.g. health-checking the existing server), use
    /// [`try_local_lock()`] directly.
    ///
    /// The returned [`LocalListenerLock`] must be kept alive for the lifetime
    /// of the server — dropping it releases the lock.
    #[cfg(unix)]
    pub fn bind_with_lock(addr: impl Into<String>) -> io::Result<(Self, LocalListenerLock)> {
        let addr = addr.into();
        match try_local_lock(&addr)? {
            LocalLockOutcome::Acquired(lock) => {
                let _ = std::fs::remove_file(&addr);
                let acceptor = Self::bind(&addr)?;
                Ok((acceptor, lock))
            }
            LocalLockOutcome::Held => Err(io::Error::new(
                io::ErrorKind::AddrInUse,
                format!("another process is serving on {addr}"),
            )),
        }
    }

    /// Bind to a local address.
    #[cfg(unix)]
    pub fn bind(addr: impl Into<String>) -> io::Result<Self> {
        let listener = tokio::net::UnixListener::bind(addr.into())?;
        Ok(Self { listener })
    }

    /// Bind to a local address (named pipe path).
    #[cfg(windows)]
    pub fn bind(addr: impl Into<String>) -> io::Result<Self> {
        use tokio::net::windows::named_pipe::ServerOptions;
        let addr = addr.into();
        let server = ServerOptions::new()
            .first_pipe_instance(true)
            .create(&addr)?;
        Ok(Self {
            addr,
            pending: vox_rt::sync::Mutex::new("local-link-acceptor.pending", server),
        })
    }

    /// Accept the next incoming connection.
    #[cfg(unix)]
    pub async fn accept(&self) -> io::Result<LocalLink> {
        let (stream, _addr) = self.listener.accept().await?;
        Ok(LocalLink {
            inner: FdStreamLink::new(stream),
        })
    }

    /// Accept the next incoming connection.
    #[cfg(windows)]
    pub async fn accept(&self) -> io::Result<LocalLink> {
        use tokio::net::windows::named_pipe::ServerOptions;
        let mut guard = self.pending.lock().await;
        guard.connect().await?;
        let next = ServerOptions::new().create(&self.addr)?;
        let connected = std::mem::replace(&mut *guard, next);
        drop(guard);
        let (r, w) = tokio::io::split(connected);
        Ok(LocalLink {
            inner: StreamLink::new(Box::new(r), Box::new(w)),
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use tokio::io::split;
    use vox_types::{Backing, Link, LinkRx, LinkTx};

    use super::*;

    type DuplexRead = tokio::io::ReadHalf<tokio::io::DuplexStream>;
    type DuplexWrite = tokio::io::WriteHalf<tokio::io::DuplexStream>;
    type DuplexLink = StreamLink<DuplexRead, DuplexWrite>;

    /// Create a connected pair of StreamLinks backed by a tokio duplex pipe.
    fn duplex_pair() -> (DuplexLink, DuplexLink) {
        let (a, b) = tokio::io::duplex(4096);
        let (a_r, a_w) = split(a);
        let (b_r, b_w) = split(b);
        (StreamLink::new(a_r, a_w), StreamLink::new(b_r, b_w))
    }

    fn payload(link: &Backing) -> &[u8] {
        match link {
            Backing::Boxed(b) => b,
            Backing::Shared(s) => s.as_bytes(),
        }
    }

    // r[verify link]
    // r[verify link.split]
    // r[verify link.message]
    // r[verify link.rx.recv]
    // r[verify link.tx.send]
    // r[verify transport.stream.kinds]
    #[tokio::test]
    async fn round_trip_single() {
        let (a, b) = duplex_pair();
        let (tx_a, _rx_a) = a.split();
        let (_tx_b, mut rx_b) = b.split();

        tx_a.send(b"hello".to_vec()).await.unwrap();

        let msg = rx_b.recv().await.unwrap().unwrap();
        assert_eq!(payload(&msg), b"hello");
    }

    // r[verify link.order]
    #[tokio::test]
    async fn multiple_messages_in_order() {
        let (a, b) = duplex_pair();
        let (tx_a, _rx_a) = a.split();
        let (_tx_b, mut rx_b) = b.split();

        let payloads: &[&[u8]] = &[b"one", b"two", b"three", b"four"];
        for p in payloads {
            tx_a.send(p.to_vec()).await.unwrap();
        }

        for expected in payloads {
            let msg = rx_b.recv().await.unwrap().unwrap();
            assert_eq!(payload(&msg), *expected);
        }
    }

    // r[verify link.message.empty]
    #[tokio::test]
    async fn empty_payload() {
        let (a, b) = duplex_pair();
        let (tx_a, _rx_a) = a.split();
        let (_tx_b, mut rx_b) = b.split();

        tx_a.send(Vec::new()).await.unwrap();

        let msg = rx_b.recv().await.unwrap().unwrap();
        assert_eq!(payload(&msg), b"");
    }

    // r[verify link.tx.alloc.limits]
    #[test]
    fn frame_len_prefix_rejects_payloads_that_exceed_u32_prefix() {
        let err = frame_len_prefix(u32::MAX as usize + 1).expect_err("payload should be too large");
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
    }

    // r[verify link.tx.cancel-safe]
    #[tokio::test]
    async fn send_cancel_before_capacity_does_not_enqueue() {
        let (tx, mut rx) = mpsc::channel::<Vec<u8>>(1);
        let link_tx = StreamLinkTx {
            tx,
            writer_task: tokio::spawn(async { Ok(()) }),
        };

        link_tx.send(b"first".to_vec()).await.unwrap();
        {
            let pending = link_tx.send(b"second".to_vec());
            tokio::pin!(pending);
            assert!(
                tokio::time::timeout(std::time::Duration::from_millis(20), &mut pending)
                    .await
                    .is_err(),
                "second send should wait for queue capacity"
            );
        }

        assert_eq!(rx.recv().await.unwrap(), b"first".to_vec());
        assert!(matches!(
            rx.try_recv(),
            Err(mpsc::error::TryRecvError::Empty)
        ));

        link_tx.close().await.unwrap();
    }

    // r[verify link.tx.close]
    // r[verify link.rx.eof]
    #[tokio::test]
    async fn eof_on_peer_close() {
        let (a, b) = duplex_pair();
        let (tx_a, _rx_a) = a.split();
        let (_tx_b, mut rx_b) = b.split();

        tx_a.close().await.unwrap();

        assert!(rx_b.recv().await.unwrap().is_none());
        // Subsequent calls also return None
        assert!(rx_b.recv().await.unwrap().is_none());
    }

    // r[verify link.rx.error]
    #[tokio::test]
    async fn recv_error_is_terminal() {
        let (a, mut b) = tokio::io::duplex(4096);
        let (a_r, a_w) = split(a);
        let receiver_link = StreamLink::new(a_r, a_w);
        let (_tx, mut rx) = receiver_link.split();

        b.write_all(&link_prologue(false)).await.unwrap();
        b.write_all(&4_u32.to_le_bytes()).await.unwrap();
        b.write_all(&[0xAA]).await.unwrap();
        drop(b);

        let err = match rx.recv().await {
            Ok(_) => panic!("partial frame should error"),
            Err(error) => error,
        };
        assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof);
        assert!(rx.recv().await.unwrap().is_none());
    }

    // r[verify rpc.transport.stream.cancel-safe-recv]
    #[tokio::test]
    async fn recv_can_be_cancelled_during_partial_frame() {
        let (a, b) = tokio::io::duplex(4096);
        let (a_r, a_w) = split(a);
        let (_b_r, mut b_w) = split(b);
        let receiver_link = StreamLink::new(a_r, a_w);
        let (_tx, mut rx) = receiver_link.split();

        let expected = b"cancel-safe-frame";
        b_w.write_all(&link_prologue(false)).await.unwrap();
        b_w.write_all(&(expected.len() as u32).to_le_bytes())
            .await
            .unwrap();
        b_w.write_all(&expected[..6]).await.unwrap();

        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(20), rx.recv())
                .await
                .is_err(),
            "partial frame should not complete"
        );

        b_w.write_all(&expected[6..]).await.unwrap();
        let msg = rx.recv().await.unwrap().unwrap();
        assert_eq!(payload(&msg), expected);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn local_link_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.sock");
        let addr = path.to_str().unwrap();

        let acceptor = LocalLinkAcceptor::bind(addr).unwrap();

        let connect_addr = addr.to_string();
        let server = tokio::spawn(async move {
            let link = acceptor.accept().await.unwrap();
            let (_tx, mut rx) = link.split();
            rx.recv().await.unwrap().unwrap()
        });

        let client_link = LocalLink::connect(&connect_addr).await.unwrap();
        let (tx, _rx) = client_link.split();
        tx.send(b"local".to_vec()).await.unwrap();

        let msg = server.await.unwrap();
        assert_eq!(payload(&msg), b"local");
    }

    /// On Unix, `LocalLink` is fd-capable: an `SCM_RIGHTS` descriptor travels
    /// across the listener-accepted connection and re-materialises into a
    /// readable file on the peer. This locks in the
    /// `LocalLinkAcceptor` → `FdStreamLink` wiring (the bug previously was that
    /// accept produced a plain `StreamLink`, so `vox::Fd` returns from a
    /// `#[vox::service]` over `LocalLink` failed at send time).
    #[cfg(unix)]
    #[tokio::test]
    async fn local_link_carries_fds() {
        use std::io::{Read, Seek, Write};
        use std::os::fd::OwnedFd;
        use vox_types::{Fd, collect_fds, provide_fds};

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fd.sock");
        let addr = path.to_str().unwrap();

        // A throwaway descriptor seeded with known bytes; reading it back
        // through the SCM_RIGHTS-delivered fd proves the descriptor moved.
        let mut tmp = tempfile::tempfile().unwrap();
        tmp.write_all(b"hello-via-scm-rights").unwrap();
        tmp.rewind().unwrap();
        let fd_msg = Fd::new(OwnedFd::from(tmp));

        let acceptor = LocalLinkAcceptor::bind(addr).unwrap();
        let connect_addr = addr.to_string();
        let server = tokio::spawn(async move {
            let link = acceptor.accept().await.unwrap();
            let (tx, _rx) = link.split();
            let (body, fds) = collect_fds(|| vox_phon::to_vec(&fd_msg).unwrap());
            assert_eq!(fds.len(), 1);
            tx.send_with_fds(body, fds).await.unwrap();
        });

        let client_link = LocalLink::connect(&connect_addr).await.unwrap();
        let (_tx, mut rx) = client_link.split();
        let backing = rx.recv().await.unwrap().unwrap();
        let frame_fds = rx.take_frame_fds();
        assert_eq!(frame_fds.len(), 1, "one fd attributed to the frame");
        let bytes = match &backing {
            Backing::Boxed(b) => b.to_vec(),
            Backing::Shared(s) => s.as_bytes().to_vec(),
        };
        let decoded: Fd = provide_fds(frame_fds, || vox_phon::from_slice(&bytes).unwrap());
        let mut f = std::fs::File::from(decoded.into_owned_fd().unwrap());
        let mut got = String::new();
        f.read_to_string(&mut got).unwrap();
        assert_eq!(got, "hello-via-scm-rights");

        server.await.unwrap();
    }
}
