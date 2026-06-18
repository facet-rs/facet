//! Unix-domain [`Link`] that carries file descriptors via `SCM_RIGHTS`.
//!
//! Every frame is written with a single `sendmsg` (`vox-fdpass`) whose iovec
//! *is* the framed bytes, so any descriptors are bound to exactly that
//! frame — there is no separate fd-only message that could desync the byte
//! stream. The reader does `recvmsg`, feeds bytes into the length-prefix
//! framer, and attributes each `SCM_RIGHTS` batch to the frame whose bytes
//! completed it. The conduit then installs those fds as the
//! [`provide_fds`](vox_types::provide_fds) source around decoding.
//!
//! This is the only [`Link`] for which [`LinkTx::supports_fd_passing`]
//! returns `true`.

use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::sync::Arc;

use tokio::net::UnixStream;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use vox_fdpass::{SCM_MAX_FD, recv_msg, send_msg_with_fds};
use vox_types::{Backing, Link, LinkRx, LinkTx};

/// A descriptor-passing [`Link`] over a `tokio` [`UnixStream`].
pub struct FdStreamLink {
    stream: UnixStream,
}

impl FdStreamLink {
    /// Wrap an already-connected Unix stream.
    pub fn new(stream: UnixStream) -> Self {
        Self { stream }
    }

    /// Connect to a Unix socket path.
    pub async fn connect(path: impl AsRef<std::path::Path>) -> io::Result<Self> {
        Ok(Self::new(UnixStream::connect(path).await?))
    }

    /// A connected in-process pair (tests / same-process wiring).
    pub fn pair() -> io::Result<(Self, Self)> {
        let (a, b) = UnixStream::pair()?;
        Ok((Self::new(a), Self::new(b)))
    }
}

/// One queued outbound frame.
enum Outgoing {
    /// Frame body, plus descriptors to deliver out-of-band with it.
    Frame(Vec<u8>, Vec<OwnedFd>),
}

impl Link for FdStreamLink {
    type Tx = FdStreamLinkTx;
    type Rx = FdStreamLinkRx;

    fn split(self) -> (Self::Tx, Self::Rx) {
        let sock = Arc::new(self.stream);
        let (tx_chan, mut rx_chan) = mpsc::channel::<Outgoing>(128);
        #[allow(clippy::type_complexity)]
        let (read_tx, read_rx) = mpsc::channel::<io::Result<Option<(Backing, Vec<OwnedFd>)>>>(128);

        // Frame = [u32 body_len LE][u32 fd_count LE][body]. The explicit
        // `fd_count` + an ordered receive FIFO makes fd→frame attribution
        // deterministic regardless of how the kernel chunks bytes vs control
        // messages across `recvmsg` (positional cmsg attribution is wrong
        // when frames and sendmsg/recvmsg boundaries don't align).
        let writer_sock = Arc::clone(&sock);
        let writer_task = tokio::spawn(async move {
            // Link prologue (no fds) before any framed message: announces magic + version +
            // fd-capability so a peer on a mismatched framing fails loudly at connect.
            send_msg_with_fds(&writer_sock, &super::link_prologue(true), &[]).await?;
            while let Some(Outgoing::Frame(body, fds)) = rx_chan.recv().await {
                let body_len = super::frame_len_prefix(body.len())?;
                let mut framed = Vec::with_capacity(8 + body.len());
                framed.extend_from_slice(&body_len);
                framed.extend_from_slice(&(fds.len() as u32).to_le_bytes());
                framed.extend_from_slice(&body);
                let raw: Vec<RawFd> = fds.iter().map(|f| f.as_raw_fd()).collect();
                send_msg_with_fds(&writer_sock, &framed, &raw).await?;
                // `fds` drop here: closes our dups; the peer holds its own.
                drop(fds);
            }
            Ok(())
        });

        let reader_sock = Arc::clone(&sock);
        let reader_task = tokio::spawn(async move {
            let mut buf = vec![0u8; 64 * 1024];
            let mut acc: Vec<u8> = Vec::new();
            // All descriptors received so far, in arrival order. SCM_RIGHTS
            // preserves fd order across the socket, so popping `fd_count`
            // per frame (in frame order) exactly reconstructs each frame's
            // set — independent of recvmsg chunking.
            let mut recv_fds: std::collections::VecDeque<OwnedFd> =
                std::collections::VecDeque::new();
            // The peer's link prologue precedes all frames; validate it once.
            let mut prologue_done = false;
            loop {
                let (n, raw_fds) = match recv_msg(&reader_sock, &mut buf).await {
                    Ok(v) => v,
                    Err(e) => {
                        let _ = read_tx.send(Err(e)).await;
                        break;
                    }
                };
                if n == 0 {
                    let _ = read_tx.send(Ok(None)).await;
                    break;
                }
                for fd in raw_fds {
                    // SAFETY: the kernel just handed us ownership of `fd`.
                    recv_fds.push_back(unsafe { OwnedFd::from_raw_fd(fd) });
                }
                acc.extend_from_slice(&buf[..n]);

                if !prologue_done {
                    if acc.len() < super::LINK_PROLOGUE_LEN {
                        continue;
                    }
                    let mut prologue = [0u8; super::LINK_PROLOGUE_LEN];
                    prologue.copy_from_slice(&acc[..super::LINK_PROLOGUE_LEN]);
                    if let Err(error) = super::validate_link_prologue(&prologue, true) {
                        let _ = read_tx.send(Err(error)).await;
                        return;
                    }
                    acc.drain(..super::LINK_PROLOGUE_LEN);
                    prologue_done = true;
                }

                loop {
                    if acc.len() < 8 {
                        break;
                    }
                    let len = u32::from_le_bytes([acc[0], acc[1], acc[2], acc[3]]) as usize;
                    let fd_count = u32::from_le_bytes([acc[4], acc[5], acc[6], acc[7]]) as usize;
                    if acc.len() < 8 + len {
                        break;
                    }
                    let body = acc[8..8 + len].to_vec();
                    acc.drain(..8 + len);
                    let mut fds = Vec::with_capacity(fd_count);
                    for _ in 0..fd_count {
                        match recv_fds.pop_front() {
                            Some(fd) => fds.push(fd),
                            None => break,
                        }
                    }
                    if read_tx
                        .send(Ok(Some((Backing::Boxed(body.into_boxed_slice()), fds))))
                        .await
                        .is_err()
                    {
                        return;
                    }
                }
            }
        });

        (
            FdStreamLinkTx {
                tx: tx_chan,
                writer_task,
            },
            FdStreamLinkRx {
                rx: read_rx,
                last_fds: Vec::new(),
                reader_task,
            },
        )
    }
}

/// Sending half of an [`FdStreamLink`].
pub struct FdStreamLinkTx {
    tx: mpsc::Sender<Outgoing>,
    writer_task: JoinHandle<io::Result<()>>,
}

impl LinkTx for FdStreamLinkTx {
    async fn send(&self, bytes: Vec<u8>) -> io::Result<()> {
        let _ = super::frame_len_prefix(bytes.len())?;
        // r[impl link.tx.send]
        // r[impl link.tx.cancel-safe]
        // r[impl link.message.empty]
        self.tx
            .send(Outgoing::Frame(bytes, Vec::new()))
            .await
            .map_err(|_| io::Error::new(io::ErrorKind::ConnectionReset, "fd-stream writer stopped"))
    }

    async fn close(self) -> io::Result<()> {
        drop(self.tx);
        self.writer_task.await.map_err(io::Error::other)?
    }

    fn supports_fd_passing(&self) -> bool {
        true
    }

    async fn send_with_fds(&self, bytes: Vec<u8>, fds: Vec<OwnedFd>) -> io::Result<()> {
        let _ = super::frame_len_prefix(bytes.len())?;
        if fds.len() > SCM_MAX_FD {
            return Err(io::Error::other(format!(
                "too many fds in one message: {} > SCM_MAX_FD ({SCM_MAX_FD})",
                fds.len()
            )));
        }
        self.tx
            .send(Outgoing::Frame(bytes, fds))
            .await
            .map_err(|_| io::Error::new(io::ErrorKind::ConnectionReset, "fd-stream writer stopped"))
    }
}

/// Receiving half of an [`FdStreamLink`].
pub struct FdStreamLinkRx {
    #[allow(clippy::type_complexity)]
    rx: mpsc::Receiver<io::Result<Option<(Backing, Vec<OwnedFd>)>>>,
    last_fds: Vec<OwnedFd>,
    reader_task: JoinHandle<()>,
}

impl LinkRx for FdStreamLinkRx {
    type Error = io::Error;

    async fn recv(&mut self) -> io::Result<Option<Backing>> {
        match self.rx.recv().await {
            Some(Ok(Some((backing, fds)))) => {
                self.last_fds = fds;
                Ok(Some(backing))
            }
            Some(Ok(None)) | None => Ok(None),
            Some(Err(e)) => Err(e),
        }
    }

    fn take_frame_fds(&mut self) -> Vec<OwnedFd> {
        std::mem::take(&mut self.last_fds)
    }
}

impl Drop for FdStreamLinkRx {
    fn drop(&mut self) {
        self.reader_task.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Seek, Write};
    use vox_types::{Fd, collect_fds, provide_fds};

    fn temp_file_with(seed: &[u8]) -> std::fs::File {
        let mut path = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        path.push(format!("vox-fdlink-{}-{nanos}", std::process::id()));
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(true)
            .open(&path)
            .unwrap();
        let _ = std::fs::remove_file(&path);
        f.write_all(seed).unwrap();
        f.rewind().unwrap();
        f
    }

    /// Plain frames still work, and `supports_fd_passing` is advertised.
    #[tokio::test]
    async fn plain_frame_round_trip() {
        let (a, b) = FdStreamLink::pair().unwrap();
        let (tx, _rxa) = a.split();
        let (_txb, mut rx) = b.split();
        assert!(tx.supports_fd_passing());

        tx.send(b"hello".to_vec()).await.unwrap();
        let msg = rx.recv().await.unwrap().unwrap();
        match msg {
            Backing::Boxed(b) => assert_eq!(&*b, b"hello"),
            Backing::Shared(s) => assert_eq!(s.as_bytes(), b"hello"),
        }
        assert!(rx.take_frame_fds().is_empty());
    }

    /// An `Fd` encoded into the frame body is delivered through the link and
    /// re-materialises as a working descriptor on the far side.
    #[tokio::test]
    async fn fd_travels_with_its_frame() {
        let (a, b) = FdStreamLink::pair().unwrap();
        let (tx, _rxa) = a.split();
        let (_txb, mut rx) = b.split();

        let fd_msg = Fd::new(OwnedFd::from(temp_file_with(b"through-the-link")));
        let (body, fds) = collect_fds(|| vox_phon::to_vec(&fd_msg).unwrap());
        assert_eq!(fds.len(), 1);
        tx.send_with_fds(body, fds).await.unwrap();

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
        assert_eq!(got, "through-the-link");
    }
}
