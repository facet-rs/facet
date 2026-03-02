//! Native (tokio-tungstenite) WebSocket transport implementing [`Link`].

use std::io;

use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::protocol::Message as WsMessage;

use roam_types::{Backing, Link, LinkRx, LinkTx, LinkTxPermit, WriteSlot};

/// A [`Link`](roam_types::Link) over a WebSocket connection.
///
/// Wraps a [`WebSocketStream`] and sends each roam payload as a single
/// binary WebSocket frame. The WebSocket protocol preserves message
/// boundaries natively, so no length-prefix framing is needed.
// r[impl transport.websocket]
// r[impl transport.websocket.platforms]
// r[impl zerocopy.framing.link.websocket]
pub struct WsLink<S> {
    stream: WebSocketStream<S>,
}

impl<S> WsLink<S> {
    /// Construct from an already-upgraded [`WebSocketStream`].
    pub fn new(stream: WebSocketStream<S>) -> Self {
        Self { stream }
    }
}

impl<S> WsLink<S>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    /// Accept a server-side WebSocket handshake over a raw stream.
    pub async fn server(stream: S) -> Result<Self, io::Error> {
        let ws = tokio_tungstenite::accept_async(stream)
            .await
            .map_err(|e| io::Error::other(e.to_string()))?;
        Ok(Self::new(ws))
    }
}

impl<S> Link for WsLink<S>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    type Tx = WsLinkTx;
    type Rx = WsLinkRx;

    fn split(self) -> (Self::Tx, Self::Rx) {
        let (tx_out, rx_out) = mpsc::channel::<Vec<u8>>(1);
        let (tx_in, rx_in) = mpsc::channel::<Result<WsMessage, io::Error>>(1);

        let io_task = tokio::spawn(io_loop(self.stream, rx_out, tx_in));

        (
            WsLinkTx {
                tx: tx_out,
                io_task,
            },
            WsLinkRx { rx: rx_in },
        )
    }
}

/// Background I/O task that owns the WebSocketStream.
///
/// Multiplexes outbound writes (from the mpsc channel) with inbound reads
/// (forwarded to the read channel). When the write channel closes, the
/// entire WebSocket stream is dropped, causing the read side to see EOF.
async fn io_loop<S>(
    mut ws: WebSocketStream<S>,
    mut rx_out: mpsc::Receiver<Vec<u8>>,
    tx_in: mpsc::Sender<Result<WsMessage, io::Error>>,
) where
    S: AsyncRead + AsyncWrite + Unpin,
{
    loop {
        tokio::select! {
            // Outbound: drain the write channel and send as binary frames.
            msg = rx_out.recv() => {
                match msg {
                    Some(bytes) => {
                        if let Err(e) = ws.feed(WsMessage::binary(bytes)).await {
                            let _ = tx_in.send(Err(io::Error::other(e.to_string()))).await;
                            return;
                        }
                        // Coalesce: drain any already-queued messages before flushing.
                        while let Ok(bytes) = rx_out.try_recv() {
                            if let Err(e) = ws.feed(WsMessage::binary(bytes)).await {
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
                        // Write channel closed — drop the WebSocket stream.
                        // This closes the underlying transport, causing the
                        // peer's read side to see EOF.
                        return;
                    }
                }
            }
            // Inbound: read from the WebSocket and forward to the read channel.
            frame = ws.next() => {
                match frame {
                    Some(Ok(msg)) => {
                        if tx_in.send(Ok(msg)).await.is_err() {
                            // Reader dropped — shut down.
                            return;
                        }
                    }
                    Some(Err(e)) => {
                        use tokio_tungstenite::tungstenite::error::ProtocolError;
                        use tokio_tungstenite::tungstenite::Error as WsError;
                        match &e {
                            // The peer dropped the connection without a close
                            // handshake — this is just EOF for our purposes.
                            WsError::Protocol(
                                ProtocolError::ResetWithoutClosingHandshake,
                            ) => {
                                return;
                            }
                            _ => {
                                let _ = tx_in.send(Err(io::Error::other(e.to_string()))).await;
                                return;
                            }
                        }
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

/// Sending half of a [`WsLink`].
///
/// Internally uses a bounded mpsc channel (capacity 1) to serialize writes
/// and provide backpressure. The I/O task drains the channel and sends
/// binary WebSocket frames.
pub struct WsLinkTx {
    tx: mpsc::Sender<Vec<u8>>,
    io_task: JoinHandle<()>,
}

/// Permit for sending one payload through a [`WsLinkTx`].
pub struct WsLinkTxPermit {
    permit: mpsc::OwnedPermit<Vec<u8>>,
}

/// Write slot for [`WsLinkTx`].
pub struct WsWriteSlot {
    buf: Vec<u8>,
    permit: mpsc::OwnedPermit<Vec<u8>>,
}

impl LinkTx for WsLinkTx {
    type Permit = WsLinkTxPermit;

    async fn reserve(&self) -> io::Result<Self::Permit> {
        let permit = self.tx.clone().reserve_owned().await.map_err(|_| {
            io::Error::new(
                io::ErrorKind::ConnectionReset,
                "websocket writer task stopped",
            )
        })?;
        Ok(WsLinkTxPermit { permit })
    }

    async fn close(self) -> io::Result<()> {
        drop(self.tx);
        self.io_task.await.map_err(io::Error::other)
    }
}

// r[impl zerocopy.send.websocket]
impl LinkTxPermit for WsLinkTxPermit {
    type Slot = WsWriteSlot;

    fn alloc(self, len: usize) -> io::Result<Self::Slot> {
        Ok(WsWriteSlot {
            buf: vec![0u8; len],
            permit: self.permit,
        })
    }
}

impl WriteSlot for WsWriteSlot {
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

/// Receiving half of a [`WsLink`].
pub struct WsLinkRx {
    rx: mpsc::Receiver<Result<WsMessage, io::Error>>,
}

/// Error type for [`WsLinkRx`].
#[derive(Debug)]
pub struct WsLinkRxError(io::Error);

impl std::fmt::Display for WsLinkRxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "websocket rx: {}", self.0)
    }
}

impl std::error::Error for WsLinkRxError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}

// r[impl zerocopy.recv.websocket]
impl LinkRx for WsLinkRx {
    type Error = WsLinkRxError;

    async fn recv(&mut self) -> Result<Option<Backing>, Self::Error> {
        loop {
            match self.rx.recv().await {
                Some(Ok(WsMessage::Binary(data))) => {
                    return Ok(Some(Backing::Boxed(Vec::from(data).into_boxed_slice())));
                }
                Some(Ok(WsMessage::Close(_))) | None => {
                    return Ok(None);
                }
                Some(Ok(WsMessage::Ping(_) | WsMessage::Pong(_) | WsMessage::Frame(_))) => {
                    continue;
                }
                Some(Ok(WsMessage::Text(_))) => {
                    return Err(WsLinkRxError(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "text frames not allowed on roam websocket link",
                    )));
                }
                Some(Err(e)) => {
                    return Err(WsLinkRxError(e));
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use roam_types::{Backing, Link, LinkRx, LinkTx, LinkTxPermit, WriteSlot};
    use tokio_tungstenite::WebSocketStream;
    use tokio_tungstenite::tungstenite::protocol::Role;

    use super::*;

    type TestWsLink = WsLink<tokio::io::DuplexStream>;

    /// Create a connected pair of WsLinks backed by a tokio duplex pipe.
    async fn ws_pair() -> (TestWsLink, TestWsLink) {
        let (a, b) = tokio::io::duplex(64 * 1024);
        let ws_a = WebSocketStream::from_raw_socket(a, Role::Server, None).await;
        let ws_b = WebSocketStream::from_raw_socket(b, Role::Client, None).await;
        (WsLink::new(ws_a), WsLink::new(ws_b))
    }

    fn payload(backing: &Backing) -> &[u8] {
        match backing {
            Backing::Boxed(b) => b,
            Backing::Shared(s) => s.as_bytes(),
        }
    }

    #[tokio::test]
    async fn round_trip_single() {
        let (a, b) = ws_pair().await;
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
        let (a, b) = ws_pair().await;
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
        let (a, b) = ws_pair().await;
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
        let (a, b) = ws_pair().await;
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
        let (a, b) = ws_pair().await;
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
        let (a, b) = ws_pair().await;
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
}
