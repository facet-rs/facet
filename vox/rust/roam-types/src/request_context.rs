use crate::{MetadataEntry, MethodDescriptor};

/// Borrowed per-request context exposed to opted-in Rust service handlers.
///
/// This is constructed by generated dispatchers from the inbound request and
/// borrows request metadata directly rather than cloning it.
#[derive(Clone, Copy, Debug)]
pub struct RequestContext<'a> {
    method: &'static MethodDescriptor,
    metadata: &'a [MetadataEntry<'static>],
}

impl<'a> RequestContext<'a> {
    /// Create a new borrowed request context.
    pub fn new(method: &'static MethodDescriptor, metadata: &'a [MetadataEntry<'static>]) -> Self {
        Self { method, metadata }
    }

    /// Static descriptor for the method being handled.
    pub fn method(&self) -> &'static MethodDescriptor {
        self.method
    }

    /// Request metadata borrowed from the inbound call.
    pub fn metadata(&self) -> &'a [MetadataEntry<'static>] {
        self.metadata
    }
}
