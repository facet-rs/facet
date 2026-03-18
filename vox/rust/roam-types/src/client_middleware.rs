use std::sync::Arc;

use crate::server_middleware::BoxMiddlewareFuture;
use crate::{
    BoxFut, CallResult, Caller, Extensions, Metadata, MetadataEntry, MetadataFlags, MetadataValue,
    MethodDescriptor, MethodId, RequestCall, RoamError, ServiceDescriptor,
};

/// Borrowed per-call context exposed to client middleware.
#[derive(Clone, Copy, Debug)]
pub struct ClientContext<'a> {
    method: Option<&'static MethodDescriptor>,
    method_id: MethodId,
    extensions: &'a Extensions,
}

impl<'a> ClientContext<'a> {
    pub fn new(
        method: Option<&'static MethodDescriptor>,
        method_id: MethodId,
        extensions: &'a Extensions,
    ) -> Self {
        Self {
            method,
            method_id,
            extensions,
        }
    }

    pub fn method(&self) -> Option<&'static MethodDescriptor> {
        self.method
    }

    pub fn method_id(&self) -> MethodId {
        self.method_id
    }

    pub fn extensions(&self) -> &'a Extensions {
        self.extensions
    }
}

/// Borrowed request wrapper exposed to client middleware.
///
/// This allows middleware to add dynamic metadata while keeping the backing
/// storage alive until the wrapped caller finishes sending the request.
pub struct ClientRequest<'call, 'state> {
    call: &'state mut RequestCall<'call>,
    owned_metadata: &'state mut OwnedMetadata,
}

impl<'call, 'state> ClientRequest<'call, 'state> {
    pub(crate) fn new(
        call: &'state mut RequestCall<'call>,
        owned_metadata: &'state mut OwnedMetadata,
    ) -> Self {
        Self {
            call,
            owned_metadata,
        }
    }

    pub fn call(&self) -> &RequestCall<'call> {
        self.call
    }

    pub fn metadata(&self) -> &[MetadataEntry<'call>] {
        &self.call.metadata
    }

    pub fn metadata_mut(&mut self) -> &mut Metadata<'call> {
        &mut self.call.metadata
    }

    pub fn push_string_metadata(
        &mut self,
        key: &'static str,
        value: impl Into<String>,
        flags: MetadataFlags,
    ) {
        let value = self.owned_metadata.store_string(value.into());
        self.call.metadata.push(MetadataEntry {
            key,
            value: MetadataValue::String(value),
            flags,
        });
    }

    pub fn push_bytes_metadata(
        &mut self,
        key: &'static str,
        value: impl Into<Vec<u8>>,
        flags: MetadataFlags,
    ) {
        let value = self.owned_metadata.store_bytes(value.into());
        self.call.metadata.push(MetadataEntry {
            key,
            value: MetadataValue::Bytes(value),
            flags,
        });
    }

    pub fn push_u64_metadata(&mut self, key: &'static str, value: u64, flags: MetadataFlags) {
        self.call.metadata.push(MetadataEntry {
            key,
            value: MetadataValue::U64(value),
            flags,
        });
    }
}

#[derive(Default)]
pub(crate) struct OwnedMetadata {
    strings: Vec<Box<str>>,
    bytes: Vec<Box<[u8]>>,
}

impl OwnedMetadata {
    fn store_string<'a>(&mut self, value: String) -> &'a str {
        self.strings.push(value.into_boxed_str());
        let value = self
            .strings
            .last()
            .expect("owned string metadata should exist after push");
        let value: *const str = &**value;
        // SAFETY: the boxed string is owned by this `OwnedMetadata` and remains alive
        // until the wrapped caller finishes awaiting the inner `call`.
        unsafe { &*value }
    }

    fn store_bytes<'a>(&mut self, value: Vec<u8>) -> &'a [u8] {
        self.bytes.push(value.into_boxed_slice());
        let value = self
            .bytes
            .last()
            .expect("owned bytes metadata should exist after push");
        let value: *const [u8] = &**value;
        // SAFETY: the boxed bytes are owned by this `OwnedMetadata` and remain alive
        // until the wrapped caller finishes awaiting the inner `call`.
        unsafe { &*value }
    }
}

#[derive(Clone, Copy)]
pub enum ClientCallOutcome<'a> {
    Response,
    Error(&'a RoamError),
}

impl ClientCallOutcome<'_> {
    pub fn is_ok(self) -> bool {
        matches!(self, Self::Response)
    }
}

pub trait ClientMiddleware: Send + Sync + 'static {
    fn pre<'a, 'call>(
        &'a self,
        _context: &'a ClientContext<'a>,
        _request: &'a mut ClientRequest<'call, 'a>,
    ) -> BoxMiddlewareFuture<'a> {
        Box::pin(async {})
    }

    fn post<'a>(
        &'a self,
        _context: &'a ClientContext<'a>,
        _outcome: ClientCallOutcome<'a>,
    ) -> BoxMiddlewareFuture<'a> {
        Box::pin(async {})
    }
}

#[derive(Clone)]
pub struct MiddlewareCaller<C> {
    caller: C,
    service: &'static ServiceDescriptor,
    middlewares: Vec<Arc<dyn ClientMiddleware>>,
}

impl<C> MiddlewareCaller<C> {
    pub fn new(caller: C, service: &'static ServiceDescriptor) -> Self {
        Self {
            caller,
            service,
            middlewares: vec![],
        }
    }

    pub fn with_middleware(mut self, middleware: impl ClientMiddleware) -> Self {
        self.middlewares.push(Arc::new(middleware));
        self
    }
}

impl<C> Caller for MiddlewareCaller<C>
where
    C: Caller,
{
    async fn call<'a>(&'a self, mut call: RequestCall<'a>) -> CallResult {
        let extensions = Extensions::new();
        let method = self.service.by_id(call.method_id);
        let context = ClientContext::new(method, call.method_id, &extensions);
        let mut owned_metadata = OwnedMetadata::default();
        if !self.middlewares.is_empty() {
            for middleware in &self.middlewares {
                let mut request = ClientRequest::new(&mut call, &mut owned_metadata);
                middleware.pre(&context, &mut request).await;
            }
        }

        let result = self.caller.call(call).await;
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

    fn closed(&self) -> BoxFut<'_, ()> {
        self.caller.closed()
    }

    fn is_connected(&self) -> bool {
        self.caller.is_connected()
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn channel_binder(&self) -> Option<&dyn crate::ChannelBinder> {
        self.caller.channel_binder()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use crate::{Backing, Payload};

    use super::{
        BoxMiddlewareFuture, ClientCallOutcome, ClientContext, ClientMiddleware, ClientRequest,
        MetadataFlags, MethodDescriptor, MethodId, MiddlewareCaller, OwnedMetadata, RequestCall,
    };
    use crate::{CallResult, Caller};
    use crate::{RequestResponse, SelfRef};

    #[test]
    fn client_request_can_add_owned_metadata() {
        let mut call = RequestCall {
            method_id: MethodId(1),
            channels: vec![],
            metadata: vec![],
            args: Payload::Incoming(&[]),
            schemas: Default::default(),
        };
        let mut owned = OwnedMetadata::default();
        let mut request = ClientRequest::new(&mut call, &mut owned);
        request.push_string_metadata("x-test", "value".to_string(), MetadataFlags::NONE);
        request.push_bytes_metadata("x-bytes", vec![1, 2, 3], MetadataFlags::NONE);
        request.push_u64_metadata("x-num", 7, MetadataFlags::NONE);

        assert_eq!(request.metadata().len(), 3);
        assert!(matches!(
            request.metadata()[0].value,
            crate::MetadataValue::String("value")
        ));
        assert!(matches!(
            request.metadata()[1].value,
            crate::MetadataValue::Bytes(bytes) if bytes == [1, 2, 3]
        ));
        assert!(matches!(
            request.metadata()[2].value,
            crate::MetadataValue::U64(7)
        ));
    }

    #[derive(Clone)]
    struct RecordingCaller {
        seen_metadata: Arc<Mutex<Vec<String>>>,
    }

    impl Caller for RecordingCaller {
        async fn call<'a>(&'a self, call: RequestCall<'a>) -> CallResult {
            let seen = call
                .metadata
                .iter()
                .map(|entry| match entry.value {
                    crate::MetadataValue::String(value) => format!("{}={value}", entry.key),
                    crate::MetadataValue::Bytes(bytes) => {
                        format!("{}=<{} bytes>", entry.key, bytes.len())
                    }
                    crate::MetadataValue::U64(value) => format!("{}={value}", entry.key),
                })
                .collect::<Vec<_>>();
            *self
                .seen_metadata
                .lock()
                .expect("seen metadata mutex poisoned") = seen;

            Ok(crate::WithTracker {
                value: SelfRef::owning(
                    Backing::Boxed(Box::<[u8]>::default()),
                    RequestResponse {
                        channels: vec![],
                        metadata: vec![],
                        ret: Payload::Incoming(&[]),
                        schemas: Default::default(),
                    },
                ),
                tracker: std::sync::Arc::new(crate::SchemaRecvTracker::new()),
            })
        }
    }

    #[derive(Clone)]
    struct InjectMetadata;

    impl ClientMiddleware for InjectMetadata {
        fn pre<'a, 'call>(
            &'a self,
            context: &'a ClientContext<'a>,
            request: &'a mut ClientRequest<'call, 'a>,
        ) -> BoxMiddlewareFuture<'a> {
            Box::pin(async move {
                context.extensions().insert(41_u32);
                request.push_string_metadata("x-test", "value".to_string(), MetadataFlags::NONE);
            })
        }

        fn post<'a>(
            &'a self,
            context: &'a ClientContext<'a>,
            outcome: ClientCallOutcome<'a>,
        ) -> BoxMiddlewareFuture<'a> {
            Box::pin(async move {
                assert_eq!(context.extensions().get_cloned::<u32>(), Some(41));
                assert!(outcome.is_ok());
            })
        }
    }

    #[tokio::test]
    async fn middleware_caller_runs_hooks_and_mutates_metadata() {
        static METHOD: MethodDescriptor = MethodDescriptor {
            id: MethodId(7),
            service_name: "Audit",
            method_name: "record",
            args: &[],
            return_shape: <() as facet::Facet<'static>>::SHAPE,
            retry: crate::RetryPolicy::VOLATILE,
            doc: None,
        };
        static SERVICE: crate::ServiceDescriptor = crate::ServiceDescriptor {
            service_name: "Audit",
            methods: &[&METHOD],
            doc: None,
        };

        let seen_metadata = Arc::new(Mutex::new(Vec::new()));
        let caller = MiddlewareCaller::new(
            RecordingCaller {
                seen_metadata: Arc::clone(&seen_metadata),
            },
            &SERVICE,
        )
        .with_middleware(InjectMetadata);

        let response: CallResult = caller
            .call(RequestCall {
                method_id: MethodId(7),
                channels: vec![],
                metadata: vec![],
                args: Payload::Incoming(&[]),
                schemas: Default::default(),
            })
            .await;

        assert!(response.is_ok());
        assert_eq!(
            *seen_metadata.lock().expect("seen metadata mutex poisoned"),
            vec!["x-test=value".to_string()]
        );
    }
}
