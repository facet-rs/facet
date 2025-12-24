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
//!                        │  transport: Transport          │
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

use bytes::Bytes;
use futures::FutureExt;
use parking_lot::Mutex;
use tokio::sync::{mpsc, oneshot};

use crate::{
    BufferPool, ErrorCode, Frame, FrameFlags, INLINE_PAYLOAD_SIZE, MsgDescHot, RpcError, Transport,
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
#[derive(Debug)]
pub struct TunnelChunk {
    /// The received frame.
    pub frame: Frame,
}

impl TunnelChunk {
    /// Borrow payload bytes for this chunk.
    pub fn payload_bytes(&self) -> &[u8] {
        self.frame.payload_bytes()
    }

    /// True if this is the final chunk (EOS received).
    pub fn is_eos(&self) -> bool {
        self.frame.desc.flags.contains(FrameFlags::EOS)
    }

    /// True if this chunk represents an error (ERROR flag set).
    pub fn is_error(&self) -> bool {
        self.frame.desc.flags.contains(FrameFlags::ERROR)
    }

    /// Convert the chunk's payload to `Bytes`, consuming the chunk.
    ///
    /// This efficiently converts the payload with zero-copy when possible
    /// (e.g., for pooled buffers that return to the pool when dropped).
    pub fn into_payload_bytes(self) -> bytes::Bytes {
        self.frame.into_payload_bytes()
    }
}

/// A frame that was received and routed.
#[derive(Debug)]
pub struct ReceivedFrame {
    pub frame: Frame,
}

impl ReceivedFrame {
    pub fn channel_id(&self) -> u32 {
        self.frame.desc.channel_id
    }

    pub fn method_id(&self) -> u32 {
        self.frame.desc.method_id
    }

    pub fn flags(&self) -> FrameFlags {
        self.frame.desc.flags
    }

    pub fn payload_bytes(&self) -> &[u8] {
        self.frame.payload_bytes()
    }
}

/// Log level for timeout errors.
///
/// Used by the `log_timeout` helper to distinguish between critical timeouts
/// (logged as errors) and less critical ones (logged as warnings).
#[derive(Debug, Clone, Copy)]
enum TimeoutLogLevel {
    /// Log at ERROR level - for critical timeouts
    Error,
    /// Log at WARN level - for less critical timeouts
    Warn,
}

/// Type alias for a boxed async dispatch function.
pub type BoxedDispatcher = Box<
    dyn Fn(Frame) -> Pin<Box<dyn Future<Output = Result<Frame, RpcError>> + Send>> + Send + Sync,
>;

/// RpcSession owns a transport and multiplexes frames between clients and servers.
///
/// # Key invariant
///
/// Only `RpcSession::run()` calls `transport.recv_frame()`. No other code should
/// touch `recv_frame` directly. This prevents the race condition where multiple
/// callers compete for incoming frames.
pub struct RpcSession {
    transport: Transport,

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

impl RpcSession {
    /// Create a new RPC session wrapping the given transport handle.
    ///
    /// The `start_channel_id` parameter allows different sessions to use different
    /// channel ID ranges, avoiding collisions in bidirectional RPC scenarios.
    /// - Odd IDs (1, 3, 5, ...): typically used by one side
    /// - Even IDs (2, 4, 6, ...): typically used by the other side
    pub fn new(transport: Transport) -> Self {
        Self::with_channel_start(transport, 1)
    }

    /// Create a new RPC session with a custom starting channel ID.
    ///
    /// Use this when you need to coordinate channel IDs between two sessions.
    /// For bidirectional RPC over a single transport pair:
    /// - Host session: start at 1 (uses odd channel IDs)
    /// - Plugin session: start at 2 (uses even channel IDs)
    pub fn with_channel_start(transport: Transport, start_channel_id: u32) -> Self {
        Self {
            transport,
            pending: Mutex::new(HashMap::new()),
            tunnels: Mutex::new(HashMap::new()),
            dispatcher: Mutex::new(None),
            next_msg_id: AtomicU64::new(1),
            next_channel_id: AtomicU32::new(start_channel_id),
        }
    }

    /// Get a reference to the buffer pool for optimized serialization.
    ///
    /// This returns the transport's buffer pool, which is shared for both
    /// sending and receiving frames. Use this in conjunction with
    /// `rapace::postcard_to_pooled_buf` to serialize RPC payloads without
    /// allocating a fresh `Vec<u8>` each time.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use rapace::postcard_to_pooled_buf;
    ///
    /// let session = Arc::new(RpcSession::new(transport));
    /// let request = MyRequest { id: 42 };
    /// let buf = postcard_to_pooled_buf(session.buffer_pool(), &request)?;
    /// ```
    pub fn buffer_pool(&self) -> &BufferPool {
        self.transport.buffer_pool()
    }

    /// Get a reference to the underlying transport.
    pub fn transport(&self) -> &Transport {
        &self.transport
    }

    /// Close the underlying transport.
    ///
    /// This signals the transport to shut down. The `run()` loop will exit
    /// once the transport is closed and all pending frames are processed.
    pub fn close(&self) {
        self.transport.close();
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

    fn has_pending(&self, channel_id: u32) -> bool {
        self.pending.lock().contains_key(&channel_id)
    }

    fn has_tunnel(&self, channel_id: u32) -> bool {
        self.tunnels.lock().contains_key(&channel_id)
    }

    /// Register a dispatcher for incoming requests.
    ///
    /// The dispatcher receives the request frame and returns a response frame.
    /// If no dispatcher is registered, incoming requests are dropped with a warning.
    pub fn set_dispatcher<F, Fut>(&self, dispatcher: F)
    where
        F: Fn(Frame) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Frame, RpcError>> + Send + 'static,
    {
        let boxed: BoxedDispatcher = Box::new(move |frame| Box::pin(dispatcher(frame)));
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
        tracing::debug!(
            channel_id,
            pending_len = pending_len + 1,
            max_pending = max,
            "registered pending waiter"
        );
        Ok(rx)
    }

    /// Try to route a frame to a pending waiter.
    /// Returns true if the frame was consumed (waiter found), false otherwise.
    fn try_route_to_pending(&self, channel_id: u32, frame: ReceivedFrame) -> Option<ReceivedFrame> {
        let pending_snapshot = self.pending_channel_ids();
        let waiter = self.pending.lock().remove(&channel_id);
        if let Some(tx) = waiter {
            // Waiter found - deliver the frame
            tracing::debug!(
                channel_id,
                msg_id = frame.frame.desc.msg_id,
                method_id = frame.frame.desc.method_id,
                flags = ?frame.frame.desc.flags,
                payload_len = frame.payload_bytes().len(),
                "try_route_to_pending: delivered to waiter"
            );
            let _ = tx.send(frame);
            None
        } else {
            tracing::debug!(
                channel_id,
                msg_id = frame.frame.desc.msg_id,
                method_id = frame.frame.desc.method_id,
                flags = ?frame.frame.desc.flags,
                pending = ?pending_snapshot,
                "try_route_to_pending: no waiter for channel"
            );
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
        tracing::debug!(channel_id, "tunnel registered");
        rx
    }

    /// Allocate a fresh tunnel channel ID and return a first-class tunnel stream.
    ///
    /// This is a convenience wrapper around `next_channel_id()` + `register_tunnel()`.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn open_tunnel_stream(self: &Arc<Self>) -> (crate::TunnelHandle, crate::TunnelStream) {
        crate::TunnelStream::open(self.clone())
    }

    /// Create a first-class tunnel stream for an existing channel ID.
    ///
    /// This registers the tunnel receiver immediately.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn tunnel_stream(self: &Arc<Self>, channel_id: u32) -> crate::TunnelStream {
        crate::TunnelStream::new(self.clone(), channel_id)
    }

    /// Try to route a frame to a tunnel.
    /// Returns `Ok(())` if the frame was handled by a tunnel, or `Err(frame)` if
    /// no tunnel exists for this channel.
    async fn try_route_to_tunnel(&self, frame: Frame) -> Result<(), Frame> {
        let channel_id = frame.desc.channel_id;
        let flags = frame.desc.flags;
        let sender = self.tunnels.lock().get(&channel_id).cloned();

        if let Some(tx) = sender {
            tracing::debug!(
                channel_id,
                msg_id = frame.desc.msg_id,
                method_id = frame.desc.method_id,
                flags = ?flags,
                payload_len = frame.payload_bytes().len(),
                is_eos = flags.contains(FrameFlags::EOS),
                is_error = flags.contains(FrameFlags::ERROR),
                "try_route_to_tunnel: routing to tunnel"
            );
            let chunk = TunnelChunk { frame };

            // Send with backpressure; if receiver dropped, remove the tunnel
            if tx.send(chunk).await.is_err() {
                tracing::debug!(
                    channel_id,
                    "try_route_to_tunnel: receiver dropped, removing tunnel"
                );
                self.tunnels.lock().remove(&channel_id);
            }

            // If EOS, remove the tunnel registration
            if flags.contains(FrameFlags::EOS) {
                tracing::debug!(
                    channel_id,
                    "try_route_to_tunnel: EOS received, removing tunnel"
                );
                self.tunnels.lock().remove(&channel_id);
            }

            Ok(()) // Frame was handled by tunnel
        } else {
            tracing::trace!(
                channel_id,
                msg_id = frame.desc.msg_id,
                method_id = frame.desc.method_id,
                payload_len = frame.payload_bytes().len(),
                is_eos = flags.contains(FrameFlags::EOS),
                is_error = flags.contains(FrameFlags::ERROR),
                flags = ?flags,
                "try_route_to_tunnel: no tunnel for channel"
            );
            Err(frame) // No tunnel, continue normal processing
        }
    }

    /// Send a chunk on a tunnel channel.
    ///
    /// This sends a DATA frame on the channel. The chunk is not marked with EOS;
    /// use `close_tunnel()` to send the final chunk.
    pub async fn send_chunk(&self, channel_id: u32, payload: Bytes) -> Result<(), RpcError> {
        let mut desc = MsgDescHot::new();
        desc.msg_id = self.next_msg_id();
        desc.channel_id = channel_id;
        desc.method_id = 0; // Tunnels don't use method_id
        desc.flags = FrameFlags::DATA;

        let payload_len = payload.len();
        tracing::debug!(channel_id, payload_len, "send_chunk");
        let frame = if payload_len <= INLINE_PAYLOAD_SIZE {
            Frame::with_inline_payload(desc, &payload).expect("inline payload should fit")
        } else {
            Frame::with_bytes(desc, payload)
        };

        self.transport
            .send_frame(frame)
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

        tracing::debug!(channel_id, "close_tunnel");

        self.transport
            .send_frame(frame)
            .await
            .map_err(RpcError::Transport)
    }

    /// Unregister a tunnel without sending EOS.
    ///
    /// Use this when the tunnel was closed by the remote side (you received EOS)
    /// and you want to clean up without sending another EOS.
    pub fn unregister_tunnel(&self, channel_id: u32) {
        tracing::debug!(channel_id, "tunnel unregistered");
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
        // Streaming calls do not have a unary response frame. The server will
        // respond by sending DATA chunks on the same channel and ending with EOS.
        // Mark the request as NO_REPLY so server-side RpcSession dispatchers
        // don't attempt to send a unary response frame that would corrupt the stream.
        desc.flags = FrameFlags::DATA | FrameFlags::EOS | FrameFlags::NO_REPLY;

        let payload_len = payload.len();
        let frame = if payload_len <= INLINE_PAYLOAD_SIZE {
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
            .send_frame(frame)
            .await
            .map_err(RpcError::Transport)?;

        tracing::debug!(method_id, channel_id, "start_streaming_call: request sent");

        Ok(rx)
    }

    /// Start a streaming RPC call using a pooled buffer.
    ///
    /// This is the optimized version of `start_streaming_call` that accepts a
    /// `PooledBuf` directly, avoiding unnecessary allocations.
    pub async fn start_streaming_call_pooled(
        &self,
        method_id: u32,
        payload: crate::PooledBuf,
    ) -> Result<mpsc::Receiver<TunnelChunk>, RpcError> {
        let channel_id = self.next_channel_id();

        // Register tunnel BEFORE sending, so responses are routed correctly
        let rx = self.register_tunnel(channel_id);

        // Build a normal unary request frame
        let mut desc = MsgDescHot::new();
        desc.msg_id = self.next_msg_id();
        desc.channel_id = channel_id;
        desc.method_id = method_id;
        desc.flags = FrameFlags::DATA | FrameFlags::EOS | FrameFlags::NO_REPLY;

        let payload_len = payload.len();
        let frame = if payload_len <= INLINE_PAYLOAD_SIZE {
            Frame::with_inline_payload(desc, &payload).expect("inline payload should fit")
        } else {
            Frame::with_pooled_payload(desc, payload)
        };

        tracing::debug!(
            method_id,
            channel_id,
            "start_streaming_call_pooled: sending request frame"
        );

        self.transport
            .send_frame(frame)
            .await
            .map_err(RpcError::Transport)?;

        tracing::debug!(
            method_id,
            channel_id,
            "start_streaming_call_pooled: request sent"
        );

        Ok(rx)
    }

    /// Log a timeout error with method name lookup from the registry.
    ///
    /// This helper reduces code duplication by centralizing the logic for:
    /// - Looking up method names from the global service registry
    /// - Logging timeout errors with or without method name information
    /// - Supporting different log levels (error vs warn)
    fn log_timeout(channel_id: u32, method_id: u32, timeout_ms: u64, level: TimeoutLogLevel) {
        let method_name = rapace_registry::ServiceRegistry::with_global(|reg| {
            reg.method_by_id(rapace_registry::MethodId(method_id))
                .map(|m| m.full_name.clone())
        });

        // Macro to eliminate duplication between error and warn branches
        macro_rules! log_at_level {
            ($level:ident) => {
                if let Some(method) = method_name {
                    tracing::$level!(
                        channel_id,
                        method = method.as_str(),
                        method_id,
                        timeout_ms,
                        "RPC call timed out waiting for response"
                    );
                } else {
                    tracing::$level!(
                        channel_id,
                        method_id,
                        timeout_ms,
                        "RPC call timed out waiting for response"
                    );
                }
            };
        }

        match level {
            TimeoutLogLevel::Error => log_at_level!(error),
            TimeoutLogLevel::Warn => log_at_level!(warn),
        }
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
        struct PendingGuard<'a> {
            session: &'a RpcSession,
            channel_id: u32,
            active: bool,
        }

        impl<'a> PendingGuard<'a> {
            fn disarm(&mut self) {
                self.active = false;
            }
        }

        impl Drop for PendingGuard<'_> {
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

        let payload_len = payload.len();
        let frame = if payload_len <= INLINE_PAYLOAD_SIZE {
            Frame::with_inline_payload(desc, &payload).expect("inline payload should fit")
        } else {
            Frame::with_payload(desc, payload)
        };

        self.transport
            .send_frame(frame)
            .await
            .map_err(RpcError::Transport)?;

        tracing::debug!(
            channel_id,
            method_id,
            msg_id = desc.msg_id,
            payload_len,
            "call: request sent"
        );

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
                Self::log_timeout(channel_id, method_id, timeout_ms, TimeoutLogLevel::Error);
                return Err(RpcError::DeadlineExceeded);
            }
        };

        guard.disarm();
        Ok(received)
    }

    /// Call a remote method using a pooled buffer.
    ///
    /// This is the optimized version of `call` that accepts a `PooledBuf` directly,
    /// avoiding the need to convert to `Vec<u8>`. The buffer is automatically returned
    /// to the pool after the frame is sent.
    ///
    /// Use this in conjunction with `self.buffer_pool()` and `postcard_to_pooled_buf`.
    pub async fn call_pooled(
        &self,
        channel_id: u32,
        method_id: u32,
        payload: crate::PooledBuf,
    ) -> Result<ReceivedFrame, RpcError> {
        struct PendingGuard<'a> {
            session: &'a RpcSession,
            channel_id: u32,
            active: bool,
        }

        impl<'a> PendingGuard<'a> {
            fn disarm(&mut self) {
                self.active = false;
            }
        }

        impl Drop for PendingGuard<'_> {
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

        let payload_len = payload.len();
        let frame = if payload_len <= INLINE_PAYLOAD_SIZE {
            Frame::with_inline_payload(desc, &payload).expect("inline payload should fit")
        } else {
            Frame::with_pooled_payload(desc, payload)
        };

        self.transport
            .send_frame(frame)
            .await
            .map_err(RpcError::Transport)?;

        tracing::debug!(
            channel_id,
            method_id,
            msg_id = desc.msg_id,
            payload_len,
            "call: request sent"
        );

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
                tracing::warn!(channel_id, method_id, "call: response channel closed");
                return Err(RpcError::Transport(TransportError::Closed));
            }
            Err(_) => {
                Self::log_timeout(channel_id, method_id, timeout_ms, TimeoutLogLevel::Warn);
                return Err(RpcError::DeadlineExceeded);
            }
        };

        guard.disarm();
        Ok(received)
    }

    /// Send a request frame without registering a waiter or waiting for a reply.
    ///
    /// This is useful for fire-and-forget notifications (e.g. tracing events).
    ///
    /// The request is sent on channel 0 (the "no channel" channel). The receiver
    /// may still dispatch it like a normal unary RPC request, but if it honors
    /// [`FrameFlags::NO_REPLY`] it will not send a response frame.
    pub async fn notify(&self, method_id: u32, payload: Vec<u8>) -> Result<(), RpcError> {
        let channel_id = 0;

        let mut desc = MsgDescHot::new();
        desc.msg_id = self.next_msg_id();
        desc.channel_id = channel_id;
        desc.method_id = method_id;
        desc.flags = FrameFlags::DATA | FrameFlags::EOS | FrameFlags::NO_REPLY;

        let frame = if payload.len() <= INLINE_PAYLOAD_SIZE {
            Frame::with_inline_payload(desc, &payload).expect("inline payload should fit")
        } else {
            Frame::with_payload(desc, payload)
        };

        self.transport
            .send_frame(frame)
            .await
            .map_err(RpcError::Transport)
    }

    /// Send a response frame.
    pub async fn send_response(&self, frame: Frame) -> Result<(), RpcError> {
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
            let has_tunnel = self.has_tunnel(channel_id);
            let has_pending = self.has_pending(channel_id);

            tracing::debug!(
                channel_id,
                method_id,
                ?flags,
                has_tunnel,
                has_pending,
                payload_len = frame.payload_bytes().len(),
                "RpcSession::run: received frame"
            );

            // 1. Try to route to a tunnel first (highest priority)
            let frame = match self.try_route_to_tunnel(frame).await {
                Ok(()) => continue,
                Err(frame) => frame,
            };

            let received = ReceivedFrame { frame };

            // 2. Try to route to a pending RPC waiter (responses only).
            //
            // In Rapace, responses are encoded with `method_id = 0`. Requests use a non-zero
            // method ID and are dispatched to the registered handler, so attempting
            // "pending waiter" routing for every request just produces log spam.
            let received = if method_id == 0 {
                match self.try_route_to_pending(channel_id, received) {
                    None => continue, // Frame was delivered to waiter
                    Some(unroutable) => {
                        // `method_id = 0` frames are responses/tunnel chunks. If a response arrives
                        // without a registered waiter (and we already failed to route it to a tunnel),
                        // there's nowhere correct to send it. Log once and drop.
                        tracing::warn!(
                            channel_id,
                            msg_id = unroutable.frame.desc.msg_id,
                            flags = ?unroutable.frame.desc.flags,
                            payload_len = unroutable.payload_bytes().len(),
                            "RpcSession::run: unroutable response frame (no pending waiter)"
                        );
                        continue;
                    }
                }
            } else {
                received
            };

            // Skip non-data frames (control frames, etc.)
            if !received.flags().contains(FrameFlags::DATA) {
                continue;
            }

            let no_reply = received.flags().contains(FrameFlags::NO_REPLY);
            tracing::debug!(channel_id, method_id, no_reply, "dispatching request");

            // Dispatch to handler
            // We need to call the dispatcher while holding the lock, then spawn the future
            let response_future = {
                let guard = self.dispatcher.lock();
                if let Some(dispatcher) = guard.as_ref() {
                    Some(dispatcher(received.frame))
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

                    if no_reply {
                        if let Err(e) = response_result {
                            tracing::debug!(
                                channel_id,
                                error = ?e,
                                "RpcSession::run: no-reply request failed"
                            );
                        } else {
                            tracing::debug!(channel_id, "RpcSession::run: no-reply request ok");
                        }
                        return;
                    }

                    match response_result {
                        Ok(mut response) => {
                            // Set the channel_id on the response
                            response.desc.channel_id = channel_id;
                            if let Err(e) = transport.send_frame(response).await {
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
                                RpcError::Serialize(e) => (
                                    ErrorCode::Internal as u32,
                                    format!("serialize error: {}", e),
                                ),
                                RpcError::Deserialize(e) => (
                                    ErrorCode::Internal as u32,
                                    format!("deserialize error: {}", e),
                                ),
                            };

                            let mut err_bytes = Vec::with_capacity(8 + message.len());
                            err_bytes.extend_from_slice(&code.to_le_bytes());
                            err_bytes.extend_from_slice(&(message.len() as u32).to_le_bytes());
                            err_bytes.extend_from_slice(message.as_bytes());

                            let frame = Frame::with_payload(desc, err_bytes);
                            if let Err(e) = transport.send_frame(frame).await {
                                tracing::warn!(
                                    channel_id,
                                    error = ?e,
                                    "RpcSession::run: failed to send error frame"
                                );
                            }
                        }
                    };
                });
            } else if !no_reply {
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

// Note: RpcSession conformance tests live in `crates/rapace-core/tests/`.
