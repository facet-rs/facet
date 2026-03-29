//! Byte-stream transport for vox.
//!
//! Implements [`Link`] over any `AsyncRead + AsyncWrite`
//! pair (TCP, Unix sockets, stdio) using 4-byte little-endian length-prefix
//! framing.

use std::io;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader, BufWriter};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use vox_types::{Backing, Link, LinkRx, LinkTx, LinkTxPermit, WriteSlot};

#[cfg(not(target_arch = "wasm32"))]
use vox_core::{Attachment, LinkSource};

/// A [`Link`] over a byte stream with length-prefix framing.
///
/// Wraps an `AsyncRead + AsyncWrite` pair. Each message is framed as
/// `[len: u32 LE][payload bytes]`.
// r[impl transport.stream]
// r[impl transport.stream.kinds]
// r[impl zerocopy.framing.link.stream]
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
pub struct TcpConnector {
    addr: String,
    nodelay: bool,
}

#[cfg(not(target_arch = "wasm32"))]
pub fn tcp_connector(addr: impl Into<String>) -> TcpConnector {
    TcpConnector::new(addr)
}

#[cfg(not(target_arch = "wasm32"))]
impl TcpConnector {
    pub fn new(addr: impl Into<String>) -> Self {
        Self {
            addr: addr.into(),
            nodelay: true,
        }
    }

    pub fn nodelay(mut self, nodelay: bool) -> Self {
        self.nodelay = nodelay;
        self
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl LinkSource for TcpConnector {
    type Link = StreamLink<tokio::net::tcp::OwnedReadHalf, tokio::net::tcp::OwnedWriteHalf>;

    async fn next_link(&mut self) -> io::Result<Attachment<Self::Link>> {
        let stream = tokio::net::TcpStream::connect(&self.addr).await?;
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
        let (tx_chan, mut rx_chan) = mpsc::channel::<Vec<u8>>(1);
        // Unbounded return channel for buffer recycling. Capacity is naturally
        // bounded by the number of in-flight buffers (at most 2: one being
        // written by the background task, one being filled by the next alloc).
        let (buf_return_tx, buf_return_rx) = mpsc::unbounded_channel::<Vec<u8>>();
        let mut writer = BufWriter::new(self.writer);

        let writer_task = tokio::spawn(async move {
            while let Some(mut bytes) = rx_chan.recv().await {
                writer
                    .write_all(&(bytes.len() as u32).to_le_bytes())
                    .await?;
                writer.write_all(&bytes).await?;
                // Return buffer to pool for reuse.
                bytes.clear();
                let _ = buf_return_tx.send(bytes);
                // Drain any already-queued messages before flushing,
                // so bursts coalesce into fewer syscalls.
                while let Ok(mut bytes) = rx_chan.try_recv() {
                    writer
                        .write_all(&(bytes.len() as u32).to_le_bytes())
                        .await?;
                    writer.write_all(&bytes).await?;
                    bytes.clear();
                    let _ = buf_return_tx.send(bytes);
                }
                writer.flush().await?;
            }
            writer.shutdown().await?;
            Ok(())
        });

        (
            StreamLinkTx {
                tx: tx_chan,
                buf_pool: std::sync::Mutex::new(buf_return_rx),
                writer_task,
            },
            StreamLinkRx {
                reader: BufReader::new(self.reader),
            },
        )
    }
}

// ---------------------------------------------------------------------------
// Tx
// ---------------------------------------------------------------------------

/// Sending half of a [`StreamLink`].
///
/// Internally uses a bounded mpsc channel (capacity 1) to serialize writes
/// and provide backpressure. A background task drains the channel and writes
/// length-prefixed frames to the underlying stream.
pub struct StreamLinkTx {
    tx: mpsc::Sender<Vec<u8>>,
    buf_pool: std::sync::Mutex<mpsc::UnboundedReceiver<Vec<u8>>>,
    writer_task: JoinHandle<io::Result<()>>,
}

/// Permit for sending one payload through a [`StreamLinkTx`].
pub struct StreamLinkTxPermit {
    permit: mpsc::OwnedPermit<Vec<u8>>,
    recycled_buf: Option<Vec<u8>>,
}

/// Write slot for [`StreamLinkTx`].
pub struct StreamWriteSlot {
    buf: Vec<u8>,
    permit: mpsc::OwnedPermit<Vec<u8>>,
}

impl LinkTx for StreamLinkTx {
    type Permit = StreamLinkTxPermit;

    async fn reserve(&self) -> io::Result<Self::Permit> {
        let permit = self.tx.clone().reserve_owned().await.map_err(|_| {
            io::Error::new(io::ErrorKind::ConnectionReset, "stream writer task stopped")
        })?;
        // Try to grab a recycled buffer from the pool (non-blocking).
        let recycled_buf = self.buf_pool.lock().unwrap().try_recv().ok();
        Ok(StreamLinkTxPermit {
            permit,
            recycled_buf,
        })
    }

    async fn close(self) -> io::Result<()> {
        drop(self.tx);
        self.writer_task.await.map_err(io::Error::other)?
    }
}

// r[impl zerocopy.send.stream]
impl LinkTxPermit for StreamLinkTxPermit {
    type Slot = StreamWriteSlot;

    fn alloc(self, len: usize) -> io::Result<Self::Slot> {
        let mut buf = self.recycled_buf.unwrap_or_default();
        buf.resize(len, 0);
        Ok(StreamWriteSlot {
            buf,
            permit: self.permit,
        })
    }
}

impl WriteSlot for StreamWriteSlot {
    fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.buf
    }

    fn commit(self) {
        drop(self.permit.send(self.buf));
    }
}

// ---------------------------------------------------------------------------
// Rx
// ---------------------------------------------------------------------------

/// Receiving half of a [`StreamLink`].
pub struct StreamLinkRx<R> {
    reader: R,
}

// r[impl zerocopy.recv.stream]
impl<R: AsyncRead + Send + Unpin + 'static> LinkRx for StreamLinkRx<R> {
    type Error = io::Error;

    async fn recv(&mut self) -> io::Result<Option<Backing>> {
        let mut len_buf = [0u8; 4];
        match self.reader.read_exact(&mut len_buf).await {
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
            Err(e) => return Err(e),
        }
        let len = u32::from_le_bytes(len_buf) as usize;
        let mut buf = vec![0u8; len];
        self.reader.read_exact(&mut buf).await?;
        Ok(Some(Backing::Boxed(buf.into_boxed_slice())))
    }
}

// ---------------------------------------------------------------------------
// LocalLink
// ---------------------------------------------------------------------------

type BoxReader = Box<dyn AsyncRead + Send + Unpin>;
type BoxWriter = Box<dyn AsyncWrite + Send + Unpin>;

/// Platform-agnostic local IPC link.
///
/// Uses Unix domain sockets on Linux/macOS, named pipes on Windows.
/// Addresses are strings: a socket path on Unix, a named pipe path on Windows
/// (e.g. `\\.\pipe\my-service`).
// r[impl transport.stream.local]
pub struct LocalLink {
    inner: StreamLink<BoxReader, BoxWriter>,
}

impl LocalLink {
    /// Connect to a local endpoint by address.
    #[cfg(unix)]
    pub async fn connect(addr: &str) -> io::Result<Self> {
        let stream = tokio::net::UnixStream::connect(addr).await?;
        let (r, w) = stream.into_split();
        Ok(Self {
            inner: StreamLink::new(Box::new(r), Box::new(w)),
        })
    }

    /// Connect to a local endpoint by address.
    #[cfg(windows)]
    pub async fn connect(addr: &str) -> io::Result<Self> {
        let pipe = tokio::net::windows::named_pipe::ClientOptions::new().open(addr)?;
        let (r, w) = tokio::io::split(pipe);
        Ok(Self {
            inner: StreamLink::new(Box::new(r), Box::new(w)),
        })
    }
}

impl Link for LocalLink {
    type Tx = StreamLinkTx;
    type Rx = StreamLinkRx<BufReader<BoxReader>>;

    fn split(self) -> (Self::Tx, Self::Rx) {
        self.inner.split()
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
    pending: moire::sync::Mutex<tokio::net::windows::named_pipe::NamedPipeServer>,
}

impl LocalLinkAcceptor {
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
            pending: moire::sync::Mutex::new("local-link-acceptor.pending", server),
        })
    }

    /// Accept the next incoming connection.
    #[cfg(unix)]
    pub async fn accept(&self) -> io::Result<LocalLink> {
        let (stream, _addr) = self.listener.accept().await?;
        let (r, w) = stream.into_split();
        Ok(LocalLink {
            inner: StreamLink::new(Box::new(r), Box::new(w)),
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
    use vox_types::{Backing, Link, LinkRx, LinkTx, LinkTxPermit, WriteSlot};

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

    #[tokio::test]
    async fn round_trip_single() {
        let (a, b) = duplex_pair();
        let (tx_a, _rx_a) = a.split();
        let (_tx_b, mut rx_b) = b.split();

        let permit = tx_a.reserve().await.unwrap();
        let mut slot = permit.alloc(5).unwrap();
        slot.as_mut_slice().copy_from_slice(b"hello");
        slot.commit();

        let msg = rx_b.recv().await.unwrap().unwrap();
        assert_eq!(payload(&msg), b"hello");
    }

    #[tokio::test]
    async fn multiple_messages_in_order() {
        let (a, b) = duplex_pair();
        let (tx_a, _rx_a) = a.split();
        let (_tx_b, mut rx_b) = b.split();

        let payloads: &[&[u8]] = &[b"one", b"two", b"three", b"four"];
        for p in payloads {
            let permit = tx_a.reserve().await.unwrap();
            let mut slot = permit.alloc(p.len()).unwrap();
            slot.as_mut_slice().copy_from_slice(p);
            slot.commit();
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

        let permit = tx_a.reserve().await.unwrap();
        let slot = permit.alloc(0).unwrap();
        slot.commit();

        let msg = rx_b.recv().await.unwrap().unwrap();
        assert_eq!(payload(&msg), b"");
    }

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

    // r[verify link.tx.permit.drop]
    #[tokio::test]
    async fn dropped_permit_sends_nothing() {
        let (a, b) = duplex_pair();
        let (tx_a, _rx_a) = a.split();
        let (_tx_b, mut rx_b) = b.split();

        // Drop permit without allocating — nothing should be sent
        let permit = tx_a.reserve().await.unwrap();
        drop(permit);

        // Then send a real message
        let permit = tx_a.reserve().await.unwrap();
        let mut slot = permit.alloc(3).unwrap();
        slot.as_mut_slice().copy_from_slice(b"yep");
        slot.commit();

        let msg = rx_b.recv().await.unwrap().unwrap();
        assert_eq!(payload(&msg), b"yep");
    }

    // r[verify link.tx.discard]
    #[tokio::test]
    async fn dropped_slot_sends_nothing() {
        let (a, b) = duplex_pair();
        let (tx_a, _rx_a) = a.split();
        let (_tx_b, mut rx_b) = b.split();

        // Drop slot without committing — nothing should be sent
        let permit = tx_a.reserve().await.unwrap();
        let slot = permit.alloc(3).unwrap();
        drop(slot);

        // Then send a real message
        let permit = tx_a.reserve().await.unwrap();
        let mut slot = permit.alloc(2).unwrap();
        slot.as_mut_slice().copy_from_slice(b"ok");
        slot.commit();

        let msg = rx_b.recv().await.unwrap().unwrap();
        assert_eq!(payload(&msg), b"ok");
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
        let permit = tx.reserve().await.unwrap();
        let mut slot = permit.alloc(5).unwrap();
        slot.as_mut_slice().copy_from_slice(b"local");
        slot.commit();

        let msg = server.await.unwrap();
        assert_eq!(payload(&msg), b"local");
    }
}
