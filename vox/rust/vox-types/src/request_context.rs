use std::sync::OnceLock;

use crate::{Extensions, MetadataEntry, MethodDescriptor};

/// Borrowed per-request context exposed to opted-in Rust service handlers.
///
/// This is constructed by generated dispatchers from the inbound request and
/// borrows request metadata directly rather than cloning it.
#[derive(Clone, Copy, Debug)]
pub struct RequestContext<'a> {
    method: &'static MethodDescriptor,
    metadata: &'a [MetadataEntry<'static>],
    extensions: &'a Extensions,
}

impl<'a> RequestContext<'a> {
    /// Create a new borrowed request context.
    pub fn new(method: &'static MethodDescriptor, metadata: &'a [MetadataEntry<'static>]) -> Self {
        Self::with_extensions(method, metadata, empty_extensions())
    }

    /// Create a new borrowed request context with middleware extensions.
    pub fn with_extensions(
        method: &'static MethodDescriptor,
        metadata: &'a [MetadataEntry<'static>],
        extensions: &'a Extensions,
    ) -> Self {
        Self {
            method,
            metadata,
            extensions,
        }
    }

    /// Static descriptor for the method being handled.
    pub fn method(&self) -> &'static MethodDescriptor {
        self.method
    }

    /// Request metadata borrowed from the inbound call.
    pub fn metadata(&self) -> &'a [MetadataEntry<'static>] {
        self.metadata
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
