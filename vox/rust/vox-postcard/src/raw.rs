//! Raw postcard passthrough types for opaque adapters.
//!
//! When a type uses `#[facet(opaque = Adapter)]`, the serializer calls the adapter's
//! `serialize_map` to get an `OpaqueSerialize { ptr, shape }`. If the shape matches
//! the sentinel shape defined in vox-schema (`RawPostcardBorrowed`), we write the
//! bytes directly (passthrough) instead of recursively serializing.

use facet_core::PtrConst;

pub use vox_schema::{RAW_POSTCARD_BORROWED_SHAPE, RawPostcardBorrowed};

/// Create an `OpaqueSerialize` that tells the serializer to write these
/// already-encoded postcard bytes directly.
pub fn opaque_encoded_borrowed(bytes: &&[u8]) -> facet::OpaqueSerialize {
    vox_schema::opaque_encoded_borrowed(bytes)
}

/// Try to extract passthrough bytes from an `OpaqueSerialize` result.
/// Returns `Some(bytes)` if this is already-encoded postcard data.
///
/// Checks against the sentinel shape defined in vox-schema using value equality.
///
/// # Safety
/// The caller must ensure `ptr` points to valid memory matching `shape`.
pub unsafe fn try_decode_passthrough_bytes<'a>(
    ptr: PtrConst,
    shape: &'static facet_core::Shape,
) -> Option<&'a [u8]> {
    if shape == &RAW_POSTCARD_BORROWED_SHAPE {
        let borrowed: &'a RawPostcardBorrowed<'a> = unsafe { ptr.get() };
        return Some(borrowed.0);
    }
    None
}
