//! First-class bidirectional tunnel streams.
//!
//! A tunnel is a bidirectional byte stream multiplexed over an RPC channel ID.
//! This wraps the low-level `register_tunnel`/`send_chunk`/`close_tunnel` APIs into
//! an ergonomic `AsyncRead + AsyncWrite` type.

use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use bytes::Bytes;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::sync::mpsc;

use crate::session::RpcSession;
use crate::{RpcError, TunnelChunk, parse_error_payload};

/// A handle identifying a tunnel channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TunnelHandle {
    pub channel_id: u32,
}

/// A bidirectional byte stream over a tunnel channel.
///
/// - Reads consume incoming tunnel chunks from `RpcSession::register_tunnel`.
/// - Writes send outgoing tunnel chunks via `RpcSession::send_chunk`.
/// - `poll_shutdown` sends an EOS via `RpcSession::close_tunnel`.
pub struct TunnelStream {
    channel_id: u32,
    session: Arc<RpcSession>,
    rx: mpsc::Receiver<TunnelChunk>,

    read_buf: Bytes,
    read_eof: bool,
    read_eos_after_buf: bool,
    logged_first_read: bool,
    logged_first_write: bool,
    logged_read_eof: bool,
    logged_shutdown: bool,

    pending_send: Option<PendingSend>,
    write_closed: bool,
}

type PendingSend =
    Pin<Box<dyn std::future::Future<Output = Result<(), RpcError>> + Send + 'static>>;

impl TunnelStream {
    /// Create a new tunnel stream for an existing `channel_id`.
    ///
    /// This registers a tunnel receiver immediately, so the peer can start sending.
    pub fn new(session: Arc<RpcSession>, channel_id: u32) -> Self {
        let rx = session.register_tunnel(channel_id);
        tracing::debug!(channel_id, "tunnel stream created");
        Self {
            channel_id,
            session,
            rx,
            read_buf: Bytes::new(),
            read_eof: false,
            read_eos_after_buf: false,
            pending_send: None,
            write_closed: false,
            logged_first_read: false,
            logged_first_write: false,
            logged_read_eof: false,
            logged_shutdown: false,
        }
    }

    /// Allocate a fresh tunnel channel ID and return a stream for it.
    pub fn open(session: Arc<RpcSession>) -> (TunnelHandle, Self) {
        let channel_id = session.next_channel_id();
        tracing::debug!(channel_id, "tunnel stream open");
        let stream = Self::new(session, channel_id);
        (TunnelHandle { channel_id }, stream)
    }

    pub fn channel_id(&self) -> u32 {
        self.channel_id
    }
}

impl Drop for TunnelStream {
    fn drop(&mut self) {
        tracing::debug!(
            channel_id = self.channel_id,
            write_closed = self.write_closed,
            read_eof = self.read_eof,
            "tunnel stream dropped"
        );
        // Always unregister locally to avoid leaking an entry in `RpcSession::tunnels`
        // when the peer stops sending without an EOS.
        self.session.unregister_tunnel(self.channel_id);

        // Best-effort half-close if the write side wasn't cleanly shut down.
        // This avoids leaving the peer waiting forever.
        if !self.write_closed {
            let session = self.session.clone();
            let channel_id = self.channel_id;
            tokio::spawn(async move {
                let _ = session.close_tunnel(channel_id).await;
            });
        }
    }
}

impl AsyncRead for TunnelStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        if self.read_eof {
            return Poll::Ready(Ok(()));
        }

        // Drain buffered bytes first.
        if !self.read_buf.is_empty() {
            let to_copy = std::cmp::min(self.read_buf.len(), buf.remaining());
            buf.put_slice(&self.read_buf.split_to(to_copy));

            if self.read_buf.is_empty() && self.read_eos_after_buf {
                self.read_eof = true;
            }

            return Poll::Ready(Ok(()));
        }

        // Buffer empty: poll for the next chunk.
        match Pin::new(&mut self.rx).poll_recv(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(None) => {
                self.read_eof = true;
                if !self.logged_read_eof {
                    self.logged_read_eof = true;
                    tracing::debug!(channel_id = self.channel_id, "tunnel read EOF (rx closed)");
                }
                Poll::Ready(Ok(()))
            }
            Poll::Ready(Some(chunk)) => {
                if !self.logged_first_read {
                    self.logged_first_read = true;
                    tracing::debug!(
                        channel_id = self.channel_id,
                        payload_len = chunk.payload_bytes().len(),
                        is_eos = chunk.is_eos(),
                        is_error = chunk.is_error(),
                        "tunnel read first chunk"
                    );
                }
                if chunk.is_error() {
                    let err = parse_error_payload(chunk.payload_bytes());
                    let (kind, msg) = match err {
                        RpcError::Status { code, message } => {
                            (std::io::ErrorKind::Other, format!("{code:?}: {message}"))
                        }
                        RpcError::Transport(e) => {
                            (std::io::ErrorKind::BrokenPipe, format!("{e:?}"))
                        }
                        RpcError::Cancelled => {
                            (std::io::ErrorKind::Interrupted, "cancelled".into())
                        }
                        RpcError::DeadlineExceeded => {
                            (std::io::ErrorKind::TimedOut, "deadline exceeded".into())
                        }
                        RpcError::Serialize(e) => (
                            std::io::ErrorKind::InvalidData,
                            format!("serialize error: {}", e),
                        ),
                        RpcError::Deserialize(e) => (
                            std::io::ErrorKind::InvalidData,
                            format!("deserialize error: {}", e),
                        ),
                    };
                    return Poll::Ready(Err(std::io::Error::new(kind, msg)));
                }

                let payload = chunk.payload_bytes();
                if chunk.is_eos() && payload.is_empty() {
                    self.read_eof = true;
                    if !self.logged_read_eof {
                        self.logged_read_eof = true;
                        tracing::debug!(
                            channel_id = self.channel_id,
                            "tunnel read EOF (empty EOS)"
                        );
                    }
                    return Poll::Ready(Ok(()));
                }

                self.read_buf = Bytes::copy_from_slice(payload);
                self.read_eos_after_buf = chunk.is_eos();

                // Recurse once to copy into ReadBuf.
                self.poll_read(cx, buf)
            }
        }
    }
}

impl AsyncWrite for TunnelStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        data: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        if self.write_closed {
            return Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "tunnel write side closed",
            )));
        }

        // Drive any pending send first.
        if let Some(fut) = self.pending_send.as_mut() {
            match fut.as_mut().poll(cx) {
                Poll::Ready(Ok(())) => self.pending_send = None,
                Poll::Ready(Err(e)) => {
                    self.pending_send = None;
                    return Poll::Ready(Err(std::io::Error::new(
                        std::io::ErrorKind::BrokenPipe,
                        format!("send failed: {e:?}"),
                    )));
                }
                Poll::Pending => return Poll::Pending,
            }
        }

        if data.is_empty() {
            return Poll::Ready(Ok(0));
        }

        let channel_id = self.channel_id;
        if !self.logged_first_write {
            self.logged_first_write = true;
            tracing::debug!(channel_id, payload_len = data.len(), "tunnel first write");
        }
        let session = self.session.clone();
        let bytes = data.to_vec();
        let len = bytes.len();
        self.pending_send = Some(Box::pin(async move {
            session.send_chunk(channel_id, bytes).await
        }));

        // Immediately poll the future once.
        if let Some(fut) = self.pending_send.as_mut() {
            match fut.as_mut().poll(cx) {
                Poll::Ready(Ok(())) => {
                    self.pending_send = None;
                    Poll::Ready(Ok(len))
                }
                Poll::Ready(Err(e)) => {
                    self.pending_send = None;
                    Poll::Ready(Err(std::io::Error::new(
                        std::io::ErrorKind::BrokenPipe,
                        format!("send failed: {e:?}"),
                    )))
                }
                Poll::Pending => Poll::Pending,
            }
        } else {
            Poll::Ready(Ok(len))
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        if let Some(fut) = self.pending_send.as_mut() {
            match fut.as_mut().poll(cx) {
                Poll::Ready(Ok(())) => {
                    self.pending_send = None;
                    Poll::Ready(Ok(()))
                }
                Poll::Ready(Err(e)) => {
                    self.pending_send = None;
                    Poll::Ready(Err(std::io::Error::new(
                        std::io::ErrorKind::BrokenPipe,
                        format!("send failed: {e:?}"),
                    )))
                }
                Poll::Pending => Poll::Pending,
            }
        } else {
            Poll::Ready(Ok(()))
        }
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        if self.write_closed {
            return Poll::Ready(Ok(()));
        }

        match self.as_mut().poll_flush(cx) {
            Poll::Ready(Ok(())) => {}
            Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
            Poll::Pending => return Poll::Pending,
        }

        self.write_closed = true;
        if !self.logged_shutdown {
            self.logged_shutdown = true;
            tracing::debug!(channel_id = self.channel_id, "tunnel shutdown");
        }
        let channel_id = self.channel_id;
        let session = self.session.clone();
        tokio::spawn(async move {
            let _ = session.close_tunnel(channel_id).await;
        });
        Poll::Ready(Ok(()))
    }
}
