// src/callback.rs

//! Callback and nested call support for bidirectional RPC.
//!
//! This module provides infrastructure for making callbacks from RPC handlers back to
//! the peer. This is essential for plugin systems where:
//! 1. Host calls Plugin::process()
//! 2. Plugin calls back to Host::get_config()
//! 3. Host returns config
//! 4. Plugin continues processing
//!
//! # Features
//!
//! - **CallContext**: Provides handlers with ability to make callbacks
//! - **Nested call tracking**: Track call depth and prevent infinite recursion
//! - **Deadline propagation**: Deadlines are inherited and reduced for nested calls
//! - **Call chain tracing**: Debug nested call sequences
//!
//! # Example
//!
//! ```ignore
//! use rapace::callback::CallContext;
//!
//! async fn handler(ctx: &CallContext<'_>, request: Vec<u8>) -> Result<Vec<u8>, RpcError> {
//!     // Make a callback to the peer during handling
//!     let config: ConfigResponse = ctx.callback::<GetConfigMethod>(ConfigRequest {}).await?;
//!
//!     // Use the config to process the request
//!     process_with_config(&request, &config)
//! }
//! ```

use crate::error::{ErrorCode, RapaceError};
use crate::types::{ChannelId, MethodId};
use std::time::{Duration, Instant};

/// Maximum allowed call depth to prevent infinite recursion.
///
/// This limit prevents stack overflow from recursive callbacks. For example:
/// - Host calls Plugin::process (depth 1)
/// - Plugin calls Host::get_config (depth 2)
/// - Host calls Plugin::validate_config (depth 3)
/// - etc.
///
/// 16 levels should be sufficient for legitimate use cases while preventing abuse.
pub const MAX_CALL_DEPTH: u32 = 16;

/// Errors specific to callback operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallbackError {
    /// Call depth exceeded MAX_CALL_DEPTH
    MaxCallDepthExceeded,
    /// Deadline expired before callback completed
    DeadlineExpired,
    /// No deadline set when one is required
    DeadlineRequired,
    /// Session is closed
    SessionClosed,
    /// Channel operation failed
    ChannelError(String),
}

impl std::fmt::Display for CallbackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CallbackError::MaxCallDepthExceeded => {
                write!(f, "maximum call depth of {} exceeded", MAX_CALL_DEPTH)
            }
            CallbackError::DeadlineExpired => {
                write!(f, "deadline expired before callback completed")
            }
            CallbackError::DeadlineRequired => {
                write!(f, "deadline required for callback")
            }
            CallbackError::SessionClosed => {
                write!(f, "session is closed")
            }
            CallbackError::ChannelError(msg) => {
                write!(f, "channel error: {}", msg)
            }
        }
    }
}

impl std::error::Error for CallbackError {}

impl From<CallbackError> for RapaceError {
    fn from(err: CallbackError) -> Self {
        match err {
            CallbackError::MaxCallDepthExceeded => {
                RapaceError::new(ErrorCode::ResourceExhausted, err.to_string())
            }
            CallbackError::DeadlineExpired => {
                RapaceError::new(ErrorCode::DeadlineExceeded, err.to_string())
            }
            CallbackError::DeadlineRequired => {
                RapaceError::new(ErrorCode::InvalidArgument, err.to_string())
            }
            CallbackError::SessionClosed => {
                RapaceError::session_closed(err.to_string())
            }
            CallbackError::ChannelError(msg) => {
                RapaceError::internal(msg)
            }
        }
    }
}

/// Direction of a call in the call chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallDirection {
    /// Outbound call: we're calling the peer
    Outbound,
    /// Inbound call: peer is calling us
    Inbound,
    /// Callback: nested call made during handler execution
    Callback,
}

impl std::fmt::Display for CallDirection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CallDirection::Outbound => write!(f, "outbound"),
            CallDirection::Inbound => write!(f, "inbound"),
            CallDirection::Callback => write!(f, "callback"),
        }
    }
}

/// Information about a single call in the call chain.
#[derive(Debug, Clone)]
pub struct CallInfo {
    /// Service name (e.g., "com.example.Echo")
    pub service: String,
    /// Method name (e.g., "Echo")
    pub method: String,
    /// Method ID for dispatch
    pub method_id: MethodId,
    /// When this call started
    pub started_at: Instant,
    /// Direction of the call
    pub direction: CallDirection,
    /// Channel ID used for this call
    pub channel_id: ChannelId,
}

// Implement PartialEq manually, ignoring started_at
impl PartialEq for CallInfo {
    fn eq(&self, other: &Self) -> bool {
        self.service == other.service
            && self.method == other.method
            && self.method_id == other.method_id
            && self.direction == other.direction
            && self.channel_id == other.channel_id
    }
}

impl CallInfo {
    /// Create a new CallInfo.
    pub fn new(
        service: impl Into<String>,
        method: impl Into<String>,
        method_id: MethodId,
        direction: CallDirection,
        channel_id: ChannelId,
    ) -> Self {
        CallInfo {
            service: service.into(),
            method: method.into(),
            method_id,
            started_at: Instant::now(),
            direction,
            channel_id,
        }
    }

    /// Get the elapsed time since this call started.
    pub fn elapsed(&self) -> Duration {
        self.started_at.elapsed()
    }
}

/// Tracks a chain of nested calls for debugging and limit enforcement.
///
/// Each RPC call creates or extends a call chain. The chain records:
/// - Trace ID for correlating related calls
/// - Sequence of calls with timing and direction
/// - Current call depth
///
/// This is useful for:
/// - Debugging complex call patterns
/// - Enforcing depth limits
/// - Performance profiling
/// - Distributed tracing
#[derive(Debug, Clone, PartialEq)]
pub struct CallChain {
    /// Unique trace ID for this call chain
    trace_id: u64,
    /// Stack of calls in this chain
    calls: Vec<CallInfo>,
}

impl CallChain {
    /// Create a new call chain with a generated trace ID.
    pub fn new() -> Self {
        CallChain {
            trace_id: generate_trace_id(),
            calls: Vec::new(),
        }
    }

    /// Create a new call chain with a specific trace ID.
    ///
    /// Use this when continuing a call chain from a remote peer.
    pub fn with_trace_id(trace_id: u64) -> Self {
        CallChain {
            trace_id,
            calls: Vec::new(),
        }
    }

    /// Get the trace ID for this call chain.
    pub fn trace_id(&self) -> u64 {
        self.trace_id
    }

    /// Get the current call depth.
    pub fn depth(&self) -> u32 {
        self.calls.len() as u32
    }

    /// Add a call to the chain.
    ///
    /// Returns an error if this would exceed MAX_CALL_DEPTH.
    pub fn push(&mut self, call: CallInfo) -> Result<(), CallbackError> {
        if self.depth() >= MAX_CALL_DEPTH {
            return Err(CallbackError::MaxCallDepthExceeded);
        }
        self.calls.push(call);
        Ok(())
    }

    /// Remove the most recent call from the chain.
    pub fn pop(&mut self) -> Option<CallInfo> {
        self.calls.pop()
    }

    /// Get all calls in the chain.
    pub fn calls(&self) -> &[CallInfo] {
        &self.calls
    }

    /// Get the most recent call, if any.
    pub fn current_call(&self) -> Option<&CallInfo> {
        self.calls.last()
    }

    /// Create a formatted trace string for logging.
    ///
    /// Example output:
    /// ```text
    /// trace_id=0x123456789abcdef0 depth=3
    ///   [0] inbound  com.example.Plugin::process (ch=1, 42ms)
    ///   [1] callback com.example.Host::get_config (ch=2, 15ms)
    ///   [2] callback com.example.Plugin::validate (ch=3, 3ms)
    /// ```
    pub fn format_trace(&self) -> String {
        let mut s = format!(
            "trace_id=0x{:016x} depth={}\n",
            self.trace_id,
            self.depth()
        );

        for (i, call) in self.calls.iter().enumerate() {
            s.push_str(&format!(
                "  [{}] {:8} {}::{} (ch={}, {:?})\n",
                i,
                call.direction,
                call.service,
                call.method,
                call.channel_id.get(),
                call.elapsed()
            ));
        }

        s
    }
}

impl Default for CallChain {
    fn default() -> Self {
        Self::new()
    }
}

/// Generate a unique trace ID using a simple counter.
///
/// In a real implementation, you might want to use:
/// - A cryptographically secure random number
/// - A combination of timestamp + process ID + counter
/// - A UUID
fn generate_trace_id() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    COUNTER.fetch_add(1, Ordering::Relaxed)
}

/// Context passed to RPC handlers, enabling callbacks to the peer.
///
/// The CallContext provides:
/// - Access to the session for making callbacks
/// - Current channel ID
/// - Call depth tracking
/// - Deadline propagation
/// - Call chain for debugging
///
/// # Lifetimes
///
/// The context borrows the session for its lifetime, ensuring the session
/// remains valid during handler execution.
///
/// # Example
///
/// ```ignore
/// use rapace::callback::CallContext;
///
/// async fn my_handler(ctx: &CallContext<'_>, request: Vec<u8>) -> Result<Vec<u8>, RpcError> {
///     // Check remaining time
///     if let Some(remaining) = ctx.remaining_deadline() {
///         if remaining < Duration::from_millis(100) {
///             return Err(RpcError::deadline_exceeded("not enough time"));
///         }
///     }
///
///     // Make a callback with a portion of our deadline
///     let response = ctx.callback_with_timeout::<SomeMethod>(
///         request,
///         Duration::from_millis(50)
///     ).await?;
///
///     Ok(response)
/// }
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct CallContext<'a> {
    /// Reference to the session for making callbacks.
    /// In a real implementation, this would be &'a Session or similar.
    /// For now, we use a marker to show the lifetime relationship.
    _session: std::marker::PhantomData<&'a ()>,

    /// Channel ID for this RPC call
    channel_id: ChannelId,

    /// Current call depth (number of nested calls)
    call_depth: u32,

    /// Deadline for this call, if any
    deadline: Option<Instant>,

    /// Call chain for debugging
    call_chain: CallChain,
}

impl<'a> CallContext<'a> {
    /// Create a new call context for a top-level RPC call.
    ///
    /// This is typically called by the session layer when dispatching
    /// an incoming RPC request.
    pub fn new(channel_id: ChannelId) -> Self {
        CallContext {
            _session: std::marker::PhantomData,
            channel_id,
            call_depth: 0,
            deadline: None,
            call_chain: CallChain::new(),
        }
    }

    /// Create a call context with a deadline.
    pub fn with_deadline(channel_id: ChannelId, deadline: Instant) -> Self {
        CallContext {
            _session: std::marker::PhantomData,
            channel_id,
            call_depth: 0,
            deadline: Some(deadline),
            call_chain: CallChain::new(),
        }
    }

    /// Create a call context with a timeout duration.
    pub fn with_timeout(channel_id: ChannelId, timeout: Duration) -> Self {
        Self::with_deadline(channel_id, Instant::now() + timeout)
    }

    /// Create a nested context for a callback.
    ///
    /// The nested context:
    /// - Inherits the deadline (or uses a shorter one if specified)
    /// - Increments the call depth
    /// - Continues the call chain
    pub fn nested(
        &self,
        channel_id: ChannelId,
        timeout: Option<Duration>,
    ) -> Result<Self, CallbackError> {
        if self.call_depth >= MAX_CALL_DEPTH {
            return Err(CallbackError::MaxCallDepthExceeded);
        }

        // Calculate deadline for nested call
        let deadline = match (self.deadline, timeout) {
            // No parent deadline, use timeout if provided
            (None, Some(t)) => Some(Instant::now() + t),
            (None, None) => None,
            // Parent deadline, no timeout: inherit parent deadline
            (Some(parent), None) => Some(parent),
            // Both: use the earlier deadline
            (Some(parent), Some(t)) => {
                let timeout_deadline = Instant::now() + t;
                Some(parent.min(timeout_deadline))
            }
        };

        Ok(CallContext {
            _session: std::marker::PhantomData,
            channel_id,
            call_depth: self.call_depth + 1,
            deadline,
            call_chain: self.call_chain.clone(),
        })
    }

    /// Get the channel ID for this call.
    pub fn channel_id(&self) -> ChannelId {
        self.channel_id
    }

    /// Get the current call depth.
    ///
    /// - Depth 0: top-level call
    /// - Depth 1: first callback
    /// - Depth 2: callback from callback
    /// - etc.
    pub fn call_depth(&self) -> u32 {
        self.call_depth
    }

    /// Get the deadline for this call, if any.
    pub fn deadline(&self) -> Option<Instant> {
        self.deadline
    }

    /// Get the remaining time until the deadline.
    ///
    /// Returns None if:
    /// - No deadline is set
    /// - The deadline has already passed
    pub fn remaining_deadline(&self) -> Option<Duration> {
        self.deadline.and_then(|d| {
            let now = Instant::now();
            if now < d {
                Some(d - now)
            } else {
                None
            }
        })
    }

    /// Check if the deadline has expired.
    pub fn is_expired(&self) -> bool {
        self.deadline.map_or(false, |d| Instant::now() >= d)
    }

    /// Get a reference to the call chain.
    pub fn call_chain(&self) -> &CallChain {
        &self.call_chain
    }

    /// Get a mutable reference to the call chain.
    pub fn call_chain_mut(&mut self) -> &mut CallChain {
        &mut self.call_chain
    }

    /// Make a callback to the peer.
    ///
    /// This opens a new channel and sends a request, awaiting the response.
    /// The callback inherits the current deadline and increases the call depth.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Maximum call depth would be exceeded
    /// - Deadline has expired
    /// - Session is closed
    /// - Channel operation fails
    ///
    /// # Type Parameters
    ///
    /// `M` is a method trait that defines the request/response types.
    /// This is a placeholder - real implementation would integrate with
    /// the dispatch system.
    pub async fn callback<M: Method>(
        &self,
        _request: M::Request,
    ) -> Result<M::Response, CallbackError> {
        // Check call depth
        if self.call_depth >= MAX_CALL_DEPTH {
            return Err(CallbackError::MaxCallDepthExceeded);
        }

        // Check deadline
        if self.is_expired() {
            return Err(CallbackError::DeadlineExpired);
        }

        // Real implementation would:
        // 1. Open a new channel for the callback
        // 2. Serialize the request
        // 3. Send the request with appropriate timeout
        // 4. Await the response
        // 5. Deserialize the response
        // 6. Update call chain

        // Placeholder for type checking
        todo!("callback implementation requires session integration")
    }

    /// Make a callback with a specific timeout.
    ///
    /// The timeout is combined with the current deadline - whichever is
    /// earlier will be used.
    pub async fn callback_with_timeout<M: Method>(
        &self,
        _request: M::Request,
        timeout: Duration,
    ) -> Result<M::Response, CallbackError> {
        // Check call depth
        if self.call_depth >= MAX_CALL_DEPTH {
            return Err(CallbackError::MaxCallDepthExceeded);
        }

        // Check deadline
        if self.is_expired() {
            return Err(CallbackError::DeadlineExpired);
        }

        // Calculate effective deadline
        let timeout_deadline = Instant::now() + timeout;
        let effective_deadline = match self.deadline {
            Some(parent) => parent.min(timeout_deadline),
            None => timeout_deadline,
        };

        // Check if we have any time left
        if Instant::now() >= effective_deadline {
            return Err(CallbackError::DeadlineExpired);
        }

        // Real implementation similar to callback()
        todo!("callback_with_timeout implementation requires session integration")
    }
}

/// Trait for RPC methods.
///
/// This trait defines the contract for RPC methods that can be used with callbacks.
/// It's compatible with the Method trait from service_macro, but defined here to
/// avoid circular dependencies.
///
/// When service_macro is enabled, methods generated by define_service! will
/// implement this trait automatically.
pub trait Method {
    /// Request type
    type Request;
    /// Response type
    type Response;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn call_direction_display() {
        assert_eq!(format!("{}", CallDirection::Outbound), "outbound");
        assert_eq!(format!("{}", CallDirection::Inbound), "inbound");
        assert_eq!(format!("{}", CallDirection::Callback), "callback");
    }

    #[test]
    fn call_info_creation() {
        let channel_id = ChannelId::new(1).unwrap();
        let method_id = MethodId::new(42);
        let call = CallInfo::new(
            "com.example.Service",
            "Method",
            method_id,
            CallDirection::Inbound,
            channel_id,
        );

        assert_eq!(call.service, "com.example.Service");
        assert_eq!(call.method, "Method");
        assert_eq!(call.method_id.get(), 42);
        assert_eq!(call.direction, CallDirection::Inbound);
        assert_eq!(call.channel_id.get(), 1);
        assert!(call.elapsed().as_secs() < 1);
    }

    #[test]
    fn call_chain_depth() {
        let mut chain = CallChain::new();
        assert_eq!(chain.depth(), 0);

        let channel_id = ChannelId::new(1).unwrap();
        let method_id = MethodId::new(1);

        chain
            .push(CallInfo::new(
                "Service1",
                "Method1",
                method_id,
                CallDirection::Inbound,
                channel_id,
            ))
            .unwrap();
        assert_eq!(chain.depth(), 1);

        chain
            .push(CallInfo::new(
                "Service2",
                "Method2",
                method_id,
                CallDirection::Callback,
                channel_id,
            ))
            .unwrap();
        assert_eq!(chain.depth(), 2);

        chain.pop();
        assert_eq!(chain.depth(), 1);
    }

    #[test]
    fn call_chain_max_depth() {
        let mut chain = CallChain::new();
        let channel_id = ChannelId::new(1).unwrap();
        let method_id = MethodId::new(1);

        // Fill up to MAX_CALL_DEPTH
        for i in 0..MAX_CALL_DEPTH {
            chain
                .push(CallInfo::new(
                    "Service",
                    format!("Method{}", i),
                    method_id,
                    CallDirection::Callback,
                    channel_id,
                ))
                .unwrap();
        }

        assert_eq!(chain.depth(), MAX_CALL_DEPTH);

        // Next push should fail
        let result = chain.push(CallInfo::new(
            "Service",
            "MethodTooMany",
            method_id,
            CallDirection::Callback,
            channel_id,
        ));

        assert_eq!(result, Err(CallbackError::MaxCallDepthExceeded));
    }

    #[test]
    fn call_chain_trace_id() {
        let chain1 = CallChain::new();
        let chain2 = CallChain::new();

        // Each chain should get a unique trace ID
        assert_ne!(chain1.trace_id(), chain2.trace_id());

        // Can create chain with specific trace ID
        let chain3 = CallChain::with_trace_id(0x123456789abcdef0);
        assert_eq!(chain3.trace_id(), 0x123456789abcdef0);
    }

    #[test]
    fn call_chain_current_call() {
        let mut chain = CallChain::new();
        assert!(chain.current_call().is_none());

        let channel_id = ChannelId::new(1).unwrap();
        let method_id = MethodId::new(1);

        chain
            .push(CallInfo::new(
                "Service1",
                "Method1",
                method_id,
                CallDirection::Inbound,
                channel_id,
            ))
            .unwrap();

        let current = chain.current_call().unwrap();
        assert_eq!(current.service, "Service1");
        assert_eq!(current.method, "Method1");
    }

    #[test]
    fn call_chain_format_trace() {
        let mut chain = CallChain::with_trace_id(0x123456789abcdef0);
        let channel_id = ChannelId::new(1).unwrap();
        let method_id = MethodId::new(42);

        chain
            .push(CallInfo::new(
                "com.example.Plugin",
                "process",
                method_id,
                CallDirection::Inbound,
                channel_id,
            ))
            .unwrap();

        let trace = chain.format_trace();
        assert!(trace.contains("trace_id=0x123456789abcdef0"));
        assert!(trace.contains("depth=1"));
        assert!(trace.contains("com.example.Plugin::process"));
        assert!(trace.contains("inbound"));
    }

    #[test]
    fn call_context_creation() {
        let channel_id = ChannelId::new(1).unwrap();
        let ctx = CallContext::new(channel_id);

        assert_eq!(ctx.channel_id().get(), 1);
        assert_eq!(ctx.call_depth(), 0);
        assert!(ctx.deadline().is_none());
        assert!(!ctx.is_expired());
    }

    #[test]
    fn call_context_with_deadline() {
        let channel_id = ChannelId::new(1).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let ctx = CallContext::with_deadline(channel_id, deadline);

        assert!(ctx.deadline().is_some());
        assert!(!ctx.is_expired());
        assert!(ctx.remaining_deadline().is_some());
    }

    #[test]
    fn call_context_with_timeout() {
        let channel_id = ChannelId::new(1).unwrap();
        let ctx = CallContext::with_timeout(channel_id, Duration::from_secs(5));

        let remaining = ctx.remaining_deadline().unwrap();
        assert!(remaining > Duration::from_secs(4));
        assert!(remaining <= Duration::from_secs(5));
    }

    #[test]
    fn call_context_expired_deadline() {
        let channel_id = ChannelId::new(1).unwrap();
        let deadline = Instant::now() - Duration::from_secs(1); // Past
        let ctx = CallContext::with_deadline(channel_id, deadline);

        assert!(ctx.is_expired());
        assert!(ctx.remaining_deadline().is_none());
    }

    #[test]
    fn call_context_nested() {
        let channel_id = ChannelId::new(1).unwrap();
        let ctx = CallContext::with_timeout(channel_id, Duration::from_secs(10));

        let nested_channel = ChannelId::new(2).unwrap();
        let nested = ctx.nested(nested_channel, None).unwrap();

        assert_eq!(nested.call_depth(), 1);
        assert_eq!(nested.channel_id().get(), 2);
        // Should inherit parent deadline
        assert!(nested.deadline().is_some());
    }

    #[test]
    fn call_context_nested_with_timeout() {
        let channel_id = ChannelId::new(1).unwrap();
        let ctx = CallContext::with_timeout(channel_id, Duration::from_secs(10));

        let nested_channel = ChannelId::new(2).unwrap();
        // Request 20 seconds but parent only has 10
        let nested = ctx
            .nested(nested_channel, Some(Duration::from_secs(20)))
            .unwrap();

        // Should use parent's deadline (earlier)
        let parent_remaining = ctx.remaining_deadline().unwrap();
        let nested_remaining = nested.remaining_deadline().unwrap();

        // Nested should have similar or less time than parent
        assert!(nested_remaining <= parent_remaining + Duration::from_millis(10));
    }

    #[test]
    fn call_context_nested_shorter_timeout() {
        let channel_id = ChannelId::new(1).unwrap();
        let ctx = CallContext::with_timeout(channel_id, Duration::from_secs(10));

        let nested_channel = ChannelId::new(2).unwrap();
        // Request 1 second (shorter than parent)
        let nested = ctx
            .nested(nested_channel, Some(Duration::from_secs(1)))
            .unwrap();

        let nested_remaining = nested.remaining_deadline().unwrap();
        // Should use the shorter timeout
        assert!(nested_remaining <= Duration::from_secs(1));
    }

    #[test]
    fn call_context_max_depth() {
        let channel_id = ChannelId::new(1).unwrap();
        let mut ctx = CallContext::new(channel_id);

        // Manually set depth to max
        ctx.call_depth = MAX_CALL_DEPTH;

        let nested_channel = ChannelId::new(2).unwrap();
        let result = ctx.nested(nested_channel, None);

        assert_eq!(result, Err(CallbackError::MaxCallDepthExceeded));
    }

    #[test]
    fn call_context_call_chain_access() {
        let channel_id = ChannelId::new(1).unwrap();
        let mut ctx = CallContext::new(channel_id);

        let trace_id = ctx.call_chain().trace_id();
        assert!(trace_id > 0);

        // Can mutate call chain
        let method_id = MethodId::new(42);
        ctx.call_chain_mut()
            .push(CallInfo::new(
                "Service",
                "Method",
                method_id,
                CallDirection::Inbound,
                channel_id,
            ))
            .unwrap();

        assert_eq!(ctx.call_chain().depth(), 1);
    }

    #[test]
    fn callback_error_display() {
        assert_eq!(
            format!("{}", CallbackError::MaxCallDepthExceeded),
            "maximum call depth of 16 exceeded"
        );
        assert_eq!(
            format!("{}", CallbackError::DeadlineExpired),
            "deadline expired before callback completed"
        );
        assert_eq!(
            format!("{}", CallbackError::SessionClosed),
            "session is closed"
        );
    }

    #[test]
    fn callback_error_to_rapace_error() {
        let err = CallbackError::MaxCallDepthExceeded;
        let rapace_err: RapaceError = err.into();
        assert_eq!(rapace_err.code(), ErrorCode::ResourceExhausted);

        let err = CallbackError::DeadlineExpired;
        let rapace_err: RapaceError = err.into();
        assert_eq!(rapace_err.code(), ErrorCode::DeadlineExceeded);

        let err = CallbackError::SessionClosed;
        let rapace_err: RapaceError = err.into();
        assert_eq!(rapace_err.code(), ErrorCode::SessionClosed);
    }
}
