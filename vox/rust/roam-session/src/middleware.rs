//! Middleware for intercepting requests after deserialization but before the handler.
//!
//! Middleware can:
//! - Inspect deserialized args via [`SendPeek`] (reflection-based, no type knowledge needed)
//! - Reject requests (e.g., authentication failure)
//! - Add values to `Context::extensions` for handlers to retrieve
//! - Log, trace, or meter requests
//!
//! # Example
//!
//! ```ignore
//! use roam_session::{Middleware, Context, Rejection, SendPeek};
//! use std::pin::Pin;
//! use std::future::Future;
//!
//! struct AuthMiddleware { /* ... */ }
//!
//! impl Middleware for AuthMiddleware {
//!     fn intercept<'a>(
//!         &'a self,
//!         ctx: &'a mut Context,
//!         args: SendPeek<'a>,
//!     ) -> Pin<Box<dyn Future<Output = Result<(), Rejection>> + Send + 'a>> {
//!         Box::pin(async move {
//!             // Check for auth token in metadata
//!             let token = ctx.metadata.iter()
//!                 .find(|(k, _)| k == "auth-token")
//!                 .map(|(_, v)| v.as_string());
//!
//!             let Some(token) = token else {
//!                 return Err(Rejection::unauthenticated("missing auth-token"));
//!             };
//!
//!             // Can also inspect args via reflection using args.peek()
//!             // e.g., args.peek().get("user_id") to check authorization
//!
//!             // Store validated info in extensions for handler access
//!             ctx.extensions.insert(AuthenticatedUser { token: token.to_string() });
//!
//!             Ok(())
//!         })
//!     }
//! }
//! ```

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use facet::Peek;

use crate::Context;

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

/// Middleware that can intercept requests after deserialization.
///
/// Middleware sees:
/// - Request context (metadata, extensions, conn_id, method_id)
/// - Deserialized args via [`SendPeek`] (reflection-based inspection)
///
/// Middleware can:
/// - Reject the request by returning `Err(Rejection)`
/// - Continue by returning `Ok(())`
/// - Add values to `ctx.extensions` for handlers
///
/// Middleware is async to support operations like database lookups for
/// token validation.
pub trait Middleware: Send + Sync {
    /// Intercept a request after deserialization but before the handler runs.
    ///
    /// # Arguments
    ///
    /// - `ctx`: Request context with metadata, extensions, conn_id, method_id
    /// - `args`: SendPeek view of deserialized args (inspect via reflection)
    ///
    /// Return `Ok(())` to continue to the handler.
    /// Return `Err(rejection)` to reject the request.
    fn intercept<'a>(
        &'a self,
        ctx: &'a mut Context,
        args: SendPeek<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<(), Rejection>> + Send + 'a>>;
}

/// Middleware that does nothing (passes all requests through).
///
/// Useful as a default or for testing.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoopMiddleware;

impl Middleware for NoopMiddleware {
    fn intercept<'a>(
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
    fn intercept<'a>(
        &'a self,
        ctx: &'a mut Context,
        args: SendPeek<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<(), Rejection>> + Send + 'a>> {
        Box::pin(async move {
            for layer in &self.layers {
                layer.intercept(ctx, args).await?;
            }
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestMiddleware {
        should_reject: bool,
    }

    impl Middleware for TestMiddleware {
        fn intercept<'a>(
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
}
