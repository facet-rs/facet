//! Raw postcard passthrough types for opaque adapters.
//!
//! When a type uses `#[facet(opaque = Adapter)]`, the serializer calls the adapter's
//! `serialize_map` to get an `OpaqueSerialize { ptr, shape }`. If the shape matches
//! our `RawPostcardBorrowed` or `RawPostcardOwned`, we write the bytes directly
//! (passthrough) instead of recursively serializing.

use facet_core::{PtrConst, Shape};

/// Transparent wrapper around borrowed bytes that are already postcard-encoded.
#[repr(transparent)]
pub struct RawPostcardBorrowed<'a>(pub &'a [u8]);

/// Transparent wrapper around owned bytes that are already postcard-encoded.
#[repr(transparent)]
pub struct RawPostcardOwned(pub Vec<u8>);

// We use pointer identity on these statics to detect passthrough.
static RAW_POSTCARD_BORROWED_SHAPE: &Shape = <&[u8] as facet::Facet>::SHAPE;
static RAW_POSTCARD_OWNED_SHAPE: &Shape = <Vec<u8> as facet::Facet>::SHAPE;

/// Create an `OpaqueSerialize` that tells the serializer to write these
/// already-encoded postcard bytes directly.
pub fn opaque_encoded_borrowed(bytes: &&[u8]) -> facet::OpaqueSerialize {
    facet::OpaqueSerialize {
        ptr: PtrConst::new((bytes as *const &[u8]).cast::<u8>()),
        shape: RAW_POSTCARD_BORROWED_SHAPE,
    }
}

/// Try to extract passthrough bytes from an `OpaqueSerialize` result.
/// Returns `Some(bytes)` if this is already-encoded postcard data.
///
/// # Safety
/// The caller must ensure `ptr` points to valid memory matching `shape`.
pub unsafe fn try_decode_passthrough_bytes<'a>(
    ptr: PtrConst,
    shape: &'static Shape,
) -> Option<&'a [u8]> {
    if std::ptr::eq(shape, RAW_POSTCARD_BORROWED_SHAPE) {
        let slice_ref: &'a &'a [u8] = unsafe { &*ptr.as_ptr::<u8>().cast::<&[u8]>() };
        return Some(slice_ref);
    }
    if std::ptr::eq(shape, RAW_POSTCARD_OWNED_SHAPE) {
        let vec_ref: &'a Vec<u8> = unsafe { &*ptr.as_ptr::<u8>().cast::<Vec<u8>>() };
        return Some(vec_ref.as_slice());
    }
    None
}
