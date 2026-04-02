use std::{
    any::{Any, TypeId},
    collections::HashMap,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
};

use facet_reflect::Peek;

use crate::{
    ConnectionId, MetadataEntry, MethodDescriptor, Payload, ReplySink, RequestContext, RequestId,
    RequestResponse,
};

/// Per-request type-indexed storage shared across middleware hooks and handlers.
#[derive(Clone, Debug, Default)]
pub struct Extensions {
    inner: Arc<Mutex<HashMap<TypeId, Box<dyn Any + Send + Sync>>>>,
}

impl Extensions {
    /// Create a new empty extensions bag.
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a typed value into the bag, returning the previous value of the same type.
    pub fn insert<T>(&self, value: T) -> Option<T>
    where
        T: Send + Sync + 'static,
    {
        let previous = self
            .inner
            .lock()
            .expect("extensions mutex poisoned")
            .insert(TypeId::of::<T>(), Box::new(value));
        previous
            .map(|boxed| {
                boxed
                    .downcast::<T>()
                    .expect("extensions type id and boxed value disagreed")
            })
            .map(|boxed| *boxed)
    }

    /// Returns `true` if a value of type `T` is present.
    pub fn contains<T>(&self) -> bool
    where
        T: Send + Sync + 'static,
    {
        self.inner
            .lock()
            .expect("extensions mutex poisoned")
            .contains_key(&TypeId::of::<T>())
    }

    /// Borrow a typed value from the bag for the duration of `f`.
    pub fn with<T, R>(&self, f: impl FnOnce(&T) -> R) -> Option<R>
    where
        T: Send + Sync + 'static,
    {
        let guard = self.inner.lock().expect("extensions mutex poisoned");
        let value = guard.get(&TypeId::of::<T>())?;
        let value = value
            .downcast_ref::<T>()
            .expect("extensions type id and boxed value disagreed");
        Some(f(value))
    }

    /// Mutably borrow a typed value from the bag for the duration of `f`.
    pub fn with_mut<T, R>(&self, f: impl FnOnce(&mut T) -> R) -> Option<R>
    where
        T: Send + Sync + 'static,
    {
        let mut guard = self.inner.lock().expect("extensions mutex poisoned");
        let value = guard.get_mut(&TypeId::of::<T>())?;
        let value = value
            .downcast_mut::<T>()
            .expect("extensions type id and boxed value disagreed");
        Some(f(value))
    }

    /// Clone a typed value from the bag.
    pub fn get_cloned<T>(&self) -> Option<T>
    where
        T: Clone + Send + Sync + 'static,
    {
        self.with(|value: &T| value.clone())
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub type BoxMiddlewareFuture<'a> = Pin<Box<dyn Future<Output = ()> + Send + 'a>>;
#[cfg(target_arch = "wasm32")]
pub type BoxMiddlewareFuture<'a> = Pin<Box<dyn Future<Output = ()> + 'a>>;

/// Outcome observed by server middleware after handler dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerCallOutcome {
    /// The handler sent a reply through the reply sink.
    Replied,
    /// The handler returned without replying; the runtime will synthesize cancellation.
    DroppedWithoutReply,
}

impl ServerCallOutcome {
    pub fn replied(self) -> bool {
        matches!(self, Self::Replied)
    }
}

/// Middleware-facing view of one decoded server request.
///
/// This is built by generated dispatchers after the inbound payload has been
/// deserialized into the method's typed argument tuple. The tuple is then
/// exposed reflectively through [`Peek`], allowing middleware to inspect the
/// decoded request without re-deserializing it.
///
/// Because this is a borrowed reflective view, middleware should extract any
/// owned data it needs before awaiting. The view itself is intended for
/// synchronous inspection within the hook body.
#[derive(Clone, Copy, Debug)]
pub struct ServerRequest<'a> {
    context: RequestContext<'a>,
    args: Peek<'a, 'static>,
}

impl<'a> ServerRequest<'a> {
    /// Create a new middleware request view from a request context and decoded args.
    pub const fn new(context: RequestContext<'a>, args: Peek<'a, 'static>) -> Self {
        Self { context, args }
    }

    /// Borrowed per-request context for this call.
    pub const fn context(&self) -> &RequestContext<'a> {
        &self.context
    }

    /// Static descriptor for the method being handled.
    pub fn method(&self) -> &'static crate::MethodDescriptor {
        self.context.method()
    }

    /// Request metadata borrowed from the inbound call.
    pub fn metadata(&self) -> &'a [crate::MetadataEntry<'a>] {
        self.context.metadata()
    }

    /// Wire-level request identifier for this call, when available.
    pub fn request_id(&self) -> Option<crate::RequestId> {
        self.context.request_id()
    }

    /// Virtual connection identifier for this call, when available.
    pub fn connection_id(&self) -> Option<crate::ConnectionId> {
        self.context.connection_id()
    }

    /// Per-request middleware extensions bag.
    pub fn extensions(&self) -> &'a Extensions {
        self.context.extensions()
    }

    /// Reflective view of the decoded argument tuple for this call.
    pub const fn args(&self) -> Peek<'a, 'static> {
        self.args
    }
}

/// Owned context available when observing an outbound server response.
#[derive(Clone, Debug)]
pub struct ServerResponseContext {
    method: &'static MethodDescriptor,
    request_id: Option<RequestId>,
    connection_id: Option<ConnectionId>,
    extensions: Extensions,
}

impl ServerResponseContext {
    /// Create a response context from transport identifiers and shared extensions.
    pub const fn new(
        method: &'static MethodDescriptor,
        request_id: Option<RequestId>,
        connection_id: Option<ConnectionId>,
        extensions: Extensions,
    ) -> Self {
        Self {
            method,
            request_id,
            connection_id,
            extensions,
        }
    }

    /// Static descriptor for the method being handled.
    pub const fn method(&self) -> &'static MethodDescriptor {
        self.method
    }

    /// Wire-level request identifier for this call, when available.
    pub const fn request_id(&self) -> Option<RequestId> {
        self.request_id
    }

    /// Virtual connection identifier for this call, when available.
    pub const fn connection_id(&self) -> Option<ConnectionId> {
        self.connection_id
    }

    /// Per-request middleware extensions bag.
    pub const fn extensions(&self) -> &Extensions {
        &self.extensions
    }
}

/// Reflective view of one outbound server response payload.
#[derive(Clone, Copy, Debug)]
pub enum ServerResponsePayload<'a> {
    Value(Peek<'a, 'static>),
    PostcardBytes(&'a [u8]),
}

/// Middleware-facing view of one outbound server response.
#[derive(Clone, Copy, Debug)]
pub struct ServerResponse<'a> {
    metadata: &'a [MetadataEntry<'a>],
    payload: ServerResponsePayload<'a>,
}

impl<'a> ServerResponse<'a> {
    pub fn new(response: &'a RequestResponse<'a>) -> Self {
        let payload = match &response.ret {
            Payload::Value { ptr, shape, .. } => {
                let peek = unsafe { Peek::unchecked_new(*ptr, shape) };
                ServerResponsePayload::Value(peek)
            }
            Payload::PostcardBytes(bytes) => ServerResponsePayload::PostcardBytes(bytes),
        };
        Self {
            metadata: &response.metadata,
            payload,
        }
    }

    pub const fn metadata(&self) -> &'a [MetadataEntry<'a>] {
        self.metadata
    }

    pub const fn payload(&self) -> ServerResponsePayload<'a> {
        self.payload
    }

    pub const fn payload_peek(&self) -> Option<Peek<'a, 'static>> {
        match self.payload {
            ServerResponsePayload::Value(peek) => Some(peek),
            ServerResponsePayload::PostcardBytes(_) => None,
        }
    }
}

/// Observe inbound server requests before and after dispatch.
pub trait ServerMiddleware: Send + Sync + 'static {
    fn pre<'a>(&'a self, _request: ServerRequest<'_>) -> BoxMiddlewareFuture<'a> {
        Box::pin(async {})
    }

    fn response<'a>(
        &'a self,
        _context: &ServerResponseContext,
        _response: ServerResponse<'_>,
    ) -> BoxMiddlewareFuture<'a> {
        Box::pin(async {})
    }

    fn post<'a>(
        &'a self,
        _context: &RequestContext<'_>,
        _outcome: ServerCallOutcome,
    ) -> BoxMiddlewareFuture<'a> {
        Box::pin(async {})
    }
}

#[derive(Clone)]
#[doc(hidden)]
pub struct ServerCallOutcomeHandle {
    outcome: Arc<Mutex<ServerCallOutcome>>,
}

impl ServerCallOutcomeHandle {
    pub fn outcome(&self) -> ServerCallOutcome {
        *self
            .outcome
            .lock()
            .expect("server call outcome mutex poisoned")
    }
}

#[doc(hidden)]
pub struct ObservedReplySink<R> {
    inner: Option<R>,
    outcome: ServerCallOutcomeHandle,
    response_context: ServerResponseContext,
    middlewares: Vec<Arc<dyn ServerMiddleware>>,
}

#[doc(hidden)]
pub fn observe_reply<R>(
    reply: R,
    response_context: ServerResponseContext,
    middlewares: Vec<Arc<dyn ServerMiddleware>>,
) -> (ObservedReplySink<R>, ServerCallOutcomeHandle) {
    let outcome = ServerCallOutcomeHandle {
        outcome: Arc::new(Mutex::new(ServerCallOutcome::DroppedWithoutReply)),
    };
    (
        ObservedReplySink {
            inner: Some(reply),
            outcome: outcome.clone(),
            response_context,
            middlewares,
        },
        outcome,
    )
}

impl<R> ReplySink for ObservedReplySink<R>
where
    R: ReplySink,
{
    async fn send_reply(mut self, response: RequestResponse<'_>) {
        for middleware in self.middlewares.iter().rev() {
            middleware
                .response(&self.response_context, ServerResponse::new(&response))
                .await;
        }
        *self
            .outcome
            .outcome
            .lock()
            .expect("server call outcome mutex poisoned") = ServerCallOutcome::Replied;
        let reply = self
            .inner
            .take()
            .expect("observed reply sink can only reply once");
        reply.send_reply(response).await;
    }

    fn channel_binder(&self) -> Option<&dyn crate::ChannelBinder> {
        self.inner.as_ref().and_then(|reply| reply.channel_binder())
    }

    fn request_id(&self) -> Option<crate::RequestId> {
        self.inner.as_ref().and_then(|reply| reply.request_id())
    }

    fn connection_id(&self) -> Option<crate::ConnectionId> {
        self.inner.as_ref().and_then(|reply| reply.connection_id())
    }
}

#[cfg(test)]
mod tests {
    use super::{Extensions, ServerCallOutcome, ServerRequest};
    use crate::{RequestContext, method_descriptor};

    #[test]
    fn extensions_store_values_by_type() {
        let extensions = Extensions::new();
        assert!(!extensions.contains::<u32>());
        assert_eq!(extensions.insert(41_u32), None);
        assert!(extensions.contains::<u32>());
        assert_eq!(extensions.get_cloned::<u32>(), Some(41));
        let updated = extensions.with_mut::<u32, _>(|value| {
            *value += 1;
            *value
        });
        assert_eq!(updated, Some(42));
        assert_eq!(extensions.get_cloned::<u32>(), Some(42));
    }

    #[test]
    fn server_call_outcome_reports_reply_state() {
        assert!(ServerCallOutcome::Replied.replied());
        assert!(!ServerCallOutcome::DroppedWithoutReply.replied());
    }

    #[test]
    fn server_request_exposes_context_and_decoded_args() {
        let method =
            method_descriptor::<(u32, u32), ()>("demo-service", "sum", &["left", "right"], None);
        let metadata = [];
        let extensions = Extensions::new();
        let context = RequestContext::with_extensions(method, &metadata, &extensions);
        let args = (7_u32, 35_u32);
        let request = ServerRequest::new(context, facet_reflect::Peek::new(&args));

        assert_eq!(request.method().method_name, "sum");
        assert_eq!(request.metadata().len(), 0);
        let tuple = request
            .args()
            .into_tuple()
            .expect("decoded args should be a tuple");
        let a = *tuple
            .field(0)
            .expect("first tuple field should exist")
            .get::<u32>()
            .expect("first tuple field should be u32");
        let b = *tuple
            .field(1)
            .expect("second tuple field should exist")
            .get::<u32>()
            .expect("second tuple field should be u32");
        assert_eq!((a, b), (7, 35));
    }
}
