extern crate alloc;

use alloc::{borrow::Cow, vec::Vec};

use facet_core::{OpaqueSerialize, PtrConst, Shape};

/// Postcard-encoded opaque payload bytes.
///
/// This represents bytes that are already encoded using postcard's opaque
/// payload encoding. It can be borrowed or owned.
#[derive(Debug, Clone)]
pub enum RawPostcard<'a> {
    /// Borrowed postcard payload bytes.
    Borrowed(&'a [u8]),
    /// Owned postcard payload bytes.
    Owned(Vec<u8>),
}

impl<'a> RawPostcard<'a> {
    /// Creates a borrowed raw-postcard payload.
    pub const fn borrowed(bytes: &'a [u8]) -> Self {
        Self::Borrowed(bytes)
    }

    /// Creates an owned raw-postcard payload.
    pub fn owned(bytes: Vec<u8>) -> Self {
        Self::Owned(bytes)
    }

    /// Returns the payload as a byte slice.
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            Self::Borrowed(bytes) => bytes,
            Self::Owned(bytes) => bytes,
        }
    }

    /// Converts this payload into opaque adapter serialization inputs.
    pub fn to_opaque_serialize(&self) -> OpaqueSerialize {
        match self {
            Self::Borrowed(bytes) => opaque_encoded_borrowed(bytes),
            Self::Owned(bytes) => opaque_encoded_owned(bytes),
        }
    }
}

impl<'a> From<&'a [u8]> for RawPostcard<'a> {
    fn from(value: &'a [u8]) -> Self {
        Self::Borrowed(value)
    }
}

impl<'a> From<Vec<u8>> for RawPostcard<'a> {
    fn from(value: Vec<u8>) -> Self {
        Self::Owned(value)
    }
}

impl<'a> From<Cow<'a, [u8]>> for RawPostcard<'a> {
    fn from(value: Cow<'a, [u8]>) -> Self {
        match value {
            Cow::Borrowed(bytes) => Self::Borrowed(bytes),
            Cow::Owned(bytes) => Self::Owned(bytes),
        }
    }
}

#[repr(transparent)]
struct RawPostcardBorrowed<'a>(&'a [u8]);

#[repr(transparent)]
struct RawPostcardOwned(Vec<u8>);

static RAW_POSTCARD_BORROWED_SHAPE: Shape =
    Shape::builder_for_sized::<RawPostcardBorrowed<'static>>("RawPostcardBorrowed").build();

static RAW_POSTCARD_OWNED_SHAPE: Shape =
    Shape::builder_for_sized::<RawPostcardOwned>("RawPostcardOwned").build();

/// Builds opaque adapter serialization inputs for borrowed postcard payload bytes.
///
/// This is intended for `FacetOpaqueAdapter::serialize_map` when the adapter
/// already has borrowed postcard-encoded payload bytes and wants passthrough
/// serialization.
pub fn opaque_encoded_borrowed(bytes: &&[u8]) -> OpaqueSerialize {
    OpaqueSerialize {
        ptr: PtrConst::new((bytes as *const &[u8]).cast::<RawPostcardBorrowed<'_>>()),
        shape: &RAW_POSTCARD_BORROWED_SHAPE,
    }
}

/// Builds opaque adapter serialization inputs for owned postcard payload bytes.
///
/// This is intended for `FacetOpaqueAdapter::serialize_map` when the adapter
/// has owned postcard-encoded payload bytes and wants passthrough serialization.
pub fn opaque_encoded_owned(bytes: &Vec<u8>) -> OpaqueSerialize {
    OpaqueSerialize {
        ptr: PtrConst::new((bytes as *const Vec<u8>).cast::<RawPostcardOwned>()),
        shape: &RAW_POSTCARD_OWNED_SHAPE,
    }
}

pub(crate) unsafe fn try_decode_passthrough_bytes<'a>(
    ptr: PtrConst,
    shape: &'static Shape,
) -> Option<&'a [u8]> {
    if shape == &RAW_POSTCARD_BORROWED_SHAPE {
        let borrowed: &'a RawPostcardBorrowed<'a> = unsafe { ptr.get() };
        return Some(borrowed.0);
    }
    if shape == &RAW_POSTCARD_OWNED_SHAPE {
        let owned: &'a RawPostcardOwned = unsafe { ptr.get() };
        return Some(owned.0.as_slice());
    }
    None
}
