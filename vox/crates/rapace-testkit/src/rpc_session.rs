//! RpcSession: A multiplexed RPC session that owns the transport.
//!
//! This module provides the `RpcSession` abstraction that enables bidirectional
//! RPC over a single transport. The key insight is that only `RpcSession` calls
//! `recv_frame()` - all frame routing happens through internal channels.
//!
//! # Architecture
//!
//! ```text
//!                        ┌─────────────────────────────────┐
//!                        │           RpcSession            │
//!                        ├─────────────────────────────────┤
//!                        │  transport: Arc<T>              │
//!                        │  pending: HashMap<channel_id,   │
//!                        │           oneshot::Sender>      │
//!                        │  tunnels: HashMap<channel_id,   │
//!                        │           mpsc::Sender>         │
//!                        │  dispatcher: Option<...>        │
//!                        └───────────┬─────────────────────┘
//!                                    │
//!                              demux loop
//!                                    │
//!        ┌───────────────────────────┼───────────────────────────┐
//!        │                           │                           │
//!  tunnel? (in tunnels)    response? (pending)        request? (dispatch)
//!        │                           │                           │
//!  ┌─────▼─────┐           ┌─────────▼─────────┐   ┌─────────────▼─────────────┐
//!  │ Route to  │           │ Route to oneshot  │   │ Dispatch to handler,      │
//!  │ mpsc chan │           │ waiter, deliver   │   │ send response back        │
//!  └───────────┘           └───────────────────┘   └───────────────────────────┘
//! ```
//!
//! # Usage
//!
//! ```ignore
//! // Create session
//! let session = RpcSession::new(transport);
//!
//! // Register a service handler
//! session.register_dispatcher(move |method_id, payload| {
//!     // Dispatch to your server
//!     server.dispatch(method_id, payload)
//! });
//!
//! // Spawn the demux loop
//! let session = Arc::new(session);
//! tokio::spawn(session.clone().run());
//!
//! // Make RPC calls (registers pending waiter automatically)
//! let channel_id = session.next_channel_id();
//! let response = session.call(channel_id, method_id, payload).await?;
//! ```
//!
//! # Tunnel Support
//!
//! For bidirectional streaming (e.g., TCP tunnels), use the tunnel APIs:
//!
//! ```ignore
//! // Register a tunnel on a channel - returns receiver for incoming chunks
//! let channel_id = session.next_channel_id();
//! let mut rx = session.register_tunnel(channel_id);
//!
//! // Send chunks on the tunnel
//! session.send_chunk(channel_id, data).await?;
//!
//! // Receive chunks (via the demux loop)
//! while let Some(chunk) = rx.recv().await {
//!     // Process chunk.payload, check chunk.is_eos
//! }
//!
//! // Close the tunnel (sends EOS)
//! session.close_tunnel(channel_id).await?;
//! ```

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

use parking_lot::Mutex;
use rapace_core::{
    ErrorCode, Frame, FrameFlags, MsgDescHot, RpcError, Transport, TransportError,
    INLINE_PAYLOAD_SIZE,
};
use tokio::sync::{mpsc, oneshot};

/// A chunk received on a tunnel channel.
///
/// This is delivered to tunnel receivers when DATA frames arrive on the channel.
#[derive(Debug, Clone)]
pub struct TunnelChunk {
    /// The payload data.
    pub payload: Vec<u8>,
    /// True if this is the final chunk (EOS received).
    pub is_eos: bool,
}

/// A frame that was received and routed.
#[derive(Debug)]
pub struct ReceivedFrame {
    pub method_id: u32,
    pub payload: Vec<u8>,
    pub flags: FrameFlags,
    pub channel_id: u32,
}

/// Type alias for a boxed async dispatch function.
pub type BoxedDispatcher = Box<
    dyn Fn(u32, u32, Vec<u8>) -> Pin<Box<dyn Future<Output = Result<Frame, RpcError>> + Send>>
        + Send
        + Sync,
>;

/// RpcSession owns a transport and multiplexes frames between clients and servers.
///
/// # Key invariant
///
/// Only `RpcSession::run()` calls `transport.recv_frame()`. No other code should
/// touch `recv_frame` directly. This prevents the race condition where multiple
/// callers compete for incoming frames.
pub struct RpcSession<T: Transport> {
    transport: Arc<T>,

    /// Pending response waiters: channel_id -> oneshot sender.
    /// When a client sends a request, it registers a waiter here.
    /// When a response arrives, the demux loop finds the waiter and delivers.
    pending: Mutex<HashMap<u32, oneshot::Sender<ReceivedFrame>>>,

    /// Active tunnel channels: channel_id -> mpsc sender.
    /// When a tunnel is registered, incoming DATA frames on that channel
    /// are routed to the tunnel's receiver instead of being dispatched as RPC.
    tunnels: Mutex<HashMap<u32, mpsc::Sender<TunnelChunk>>>,

    /// Optional dispatcher for incoming requests.
    /// If set, incoming requests (frames that don't match a pending waiter)
    /// are dispatched through this function.
    dispatcher: Mutex<Option<BoxedDispatcher>>,

    /// Next message ID for outgoing frames.
    next_msg_id: AtomicU64,

    /// Next channel ID for new RPC calls.
    next_channel_id: AtomicU32,
}

impl<T: Transport + Send + Sync + 'static> RpcSession<T> {
    /// Create a new RPC session wrapping the given transport.
    ///
    /// The `start_channel_id` parameter allows different sessions to use different
    /// channel ID ranges, avoiding collisions in bidirectional RPC scenarios.
    /// - Odd IDs (1, 3, 5, ...): typically used by one side
    /// - Even IDs (2, 4, 6, ...): typically used by the other side
    pub fn new(transport: Arc<T>) -> Self {
        Self::with_channel_start(transport, 1)
    }

    /// Create a new RPC session with a custom starting channel ID.
    ///
    /// Use this when you need to coordinate channel IDs between two sessions.
    /// For bidirectional RPC over a single transport pair:
    /// - Host session: start at 1 (uses odd channel IDs)
    /// - Plugin session: start at 2 (uses even channel IDs)
    pub fn with_channel_start(transport: Arc<T>, start_channel_id: u32) -> Self {
        Self {
            transport,
            pending: Mutex::new(HashMap::new()),
            tunnels: Mutex::new(HashMap::new()),
            dispatcher: Mutex::new(None),
            next_msg_id: AtomicU64::new(1),
            next_channel_id: AtomicU32::new(start_channel_id),
        }
    }

    /// Get a reference to the underlying transport.
    pub fn transport(&self) -> &T {
        &self.transport
    }

    /// Get the next message ID.
    pub fn next_msg_id(&self) -> u64 {
        self.next_msg_id.fetch_add(1, Ordering::Relaxed)
    }

    /// Get the next channel ID.
    ///
    /// Channel IDs increment by 2 to allow interleaving between two sessions:
    /// - Session A starts at 1: uses 1, 3, 5, 7, ...
    /// - Session B starts at 2: uses 2, 4, 6, 8, ...
    ///
    /// This prevents collisions in bidirectional RPC scenarios.
    pub fn next_channel_id(&self) -> u32 {
        self.next_channel_id.fetch_add(2, Ordering::Relaxed)
    }

    /// Register a dispatcher for incoming requests.
    ///
    /// The dispatcher receives (channel_id, method_id, payload) and returns a response frame.
    /// If no dispatcher is registered, incoming requests are dropped with a warning.
    pub fn set_dispatcher<F, Fut>(&self, dispatcher: F)
    where
        F: Fn(u32, u32, Vec<u8>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Frame, RpcError>> + Send + 'static,
    {
        let boxed: BoxedDispatcher = Box::new(move |channel_id, method_id, payload| {
            Box::pin(dispatcher(channel_id, method_id, payload))
        });
        *self.dispatcher.lock() = Some(boxed);
    }

    /// Register a pending waiter for a response on the given channel.
    fn register_pending(&self, channel_id: u32) -> oneshot::Receiver<ReceivedFrame> {
        let (tx, rx) = oneshot::channel();
        self.pending.lock().insert(channel_id, tx);
        rx
    }

    /// Try to route a frame to a pending waiter.
    /// Returns true if the frame was consumed (waiter found), false otherwise.
    fn try_route_to_pending(&self, channel_id: u32, frame: ReceivedFrame) -> Option<ReceivedFrame> {
        let waiter = self.pending.lock().remove(&channel_id);
        if let Some(tx) = waiter {
            // Waiter found - deliver the frame
            let _ = tx.send(frame);
            None
        } else {
            // No waiter - return frame for further processing
            Some(frame)
        }
    }

    // ========================================================================
    // Tunnel APIs
    // ========================================================================

    /// Register a tunnel on the given channel.
    ///
    /// Returns a receiver that will receive `TunnelChunk`s as DATA frames arrive
    /// on the channel. The tunnel is active until:
    /// - An EOS frame is received (final chunk has `is_eos = true`)
    /// - `close_tunnel()` is called
    /// - The receiver is dropped
    ///
    /// # Panics
    ///
    /// Panics if a tunnel is already registered on this channel.
    pub fn register_tunnel(&self, channel_id: u32) -> mpsc::Receiver<TunnelChunk> {
        let (tx, rx) = mpsc::channel(64); // Reasonable buffer for flow control
        let prev = self.tunnels.lock().insert(channel_id, tx);
        assert!(
            prev.is_none(),
            "tunnel already registered on channel {}",
            channel_id
        );
        rx
    }

    /// Try to route a frame to a tunnel.
    /// Returns `Some(frame)` if no tunnel exists, `None` if routed to tunnel.
    fn try_route_to_tunnel(&self, channel_id: u32, payload: Vec<u8>, flags: FrameFlags) -> bool {
        let sender = {
            let tunnels = self.tunnels.lock();
            tunnels.get(&channel_id).cloned()
        };

        if let Some(tx) = sender {
            let is_eos = flags.contains(FrameFlags::EOS);
            let chunk = TunnelChunk { payload, is_eos };

            // Try to send; if receiver dropped, remove the tunnel
            if tx.try_send(chunk).is_err() {
                self.tunnels.lock().remove(&channel_id);
            }

            // If EOS, remove the tunnel registration
            if is_eos {
                self.tunnels.lock().remove(&channel_id);
            }

            true // Frame was handled by tunnel
        } else {
            false // No tunnel, continue normal processing
        }
    }

    /// Send a chunk on a tunnel channel.
    ///
    /// This sends a DATA frame on the channel. The chunk is not marked with EOS;
    /// use `close_tunnel()` to send the final chunk.
    pub async fn send_chunk(&self, channel_id: u32, payload: Vec<u8>) -> Result<(), RpcError> {
        let mut desc = MsgDescHot::new();
        desc.msg_id = self.next_msg_id();
        desc.channel_id = channel_id;
        desc.method_id = 0; // Tunnels don't use method_id
        desc.flags = FrameFlags::DATA;

        let frame = if payload.len() <= INLINE_PAYLOAD_SIZE {
            Frame::with_inline_payload(desc, &payload).expect("inline payload should fit")
        } else {
            Frame::with_payload(desc, payload)
        };

        self.transport
            .send_frame(&frame)
            .await
            .map_err(RpcError::Transport)
    }

    /// Close a tunnel by sending EOS (half-close).
    ///
    /// This sends a final DATA|EOS frame (with empty payload) to signal
    /// the end of the outgoing stream. The tunnel receiver remains active
    /// to receive the peer's remaining chunks until they also send EOS.
    ///
    /// After calling this, no more chunks should be sent on this channel.
    pub async fn close_tunnel(&self, channel_id: u32) -> Result<(), RpcError> {
        // Note: We don't remove the tunnel from the registry here.
        // The tunnel will be removed when we receive EOS from the peer.
        // This allows half-close semantics where we can still receive
        // after we've finished sending.

        let mut desc = MsgDescHot::new();
        desc.msg_id = self.next_msg_id();
        desc.channel_id = channel_id;
        desc.method_id = 0;
        desc.flags = FrameFlags::DATA | FrameFlags::EOS;

        // Send EOS with empty payload
        let frame = Frame::with_inline_payload(desc, &[]).expect("empty payload should fit");

        self.transport
            .send_frame(&frame)
            .await
            .map_err(RpcError::Transport)
    }

    /// Unregister a tunnel without sending EOS.
    ///
    /// Use this when the tunnel was closed by the remote side (you received EOS)
    /// and you want to clean up without sending another EOS.
    pub fn unregister_tunnel(&self, channel_id: u32) {
        self.tunnels.lock().remove(&channel_id);
    }

    // ========================================================================
    // RPC APIs
    // ========================================================================

    /// Send a request and wait for a response.
    ///
    /// This is the main client entry point. It:
    /// 1. Registers a pending waiter for the channel
    /// 2. Sends the request frame
    /// 3. Waits for the response to be delivered by the demux loop
    pub async fn call(
        &self,
        channel_id: u32,
        method_id: u32,
        payload: Vec<u8>,
    ) -> Result<ReceivedFrame, RpcError> {
        // Register waiter before sending
        let rx = self.register_pending(channel_id);

        // Build and send request frame
        let mut desc = MsgDescHot::new();
        desc.msg_id = self.next_msg_id();
        desc.channel_id = channel_id;
        desc.method_id = method_id;
        desc.flags = FrameFlags::DATA | FrameFlags::EOS;

        let frame = if payload.len() <= INLINE_PAYLOAD_SIZE {
            Frame::with_inline_payload(desc, &payload).expect("inline payload should fit")
        } else {
            Frame::with_payload(desc, payload)
        };

        self.transport
            .send_frame(&frame)
            .await
            .map_err(RpcError::Transport)?;

        // Wait for response
        rx.await.map_err(|_| RpcError::Status {
            code: ErrorCode::Internal,
            message: "response channel closed".into(),
        })
    }

    /// Send a response frame.
    pub async fn send_response(&self, frame: &Frame) -> Result<(), RpcError> {
        self.transport
            .send_frame(frame)
            .await
            .map_err(RpcError::Transport)
    }

    /// Run the demux loop.
    ///
    /// This is the main event loop that:
    /// 1. Receives frames from the transport
    /// 2. Routes tunnel frames to registered tunnel receivers
    /// 3. Routes responses to waiting clients
    /// 4. Dispatches requests to the registered handler
    ///
    /// This method consumes self and runs until the transport closes.
    pub async fn run(self: Arc<Self>) -> Result<(), TransportError> {
        loop {
            // Receive next frame
            let frame = match self.transport.recv_frame().await {
                Ok(f) => f,
                Err(TransportError::Closed) => return Ok(()),
                Err(e) => return Err(e),
            };

            let channel_id = frame.desc.channel_id;
            let method_id = frame.desc.method_id;
            let flags = frame.desc.flags;
            let payload = frame.payload.to_vec();

            // 1. Try to route to a tunnel first (highest priority)
            if self.try_route_to_tunnel(channel_id, payload.clone(), flags) {
                continue;
            }

            let received = ReceivedFrame {
                method_id,
                payload,
                flags,
                channel_id,
            };

            // 2. Try to route to a pending RPC waiter
            let received = match self.try_route_to_pending(channel_id, received) {
                None => continue, // Frame was delivered to waiter
                Some(r) => r,     // No waiter, proceed to dispatch
            };

            // Skip non-data frames (control frames, etc.)
            if !received.flags.contains(FrameFlags::DATA) {
                continue;
            }

            // Dispatch to handler
            // We need to call the dispatcher while holding the lock, then spawn the future
            let response_future = {
                let guard = self.dispatcher.lock();
                if let Some(dispatcher) = guard.as_ref() {
                    Some(dispatcher(channel_id, method_id, received.payload))
                } else {
                    None
                }
            };

            if let Some(response_future) = response_future {

                // Spawn the dispatch to avoid blocking the demux loop
                let transport = self.transport.clone();
                tokio::spawn(async move {
                    match response_future.await {
                        Ok(mut response) => {
                            // Set the channel_id on the response
                            response.desc.channel_id = channel_id;
                            let _ = transport.send_frame(&response).await;
                        }
                        Err(e) => {
                            // Send error response
                            let mut desc = MsgDescHot::new();
                            desc.channel_id = channel_id;
                            desc.flags = FrameFlags::ERROR | FrameFlags::EOS;

                            let (code, message): (u32, String) = match &e {
                                RpcError::Status { code, message } => (*code as u32, message.clone()),
                                RpcError::Transport(_) => {
                                    (ErrorCode::Internal as u32, "transport error".into())
                                }
                                RpcError::Cancelled => {
                                    (ErrorCode::Cancelled as u32, "cancelled".into())
                                }
                                RpcError::DeadlineExceeded => {
                                    (ErrorCode::DeadlineExceeded as u32, "deadline exceeded".into())
                                }
                            };

                            let mut err_bytes = Vec::with_capacity(8 + message.len());
                            err_bytes.extend_from_slice(&code.to_le_bytes());
                            err_bytes.extend_from_slice(&(message.len() as u32).to_le_bytes());
                            err_bytes.extend_from_slice(message.as_bytes());

                            let frame = Frame::with_payload(desc, err_bytes);
                            let _ = transport.send_frame(&frame).await;
                        }
                    }
                });
            }
        }
    }
}

/// Helper to parse an error from a response payload.
pub fn parse_error_payload(payload: &[u8]) -> RpcError {
    if payload.len() < 8 {
        return RpcError::Status {
            code: ErrorCode::Internal,
            message: "malformed error response".into(),
        };
    }

    let error_code = u32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]]);
    let message_len = u32::from_le_bytes([payload[4], payload[5], payload[6], payload[7]]) as usize;

    if payload.len() < 8 + message_len {
        return RpcError::Status {
            code: ErrorCode::Internal,
            message: "malformed error response".into(),
        };
    }

    let code = ErrorCode::from_u32(error_code).unwrap_or(ErrorCode::Internal);
    let message = String::from_utf8_lossy(&payload[8..8 + message_len]).into_owned();

    RpcError::Status { code, message }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rapace_transport_mem::InProcTransport;

    #[tokio::test]
    async fn test_basic_rpc() {
        let (client_transport, server_transport) = InProcTransport::pair();
        let client_transport = Arc::new(client_transport);
        let server_transport = Arc::new(server_transport);

        // Create sessions
        let client_session = Arc::new(RpcSession::new(client_transport));
        let server_session = Arc::new(RpcSession::new(server_transport));

        // Set up server dispatcher - simple echo
        server_session.set_dispatcher(|_channel_id, _method_id, payload| async move {
            let mut desc = MsgDescHot::new();
            desc.flags = FrameFlags::DATA | FrameFlags::EOS;
            Ok(Frame::with_payload(desc, payload))
        });

        // Spawn demux loops
        let client_session_clone = client_session.clone();
        let client_handle = tokio::spawn(async move { client_session_clone.run().await });

        let server_session_clone = server_session.clone();
        let server_handle = tokio::spawn(async move { server_session_clone.run().await });

        // Make an RPC call
        let channel_id = client_session.next_channel_id();
        let response = client_session
            .call(channel_id, 1, b"hello".to_vec())
            .await
            .unwrap();

        assert_eq!(response.payload, b"hello");

        // Clean up
        let _ = client_session.transport().close().await;
        let _ = server_session.transport().close().await;
        client_handle.abort();
        server_handle.abort();
    }

    #[tokio::test]
    async fn test_bidirectional_rpc() {
        let (transport_a, transport_b) = InProcTransport::pair();
        let transport_a = Arc::new(transport_a);
        let transport_b = Arc::new(transport_b);

        // Create sessions with different channel ID ranges
        let session_a = Arc::new(RpcSession::with_channel_start(transport_a, 1));
        let session_b = Arc::new(RpcSession::with_channel_start(transport_b, 2));

        // Session A responds with "A:" prefix
        session_a.set_dispatcher(|_channel_id, _method_id, payload| async move {
            let mut response = b"A:".to_vec();
            response.extend(payload);
            let mut desc = MsgDescHot::new();
            desc.flags = FrameFlags::DATA | FrameFlags::EOS;
            Ok(Frame::with_payload(desc, response))
        });

        // Session B responds with "B:" prefix
        session_b.set_dispatcher(|_channel_id, _method_id, payload| async move {
            let mut response = b"B:".to_vec();
            response.extend(payload);
            let mut desc = MsgDescHot::new();
            desc.flags = FrameFlags::DATA | FrameFlags::EOS;
            Ok(Frame::with_payload(desc, response))
        });

        // Spawn demux loops
        let session_a_clone = session_a.clone();
        let handle_a = tokio::spawn(async move { session_a_clone.run().await });

        let session_b_clone = session_b.clone();
        let handle_b = tokio::spawn(async move { session_b_clone.run().await });

        // A calls B
        let channel_id = session_a.next_channel_id();
        let response = session_a
            .call(channel_id, 1, b"test".to_vec())
            .await
            .unwrap();
        assert_eq!(response.payload, b"B:test");

        // B calls A
        let channel_id = session_b.next_channel_id();
        let response = session_b
            .call(channel_id, 1, b"test".to_vec())
            .await
            .unwrap();
        assert_eq!(response.payload, b"A:test");

        // Clean up
        let _ = session_a.transport().close().await;
        let _ = session_b.transport().close().await;
        handle_a.abort();
        handle_b.abort();
    }

    #[tokio::test]
    async fn test_tunnel_bidirectional() {
        let (transport_a, transport_b) = InProcTransport::pair();
        let transport_a = Arc::new(transport_a);
        let transport_b = Arc::new(transport_b);

        // Create sessions with different channel ID ranges
        let session_a = Arc::new(RpcSession::with_channel_start(transport_a, 1));
        let session_b = Arc::new(RpcSession::with_channel_start(transport_b, 2));

        // Spawn demux loops
        let session_a_clone = session_a.clone();
        let handle_a = tokio::spawn(async move { session_a_clone.run().await });

        let session_b_clone = session_b.clone();
        let handle_b = tokio::spawn(async move { session_b_clone.run().await });

        // Allocate a channel from A's range
        let channel_id = session_a.next_channel_id();

        // Both sides register the tunnel on the same channel
        let mut rx_a = session_a.register_tunnel(channel_id);
        let mut rx_b = session_b.register_tunnel(channel_id);

        // A sends chunks to B
        session_a.send_chunk(channel_id, b"hello".to_vec()).await.unwrap();
        session_a.send_chunk(channel_id, b"world".to_vec()).await.unwrap();

        // B receives them
        let chunk1 = rx_b.recv().await.unwrap();
        assert_eq!(chunk1.payload, b"hello");
        assert!(!chunk1.is_eos);

        let chunk2 = rx_b.recv().await.unwrap();
        assert_eq!(chunk2.payload, b"world");
        assert!(!chunk2.is_eos);

        // B sends chunks to A
        session_b.send_chunk(channel_id, b"response1".to_vec()).await.unwrap();
        session_b.send_chunk(channel_id, b"response2".to_vec()).await.unwrap();

        // A receives them
        let chunk1 = rx_a.recv().await.unwrap();
        assert_eq!(chunk1.payload, b"response1");
        assert!(!chunk1.is_eos);

        let chunk2 = rx_a.recv().await.unwrap();
        assert_eq!(chunk2.payload, b"response2");
        assert!(!chunk2.is_eos);

        // A closes the tunnel
        session_a.close_tunnel(channel_id).await.unwrap();

        // B receives EOS
        let eos = rx_b.recv().await.unwrap();
        assert!(eos.is_eos);

        // B closes the tunnel
        session_b.close_tunnel(channel_id).await.unwrap();

        // A receives EOS
        let eos = rx_a.recv().await.unwrap();
        assert!(eos.is_eos);

        // Clean up
        let _ = session_a.transport().close().await;
        let _ = session_b.transport().close().await;
        handle_a.abort();
        handle_b.abort();
    }

    #[tokio::test]
    async fn test_tunnel_large_payload() {
        let (transport_a, transport_b) = InProcTransport::pair();
        let transport_a = Arc::new(transport_a);
        let transport_b = Arc::new(transport_b);

        let session_a = Arc::new(RpcSession::with_channel_start(transport_a, 1));
        let session_b = Arc::new(RpcSession::with_channel_start(transport_b, 2));

        let session_a_clone = session_a.clone();
        let handle_a = tokio::spawn(async move { session_a_clone.run().await });

        let session_b_clone = session_b.clone();
        let handle_b = tokio::spawn(async move { session_b_clone.run().await });

        let channel_id = session_a.next_channel_id();
        let _rx_a = session_a.register_tunnel(channel_id);
        let mut rx_b = session_b.register_tunnel(channel_id);

        // Send a large payload (larger than inline size of 16 bytes)
        let large_data = vec![42u8; 4096];
        session_a.send_chunk(channel_id, large_data.clone()).await.unwrap();

        let chunk = rx_b.recv().await.unwrap();
        assert_eq!(chunk.payload, large_data);
        assert!(!chunk.is_eos);

        // Clean up
        let _ = session_a.transport().close().await;
        let _ = session_b.transport().close().await;
        handle_a.abort();
        handle_b.abort();
    }
}
