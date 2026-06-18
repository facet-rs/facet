use crate::server_middleware::BoxMiddlewareFuture;
use crate::{Extensions, Metadata, MethodDescriptor, MethodId, RequestCall, VoxError, meta_set};

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
}

impl<'call, 'state> ClientRequest<'call, 'state> {
    pub fn new(call: &'state mut RequestCall<'call>) -> Self {
        Self { call }
    }

    pub fn call(&self) -> &RequestCall<'call> {
        self.call
    }

    /// The call's metadata (a self-describing [`Value`](facet_value::Value) map).
    pub fn metadata(&self) -> &Metadata {
        &self.call.metadata
    }

    pub fn metadata_mut(&mut self) -> &mut Metadata {
        &mut self.call.metadata
    }

    pub fn push_string_metadata(&mut self, key: &'static str, value: impl Into<String>) {
        meta_set(&mut self.call.metadata, key, value.into());
    }

    pub fn push_bytes_metadata(&mut self, key: &'static str, value: impl Into<Vec<u8>>) {
        meta_set(&mut self.call.metadata, key, value.into());
    }

    pub fn push_u64_metadata(&mut self, key: &'static str, value: u64) {
        meta_set(&mut self.call.metadata, key, value);
    }
}

#[derive(Clone, Copy)]
pub enum ClientCallOutcome<'a> {
    Response,
    Error(&'a VoxError),
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

#[cfg(test)]
mod tests {
    use crate::{MetadataExt, Payload};

    use super::{ClientRequest, MethodId, RequestCall};

    #[test]
    fn client_request_can_add_metadata() {
        let mut call = RequestCall {
            method_id: MethodId(1),
            channels: Vec::new(),
            metadata: Default::default(),
            args: Payload::Encoded(&[]),
            schemas: Default::default(),
        };
        let mut request = ClientRequest::new(&mut call);
        request.push_string_metadata("x-test", "value".to_string());
        request.push_bytes_metadata("x-bytes", vec![1, 2, 3]);
        request.push_u64_metadata("x-num", 7);

        assert_eq!(request.metadata().meta_len(), 3);
        assert_eq!(request.metadata().meta_str("x-test"), Some("value"));
        assert_eq!(
            request.metadata().meta_bytes("x-bytes"),
            Some(&[1u8, 2, 3][..])
        );
        assert_eq!(request.metadata().meta_u64("x-num"), Some(7));
    }
}
