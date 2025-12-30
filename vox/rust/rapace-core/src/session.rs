//! RpcSession: A multiplexed RPC session that owns the transport.
//!
//! See spec: [Core Protocol](https://rapace.dev/spec/core/)
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

use futures_util::FutureExt;
use parking_lot::Mutex;
use tokio::sync::{mpsc, oneshot};

use crate::{
    BufferPool, CancelChannel, CancelReason, ErrorCode, Frame, FrameFlags, Hello,
    INLINE_PAYLOAD_SIZE, Limits, MsgDescHot, OpenChannel, Ping, Pong, PooledBuf, Role, RpcError,
    Transport, TransportError, control_method,
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
///
/// Spec: `[impl core.tunnel.raw-bytes]` - TUNNEL payloads are raw bytes, not Postcard.
/// Spec: `[impl core.tunnel.frame-boundaries]` - frame boundaries are transport artifacts.
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
    ///
    /// Spec: `[impl core.eos.after-send]` - EOS means sender will not send more DATA.
    /// Spec: `[impl core.tunnel.semantics]` - EOS indicates half-close (like TCP FIN).
    pub fn is_eos(&self) -> bool {
        self.frame.desc.flags.contains(FrameFlags::EOS)
    }

    /// True if this chunk represents an error (ERROR flag set).
    pub fn is_error(&self) -> bool {
        self.frame.desc.flags.contains(FrameFlags::ERROR)
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
/// Spec: `[impl core.channel.lifecycle]` - channels are opened via OpenChannel, closed via EOS/Cancel.
/// Spec: `[impl core.channel.id.no-reuse]` - channel IDs MUST NOT be reused within a connection.
///
/// # Key invariant
///
/// Only `RpcSession::run()` calls `transport.recv_frame()`. No other code should
/// touch `recv_frame` directly. This prevents the race condition where multiple
/// callers compete for incoming frames.
///
/// # Generic Transport
///
/// `RpcSession` is generic over the transport type `T`, enabling:
/// - **Monomorphization**: `RpcSession<ShmTransport>` compiles to transport-specific code
///   with zero abstraction overhead and full dead code elimination
/// - **Type erasure**: `RpcSession<AnyTransport>` provides runtime polymorphism when needed
///
/// Use the concrete transport type when you know it at compile time for best performance.
/// Use `AnyTransport` when you need to handle multiple transport types dynamically.
pub struct RpcSession<T: Transport> {
    transport: T,

    /// Our role in the connection (Initiator or Acceptor).
    /// Spec: `[impl handshake.role.validation]` - roles must be complementary.
    role: Role,

    /// Pending response waiters: channel_id -> oneshot sender.
    /// When a client sends a request, it registers a waiter here.
    /// When a response arrives, the demux loop finds the waiter and delivers.
    pending: Mutex<HashMap<u32, oneshot::Sender<ReceivedFrame>>>,

    /// Active tunnel channels: channel_id -> mpsc sender.
    /// When a tunnel is registered, incoming DATA frames on that channel
    /// are routed to the tunnel's receiver instead of being dispatched as RPC.
    tunnels: Mutex<HashMap<u32, mpsc::Sender<TunnelChunk>>>,

    /// Open channels (channels that have been opened via OpenChannel).
    /// Spec: `[impl core.channel.open]` - channels MUST be opened via OpenChannel before use.
    open_channels: Mutex<std::collections::HashSet<u32>>,

    /// Optional dispatcher for incoming requests.
    /// If set, incoming requests (frames that don't match a pending waiter)
    /// are dispatched through this function.
    dispatcher: Mutex<Option<BoxedDispatcher>>,

    /// Next message ID for outgoing frames.
    next_msg_id: AtomicU64,

    /// Next channel ID for new RPC calls.
    next_channel_id: AtomicU32,
}

impl<T: Transport> RpcSession<T> {
    /// Create a new RPC session as the initiator (client).
    ///
    /// The initiator:
    /// - Sends Hello first during handshake
    /// - Uses odd channel IDs (1, 3, 5, ...)
    ///
    /// Use this when your code initiates the connection (e.g., client connecting to server).
    ///
    /// Spec: `[impl core.channel.id.parity.initiator]` - initiator uses odd IDs.
    /// Spec: `[impl handshake.ordering]` - initiator sends Hello first.
    pub fn new(transport: T) -> Self {
        Self::new_initiator(transport)
    }

    /// Create a new RPC session as the initiator (client).
    ///
    /// The initiator:
    /// - Sends Hello first during handshake
    /// - Uses odd channel IDs (1, 3, 5, ...)
    ///
    /// Use this when your code initiates the connection (e.g., client connecting to server).
    ///
    /// Spec: `[impl core.channel.id.parity.initiator]` - initiator uses odd IDs.
    /// Spec: `[impl handshake.ordering]` - initiator sends Hello first.
    pub fn new_initiator(transport: T) -> Self {
        Self::with_role(transport, Role::Initiator)
    }

    /// Create a new RPC session as the acceptor (server).
    ///
    /// The acceptor:
    /// - Waits for peer's Hello first during handshake
    /// - Uses even channel IDs (2, 4, 6, ...)
    ///
    /// Use this when your code accepts incoming connections (e.g., server handling clients).
    ///
    /// Spec: `[impl core.channel.id.parity.acceptor]` - acceptor uses even IDs.
    /// Spec: `[impl handshake.ordering]` - acceptor receives Hello first.
    pub fn new_acceptor(transport: T) -> Self {
        Self::with_role(transport, Role::Acceptor)
    }

    /// Create a new RPC session with an explicit role.
    ///
    /// This is the most explicit constructor - use it when you want to be
    /// completely clear about the session's role in the connection.
    ///
    /// # Arguments
    ///
    /// * `transport` - The underlying transport for sending/receiving frames
    /// * `role` - Whether this session is the Initiator or Acceptor
    ///
    /// The role determines:
    /// - Handshake order (initiator sends Hello first)
    /// - Channel ID parity (initiator uses odd IDs, acceptor uses even)
    pub fn with_role(transport: T, role: Role) -> Self {
        let start_channel_id = match role {
            Role::Initiator => 1, // Odd IDs: 1, 3, 5, ...
            Role::Acceptor => 2,  // Even IDs: 2, 4, 6, ...
        };
        Self {
            transport,
            role,
            pending: Mutex::new(HashMap::new()),
            tunnels: Mutex::new(HashMap::new()),
            open_channels: Mutex::new(std::collections::HashSet::new()),
            dispatcher: Mutex::new(None),
            next_msg_id: AtomicU64::new(1),
            next_channel_id: AtomicU32::new(start_channel_id),
        }
    }

    /// Create a new RPC session with a custom starting channel ID.
    ///
    /// **Prefer `new_initiator()`, `new_acceptor()`, or `with_role()` instead.**
    ///
    /// This constructor exists for advanced use cases where you need fine-grained
    /// control over channel ID allocation. The role is derived from the channel ID:
    /// - Odd start (1, 3, ...) → Initiator
    /// - Even start (2, 4, ...) → Acceptor
    ///
    /// # Example
    ///
    /// ```ignore
    /// // For bidirectional RPC over a single transport pair:
    /// let host_session = RpcSession::with_channel_start(transport_a, 1);   // Initiator
    /// let plugin_session = RpcSession::with_channel_start(transport_b, 2); // Acceptor
    /// ```
    pub fn with_channel_start(transport: T, start_channel_id: u32) -> Self {
        let role = if start_channel_id % 2 == 1 {
            Role::Initiator
        } else {
            Role::Acceptor
        };
        Self {
            transport,
            role,
            pending: Mutex::new(HashMap::new()),
            tunnels: Mutex::new(HashMap::new()),
            open_channels: Mutex::new(std::collections::HashSet::new()),
            dispatcher: Mutex::new(None),
            next_msg_id: AtomicU64::new(1),
            next_channel_id: AtomicU32::new(start_channel_id),
        }
    }

    /// Get our role in this connection.
    pub fn role(&self) -> Role {
        self.role
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
    pub fn transport(&self) -> &T {
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
    ///
    /// Spec: `[impl frame.msg-id.scope]` - msg_id is scoped per connection, monotonically increasing.
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
    ///
    /// Spec: `[impl core.channel.id.allocation]` - channel IDs are 32-bit unsigned.
    /// Spec: `[impl core.channel.id.no-reuse]` - IDs MUST NOT be reused within a connection.
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
    pub fn open_tunnel_stream(self: &Arc<Self>) -> (crate::TunnelHandle, crate::TunnelStream<T>) {
        crate::TunnelStream::open(self.clone())
    }

    /// Create a first-class tunnel stream for an existing channel ID.
    ///
    /// This registers the tunnel receiver immediately.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn tunnel_stream(self: &Arc<Self>, channel_id: u32) -> crate::TunnelStream<T> {
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

    /// Handle a control frame (channel 0).
    ///
    /// Spec: `[impl core.control.reserved]` - channel 0 is reserved for control messages.
    /// Spec: `[impl core.ping.semantics]` - receiver MUST respond with Pong echoing payload.
    /// Spec: `[impl core.cancel.idempotent]` - multiple cancels are harmless.
    /// Spec: `[impl core.control.unknown-extension]` - unknown extension verbs SHOULD be ignored.
    async fn handle_control_frame(&self, frame: &ReceivedFrame) -> Result<(), RpcError> {
        let method_id = frame.method_id();

        match method_id {
            control_method::PING => {
                // Spec: `[impl core.ping.semantics]` - respond with Pong echoing the payload
                let ping: Ping =
                    facet_postcard::from_slice(frame.payload_bytes()).map_err(|e| {
                        RpcError::Status {
                            code: ErrorCode::InvalidArgument,
                            message: format!("failed to decode Ping: {}", e),
                        }
                    })?;

                let pong = Pong {
                    payload: ping.payload,
                };

                let pong_bytes = facet_postcard::to_vec(&pong).map_err(|e| RpcError::Status {
                    code: ErrorCode::Internal,
                    message: format!("failed to encode Pong: {}", e),
                })?;

                let mut desc = MsgDescHot::new();
                desc.msg_id = self.next_msg_id();
                desc.channel_id = 0;
                desc.method_id = control_method::PONG;
                desc.flags = FrameFlags::CONTROL;

                let response = Frame::with_inline_payload(desc, &pong_bytes)
                    .expect("Pong payload should fit inline");

                self.transport
                    .send_frame(response)
                    .await
                    .map_err(RpcError::Transport)?;

                tracing::debug!("handled Ping, sent Pong");
                Ok(())
            }

            control_method::PONG => {
                // Pong is a response to Ping - we don't currently track outstanding pings,
                // so we just log and ignore.
                tracing::debug!("received Pong (ignored)");
                Ok(())
            }

            control_method::CANCEL_CHANNEL => {
                // Spec: `[impl core.cancel.idempotent]` - multiple cancels are harmless.
                // We accept CancelChannel even for non-existent channels without error.
                // The harness expects us to stay connected after receiving CancelChannel.
                tracing::debug!(
                    payload_len = frame.payload_bytes().len(),
                    "received CancelChannel (acknowledged)"
                );
                Ok(())
            }

            control_method::OPEN_CHANNEL => {
                // Spec: `[impl core.channel.open]` - channels MUST be opened via OpenChannel.
                // Spec: `[impl core.channel.id.zero-reserved]` - channel 0 is reserved.
                let open: OpenChannel =
                    facet_postcard::from_slice(frame.payload_bytes()).map_err(|e| {
                        RpcError::Status {
                            code: ErrorCode::InvalidArgument,
                            message: format!("failed to decode OpenChannel: {}", e),
                        }
                    })?;

                // Reject channel 0
                if open.channel_id == 0 {
                    tracing::warn!("rejecting OpenChannel for reserved channel 0");
                    self.send_cancel_channel(0, CancelReason::ProtocolViolation)
                        .await?;
                    return Ok(());
                }

                // Track the channel as open
                self.open_channels.lock().insert(open.channel_id);
                tracing::debug!(channel_id = open.channel_id, kind = ?open.kind, "channel opened");
                Ok(())
            }

            control_method::HELLO
            | control_method::CLOSE_CHANNEL
            | control_method::GRANT_CREDITS
            | control_method::GO_AWAY => {
                // These are handled elsewhere or not yet implemented.
                // Log and ignore for now.
                tracing::debug!(method_id, "control frame not yet handled");
                Ok(())
            }

            _ => {
                // Spec: `[impl core.control.unknown-extension]` - unknown extension verbs
                // SHOULD be ignored (not cause connection closure).
                tracing::debug!(method_id, "unknown control verb, ignoring");
                Ok(())
            }
        }
    }

    /// Send a CancelChannel control message.
    ///
    /// Spec: `[impl core.cancel.behavior]` - used to abort channels.
    async fn send_cancel_channel(
        &self,
        channel_id: u32,
        reason: CancelReason,
    ) -> Result<(), RpcError> {
        let cancel = CancelChannel { channel_id, reason };

        let cancel_bytes = facet_postcard::to_vec(&cancel).map_err(|e| RpcError::Status {
            code: ErrorCode::Internal,
            message: format!("failed to encode CancelChannel: {}", e),
        })?;

        let mut desc = MsgDescHot::new();
        desc.msg_id = self.next_msg_id();
        desc.channel_id = 0;
        desc.method_id = control_method::CANCEL_CHANNEL;
        desc.flags = FrameFlags::CONTROL;

        let frame = Frame::with_inline_payload(desc, &cancel_bytes)
            .expect("CancelChannel payload should fit inline");

        self.transport
            .send_frame(frame)
            .await
            .map_err(RpcError::Transport)?;

        tracing::debug!(channel_id, ?reason, "sent CancelChannel");
        Ok(())
    }

    /// Send a chunk on a tunnel channel.
    ///
    /// This sends a DATA frame on the channel. The chunk is not marked with EOS;
    /// use `close_tunnel()` to send the final chunk.
    ///
    /// For best performance, get a buffer from `session.buffer_pool().get()`,
    /// write directly into it, and pass it here. This avoids copies and the
    /// buffer will be returned to the pool after the frame is sent.
    pub async fn send_chunk(&self, channel_id: u32, payload: PooledBuf) -> Result<(), RpcError> {
        let mut desc = MsgDescHot::new();
        desc.msg_id = self.next_msg_id();
        desc.channel_id = channel_id;
        desc.method_id = 0; // Tunnels don't use method_id
        desc.flags = FrameFlags::DATA;

        let payload_len = payload.len();
        tracing::debug!(channel_id, payload_len, "send_chunk");
        let frame = if payload_len <= INLINE_PAYLOAD_SIZE {
            Frame::with_inline_payload(desc, payload.as_ref()).expect("inline payload should fit")
        } else {
            Frame::with_pooled_payload(desc, payload)
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
    // Handshake
    // ========================================================================

    /// Send a Hello frame.
    ///
    /// Spec: `[impl handshake.required]` - Hello MUST be sent before any other frames.
    async fn send_hello(&self) -> Result<(), RpcError> {
        let hello = Hello {
            protocol_version: rapace_protocol::PROTOCOL_VERSION_1_0,
            role: self.role,
            required_features: rapace_protocol::features::ATTACHED_STREAMS
                | rapace_protocol::features::CALL_ENVELOPE,
            supported_features: rapace_protocol::features::ATTACHED_STREAMS
                | rapace_protocol::features::CALL_ENVELOPE
                | rapace_protocol::features::CREDIT_FLOW_CONTROL
                | rapace_protocol::features::RAPACE_PING,
            limits: Limits::default(),
            // Empty methods list is valid per spec - method registry is optional
            // and will be populated when service registration is implemented.
            methods: Vec::new(),
            params: Vec::new(),
        };

        let payload = facet_postcard::to_vec(&hello).map_err(|e| RpcError::Status {
            code: ErrorCode::Internal,
            message: format!("failed to encode Hello: {}", e),
        })?;

        let mut desc = MsgDescHot::new();
        desc.msg_id = self.next_msg_id();
        desc.channel_id = 0;
        desc.method_id = control_method::HELLO;
        desc.flags = FrameFlags::CONTROL;

        let frame = if payload.len() <= INLINE_PAYLOAD_SIZE {
            Frame::with_inline_payload(desc, &payload).expect("inline payload should fit")
        } else {
            Frame::with_payload(desc, payload)
        };

        tracing::debug!(role = ?self.role, "sending Hello");
        self.transport
            .send_frame(frame)
            .await
            .map_err(RpcError::Transport)
    }

    /// Receive and validate a Hello frame from the peer.
    ///
    /// Spec: `[impl handshake.ordering]` - Hello must be the first frame received.
    /// Spec: `[impl handshake.role.validation]` - roles must be complementary.
    async fn recv_hello(&self) -> Result<Hello, RpcError> {
        let frame = self
            .transport
            .recv_frame()
            .await
            .map_err(RpcError::Transport)?;

        // Validate it's a Hello on channel 0
        if frame.desc.channel_id != 0 {
            return Err(RpcError::Status {
                code: ErrorCode::InvalidArgument,
                message: format!(
                    "[verify handshake.ordering]: first frame must be on channel 0, got {}",
                    frame.desc.channel_id
                ),
            });
        }

        if !frame.desc.flags.contains(FrameFlags::CONTROL) {
            return Err(RpcError::Status {
                code: ErrorCode::InvalidArgument,
                message: "[verify core.control.flag-set]: Hello must have CONTROL flag set"
                    .to_string(),
            });
        }

        if frame.desc.method_id != control_method::HELLO {
            return Err(RpcError::Status {
                code: ErrorCode::InvalidArgument,
                message: format!(
                    "[verify handshake.ordering]: first frame must be Hello (method_id=0), got {}",
                    frame.desc.method_id
                ),
            });
        }

        // Decode Hello payload
        let hello: Hello =
            facet_postcard::from_slice(frame.payload_bytes()).map_err(|e| RpcError::Status {
                code: ErrorCode::InvalidArgument,
                message: format!(
                    "[verify core.control.payload-encoding]: failed to decode Hello: {}",
                    e
                ),
            })?;

        // Validate role is complementary
        let expected_peer_role = match self.role {
            Role::Initiator => Role::Acceptor,
            Role::Acceptor => Role::Initiator,
        };
        if hello.role != expected_peer_role {
            return Err(RpcError::Status {
                code: ErrorCode::InvalidArgument,
                message: format!(
                    "[verify handshake.role.validation]: expected peer role {:?}, got {:?}",
                    expected_peer_role, hello.role
                ),
            });
        }

        // Validate protocol version (major must be 1)
        let major = hello.protocol_version >> 16;
        if major != 1 {
            return Err(RpcError::Status {
                code: ErrorCode::InvalidArgument,
                message: format!(
                    "[verify handshake.version.major]: expected major version 1, got {}",
                    major
                ),
            });
        }

        // Check that peer supports our required features
        let our_required =
            rapace_protocol::features::ATTACHED_STREAMS | rapace_protocol::features::CALL_ENVELOPE;
        if hello.supported_features & our_required != our_required {
            return Err(RpcError::Status {
                code: ErrorCode::InvalidArgument,
                message: format!(
                    "[verify handshake.features.required]: peer doesn't support required features (need {:x}, got {:x})",
                    our_required, hello.supported_features
                ),
            });
        }

        tracing::debug!(
            peer_role = ?hello.role,
            protocol_version = hello.protocol_version,
            "received Hello"
        );

        Ok(hello)
    }

    /// Perform the handshake sequence.
    ///
    /// - Initiator: send Hello first, then receive peer's Hello
    /// - Acceptor: receive peer's Hello first, then send our Hello
    ///
    /// Spec: `[impl handshake.required]` - explicit handshake is mandatory.
    /// Spec: `[impl handshake.ordering]` - initiator sends first.
    async fn perform_handshake(&self) -> Result<Hello, RpcError> {
        match self.role {
            Role::Initiator => {
                // Initiator sends first
                self.send_hello().await?;
                self.recv_hello().await
            }
            Role::Acceptor => {
                // Acceptor waits first
                let peer_hello = self.recv_hello().await?;
                self.send_hello().await?;
                Ok(peer_hello)
            }
        }
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

        #[cfg(not(target_arch = "wasm32"))]
        let received =
            match tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), rx).await {
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

        #[cfg(target_arch = "wasm32")]
        let received = {
            use futures_util::future::{Either, select};
            use gloo_timers::future::TimeoutFuture;
            let timeout = TimeoutFuture::new(timeout_ms as u32);
            match select(rx, timeout).await {
                Either::Left((Ok(frame), _)) => frame,
                Either::Left((Err(_), _)) => {
                    return Err(RpcError::Status {
                        code: ErrorCode::Internal,
                        message: "response channel closed".into(),
                    });
                }
                Either::Right(((), _)) => {
                    Self::log_timeout(channel_id, method_id, timeout_ms, TimeoutLogLevel::Error);
                    return Err(RpcError::DeadlineExceeded);
                }
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

        #[cfg(not(target_arch = "wasm32"))]
        let received =
            match tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), rx).await {
                Ok(Ok(frame)) => frame,
                Ok(Err(_)) => {
                    tracing::warn!(channel_id, method_id, "call: response channel closed");
                    return Err(RpcError::Transport(TransportError::Closed));
                }
                Err(_elapsed) => {
                    Self::log_timeout(channel_id, method_id, timeout_ms, TimeoutLogLevel::Warn);
                    return Err(RpcError::DeadlineExceeded);
                }
            };

        #[cfg(target_arch = "wasm32")]
        let received = {
            use futures_util::future::{Either, select};
            use gloo_timers::future::TimeoutFuture;
            let timeout = TimeoutFuture::new(timeout_ms as u32);
            match select(rx, timeout).await {
                Either::Left((Ok(frame), _)) => frame,
                Either::Left((Err(_), _)) => {
                    tracing::warn!(channel_id, method_id, "call: response channel closed");
                    return Err(RpcError::Transport(TransportError::Closed));
                }
                Either::Right(((), _)) => {
                    Self::log_timeout(channel_id, method_id, timeout_ms, TimeoutLogLevel::Warn);
                    return Err(RpcError::DeadlineExceeded);
                }
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
    /// 1. Performs the Hello handshake
    /// 2. Receives frames from the transport
    /// 3. Routes tunnel frames to registered tunnel receivers
    /// 4. Routes responses to waiting clients
    /// 5. Dispatches requests to the registered handler
    ///
    /// This method consumes self and runs until the transport closes.
    ///
    /// Spec: `[impl handshake.required]` - Hello handshake is mandatory.
    pub async fn run(self: Arc<Self>) -> Result<(), TransportError> {
        // Perform handshake first
        tracing::debug!(role = ?self.role, "RpcSession::run: performing handshake");
        match self.perform_handshake().await {
            Ok(peer_hello) => {
                tracing::debug!(
                    peer_role = ?peer_hello.role,
                    peer_version = peer_hello.protocol_version,
                    "RpcSession::run: handshake complete"
                );
            }
            Err(e) => {
                tracing::error!(?e, "RpcSession::run: handshake failed");
                self.transport.close();
                // Convert RpcError to TransportError, preserving error details
                let transport_err = match e {
                    RpcError::Transport(te) => te,
                    RpcError::Status { code, message } => {
                        TransportError::HandshakeFailed { code, message }
                    }
                    other => TransportError::HandshakeFailed {
                        code: ErrorCode::Internal,
                        message: other.to_string(),
                    },
                };
                return Err(transport_err);
            }
        }

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
            // Spec: `[impl core.call.response.flags]` - responses have RESPONSE flag set.
            // Spec: `[impl core.call.response.method-id]` - responses echo the request method_id.
            let received = if flags.contains(FrameFlags::RESPONSE) {
                match self.try_route_to_pending(channel_id, received) {
                    None => continue, // Frame was delivered to waiter
                    Some(unroutable) => {
                        // Response frame without a registered waiter - nowhere to send it.
                        tracing::warn!(
                            channel_id,
                            method_id,
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

            // Handle control frames (channel 0)
            //
            // Spec: `[impl core.control.reserved]` - channel 0 is reserved for control messages.
            // Spec: `[impl core.control.unknown-extension]` - unknown extension verbs SHOULD be ignored.
            if channel_id == 0 {
                match self.handle_control_frame(&received).await {
                    Ok(()) => continue,
                    Err(e) => {
                        tracing::warn!(method_id, error = ?e, "control frame handling failed");
                        continue;
                    }
                }
            }

            // Skip non-data frames
            if !received.flags().contains(FrameFlags::DATA) {
                continue;
            }

            // Spec: `[impl core.channel.open]` - channels MUST be opened via OpenChannel.
            // Reject data frames on channels that were never opened.
            if !self.open_channels.lock().contains(&channel_id) {
                tracing::warn!(
                    channel_id,
                    method_id,
                    "rejecting data frame on unopened channel"
                );
                // Send CancelChannel to reject
                if let Err(e) = self
                    .send_cancel_channel(channel_id, CancelReason::ProtocolViolation)
                    .await
                {
                    tracing::error!(error = ?e, "failed to send CancelChannel");
                }
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
