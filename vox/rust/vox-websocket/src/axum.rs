//! axum integration: bridge an axum websocket route to a vox [`Link`].
//!
//! An [`AxumWsLink`] wraps the [`WebSocket`] handed to
//! [`WebSocketUpgrade::on_upgrade`](axum::extract::ws::WebSocketUpgrade::on_upgrade),
//! so an existing axum HTTP server can host a vox endpoint alongside its other
//! routes. Framing is identical to the native [`WsLink`](crate::WsLink): each
//! vox payload maps 1:1 to a binary WebSocket frame.

use std::io;

use axum::extract::ws::{Message as AxumMessage, WebSocket};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use vox_types::{Backing, Link, LinkRx, LinkTx};

/// A [`Link`] over an axum [`WebSocket`].
pub struct AxumWsLink {
    socket: WebSocket,
}

impl AxumWsLink {
    /// Wrap an already-upgraded axum [`WebSocket`].
    pub fn new(socket: WebSocket) -> Self {
        Self { socket }
    }
}

impl Link for AxumWsLink {
    type Tx = AxumWsLinkTx;
    type Rx = AxumWsLinkRx;

    fn split(self) -> (Self::Tx, Self::Rx) {
        let (tx_out, rx_out) = mpsc::channel::<Vec<u8>>(1);
        let (tx_in, rx_in) = mpsc::channel::<Result<AxumMessage, io::Error>>(1);

        let io_task = tokio::spawn(io_loop(self.socket, rx_out, tx_in));

        (
            AxumWsLinkTx {
                tx: tx_out,
                io_task,
            },
            AxumWsLinkRx { rx: rx_in },
        )
    }
}

/// Background I/O task that owns the axum [`WebSocket`].
///
/// Mirrors the native [`WsLink`](crate::WsLink) I/O loop: outbound payloads are
/// coalesced then flushed as binary frames, and inbound frames are forwarded to
/// the read channel. When the write channel closes the socket is dropped,
/// causing the peer to see EOF.
async fn io_loop(
    mut ws: WebSocket,
    mut rx_out: mpsc::Receiver<Vec<u8>>,
    tx_in: mpsc::Sender<Result<AxumMessage, io::Error>>,
) {
    loop {
        tokio::select! {
            // Outbound: drain the write channel and send as binary frames.
            msg = rx_out.recv() => {
                match msg {
                    Some(bytes) => {
                        if let Err(e) = ws.feed(AxumMessage::binary(bytes)).await {
                            let _ = tx_in.send(Err(io::Error::other(e.to_string()))).await;
                            return;
                        }
                        // Coalesce: drain any already-queued messages before flushing.
                        while let Ok(bytes) = rx_out.try_recv() {
                            if let Err(e) = ws.feed(AxumMessage::binary(bytes)).await {
                                let _ = tx_in.send(Err(io::Error::other(e.to_string()))).await;
                                return;
                            }
                        }
                        if let Err(e) = ws.flush().await {
                            let _ = tx_in.send(Err(io::Error::other(e.to_string()))).await;
                            return;
                        }
                    }
                    None => {
                        // Write channel closed — drop the socket, EOF for the peer.
                        return;
                    }
                }
            }
            // Inbound: read from the socket and forward to the read channel.
            frame = ws.next() => {
                match frame {
                    Some(Ok(msg)) => {
                        if tx_in.send(Ok(msg)).await.is_err() {
                            // Reader dropped — shut down.
                            return;
                        }
                    }
                    Some(Err(e)) => {
                        let _ = tx_in.send(Err(io::Error::other(e.to_string()))).await;
                        return;
                    }
                    None => {
                        // WebSocket stream ended.
                        return;
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tx
// ---------------------------------------------------------------------------

/// Sending half of an [`AxumWsLink`].
pub struct AxumWsLinkTx {
    tx: mpsc::Sender<Vec<u8>>,
    io_task: JoinHandle<()>,
}

impl LinkTx for AxumWsLinkTx {
    async fn send(&self, bytes: Vec<u8>) -> io::Result<()> {
        let permit = self.tx.clone().reserve_owned().await.map_err(|_| {
            io::Error::new(
                io::ErrorKind::ConnectionReset,
                "websocket writer task stopped",
            )
        })?;
        drop(permit.send(bytes));
        Ok(())
    }

    async fn close(self) -> io::Result<()> {
        drop(self.tx);
        self.io_task.await.map_err(io::Error::other)
    }
}

// ---------------------------------------------------------------------------
// Rx
// ---------------------------------------------------------------------------

/// Receiving half of an [`AxumWsLink`].
pub struct AxumWsLinkRx {
    rx: mpsc::Receiver<Result<AxumMessage, io::Error>>,
}

/// Error type for [`AxumWsLinkRx`].
#[derive(Debug)]
pub struct AxumWsLinkRxError(io::Error);

impl std::fmt::Display for AxumWsLinkRxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "axum websocket rx: {}", self.0)
    }
}

impl std::error::Error for AxumWsLinkRxError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}

impl LinkRx for AxumWsLinkRx {
    type Error = AxumWsLinkRxError;

    async fn recv(&mut self) -> Result<Option<Backing>, Self::Error> {
        loop {
            match self.rx.recv().await {
                Some(Ok(AxumMessage::Binary(data))) => {
                    return Ok(Some(Backing::Boxed(data.to_vec().into_boxed_slice())));
                }
                Some(Ok(AxumMessage::Close(_))) | None => {
                    return Ok(None);
                }
                Some(Ok(AxumMessage::Ping(_) | AxumMessage::Pong(_))) => {
                    continue;
                }
                Some(Ok(AxumMessage::Text(_))) => {
                    return Err(AxumWsLinkRxError(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "text frames not allowed on vox websocket link",
                    )));
                }
                Some(Err(e)) => {
                    return Err(AxumWsLinkRxError(e));
                }
            }
        }
    }
}
