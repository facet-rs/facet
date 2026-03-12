use std::{future::Future, pin::Pin, sync::Arc};

use crate::{
    ClientCallOutcome, ClientContext, ClientMiddleware, ClientRequest, Extensions, MaybeSend,
    MaybeSync, Metadata, RequestCall, RequestResponse, RoamError, SelfRef, ServiceDescriptor,
};

// As a recap, a service defined like so:
//
// #[roam::service]
// trait Hash {
//   async fn hash(&self, payload: &[u8]) -> Result<&[u8], E>;
// }
//
// Would expand to the following caller:
//
// impl HashClient {
//   async fn hash(&self, payload: &[u8]) -> Result<SelfRef<&[u8]>, RoamError<E>>;
// }
//
// Would expand to a service trait (what users implement):
//
// trait Hash {
//   async fn hash(&self, call: impl Call<&[u8], E>, payload: &[u8]);
// }
//
// And a HashDispatcher<S: Hash> that implements Handler<R: ReplySink>:
// it deserializes args, constructs an ErasedCall<T, E> from the ReplySink,
// and routes to the appropriate method by method ID.
//
// For owned success returns, generated methods return values directly and
// the dispatcher sends replies on their behalf.
//
// HashDispatcher<S> implements Handler<R>, and can be stored as
// Box<dyn Handler<R>> to erase both S and the service type.
//
// Why impl Call in HashServer? So that the server can reply with something
// _borrowed_ from its own stack frame.
//
// For example:
//
// impl Hash for MyHasher {
//   async fn hash(&self, call: impl Call<&[u8], E>, payload: &[u8]) {
//     let result: [u8; 16] = compute_hash(payload);
//     call.ok(&result).await;
//   }
// }
//
// Call's public API is:
//
// trait Call<T, E> {
//   async fn reply(self, result: Result<T, E>);
//   async fn ok(self, value: T) { self.reply(Ok(value)).await }
//   async fn err(self, error: E) { self.reply(Err(error)).await }
// }
//
// If a Call is dropped before reply/ok/err is called, the caller will
// receive a RoamError::Cancelled error. This is to ensure that the caller
// is always notified, even if the handler panics or otherwise fails to
// reply.

/// Represents an in-progress call from a client that must be replied to.
///
/// A `Call` is handed to a [`Handler`] implementation and provides the
/// mechanism for sending a response back to the caller. The response can
/// be sent via [`Call::reply`], [`Call::ok`], or [`Call::err`].
///
/// # Cancellation
///
/// If a `Call` is dropped without a reply being sent, the caller will
/// automatically receive a [`RoamError::Cancelled`] error. This guarantees
/// that the caller is always notified, even if the handler panics or
/// otherwise fails to produce a reply.
///
/// # Type Parameters
///
/// - `T`: The success value type of the response.
/// - `E`: The error value type of the response.
pub trait Call<'wire, T, E>: MaybeSend
where
    T: facet::Facet<'wire> + MaybeSend,
    E: facet::Facet<'wire> + MaybeSend,
{
    /// Send a [`Result`] back to the caller, consuming this `Call`.
    fn reply(self, result: Result<T, E>) -> impl std::future::Future<Output = ()> + MaybeSend;

    /// Send a successful response back to the caller, consuming this `Call`.
    ///
    /// Equivalent to `self.reply(Ok(value)).await`.
    fn ok(self, value: T) -> impl std::future::Future<Output = ()> + MaybeSend
    where
        Self: Sized,
    {
        self.reply(Ok(value))
    }

    /// Send an error response back to the caller, consuming this `Call`.
    ///
    /// Equivalent to `self.reply(Err(error)).await`.
    fn err(self, error: E) -> impl std::future::Future<Output = ()> + MaybeSend
    where
        Self: Sized,
    {
        self.reply(Err(error))
    }
}

/// Sink for sending a reply back to the caller.
///
/// Implemented by the session driver. Provides backpressure: `send_reply`
/// awaits until the transport can accept the response before serializing it.
///
/// # Cancellation
///
/// If the `ReplySink` is dropped without `send_reply` being called, the caller
/// will automatically receive a [`crate::RoamError::Cancelled`] error.
pub trait ReplySink: MaybeSend + MaybeSync + 'static {
    /// Send the response, consuming the sink. Any error that happens during send_reply
    /// must set a flag in the driver for it to reply with an error.
    ///
    /// This cannot return a Result because we cannot trust callers to deal with it, and
    /// it's not like they can try sending a second reply anyway.
    ///
    /// Do not spawn a task to send the error because it too, might fail.
    fn send_reply(
        self,
        response: RequestResponse<'_>,
    ) -> impl std::future::Future<Output = ()> + MaybeSend;

    /// Send an error response back to the caller, consuming the sink.
    ///
    /// This is a convenience method used by generated dispatchers when
    /// deserialization fails or the method ID is unknown.
    fn send_error<E: for<'a> facet::Facet<'a> + MaybeSend>(
        self,
        error: RoamError<E>,
    ) -> impl std::future::Future<Output = ()> + MaybeSend
    where
        Self: Sized,
    {
        use crate::{Payload, RequestResponse};
        // Wire format is always Result<T, RoamError<E>>. We don't know T here,
        // but postcard encodes () as zero bytes, so Result<(), RoamError<E>>
        // produces the same Err variant encoding as any Result<T, RoamError<E>>.
        async move {
            let wire: Result<(), RoamError<E>> = Err(error);
            self.send_reply(RequestResponse {
                ret: Payload::outgoing(&wire),
                channels: vec![],
                metadata: Default::default(),
            })
            .await;
        }
    }

    /// Return a channel binder for binding Tx/Rx handles in deserialized args.
    ///
    /// Returns `None` by default. The driver's `ReplySink` implementation
    /// overrides this to provide actual channel binding.
    #[cfg(not(target_arch = "wasm32"))]
    fn channel_binder(&self) -> Option<&dyn crate::ChannelBinder> {
        None
    }
}

/// Type-erased handler for incoming service calls.
///
/// Implemented (by the macro-generated dispatch code) for server-side types.
/// Takes a fully decoded [`RequestCall`](crate::RequestCall) — already parsed
/// from the wire — and a [`ReplySink`] through which the response is sent.
///
/// The dispatch impl decodes the args, routes by [`crate::MethodId`], and
/// invokes the appropriate typed [`Call`]-based method on the concrete server type.
/// A cloneable handle to a connection, handed out by the session driver.
///
/// Generated clients hold an [`ErasedCaller`] and use it to send calls. The caller
/// serializes the outgoing [`RequestCall`] (with borrowed args), registers a
/// pending response slot, and awaits the response from the peer.
pub trait Caller: Clone + MaybeSend + MaybeSync + 'static {
    /// Send a call and wait for the response.
    fn call<'a>(
        &'a self,
        call: RequestCall<'a>,
    ) -> impl Future<Output = Result<SelfRef<RequestResponse<'static>>, RoamError>> + MaybeSend + 'a;

    /// Resolve when the underlying connection closes.
    ///
    /// Runtime-backed callers can override this to expose connection liveness.
    /// The default implementation never resolves.
    #[cfg(not(target_arch = "wasm32"))]
    fn closed(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(std::future::pending())
    }
    #[cfg(target_arch = "wasm32")]
    fn closed(&self) -> Pin<Box<dyn Future<Output = ()> + '_>> {
        Box::pin(std::future::pending())
    }

    /// Return whether the underlying connection is still considered connected.
    ///
    /// Runtime-backed callers can override this to provide eager liveness
    /// checks. The default implementation assumes the connection is live.
    fn is_connected(&self) -> bool {
        true
    }

    /// Return a channel binder for binding Tx/Rx handles in args before sending.
    ///
    /// Returns `None` by default. The driver's `Caller` implementation
    /// overrides this to provide actual channel binding.
    #[cfg(not(target_arch = "wasm32"))]
    fn channel_binder(&self) -> Option<&dyn crate::ChannelBinder> {
        None
    }
}

trait ErasedCallerDyn: MaybeSend + MaybeSync + 'static {
    #[cfg(not(target_arch = "wasm32"))]
    fn call<'a>(
        &'a self,
        call: RequestCall<'a>,
    ) -> Pin<
        Box<dyn Future<Output = Result<SelfRef<RequestResponse<'static>>, RoamError>> + Send + 'a>,
    >;
    #[cfg(target_arch = "wasm32")]
    fn call<'a>(
        &'a self,
        call: RequestCall<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<SelfRef<RequestResponse<'static>>, RoamError>> + 'a>>;

    #[cfg(not(target_arch = "wasm32"))]
    fn closed(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>;
    #[cfg(target_arch = "wasm32")]
    fn closed(&self) -> Pin<Box<dyn Future<Output = ()> + '_>>;

    fn is_connected(&self) -> bool;

    #[cfg(not(target_arch = "wasm32"))]
    fn channel_binder(&self) -> Option<&dyn crate::ChannelBinder>;
}

impl<C: Caller> ErasedCallerDyn for C {
    #[cfg(not(target_arch = "wasm32"))]
    fn call<'a>(
        &'a self,
        call: RequestCall<'a>,
    ) -> Pin<
        Box<dyn Future<Output = Result<SelfRef<RequestResponse<'static>>, RoamError>> + Send + 'a>,
    > {
        Box::pin(Caller::call(self, call))
    }
    #[cfg(target_arch = "wasm32")]
    fn call<'a>(
        &'a self,
        call: RequestCall<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<SelfRef<RequestResponse<'static>>, RoamError>> + 'a>>
    {
        Box::pin(Caller::call(self, call))
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn closed(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Caller::closed(self)
    }
    #[cfg(target_arch = "wasm32")]
    fn closed(&self) -> Pin<Box<dyn Future<Output = ()> + '_>> {
        Caller::closed(self)
    }

    fn is_connected(&self) -> bool {
        Caller::is_connected(self)
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn channel_binder(&self) -> Option<&dyn crate::ChannelBinder> {
        Caller::channel_binder(self)
    }
}

/// Type-erased [`Caller`] wrapper used by generated clients.
#[derive(Clone)]
pub struct ErasedCaller {
    inner: Arc<dyn ErasedCallerDyn>,
    service: Option<&'static ServiceDescriptor>,
    middlewares: Vec<Arc<dyn ClientMiddleware>>,
}

impl ErasedCaller {
    pub fn new<C: Caller>(caller: C) -> Self {
        Self {
            inner: Arc::new(caller),
            service: None,
            middlewares: vec![],
        }
    }

    pub fn with_middleware(
        mut self,
        service: &'static ServiceDescriptor,
        middleware: impl ClientMiddleware,
    ) -> Self {
        if let Some(existing_service) = self.service {
            assert_eq!(
                existing_service.service_name, service.service_name,
                "ErasedCaller middleware service mismatch"
            );
        } else {
            self.service = Some(service);
        }
        self.middlewares.push(Arc::new(middleware));
        self
    }
}

impl Caller for ErasedCaller {
    fn call<'a>(
        &'a self,
        mut call: RequestCall<'a>,
    ) -> impl Future<Output = Result<SelfRef<RequestResponse<'static>>, RoamError>> + MaybeSend + 'a
    {
        async move {
            let Some(service) = self.service else {
                return self.inner.call(call).await;
            };

            let extensions = Extensions::new();
            let method = service.by_id(call.method_id);
            let context = ClientContext::new(method, call.method_id, &extensions);
            let mut owned_metadata = crate::client_middleware::OwnedMetadata::default();

            if !self.middlewares.is_empty() {
                for middleware in &self.middlewares {
                    let mut request = ClientRequest::new(&mut call, &mut owned_metadata);
                    middleware.pre(&context, &mut request).await;
                }
            }

            let result = self.inner.call(call).await;
            if !self.middlewares.is_empty() {
                let outcome = match &result {
                    Ok(_) => ClientCallOutcome::Response,
                    Err(error) => ClientCallOutcome::Error(error),
                };
                for middleware in self.middlewares.iter().rev() {
                    middleware.post(&context, outcome).await;
                }
            }
            result
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn closed(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        self.inner.closed()
    }

    #[cfg(target_arch = "wasm32")]
    fn closed(&self) -> Pin<Box<dyn Future<Output = ()> + '_>> {
        self.inner.closed()
    }

    fn is_connected(&self) -> bool {
        self.inner.is_connected()
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn channel_binder(&self) -> Option<&dyn crate::ChannelBinder> {
        self.inner.channel_binder()
    }
}

pub trait Handler<R: ReplySink>: MaybeSend + MaybeSync + 'static {
    /// Return the static retry policy for a method ID served by this handler.
    fn retry_policy(&self, _method_id: crate::MethodId) -> crate::RetryPolicy {
        crate::RetryPolicy::VOLATILE
    }

    /// Dispatch an incoming call to the appropriate method implementation.
    fn handle(
        &self,
        call: SelfRef<crate::RequestCall<'static>>,
        reply: R,
    ) -> impl std::future::Future<Output = ()> + MaybeSend + '_;
}

impl<R: ReplySink> Handler<R> for () {
    async fn handle(&self, _call: SelfRef<crate::RequestCall<'static>>, _reply: R) {}
}

/// A decoded response value paired with response metadata.
///
/// This helper is available for lower-level callers that need both the
/// decoded value and metadata together. Generated Rust client methods do
/// not expose response metadata in their return types.
pub struct ResponseParts<'a, T> {
    /// The decoded return value.
    pub ret: T,
    /// Metadata attached to the response by the server.
    pub metadata: Metadata<'a>,
}

impl<'a, T> std::ops::Deref for ResponseParts<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.ret
    }
}

/// Concrete [`Call`] implementation backed by a [`ReplySink`].
///
/// Constructed by the dispatcher and handed to the server method.
/// When the server calls [`Call::reply`], the result is serialized and
/// sent through the sink.
pub struct SinkCall<R: ReplySink> {
    reply: R,
}

impl<R: ReplySink> SinkCall<R> {
    pub fn new(reply: R) -> Self {
        Self { reply }
    }
}

impl<'wire, T, E, R> Call<'wire, T, E> for SinkCall<R>
where
    T: facet::Facet<'wire> + MaybeSend,
    E: facet::Facet<'wire> + MaybeSend,
    R: ReplySink,
{
    async fn reply(self, result: Result<T, E>) {
        use crate::{Payload, RequestResponse};
        let wire: Result<T, crate::RoamError<E>> = result.map_err(crate::RoamError::User);
        let ptr =
            facet::PtrConst::new((&wire as *const Result<T, crate::RoamError<E>>).cast::<u8>());
        let shape = <Result<T, crate::RoamError<E>> as facet::Facet<'wire>>::SHAPE;
        // SAFETY: `wire` lives until `send_reply(...).await` completes in this function,
        // and `shape` matches the pointed value exactly.
        let ret = unsafe { Payload::outgoing_unchecked(ptr, shape) };
        self.reply
            .send_reply(RequestResponse {
                ret,
                channels: vec![],
                metadata: Default::default(),
            })
            .await;
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use crate::{MaybeSend, Metadata, Payload, RequestCall, RequestResponse};

    use super::{Call, Caller, Handler, ReplySink, ResponseParts};

    struct RecordingCall<T, E> {
        observed: Arc<Mutex<Option<Result<T, E>>>>,
    }

    impl<'wire, T, E> Call<'wire, T, E> for RecordingCall<T, E>
    where
        T: facet::Facet<'wire> + MaybeSend + Send + 'static,
        E: facet::Facet<'wire> + MaybeSend + Send + 'static,
    {
        async fn reply(self, result: Result<T, E>) {
            let mut guard = self.observed.lock().expect("recording mutex poisoned");
            *guard = Some(result);
        }
    }

    struct RecordingReplySink {
        saw_send_reply: Arc<Mutex<bool>>,
        saw_outgoing_payload: Arc<Mutex<bool>>,
    }

    impl ReplySink for RecordingReplySink {
        async fn send_reply(self, response: RequestResponse<'_>) {
            let mut saw_send_reply = self
                .saw_send_reply
                .lock()
                .expect("send-reply mutex poisoned");
            *saw_send_reply = true;

            let mut saw_outgoing = self
                .saw_outgoing_payload
                .lock()
                .expect("payload-kind mutex poisoned");
            *saw_outgoing = matches!(response.ret, Payload::Outgoing { .. });
        }
    }

    #[derive(Clone)]
    struct NoopCaller;

    impl Caller for NoopCaller {
        fn call<'a>(
            &'a self,
            _call: RequestCall<'a>,
        ) -> impl Future<
            Output = Result<crate::SelfRef<RequestResponse<'static>>, crate::RoamError>,
        > + MaybeSend
        + 'a {
            async move { unreachable!("NoopCaller::call is not used by this test") }
        }
    }

    #[tokio::test]
    async fn call_ok_and_err_route_through_reply() {
        let observed_ok: Arc<Mutex<Option<Result<u32, &'static str>>>> = Arc::new(Mutex::new(None));
        RecordingCall {
            observed: Arc::clone(&observed_ok),
        }
        .ok(7)
        .await;
        assert!(matches!(
            *observed_ok.lock().expect("ok mutex poisoned"),
            Some(Ok(7))
        ));

        let observed_err: Arc<Mutex<Option<Result<u32, &'static str>>>> =
            Arc::new(Mutex::new(None));
        RecordingCall {
            observed: Arc::clone(&observed_err),
        }
        .err("boom")
        .await;
        assert!(matches!(
            *observed_err.lock().expect("err mutex poisoned"),
            Some(Err("boom"))
        ));
    }

    #[tokio::test]
    async fn reply_sink_send_error_uses_outgoing_payload_and_reply_path() {
        let saw_send_reply = Arc::new(Mutex::new(false));
        let saw_outgoing_payload = Arc::new(Mutex::new(false));
        let sink = RecordingReplySink {
            saw_send_reply: Arc::clone(&saw_send_reply),
            saw_outgoing_payload: Arc::clone(&saw_outgoing_payload),
        };

        sink.send_error(crate::RoamError::<String>::Cancelled).await;

        assert!(*saw_send_reply.lock().expect("send-reply mutex poisoned"));
        assert!(
            *saw_outgoing_payload
                .lock()
                .expect("payload-kind mutex poisoned")
        );
    }

    #[tokio::test]
    async fn unit_handler_is_noop() {
        let req = crate::SelfRef::owning(
            crate::Backing::Boxed(Box::<[u8]>::default()),
            RequestCall {
                method_id: crate::MethodId(1),
                channels: vec![],
                metadata: Metadata::default(),
                args: Payload::Incoming(&[]),
            },
        );
        ().handle(
            req,
            RecordingReplySink {
                saw_send_reply: Arc::new(Mutex::new(false)),
                saw_outgoing_payload: Arc::new(Mutex::new(false)),
            },
        )
        .await;
    }

    #[test]
    fn response_parts_deref_exposes_ret() {
        let parts = ResponseParts {
            ret: 42_u32,
            metadata: Metadata::default(),
        };
        assert_eq!(*parts, 42);
    }

    #[test]
    fn default_channel_binder_accessor_for_caller_returns_none() {
        let caller = NoopCaller;
        assert!(caller.channel_binder().is_none());
    }

    #[test]
    fn default_channel_binder_accessor_for_reply_sink_returns_none() {
        let sink = RecordingReplySink {
            saw_send_reply: Arc::new(Mutex::new(false)),
            saw_outgoing_payload: Arc::new(Mutex::new(false)),
        };
        assert!(sink.channel_binder().is_none());
    }
}
