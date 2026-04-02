use std::sync::OnceLock;

use crate::{ConnectionId, Extensions, MetadataEntry, MethodDescriptor, RequestId};

/// Borrowed per-request context exposed to opted-in Rust service handlers.
///
/// This is constructed by generated dispatchers from the inbound request and
/// borrows request metadata directly rather than cloning it.
#[derive(Clone, Copy, Debug)]
pub struct RequestContext<'a> {
    method: &'static MethodDescriptor,
    metadata: &'a [MetadataEntry<'a>],
    request_id: Option<RequestId>,
    connection_id: Option<ConnectionId>,
    extensions: &'a Extensions,
}

impl<'a> RequestContext<'a> {
    /// Create a new borrowed request context.
    pub fn new(method: &'static MethodDescriptor, metadata: &'a [MetadataEntry<'static>]) -> Self {
        Self::with_transport(method, metadata, None, None, empty_extensions())
    }

    /// Create a new borrowed request context with middleware extensions.
    pub fn with_extensions(
        method: &'static MethodDescriptor,
        metadata: &'a [MetadataEntry<'a>],
        extensions: &'a Extensions,
    ) -> Self {
        Self::with_transport(method, metadata, None, None, extensions)
    }

    /// Create a new borrowed request context with transport identifiers.
    pub fn with_transport(
        method: &'static MethodDescriptor,
        metadata: &'a [MetadataEntry<'a>],
        request_id: Option<RequestId>,
        connection_id: Option<ConnectionId>,
        extensions: &'a Extensions,
    ) -> Self {
        Self {
            method,
            metadata,
            request_id,
            connection_id,
            extensions,
        }
    }

    /// Static descriptor for the method being handled.
    pub fn method(&self) -> &'static MethodDescriptor {
        self.method
    }

    /// Request metadata borrowed from the inbound call.
    pub fn metadata(&self) -> &'a [MetadataEntry<'a>] {
        self.metadata
    }

    /// Wire-level request identifier for this call, when the reply sink exposes it.
    pub fn request_id(&self) -> Option<RequestId> {
        self.request_id
    }

    /// Virtual connection identifier for this call, when the reply sink exposes it.
    pub fn connection_id(&self) -> Option<ConnectionId> {
        self.connection_id
    }

    /// Per-request middleware extensions bag.
    pub fn extensions(&self) -> &'a Extensions {
        self.extensions
    }
}

fn empty_extensions() -> &'static Extensions {
    static EMPTY: OnceLock<Extensions> = OnceLock::new();
    EMPTY.get_or_init(Extensions::new)
}

#[cfg(test)]
mod tests {
    use crate::{ConnectionId, MetadataEntry, RequestId, method_descriptor};

    use super::RequestContext;

    #[test]
    fn transport_identifiers_are_exposed_when_present() {
        let method = method_descriptor::<(), ()>("demo-service", "demo", &[], None);
        let metadata: [MetadataEntry<'static>; 0] = [];

        let context = RequestContext::with_transport(
            method,
            &metadata,
            Some(RequestId(11)),
            Some(ConnectionId(13)),
            super::empty_extensions(),
        );

        assert_eq!(context.request_id(), Some(RequestId(11)));
        assert_eq!(context.connection_id(), Some(ConnectionId(13)));
        assert_eq!(context.method().id, method.id);
        assert_eq!(context.method().method_name, "demo");
    }
}
