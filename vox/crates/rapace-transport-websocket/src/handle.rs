//! Handle + Driver implementation for WebSocket transport.
//!
//! This module provides the new handle+driver pattern for WebSocket transport:
//! - `WebSocketHandle`: cloneable handle for send/recv operations
//! - `WebSocketDriver`: future that owns the WebSocket and runs the I/O loop
//!
//! # Example
//!
//! ```ignore
//! let ws = connect_websocket().await?;
//! let parts = ws.into_transport_parts();
//!
//! // Spawn the driver
//! tokio::spawn(async move {
//!     if let Err(e) = parts.driver.await {
//!         eprintln!("WebSocket driver error: {e}");
//!     }
//! });
//!
//! // Use the handle
//! parts.handle.send_frame(frame).await?;
//! let recv = parts.handle.recv_frame().await?;
//! ```

use std::sync::Arc;

use futures::{Sink, SinkExt, Stream, StreamExt};
use rapace_core::{
    DecodeError, Prioritized, QoSClass, RecvFrame, SendFrame, TransportDriver, TransportError,
    TransportHandle, TransportParts,
};
use tokio::sync::{Mutex as AsyncMutex, mpsc, watch};

use crate::WsMessage;
use crate::native::StreamErrorType;
use crate::shared::{DESC_SIZE, bytes_to_desc, desc_to_bytes};

/// Default outbound queue size (number of messages).
pub const DEFAULT_OUTBOUND_QUEUE_SIZE: usize = 256;

/// Default inbound queue size (number of frames).
pub const DEFAULT_INBOUND_QUEUE_SIZE: usize = 256;

/// Outbound message types sent from handle to driver.
#[derive(Debug)]
pub enum OutboundMsg {
    /// A frame to send.
    Frame(SendFrame<Vec<u8>>),
    /// Request to close the connection.
    Close,
}

/// Driver state, published via watch channel.
#[derive(Debug, Clone)]
pub enum DriverState {
    /// Driver is running normally.
    Running,
    /// Graceful close in progress.
    Closing,
    /// Connection closed normally.
    Closed,
    /// Driver failed with error.
    Failed(String),
}

impl DriverState {
    /// Check if the driver is in a terminal state.
    pub fn is_terminal(&self) -> bool {
        matches!(self, DriverState::Closed | DriverState::Failed(_))
    }
}

/// Configuration for WebSocket handle+driver.
#[derive(Debug, Clone)]
pub struct WebSocketConfig {
    /// Outbound queue size (number of messages).
    pub outbound_queue_size: usize,
    /// Inbound queue size (number of frames).
    pub inbound_queue_size: usize,
}

impl Default for WebSocketConfig {
    fn default() -> Self {
        Self {
            outbound_queue_size: DEFAULT_OUTBOUND_QUEUE_SIZE,
            inbound_queue_size: DEFAULT_INBOUND_QUEUE_SIZE,
        }
    }
}

// Type aliases for complex channel types
type InboundReceiver = mpsc::Receiver<Result<RecvFrame<Vec<u8>>, TransportError>>;
type OutboundSender = mpsc::Sender<Prioritized<OutboundMsg>>;

/// WebSocket transport handle.
///
/// This is a cloneable handle for sending and receiving frames.
/// It communicates with a driver task via channels.
#[derive(Clone)]
pub struct WebSocketHandle {
    /// Sender for outbound messages.
    out_tx: OutboundSender,
    /// Receiver for inbound frames (behind mutex for &self access).
    in_rx: Arc<AsyncMutex<InboundReceiver>>,
    /// Watch receiver for driver state.
    state_rx: watch::Receiver<DriverState>,
}

impl WebSocketHandle {
    /// Check the current driver state.
    pub fn state(&self) -> DriverState {
        self.state_rx.borrow().clone()
    }
}

impl TransportHandle for WebSocketHandle {
    type SendPayload = Vec<u8>;
    type RecvPayload = Vec<u8>;

    async fn send_frame(&self, frame: SendFrame<Self::SendPayload>) -> Result<(), TransportError> {
        // Fast-fail if driver is in terminal state
        match &*self.state_rx.borrow() {
            DriverState::Closed => return Err(TransportError::Closed),
            DriverState::Failed(e) => {
                return Err(TransportError::Io(std::io::Error::other(e.clone())));
            }
            _ => {}
        }

        // Send with normal priority (QoS can be added later)
        let msg = Prioritized::normal(OutboundMsg::Frame(frame));
        self.out_tx
            .send(msg)
            .await
            .map_err(|_| TransportError::Closed)
    }

    async fn recv_frame(&self) -> Result<RecvFrame<Self::RecvPayload>, TransportError> {
        // Fast-fail if driver failed
        if let DriverState::Failed(e) = &*self.state_rx.borrow() {
            return Err(TransportError::Io(std::io::Error::other(e.clone())));
        }

        // Lock and receive
        let mut rx = self.in_rx.lock().await;
        match rx.recv().await {
            Some(Ok(frame)) => Ok(frame),
            Some(Err(e)) => Err(e),
            None => {
                // Channel closed - check state for reason
                match &*self.state_rx.borrow() {
                    DriverState::Failed(e) => {
                        Err(TransportError::Io(std::io::Error::other(e.clone())))
                    }
                    _ => Err(TransportError::Closed),
                }
            }
        }
    }

    fn close(&self) {
        // Best-effort send close message
        let _ = self.out_tx.try_send(Prioritized::with_class(
            QoSClass::Control,
            OutboundMsg::Close,
        ));
    }

    fn is_closed(&self) -> bool {
        self.state_rx.borrow().is_terminal()
    }
}

/// Create WebSocket handle + driver from a WebSocket stream.
///
/// The driver future must be spawned or polled by the caller.
pub fn into_parts<WS, M>(ws: WS) -> TransportParts<WebSocketHandle>
where
    WS: Stream<Item = Result<M, <WS as StreamErrorType>::Error>>
        + Sink<M>
        + StreamErrorType
        + Unpin
        + Send
        + 'static,
    <WS as Sink<M>>::Error: std::error::Error + Send + Sync + 'static,
    M: WsMessage + 'static,
{
    into_parts_with_config(ws, WebSocketConfig::default())
}

/// Create WebSocket handle + driver with custom configuration.
pub fn into_parts_with_config<WS, M>(
    ws: WS,
    config: WebSocketConfig,
) -> TransportParts<WebSocketHandle>
where
    WS: Stream<Item = Result<M, <WS as StreamErrorType>::Error>>
        + Sink<M>
        + StreamErrorType
        + Unpin
        + Send
        + 'static,
    <WS as Sink<M>>::Error: std::error::Error + Send + Sync + 'static,
    M: WsMessage + 'static,
{
    // Create channels
    let (out_tx, out_rx) = mpsc::channel(config.outbound_queue_size);
    let (in_tx, in_rx) = mpsc::channel(config.inbound_queue_size);
    let (state_tx, state_rx) = watch::channel(DriverState::Running);

    // Create handle
    let handle = WebSocketHandle {
        out_tx,
        in_rx: Arc::new(AsyncMutex::new(in_rx)),
        state_rx,
    };

    // Create driver
    let driver: TransportDriver = Box::pin(run_driver(ws, out_rx, in_tx, state_tx));

    TransportParts { handle, driver }
}

/// The WebSocket driver loop.
///
/// This function owns the WebSocket and runs the mux/demux loop.
async fn run_driver<WS, M>(
    mut ws: WS,
    mut out_rx: mpsc::Receiver<Prioritized<OutboundMsg>>,
    in_tx: mpsc::Sender<Result<RecvFrame<Vec<u8>>, TransportError>>,
    state_tx: watch::Sender<DriverState>,
) -> Result<(), TransportError>
where
    WS: Stream<Item = Result<M, <WS as StreamErrorType>::Error>>
        + Sink<M>
        + StreamErrorType
        + Unpin
        + Send
        + 'static,
    <WS as Sink<M>>::Error: std::error::Error + Send + Sync + 'static,
    M: WsMessage + 'static,
{
    loop {
        tokio::select! {
            biased;

            // 1) Prefer draining outbound (reduces latency, releases buffers faster)
            msg = out_rx.recv() => {
                match msg {
                    Some(Prioritized { item: OutboundMsg::Frame(frame), .. }) => {
                        // Encode frame: descriptor + payload
                        let payload = frame.payload_bytes();
                        let mut data = Vec::with_capacity(DESC_SIZE + payload.len());
                        data.extend_from_slice(&desc_to_bytes(&frame.desc));
                        data.extend_from_slice(payload);

                        // Send as binary WebSocket message
                        if let Err(e) = ws.send(M::binary(data)).await {
                            let err_msg = e.to_string();
                            let _ = state_tx.send(DriverState::Failed(err_msg.clone()));
                            let _ = in_tx.send(Err(TransportError::Io(std::io::Error::other(err_msg)))).await;
                            return Ok(());
                        }
                    }
                    Some(Prioritized { item: OutboundMsg::Close, .. }) => {
                        // Send close frame and exit
                        let _ = state_tx.send(DriverState::Closing);
                        let _ = ws.send(M::close()).await;
                        let _ = state_tx.send(DriverState::Closed);
                        return Ok(());
                    }
                    None => {
                        // All senders dropped - graceful shutdown
                        let _ = state_tx.send(DriverState::Closing);
                        let _ = ws.send(M::close()).await;
                        let _ = state_tx.send(DriverState::Closed);
                        return Ok(());
                    }
                }
            }

            // 2) Poll inbound from WebSocket
            incoming = ws.next() => {
                match incoming {
                    Some(Ok(msg)) => {
                        // Skip non-binary messages
                        if msg.should_skip() {
                            continue;
                        }

                        // Handle close
                        if msg.is_close() {
                            let _ = state_tx.send(DriverState::Closed);
                            return Ok(());
                        }

                        // Extract binary data
                        if let Some(data) = msg.into_binary() {
                            // Parse frame
                            match parse_frame(data) {
                                Ok(frame) => {
                                    if in_tx.send(Ok(frame)).await.is_err() {
                                        // Receiver dropped - stop
                                        let _ = state_tx.send(DriverState::Closed);
                                        return Ok(());
                                    }
                                }
                                Err(e) => {
                                    let err_msg = e.to_string();
                                    let _ = state_tx.send(DriverState::Failed(err_msg.clone()));
                                    let _ = in_tx.send(Err(e)).await;
                                    return Ok(());
                                }
                            }
                        }
                    }
                    Some(Err(e)) => {
                        // WebSocket error
                        let err_msg = e.to_string();
                        let terr = TransportError::Io(std::io::Error::other(err_msg.clone()));
                        let _ = state_tx.send(DriverState::Failed(err_msg));
                        let _ = in_tx.send(Err(terr)).await;
                        return Ok(());
                    }
                    None => {
                        // WebSocket closed
                        let _ = state_tx.send(DriverState::Closed);
                        return Ok(());
                    }
                }
            }
        }
    }
}

/// Parse a binary WebSocket message into a RecvFrame.
fn parse_frame(data: Vec<u8>) -> Result<RecvFrame<Vec<u8>>, TransportError> {
    if data.len() < DESC_SIZE {
        return Err(TransportError::Decode(DecodeError::UnexpectedEof));
    }

    // Parse descriptor
    let desc_bytes: [u8; DESC_SIZE] = data[..DESC_SIZE]
        .try_into()
        .map_err(|_| TransportError::Decode(DecodeError::UnexpectedEof))?;
    let desc = bytes_to_desc(&desc_bytes);

    // Extract payload
    let payload_data = data[DESC_SIZE..].to_vec();

    if desc.is_inline() {
        Ok(RecvFrame::inline(desc))
    } else {
        Ok(RecvFrame::with_payload(desc, payload_data))
    }
}

// Static assertions
static_assertions::assert_impl_all!(WebSocketHandle: Send, Sync, Clone);

#[cfg(test)]
mod tests {
    use super::*;
    use rapace_core::{FrameFlags, MsgDescHot};

    #[tokio::test]
    async fn test_handle_driver_roundtrip() {
        // Create duplex streams for WebSocket handshake
        let (client_stream, server_stream) = tokio::io::duplex(65536);

        let (ws_client, ws_server) = tokio::join!(
            async {
                tokio_tungstenite::client_async("ws://localhost/", client_stream)
                    .await
                    .expect("client handshake failed")
                    .0
            },
            async {
                tokio_tungstenite::accept_async(server_stream)
                    .await
                    .expect("server handshake failed")
            }
        );

        // Create handle+driver pairs
        let client_parts = into_parts(ws_client);
        let server_parts = into_parts(ws_server);

        // Spawn drivers
        let client_driver = tokio::spawn(client_parts.driver);
        let server_driver = tokio::spawn(server_parts.driver);

        let client = client_parts.handle;
        let server = server_parts.handle;

        // Send a frame from client to server
        let mut desc = MsgDescHot::new();
        desc.msg_id = 42;
        desc.channel_id = 1;
        desc.method_id = 100;
        desc.flags = FrameFlags::DATA;

        let frame = SendFrame::with_payload(desc, b"hello from client".to_vec());
        client.send_frame(frame).await.unwrap();

        // Receive on server
        let recv = server.recv_frame().await.unwrap();
        assert_eq!(recv.desc.msg_id, 42);
        assert_eq!(recv.desc.channel_id, 1);
        assert_eq!(recv.desc.method_id, 100);
        assert_eq!(recv.payload_bytes(), b"hello from client");

        // Send reply from server
        let mut reply_desc = MsgDescHot::new();
        reply_desc.msg_id = 42;
        reply_desc.flags = FrameFlags::DATA | FrameFlags::EOS;

        let reply = SendFrame::with_payload(reply_desc, b"hello from server".to_vec());
        server.send_frame(reply).await.unwrap();

        // Receive reply on client
        let recv_reply = client.recv_frame().await.unwrap();
        assert_eq!(recv_reply.desc.msg_id, 42);
        assert_eq!(recv_reply.payload_bytes(), b"hello from server");

        // Close
        client.close();
        server.close();

        // Wait for drivers to finish
        let _ = tokio::time::timeout(std::time::Duration::from_secs(1), client_driver).await;
        let _ = tokio::time::timeout(std::time::Duration::from_secs(1), server_driver).await;
    }

    #[tokio::test]
    async fn test_handle_is_clone() {
        let (client_stream, server_stream) = tokio::io::duplex(65536);

        let (ws_client, _ws_server) = tokio::join!(
            async {
                tokio_tungstenite::client_async("ws://localhost/", client_stream)
                    .await
                    .expect("client handshake failed")
                    .0
            },
            async {
                tokio_tungstenite::accept_async(server_stream)
                    .await
                    .expect("server handshake failed")
            }
        );

        let parts = into_parts(ws_client);
        let _driver = tokio::spawn(parts.driver);

        // Clone the handle
        let handle1 = parts.handle.clone();
        let handle2 = parts.handle.clone();

        // Both should report same state
        assert!(!handle1.is_closed());
        assert!(!handle2.is_closed());

        // Close via one handle
        handle1.close();

        // Give driver time to process
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Both should see closed state
        assert!(handle1.is_closed());
        assert!(handle2.is_closed());
    }
}
