//! Opaque adapter definition for container-level custom opaque serde.
//!
//! This powers `#[facet(opaque = AdapterType)]`, allowing formats to bridge
//! opaque values through a typed adapter contract.

use crate::{PtrConst, PtrMut, PtrUninit, Shape};

#[cfg(feature = "alloc")]
use alloc::{string::String, vec::Vec};

/// Erased serialization inputs returned by an opaque adapter.
#[cfg(feature = "alloc")]
#[derive(Clone, Copy, Debug)]
pub enum OpaqueSerialize {
    /// Serialize a typed value by pointer + shape (existing behavior).
    Mapped {
        /// Pointer to the value to serialize.
        ptr: PtrConst,
        /// Shape describing `ptr`.
        shape: &'static Shape,
    },

    /// Serialize bytes that are already encoded in the format's opaque payload encoding.
    ///
    /// The pointed bytes must remain valid for the duration of the serialize call.
    EncodedBytes {
        /// Pointer to already-encoded payload bytes.
        ptr: *const u8,
        /// Length of `ptr` in bytes.
        len: usize,
    },
}

#[cfg(feature = "alloc")]
impl OpaqueSerialize {
    /// Constructs a mapped opaque serialization input from a typed pointer + shape.
    pub const fn mapped(ptr: PtrConst, shape: &'static Shape) -> Self {
        Self::Mapped { ptr, shape }
    }

    /// Constructs an encoded-bytes opaque serialization input.
    ///
    /// The slice must outlive the ongoing serialization call.
    pub fn encoded_bytes(bytes: &[u8]) -> Self {
        Self::EncodedBytes {
            ptr: bytes.as_ptr(),
            len: bytes.len(),
        }
    }
}

/// Input bytes provided to an opaque adapter during deserialization.
#[cfg(feature = "alloc")]
#[derive(Clone, Debug)]
pub enum OpaqueDeserialize<'de> {
    /// Borrowed input bytes from the parser buffer.
    Borrowed(&'de [u8]),
    /// Owned input bytes when borrowing is unavailable or disabled.
    Owned(Vec<u8>),
}

/// Typed contract for `#[facet(opaque = AdapterType)]`.
#[cfg(feature = "alloc")]
pub trait FacetOpaqueAdapter {
    /// Adapter-specific deserialize error type.
    type Error: core::fmt::Display;

    /// Typed outgoing value seen by `serialize_map`.
    type SendValue<'a>;

    /// Typed incoming value produced by `deserialize_build`.
    type RecvValue<'de>;

    /// Outgoing path: map typed value to erased serialization inputs.
    fn serialize_map(value: &Self::SendValue<'_>) -> OpaqueSerialize;

    /// Incoming path: build deferred payload representation.
    fn deserialize_build<'de>(
        input: OpaqueDeserialize<'de>,
    ) -> Result<Self::RecvValue<'de>, Self::Error>;
}

/// Erased serialize trampoline for opaque adapters.
#[cfg(feature = "alloc")]
pub type OpaqueAdapterSerializeFn = unsafe fn(target_ptr: PtrConst) -> OpaqueSerialize;

/// Erased deserialize trampoline for opaque adapters.
#[cfg(feature = "alloc")]
pub type OpaqueAdapterDeserializeFn = for<'de> unsafe fn(
    input: OpaqueDeserialize<'de>,
    target_ptr: PtrUninit,
) -> Result<PtrMut, String>;

/// Erased runtime definition used by `Shape` for adapter dispatch.
#[cfg(feature = "alloc")]
#[derive(Clone, Copy)]
pub struct OpaqueAdapterDef {
    /// Serialize trampoline.
    pub serialize: OpaqueAdapterSerializeFn,
    /// Deserialize trampoline.
    pub deserialize: OpaqueAdapterDeserializeFn,
}

#[cfg(feature = "alloc")]
impl core::fmt::Debug for OpaqueAdapterDef {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("OpaqueAdapterDef")
            .field("serialize", &"<fn>")
            .field("deserialize", &"<fn>")
            .finish()
    }
}
