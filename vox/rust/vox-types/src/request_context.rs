use std::sync::OnceLock;

use crate::{
    Extensions, LaneId, Metadata, MethodDescriptor, RequestAuthorizationContext, RequestId,
};

/// Borrowed per-request context exposed to opted-in Rust service handlers.
///
/// This is constructed by generated dispatchers from the inbound request and
/// borrows request metadata directly rather than cloning it.
// r[impl request.authorization]
#[derive(Clone, Copy, Debug)]
pub struct RequestContext<'a> {
    method: &'static MethodDescriptor,
    metadata: &'a Metadata,
    request_id: Option<RequestId>,
    lane_id: Option<LaneId>,
    extensions: &'a Extensions,
}

impl<'a> RequestContext<'a> {
    /// Create a new borrowed request context.
    pub fn new(method: &'static MethodDescriptor, metadata: &'a Metadata) -> Self {
        Self::with_transport(method, metadata, None, None, empty_extensions())
    }

    /// Create a new borrowed request context with middleware extensions.
    pub fn with_extensions(
        method: &'static MethodDescriptor,
        metadata: &'a Metadata,
        extensions: &'a Extensions,
    ) -> Self {
        Self::with_transport(method, metadata, None, None, extensions)
    }

    /// Create a new borrowed request context with transport identifiers.
    pub fn with_transport(
        method: &'static MethodDescriptor,
        metadata: &'a Metadata,
        request_id: Option<RequestId>,
        lane_id: Option<LaneId>,
        extensions: &'a Extensions,
    ) -> Self {
        Self {
            method,
            metadata,
            request_id,
            lane_id,
            extensions,
        }
    }

    /// Static descriptor for the method being handled.
    pub fn method(&self) -> &'static MethodDescriptor {
        self.method
    }

    /// Request metadata borrowed from the inbound call.
    pub fn metadata(&self) -> &'a Metadata {
        self.metadata
    }

    /// Wire-level request identifier for this call, when the reply sink exposes it.
    pub fn request_id(&self) -> Option<RequestId> {
        self.request_id
    }

    /// Lane identifier for this call, when the reply sink exposes it.
    pub fn lane_id(&self) -> Option<LaneId> {
        self.lane_id
    }

    /// Per-request middleware extensions bag.
    pub fn extensions(&self) -> &'a Extensions {
        self.extensions
    }

    /// Authorization context resolved by the driver for this request, when available.
    pub fn authorization(&self) -> Option<RequestAuthorizationContext> {
        self.extensions.get_cloned()
    }
}

fn empty_extensions() -> &'static Extensions {
    static EMPTY: OnceLock<Extensions> = OnceLock::new();
    EMPTY.get_or_init(Extensions::new)
}

#[cfg(test)]
mod tests {
    use crate::{
        LaneGrant, LaneId, Metadata, MethodDescriptorOptions, RequestAuthorizationContext,
        RequestId, method_descriptor,
    };

    use super::RequestContext;

    #[test]
    fn transport_identifiers_are_exposed_when_present() {
        let method = method_descriptor::<(), ()>(
            "demo-service",
            "demo",
            &[],
            &[],
            MethodDescriptorOptions {
                response_wire_shape: <Result<(), crate::VoxError> as facet::Facet>::SHAPE,
                doc: None,
            },
        );
        let metadata = Metadata::default();

        let context = RequestContext::with_transport(
            method,
            &metadata,
            Some(RequestId(11)),
            Some(LaneId(13)),
            super::empty_extensions(),
        );

        assert_eq!(context.request_id(), Some(RequestId(11)));
        assert_eq!(context.lane_id(), Some(LaneId(13)));
        assert_eq!(context.method().id, method.id);
        assert_eq!(context.method().method_name, "demo");
    }

    #[test]
    fn authorization_context_is_exposed_from_extensions() {
        let method = method_descriptor::<(), ()>(
            "demo-service",
            "demo",
            &[],
            &[],
            MethodDescriptorOptions {
                response_wire_shape: <Result<(), crate::VoxError> as facet::Facet>::SHAPE,
                doc: None,
            },
        );
        let metadata = Metadata::default();
        let extensions = crate::Extensions::new();
        extensions.insert(RequestAuthorizationContext::new(
            crate::PeerIdentity::anonymous(),
            crate::PeerEvidence::none(),
            LaneGrant::empty(),
        ));

        let context = RequestContext::with_extensions(method, &metadata, &extensions);

        let authorization = context
            .authorization()
            .expect("authorization context should be present");
        assert!(authorization.peer_identity().is_anonymous());
        assert!(authorization.peer_evidence().is_empty());
        assert!(authorization.lane_grant().is_empty());
    }
}
