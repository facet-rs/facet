//! Middleware for intercepting requests before and after the handler.
//!
//! Middleware has two phases:
//! - **pre**: Runs after deserialization, before the handler. Can reject requests.
//! - **post**: Runs after the handler completes. Observes the outcome (success or error).
//!
//! This pattern enables proper observability (e.g., OpenTelemetry):
//! - `pre()` starts a tracing span, stores it in `ctx.extensions`
//! - `post()` retrieves the span, records the outcome, and ends it
//!
//! # Example
//!
//! ```ignore
//! use roam_session::{Middleware, Context, Rejection, SendPeek, MethodOutcome};
//! use std::pin::Pin;
//! use std::future::Future;
//!
//! struct TracingMiddleware { /* ... */ }
//!
//! impl Middleware for TracingMiddleware {
//!     fn pre<'a>(
//!         &'a self,
//!         ctx: &'a mut Context,
//!         _args: SendPeek<'a>,
//!     ) -> Pin<Box<dyn Future<Output = Result<(), Rejection>> + Send + 'a>> {
//!         Box::pin(async move {
//!             // Start a span and store it in extensions
//!             let span = Span::start(ctx.method_id());
//!             ctx.extensions.insert(span);
//!             Ok(())
//!         })
//!     }
//!
//!     fn post<'a>(
//!         &'a self,
//!         ctx: &'a Context,
//!         outcome: MethodOutcome<'a>,
//!     ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
//!         Box::pin(async move {
//!             // Get the span we stored in pre()
//!             if let Some(span) = ctx.extensions.get::<Span>() {
//!                 span.set_status(outcome.is_ok());
//!                 span.end();
//!             }
//!         })
//!     }
//! }
//! ```

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use facet::Peek;

use crate::Context;

/// The outcome of a method call, used by post-middleware.
///
/// Represents the three possible outcomes:
/// - `Ok`: Handler returned successfully
/// - `Err`: Handler returned an error
/// - `Rejected`: Pre-middleware rejected the request (handler never ran)
#[derive(Clone, Copy)]
pub enum MethodOutcome<'mem> {
    /// Handler returned Ok(value)
    Ok(SendPeek<'mem>),
    /// Handler returned Err(error)
    Err(SendPeek<'mem>),
    /// Pre-middleware rejected the request (handler never ran)
    Rejected,
}

impl MethodOutcome<'_> {
    /// Returns true if this is an Ok outcome.
    pub fn is_ok(&self) -> bool {
        matches!(self, MethodOutcome::Ok(_))
    }

    /// Returns true if this is an Err outcome (not including Rejected).
    pub fn is_err(&self) -> bool {
        matches!(self, MethodOutcome::Err(_))
    }

    /// Returns true if the request was rejected by pre-middleware.
    pub fn is_rejected(&self) -> bool {
        matches!(self, MethodOutcome::Rejected)
    }

    /// Get the inner SendPeek if the handler ran (Ok or Err).
    /// Returns None if the request was rejected before the handler ran.
    pub fn peek(&self) -> Option<SendPeek<'_>> {
        match self {
            MethodOutcome::Ok(p) | MethodOutcome::Err(p) => Some(*p),
            MethodOutcome::Rejected => None,
        }
    }
}

/// A Send-safe wrapper around [`Peek`].
///
/// [`Peek`] contains raw pointers and doesn't implement `Send`. However, in the
/// dispatch flow, we need to pass it to middleware which returns a `Send` future
/// (because `dispatch()` is spawned).
///
/// # Safety
///
/// This is safe when:
/// 1. The underlying args type is `Send` (enforced by `#[service]` macro)
/// 2. The args data outlives this wrapper
/// 3. The Peek is only accessed from one thread at a time (guaranteed by async/await)
///
/// The `#[service]` macro enforces that all argument types are `Send`, so the
/// data that `SendPeek` points to is safe to access from any thread.
#[derive(Clone, Copy)]
pub struct SendPeek<'mem>(Peek<'mem, 'static>);

// SAFETY: The underlying data is Send (enforced by macro), and we control
// the access pattern - only one thread accesses the data at a time through
// normal async/await execution.
#[allow(unsafe_code)]
unsafe impl Send for SendPeek<'_> {}
#[allow(unsafe_code)]
unsafe impl Sync for SendPeek<'_> {}

impl<'mem> SendPeek<'mem> {
    /// Create a new SendPeek wrapper.
    ///
    /// # Safety
    ///
    /// Caller must ensure:
    /// - The underlying args type is `Send`
    /// - The args data outlives this wrapper
    /// - The data won't be mutated while this Peek exists
    #[allow(unsafe_code)]
    pub unsafe fn new(peek: Peek<'mem, 'static>) -> Self {
        Self(peek)
    }

    /// Get the inner Peek for inspection.
    pub fn peek(&self) -> Peek<'mem, 'static> {
        self.0
    }
}

/// Reason for rejecting a request.
///
/// When middleware rejects a request, this is sent back as the response.
#[derive(Debug, Clone)]
pub struct Rejection {
    /// Error code for programmatic handling.
    pub code: RejectionCode,
    /// Human-readable message.
    pub message: String,
}

/// Standard rejection codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum RejectionCode {
    /// Request lacks required authentication.
    Unauthenticated,
    /// Caller is authenticated but not authorized for this operation.
    PermissionDenied,
    /// Rate limit exceeded.
    RateLimited,
    /// Request is invalid (bad metadata, etc.).
    InvalidRequest,
    /// Internal middleware error.
    Internal,
}

impl Rejection {
    /// Create an "unauthenticated" rejection.
    pub fn unauthenticated(message: impl Into<String>) -> Self {
        Self {
            code: RejectionCode::Unauthenticated,
            message: message.into(),
        }
    }

    /// Create a "permission denied" rejection.
    pub fn permission_denied(message: impl Into<String>) -> Self {
        Self {
            code: RejectionCode::PermissionDenied,
            message: message.into(),
        }
    }

    /// Create a "rate limited" rejection.
    pub fn rate_limited(message: impl Into<String>) -> Self {
        Self {
            code: RejectionCode::RateLimited,
            message: message.into(),
        }
    }

    /// Create an "invalid request" rejection.
    pub fn invalid_request(message: impl Into<String>) -> Self {
        Self {
            code: RejectionCode::InvalidRequest,
            message: message.into(),
        }
    }

    /// Create an "internal" rejection.
    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            code: RejectionCode::Internal,
            message: message.into(),
        }
    }
}

/// Middleware that can intercept requests before and after the handler.
///
/// ## Pre-middleware
///
/// `pre()` runs after deserialization but before the handler. It can:
/// - Reject the request by returning `Err(Rejection)`
/// - Add values to `ctx.extensions` for the handler and post-middleware
/// - Inspect args via reflection
///
/// ## Post-middleware
///
/// `post()` runs after the handler completes (or is skipped due to rejection).
/// It can:
/// - Observe the method outcome (success or error)
/// - Retrieve values from extensions (e.g., to end a tracing span)
/// - Record metrics, logs, etc.
///
/// ## Execution order
///
/// For middleware stack [A, B, C]:
/// - Pre runs first-to-last: A.pre() → B.pre() → C.pre() → handler
/// - Post runs last-to-first: C.post() → B.post() → A.post()
///
/// This mirrors standard "wrap" semantics: the first middleware added is the
/// outermost wrapper.
pub trait Middleware: Send + Sync {
    /// Run before the handler.
    ///
    /// # Arguments
    ///
    /// - `ctx`: Request context with metadata, extensions, conn_id, method_id
    /// - `args`: SendPeek view of deserialized args (inspect via reflection)
    ///
    /// Return `Ok(())` to continue to the handler.
    /// Return `Err(rejection)` to reject the request. Note: `post()` will still
    /// be called even if `pre()` rejects, so you can clean up resources.
    fn pre<'a>(
        &'a self,
        ctx: &'a mut Context,
        args: SendPeek<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<(), Rejection>> + Send + 'a>>;

    /// Run after the handler (or after rejection).
    ///
    /// # Arguments
    ///
    /// - `ctx`: Request context (note: immutable, can only read extensions)
    /// - `outcome`: The method outcome - either Ok(result) or Err(error)
    ///
    /// The default implementation does nothing.
    fn post<'a>(
        &'a self,
        _ctx: &'a Context,
        _outcome: MethodOutcome<'a>,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async {})
    }
}

/// Middleware that does nothing (passes all requests through).
///
/// Useful as a default or for testing.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoopMiddleware;

impl Middleware for NoopMiddleware {
    fn pre<'a>(
        &'a self,
        _ctx: &'a mut Context,
        _args: SendPeek<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<(), Rejection>> + Send + 'a>> {
        Box::pin(async { Ok(()) })
    }
}

/// Compose multiple middleware into a single middleware.
///
/// Middleware runs in order: first middleware added runs first.
pub struct MiddlewareStack {
    layers: Vec<Arc<dyn Middleware>>,
}

impl MiddlewareStack {
    /// Create a new empty middleware stack.
    pub fn new() -> Self {
        Self { layers: Vec::new() }
    }

    /// Add middleware to the stack.
    ///
    /// Middleware runs in the order added.
    pub fn with<M: Middleware + 'static>(mut self, middleware: M) -> Self {
        self.layers.push(Arc::new(middleware));
        self
    }

    /// Add an already-Arc'd middleware to the stack.
    pub fn with_arc(mut self, middleware: Arc<dyn Middleware>) -> Self {
        self.layers.push(middleware);
        self
    }
}

impl Default for MiddlewareStack {
    fn default() -> Self {
        Self::new()
    }
}

impl Middleware for MiddlewareStack {
    fn pre<'a>(
        &'a self,
        ctx: &'a mut Context,
        args: SendPeek<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<(), Rejection>> + Send + 'a>> {
        Box::pin(async move {
            // Pre runs first-to-last
            for layer in &self.layers {
                layer.pre(ctx, args).await?;
            }
            Ok(())
        })
    }

    fn post<'a>(
        &'a self,
        ctx: &'a Context,
        outcome: MethodOutcome<'a>,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            // Post runs last-to-first (reverse order)
            for layer in self.layers.iter().rev() {
                layer.post(ctx, outcome).await;
            }
        })
    }
}

#[cfg(test)]
#[allow(unsafe_code)] // Tests need unsafe to create SendPeek
mod tests {
    use super::*;
    use facet::Facet;

    struct TestMiddleware {
        should_reject: bool,
    }

    impl Middleware for TestMiddleware {
        fn pre<'a>(
            &'a self,
            ctx: &'a mut Context,
            _args: SendPeek<'a>,
        ) -> Pin<Box<dyn Future<Output = Result<(), Rejection>> + Send + 'a>> {
            let should_reject = self.should_reject;
            Box::pin(async move {
                if should_reject {
                    Err(Rejection::unauthenticated("test rejection"))
                } else {
                    ctx.extensions.insert(42i32);
                    Ok(())
                }
            })
        }
    }

    #[test]
    fn test_middleware_stack() {
        // Just test that it compiles and types work
        let stack = MiddlewareStack::new()
            .with(NoopMiddleware)
            .with(TestMiddleware {
                should_reject: false,
            });

        assert_eq!(stack.layers.len(), 2);
    }

    // Test that middleware can actually inspect arguments via reflection
    #[test]
    fn test_middleware_inspects_args() {
        use std::sync::atomic::{AtomicBool, Ordering};

        // A simple argument type
        #[derive(Facet)]
        struct TestArgs {
            user_id: u64,
            message: String,
        }

        static INSPECTED: AtomicBool = AtomicBool::new(false);
        static FOUND_USER_ID: AtomicBool = AtomicBool::new(false);

        struct InspectingMiddleware;

        impl Middleware for InspectingMiddleware {
            fn pre<'a>(
                &'a self,
                _ctx: &'a mut Context,
                args: SendPeek<'a>,
            ) -> Pin<Box<dyn Future<Output = Result<(), Rejection>> + Send + 'a>> {
                Box::pin(async move {
                    INSPECTED.store(true, Ordering::SeqCst);

                    // Use Peek to inspect the args structure
                    let peek = args.peek();

                    // Try to access as a struct and get the user_id field
                    if let Ok(ps) = peek.into_struct()
                        && let Ok(field) = ps.field_by_name("user_id")
                        && let Ok(&uid) = field.get::<u64>()
                        && uid == 12345
                    {
                        FOUND_USER_ID.store(true, Ordering::SeqCst);
                    }

                    Ok(())
                })
            }
        }

        // Create test args
        let args = TestArgs {
            user_id: 12345,
            message: "hello".to_string(),
        };

        // Create a SendPeek from the args
        let peek = Peek::new(&args);
        // SAFETY: TestArgs is Send, we own it, it won't be mutated
        let send_peek = unsafe { SendPeek::new(peek) };

        // Create a context (method_id doesn't matter for this test)
        let mut ctx = Context::new(
            roam_wire::ConnectionId::new(1),
            roam_wire::RequestId::new(1),
            roam_wire::MethodId::new(0),
            roam_wire::Metadata::default(),
            vec![],
        );

        // Run the middleware synchronously (for test purposes)
        let mw = InspectingMiddleware;
        let future = mw.pre(&mut ctx, send_peek);

        // Use a simple blocking executor for the test
        futures_util::pin_mut!(future);
        let waker = futures_util::task::noop_waker();
        let mut cx = std::task::Context::from_waker(&waker);
        loop {
            match future.as_mut().poll(&mut cx) {
                std::task::Poll::Ready(result) => {
                    assert!(result.is_ok());
                    break;
                }
                std::task::Poll::Pending => continue,
            }
        }

        assert!(
            INSPECTED.load(Ordering::SeqCst),
            "middleware was not called"
        );
        assert!(
            FOUND_USER_ID.load(Ordering::SeqCst),
            "middleware did not find user_id=12345"
        );
    }

    // Test that post-middleware can observe the outcome
    #[test]
    fn test_post_middleware_observes_outcome() {
        use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

        static POST_CALLED: AtomicBool = AtomicBool::new(false);
        static OUTCOME_TYPE: AtomicU8 = AtomicU8::new(0); // 0=none, 1=ok, 2=err, 3=rejected

        struct ObservingMiddleware;

        impl Middleware for ObservingMiddleware {
            fn pre<'a>(
                &'a self,
                _ctx: &'a mut Context,
                _args: SendPeek<'a>,
            ) -> Pin<Box<dyn Future<Output = Result<(), Rejection>> + Send + 'a>> {
                Box::pin(async { Ok(()) })
            }

            fn post<'a>(
                &'a self,
                _ctx: &'a Context,
                outcome: MethodOutcome<'a>,
            ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
                Box::pin(async move {
                    POST_CALLED.store(true, Ordering::SeqCst);
                    match outcome {
                        MethodOutcome::Ok(_) => OUTCOME_TYPE.store(1, Ordering::SeqCst),
                        MethodOutcome::Err(_) => OUTCOME_TYPE.store(2, Ordering::SeqCst),
                        MethodOutcome::Rejected => OUTCOME_TYPE.store(3, Ordering::SeqCst),
                    }
                })
            }
        }

        let ctx = Context::new(
            roam_wire::ConnectionId::new(1),
            roam_wire::RequestId::new(1),
            roam_wire::MethodId::new(0),
            roam_wire::Metadata::default(),
            vec![],
        );

        let mw = ObservingMiddleware;

        // Test with Ok outcome
        let result_value = 42u64;
        let peek = Peek::new(&result_value);
        let send_peek = unsafe { SendPeek::new(peek) };
        let outcome = MethodOutcome::Ok(send_peek);

        let future = mw.post(&ctx, outcome);
        futures_util::pin_mut!(future);
        let waker = futures_util::task::noop_waker();
        let mut cx = std::task::Context::from_waker(&waker);
        loop {
            match future.as_mut().poll(&mut cx) {
                std::task::Poll::Ready(()) => break,
                std::task::Poll::Pending => continue,
            }
        }

        assert!(POST_CALLED.load(Ordering::SeqCst));
        assert_eq!(OUTCOME_TYPE.load(Ordering::SeqCst), 1); // Ok

        // Test with Rejected outcome
        POST_CALLED.store(false, Ordering::SeqCst);
        let future = mw.post(&ctx, MethodOutcome::Rejected);
        futures_util::pin_mut!(future);
        loop {
            match future.as_mut().poll(&mut cx) {
                std::task::Poll::Ready(()) => break,
                std::task::Poll::Pending => continue,
            }
        }

        assert!(POST_CALLED.load(Ordering::SeqCst));
        assert_eq!(OUTCOME_TYPE.load(Ordering::SeqCst), 3); // Rejected
    }
}
