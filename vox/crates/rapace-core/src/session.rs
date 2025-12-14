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
use std::panic::AssertUnwindSafe;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use futures::FutureExt;
use parking_lot::Mutex;
use tokio::sync::{mpsc, oneshot};

use crate::{
    ErrorCode, Frame, FrameFlags, INLINE_PAYLOAD_SIZE, MsgDescHot, RpcError, Transport,
    TransportError,
};

const DEFAULT_MAX_PENDING: usize = 8192;

fn max_pending() -> usize {
    std::env::var("RAPACE_MAX_PENDING")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(DEFAULT_MAX_PENDING)
}

/// A chunk received on a tunnel channel.
///
/// This is delivered to tunnel receivers when DATA frames arrive on the channel.
/// For streaming RPCs, this is also used to deliver typed responses that need
/// to be deserialized by the client.
#[derive(Debug, Clone)]
pub struct TunnelChunk {
    /// The payload data.
    pub payload: Vec<u8>,
    /// True if this is the final chunk (EOS received).
    pub is_eos: bool,
    /// True if this chunk represents an error (ERROR flag set).
    /// When true, payload should be parsed as an error using `parse_error_payload`.
    pub is_error: bool,
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

    /// Get the channel IDs of pending RPC calls (for diagnostics).
    ///
    /// Returns a sorted list of channel IDs that are waiting for responses.
    pub fn pending_channel_ids(&self) -> Vec<u32> {
        let pending = self.pending.lock();
        let mut ids: Vec<u32> = pending.keys().copied().collect();
        ids.sort_unstable();
        ids
    }

    /// Get the channel IDs of active tunnels (for diagnostics).
    ///
    /// Returns a sorted list of channel IDs with registered tunnels.
    pub fn tunnel_channel_ids(&self) -> Vec<u32> {
        let tunnels = self.tunnels.lock();
        let mut ids: Vec<u32> = tunnels.keys().copied().collect();
        ids.sort_unstable();
        ids
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
    fn register_pending(
        &self,
        channel_id: u32,
    ) -> Result<oneshot::Receiver<ReceivedFrame>, RpcError> {
        let mut pending = self.pending.lock();
        let pending_len = pending.len();
        let max = max_pending();
        if pending_len >= max {
            tracing::warn!(
                pending_len,
                max_pending = max,
                "too many pending RPC calls; refusing new call"
            );
            return Err(RpcError::Status {
                code: ErrorCode::ResourceExhausted,
                message: "too many pending RPC calls".into(),
            });
        }

        let (tx, rx) = oneshot::channel();
        pending.insert(channel_id, tx);
        Ok(rx)
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
    /// Returns `true` if routed to tunnel, `false` if no tunnel exists.
    async fn try_route_to_tunnel(
        &self,
        channel_id: u32,
        payload: Vec<u8>,
        flags: FrameFlags,
    ) -> bool {
        let sender = {
            let tunnels = self.tunnels.lock();
            tunnels.get(&channel_id).cloned()
        };

        if let Some(tx) = sender {
            let is_eos = flags.contains(FrameFlags::EOS);
            let is_error = flags.contains(FrameFlags::ERROR);
            tracing::debug!(
                channel_id,
                payload_len = payload.len(),
                is_eos,
                is_error,
                "try_route_to_tunnel: routing to tunnel"
            );
            let chunk = TunnelChunk {
                payload,
                is_eos,
                is_error,
            };

            // Send with backpressure; if receiver dropped, remove the tunnel
            if tx.send(chunk).await.is_err() {
                tracing::debug!(
                    channel_id,
                    "try_route_to_tunnel: receiver dropped, removing tunnel"
                );
                self.tunnels.lock().remove(&channel_id);
            }

            // If EOS, remove the tunnel registration
            if is_eos {
                tracing::debug!(
                    channel_id,
                    "try_route_to_tunnel: EOS received, removing tunnel"
                );
                self.tunnels.lock().remove(&channel_id);
            }

            true // Frame was handled by tunnel
        } else {
            tracing::trace!(channel_id, "try_route_to_tunnel: no tunnel for channel");
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

    /// Start a streaming RPC call.
    ///
    /// This sends the request and returns a receiver for streaming responses.
    /// Unlike `call()`, this doesn't wait for a single response - instead,
    /// responses are routed to the returned receiver as `TunnelChunk`s.
    ///
    /// The caller should:
    /// 1. Consume chunks from the receiver
    /// 2. Check `chunk.is_error` and parse as error if true
    /// 3. Otherwise deserialize `chunk.payload` as the expected type
    /// 4. Stop when `chunk.is_eos` is true
    ///
    /// # Example
    ///
    /// ```ignore
    /// let rx = session.start_streaming_call(method_id, payload).await?;
    /// while let Some(chunk) = rx.recv().await {
    ///     if chunk.is_error {
    ///         let err = parse_error_payload(&chunk.payload);
    ///         return Err(err);
    ///     }
    ///     if chunk.is_eos && chunk.payload.is_empty() {
    ///         break; // Stream ended normally
    ///     }
    ///     let item: T = deserialize(&chunk.payload)?;
    ///     // process item...
    /// }
    /// ```
    pub async fn start_streaming_call(
        &self,
        method_id: u32,
        payload: Vec<u8>,
    ) -> Result<mpsc::Receiver<TunnelChunk>, RpcError> {
        let channel_id = self.next_channel_id();

        // Register tunnel BEFORE sending, so responses are routed correctly
        let rx = self.register_tunnel(channel_id);

        // Build a normal unary request frame
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

        tracing::debug!(
            method_id,
            channel_id,
            "start_streaming_call: sending request frame"
        );

        self.transport
            .send_frame(&frame)
            .await
            .map_err(RpcError::Transport)?;

        tracing::debug!(method_id, channel_id, "start_streaming_call: request sent");

        Ok(rx)
    }

    /// Send a request and wait for a response.
    ///
    /// # Here be dragons
    ///
    /// This is a low-level API. Prefer using generated service clients (e.g.,
    /// `FooClient::new(session).bar(...)`) which handle method IDs correctly.
    ///
    /// Method IDs are FNV-1a hashes, not sequential integers. Hardcoding method
    /// IDs will break when services change and produce cryptic errors.
    #[doc(hidden)]
    pub async fn call(
        &self,
        channel_id: u32,
        method_id: u32,
        payload: Vec<u8>,
    ) -> Result<ReceivedFrame, RpcError> {
        struct PendingGuard<'a, T: Transport> {
            session: &'a RpcSession<T>,
            channel_id: u32,
            active: bool,
        }

        impl<'a, T: Transport> PendingGuard<'a, T> {
            fn disarm(&mut self) {
                self.active = false;
            }
        }

        impl<T: Transport> Drop for PendingGuard<'_, T> {
            fn drop(&mut self) {
                if !self.active {
                    return;
                }
                if self
                    .session
                    .pending
                    .lock()
                    .remove(&self.channel_id)
                    .is_some()
                {
                    tracing::debug!(
                        channel_id = self.channel_id,
                        "call cancelled/dropped: removed pending waiter"
                    );
                }
            }
        }

        // Register waiter before sending
        let rx = self.register_pending(channel_id)?;
        let mut guard = PendingGuard {
            session: self,
            channel_id,
            active: true,
        };

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

        // Wait for response with timeout (cross-platform: works on native and WASM)
        let timeout_ms = std::env::var("RAPACE_CALL_TIMEOUT_MS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(30_000); // Default 30 seconds

        use futures_timeout::TimeoutExt;
        let received = match rx
            .timeout(std::time::Duration::from_millis(timeout_ms))
            .await
        {
            Ok(Ok(frame)) => frame,
            Ok(Err(_)) => {
                return Err(RpcError::Status {
                    code: ErrorCode::Internal,
                    message: "response channel closed".into(),
                });
            }
            Err(_elapsed) => {
                tracing::error!(
                    channel_id,
                    method_id,
                    timeout_ms,
                    "RPC call timed out waiting for response"
                );
                return Err(RpcError::DeadlineExceeded);
            }
        };

        guard.disarm();
        Ok(received)
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
        tracing::debug!("RpcSession::run: starting demux loop");
        loop {
            // Receive next frame
            let frame = match self.transport.recv_frame().await {
                Ok(f) => f,
                Err(TransportError::Closed) => {
                    tracing::debug!("RpcSession::run: transport closed");
                    return Ok(());
                }
                Err(e) => {
                    tracing::error!(?e, "RpcSession::run: transport error");
                    return Err(e);
                }
            };

            let channel_id = frame.desc.channel_id;
            let method_id = frame.desc.method_id;
            let flags = frame.desc.flags;
            let payload = frame.payload.to_vec();

            tracing::debug!(
                channel_id,
                method_id,
                ?flags,
                payload_len = payload.len(),
                "RpcSession::run: received frame"
            );

            // 1. Try to route to a tunnel first (highest priority)
            if self
                .try_route_to_tunnel(channel_id, payload.clone(), flags)
                .await
            {
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
                    // If a service handler panics, without this the client can hang forever
                    // waiting for a response on this channel.
                    let result = AssertUnwindSafe(response_future).catch_unwind().await;

                    let response_result: Result<Frame, RpcError> = match result {
                        Ok(r) => r,
                        Err(panic) => {
                            let message = if let Some(s) = panic.downcast_ref::<&str>() {
                                format!("panic in dispatcher: {s}")
                            } else if let Some(s) = panic.downcast_ref::<String>() {
                                format!("panic in dispatcher: {s}")
                            } else {
                                "panic in dispatcher".to_string()
                            };
                            Err(RpcError::Status {
                                code: ErrorCode::Internal,
                                message,
                            })
                        }
                    };

                    match response_result {
                        Ok(mut response) => {
                            // Set the channel_id on the response
                            response.desc.channel_id = channel_id;
                            if let Err(e) = transport.send_frame(&response).await {
                                tracing::warn!(
                                    channel_id,
                                    error = ?e,
                                    "RpcSession::run: failed to send response frame"
                                );
                            }
                        }
                        Err(e) => {
                            // Send error response
                            let mut desc = MsgDescHot::new();
                            desc.channel_id = channel_id;
                            desc.flags = FrameFlags::ERROR | FrameFlags::EOS;

                            let (code, message): (u32, String) = match &e {
                                RpcError::Status { code, message } => {
                                    (*code as u32, message.clone())
                                }
                                RpcError::Transport(_) => {
                                    (ErrorCode::Internal as u32, "transport error".into())
                                }
                                RpcError::Cancelled => {
                                    (ErrorCode::Cancelled as u32, "cancelled".into())
                                }
                                RpcError::DeadlineExceeded => (
                                    ErrorCode::DeadlineExceeded as u32,
                                    "deadline exceeded".into(),
                                ),
                            };

                            let mut err_bytes = Vec::with_capacity(8 + message.len());
                            err_bytes.extend_from_slice(&code.to_le_bytes());
                            err_bytes.extend_from_slice(&(message.len() as u32).to_le_bytes());
                            err_bytes.extend_from_slice(message.as_bytes());

                            let frame = Frame::with_payload(desc, err_bytes);
                            if let Err(e) = transport.send_frame(&frame).await {
                                tracing::warn!(
                                    channel_id,
                                    error = ?e,
                                    "RpcSession::run: failed to send error frame"
                                );
                            }
                        }
                    };
                });
            } else {
                tracing::warn!(
                    channel_id,
                    method_id,
                    "RpcSession::run: no dispatcher registered; dropping request (client may hang)"
                );
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
mod pending_cleanup_tests {
    use super::*;
    use crate::{EncodeCtx, EncodeError, TransportError};
    use tokio::sync::mpsc;

    struct DummyEncoder {
        payload: Vec<u8>,
    }

    impl EncodeCtx for DummyEncoder {
        fn encode_bytes(&mut self, bytes: &[u8]) -> Result<(), EncodeError> {
            self.payload.extend_from_slice(bytes);
            Ok(())
        }

        fn finish(self: Box<Self>) -> Result<Frame, EncodeError> {
            Ok(Frame::with_payload(MsgDescHot::new(), self.payload))
        }
    }

    struct SinkTransport {
        tx: mpsc::Sender<Frame>,
    }

    impl Transport for SinkTransport {
        async fn send_frame(&self, frame: &Frame) -> Result<(), TransportError> {
            self.tx
                .send(frame.clone())
                .await
                .map_err(|_| TransportError::Closed)
        }

        async fn recv_frame(&self) -> Result<crate::FrameView<'_>, TransportError> {
            Err(TransportError::Closed)
        }

        fn encoder(&self) -> Box<dyn EncodeCtx + '_> {
            Box::new(DummyEncoder {
                payload: Vec::new(),
            })
        }

        async fn close(&self) -> Result<(), TransportError> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_call_cancellation_cleans_pending() {
        let (tx, _rx) = mpsc::channel(8);
        let client_transport = SinkTransport { tx };
        let client = Arc::new(RpcSession::with_channel_start(
            Arc::new(client_transport),
            2,
        ));

        let client2 = client.clone();
        let channel_id = client.next_channel_id();
        let task = tokio::spawn(async move {
            let _ = client2.call(channel_id, 123, vec![1, 2, 3]).await;
        });

        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(1);
        while !client.pending.lock().contains_key(&channel_id) {
            if tokio::time::Instant::now() >= deadline {
                panic!("call did not register pending waiter in time");
            }
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        }

        task.abort();
        let _ = task.await;

        assert_eq!(client.pending.lock().len(), 0);
    }
}

// Note: RpcSession tests live in rapace-testkit to avoid circular dev-dependencies
// between rapace-core and rapace-transport-mem. See rapace-testkit for test coverage.
