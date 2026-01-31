//! MsgPack binary format for facet.
//!
//! This crate provides serialization and deserialization for the MessagePack binary format.
//!
//! # Serialization
//!
//! ```
//! use facet::Facet;
//! use facet_msgpack::to_vec;
//!
//! #[derive(Facet)]
//! struct Point { x: i32, y: i32 }
//!
//! let point = Point { x: 10, y: 20 };
//! let bytes = to_vec(&point).unwrap();
//! ```
//!
//! # Deserialization
//!
//! There are two deserialization functions:
//!
//! - [`from_slice`]: Deserializes into owned types (`T: Facet<'static>`)
//! - [`from_slice_borrowed`]: Deserializes with zero-copy borrowing from the input buffer
//! - [`from_slice_into`]: Deserializes into an existing `Partial` (type-erased, owned)
//! - [`from_slice_into_borrowed`]: Deserializes into an existing `Partial` (type-erased, zero-copy)
//!
//! ```
//! use facet::Facet;
//! use facet_msgpack::from_slice;
//!
//! #[derive(Facet, Debug, PartialEq)]
//! struct Point { x: i32, y: i32 }
//!
//! // MsgPack encoding of {"x": 10, "y": 20}
//! let bytes = &[0x82, 0xa1, b'x', 0x0a, 0xa1, b'y', 0x14];
//! let point: Point = from_slice(bytes).unwrap();
//! assert_eq!(point.x, 10);
//! assert_eq!(point.y, 20);
//! ```
//!
//! Both functions use Tier-2 JIT for compatible types (when the `jit` feature is enabled),
//! with automatic fallback to Tier-0 reflection for all other types.

// Note: unsafe code is used for lifetime transmutes in from_slice_into
// when BORROW=false, mirroring the approach used in facet-json.

extern crate alloc;

mod error;
mod parser;
mod serializer;

#[cfg(feature = "jit")]
pub mod jit;

#[cfg(feature = "axum")]
mod axum;

pub use error::MsgPackError;

#[cfg(feature = "axum")]
pub use axum::{MsgPack, MsgPackRejection, MsgPackSerializeRejection};
#[cfg(feature = "jit")]
pub use jit::MsgPackJitFormat;
pub use parser::MsgPackParser;
pub use serializer::{MsgPackSerializeError, MsgPackSerializer, to_vec, to_writer};

// Re-export DeserializeError for convenience
pub use facet_format::DeserializeError;

/// Deserialize a value from MsgPack bytes into an owned type.
///
/// This is the recommended default for most use cases. The input does not need
/// to outlive the result, making it suitable for deserializing from temporary
/// buffers (e.g., HTTP request bodies).
///
/// Types containing `&str` or `&[u8]` fields cannot be deserialized with this
/// function; use `String`/`Vec<u8>` or `Cow<str>`/`Cow<[u8]>` instead. For
/// zero-copy deserialization into borrowed types, use [`from_slice_borrowed`].
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_msgpack::from_slice;
///
/// #[derive(Facet, Debug, PartialEq)]
/// struct Point {
///     x: i32,
///     y: i32,
/// }
///
/// // MsgPack encoding of {"x": 10, "y": 20}
/// let bytes = &[0x82, 0xa1, b'x', 0x0a, 0xa1, b'y', 0x14];
/// let point: Point = from_slice(bytes).unwrap();
/// assert_eq!(point.x, 10);
/// assert_eq!(point.y, 20);
/// ```
pub fn from_slice<T>(input: &[u8]) -> Result<T, DeserializeError>
where
    T: facet_core::Facet<'static>,
{
    use facet_format::FormatDeserializer;
    let mut parser = MsgPackParser::new(input);
    let mut de = FormatDeserializer::new_owned(&mut parser);
    de.deserialize()
}

/// Deserialize a value from MsgPack bytes, allowing zero-copy borrowing.
///
/// This variant requires the input to outlive the result (`'input: 'facet`),
/// enabling zero-copy deserialization of byte slices as `&[u8]` or `Cow<[u8]>`.
///
/// Use this when you need maximum performance and can guarantee the input
/// buffer outlives the deserialized value. For most use cases, prefer
/// [`from_slice`] which doesn't have lifetime requirements.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_msgpack::from_slice_borrowed;
///
/// #[derive(Facet, Debug, PartialEq)]
/// struct Message<'a> {
///     id: u32,
///     data: &'a [u8],
/// }
///
/// // MsgPack encoding of {"id": 1, "data": <bin8 with 3 bytes>}
/// let bytes = &[0x82, 0xa2, b'i', b'd', 0x01, 0xa4, b'd', b'a', b't', b'a', 0xc4, 0x03, 0xAB, 0xCD, 0xEF];
/// let msg: Message = from_slice_borrowed(bytes).unwrap();
/// assert_eq!(msg.id, 1);
/// assert_eq!(msg.data, &[0xAB, 0xCD, 0xEF]);
/// ```
pub fn from_slice_borrowed<'input, 'facet, T>(input: &'input [u8]) -> Result<T, DeserializeError>
where
    T: facet_core::Facet<'facet>,
    'input: 'facet,
{
    use facet_format::FormatDeserializer;
    let mut parser = MsgPackParser::new(input);
    let mut de = FormatDeserializer::new(&mut parser);
    de.deserialize()
}

/// Deserialize MsgPack bytes into an existing Partial.
///
/// This is useful for reflection-based deserialization where you don't have
/// a concrete type `T` at compile time, only its Shape metadata. The Partial
/// must already be allocated for the target type.
///
/// This version produces owned strings (no borrowing from input).
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_msgpack::from_slice_into;
/// use facet_reflect::Partial;
///
/// #[derive(Facet, Debug, PartialEq)]
/// struct Point {
///     x: i32,
///     y: i32,
/// }
///
/// // MsgPack encoding of {"x": 10, "y": 20}
/// let bytes = &[0x82, 0xa1, b'x', 0x0a, 0xa1, b'y', 0x14];
/// let partial = Partial::alloc_owned::<Point>().unwrap();
/// let partial = from_slice_into(bytes, partial).unwrap();
/// let value = partial.build().unwrap();
/// let point: Point = value.materialize().unwrap();
/// assert_eq!(point.x, 10);
/// assert_eq!(point.y, 20);
/// ```
pub fn from_slice_into<'facet>(
    input: &[u8],
    partial: facet_reflect::Partial<'facet, false>,
) -> Result<facet_reflect::Partial<'facet, false>, DeserializeError> {
    use facet_format::FormatDeserializer;
    let mut parser = MsgPackParser::new(input);
    let mut de = FormatDeserializer::new_owned(&mut parser);

    // SAFETY: The deserializer expects Partial<'input, false> where 'input is the
    // lifetime of the MsgPack bytes. Since BORROW=false, no data is borrowed from the
    // input, so the actual 'facet lifetime of the Partial is independent of 'input.
    // We transmute to satisfy the type system, then transmute back after deserialization.
    #[allow(unsafe_code)]
    let partial: facet_reflect::Partial<'_, false> = unsafe {
        core::mem::transmute::<
            facet_reflect::Partial<'facet, false>,
            facet_reflect::Partial<'_, false>,
        >(partial)
    };

    let partial = de.deserialize_into(partial, None)?;

    // SAFETY: Same reasoning - no borrowed data since BORROW=false.
    #[allow(unsafe_code)]
    let partial: facet_reflect::Partial<'facet, false> = unsafe {
        core::mem::transmute::<
            facet_reflect::Partial<'_, false>,
            facet_reflect::Partial<'facet, false>,
        >(partial)
    };

    Ok(partial)
}

/// Deserialize MsgPack bytes into an existing Partial, allowing zero-copy borrowing.
///
/// This variant requires the input to outlive the Partial's lifetime (`'input: 'facet`),
/// enabling zero-copy deserialization of byte slices as `&[u8]` or `Cow<[u8]>`.
///
/// This is useful for reflection-based deserialization where you don't have
/// a concrete type `T` at compile time, only its Shape metadata.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_msgpack::from_slice_into_borrowed;
/// use facet_reflect::Partial;
///
/// #[derive(Facet, Debug, PartialEq)]
/// struct Message<'a> {
///     id: u32,
///     data: &'a [u8],
/// }
///
/// // MsgPack encoding of {"id": 1, "data": <bin8 with 3 bytes>}
/// let bytes = &[0x82, 0xa2, b'i', b'd', 0x01, 0xa4, b'd', b'a', b't', b'a', 0xc4, 0x03, 0xAB, 0xCD, 0xEF];
/// let partial = Partial::alloc::<Message>().unwrap();
/// let partial = from_slice_into_borrowed(bytes, partial).unwrap();
/// let value = partial.build().unwrap();
/// let msg: Message = value.materialize().unwrap();
/// assert_eq!(msg.id, 1);
/// assert_eq!(msg.data, &[0xAB, 0xCD, 0xEF]);
/// ```
pub fn from_slice_into_borrowed<'input, 'facet>(
    input: &'input [u8],
    partial: facet_reflect::Partial<'facet, true>,
) -> Result<facet_reflect::Partial<'facet, true>, DeserializeError>
where
    'input: 'facet,
{
    use facet_format::FormatDeserializer;
    let mut parser = MsgPackParser::new(input);
    let mut de = FormatDeserializer::new(&mut parser);
    de.deserialize_into(partial, None)
}
