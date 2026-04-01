use crate::server_middleware::BoxMiddlewareFuture;
use crate::{
    Extensions, Metadata, MetadataEntry, MetadataFlags, MetadataValue, MethodDescriptor, MethodId,
    RequestCall, VoxError,
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
    pub fn new(
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
        self.owned_metadata
            .strings
            .push(value.into().into_boxed_str());
        let stored = self.owned_metadata.strings.last().unwrap();
        // SAFETY: The boxed string is heap-allocated (stable address) and owned by
        // `owned_metadata`, which lives in the same stack frame as `call` in
        // MiddlewareCaller::call. It won't be dropped until after `call` is consumed.
        let value: &'call str = unsafe { &*((&**stored) as *const str) };
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
        self.owned_metadata
            .bytes
            .push(value.into().into_boxed_slice());
        let stored = self.owned_metadata.bytes.last().unwrap();
        // SAFETY: same reasoning as push_string_metadata above.
        let value: &'call [u8] = unsafe { &*((&**stored) as *const [u8]) };
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
pub struct OwnedMetadata {
    strings: Vec<Box<str>>,
    bytes: Vec<Box<[u8]>>,
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
    use crate::Payload;

    use super::{ClientRequest, MetadataFlags, MethodId, OwnedMetadata, RequestCall};

    #[test]
    fn client_request_can_add_owned_metadata() {
        let mut call = RequestCall {
            method_id: MethodId(1),
            metadata: vec![],
            args: Payload::PostcardBytes(&[]),
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
}
