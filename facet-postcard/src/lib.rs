//! Postcard binary format for facet.
//!
//! This crate provides serialization and deserialization for the postcard binary format.
//!
//! # Serialization
//!
//! Serialization supports all types that implement [`facet_core::Facet`]:
//!
//! ```
//! use facet::Facet;
//! use facet_postcard::to_vec;
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
//! There are three deserialization functions:
//!
//! - [`from_slice`]: Deserializes into owned types (`T: Facet<'static>`)
//! - [`from_slice_borrowed`]: Deserializes with zero-copy borrowing from the input buffer
//! - [`from_slice_with_shape`]: Deserializes into `Value` using runtime shape information
//! - [`from_slice_into`]: Deserializes into an existing `Partial` (type-erased, owned)
//! - [`from_slice_into_borrowed`]: Deserializes into an existing `Partial` (type-erased, zero-copy)
//!
//! ```
//! use facet_postcard::from_slice;
//!
//! // Postcard encoding: [length=3, true, false, true]
//! let bytes = &[0x03, 0x01, 0x00, 0x01];
//! let result: Vec<bool> = from_slice(bytes).unwrap();
//! assert_eq!(result, vec![true, false, true]);
//! ```
//!
//! Both functions automatically select the best deserialization tier:
//! - **Tier-2 (Format JIT)**: Fastest path for compatible types (primitives, structs, vecs, simple enums)
//! - **Tier-0 (Reflection)**: Fallback for all other types (nested enums, complex types)
//!
//! This ensures all `Facet` types can be deserialized.

// Note: unsafe code is used for lifetime transmutes in from_slice_into
// when BORROW=false, mirroring the approach used in facet-json.

extern crate alloc;

mod error;
mod parser;
mod serialize;
mod shape_deser;

#[cfg(feature = "jit")]
pub mod jit;

#[cfg(feature = "axum")]
mod axum;

#[cfg(feature = "axum")]
pub use axum::{Postcard, PostcardRejection, PostcardSerializeRejection};
pub use error::{PostcardError, SerializeError};
#[cfg(feature = "jit")]
pub use jit::PostcardJitFormat;
pub use parser::PostcardParser;
pub use serialize::{Writer, peek_to_vec, to_vec, to_vec_with_shape, to_writer_fallible};
pub use shape_deser::from_slice_with_shape;

// Re-export DeserializeError for convenience
pub use facet_format::DeserializeError;

/// Deserialize a value from postcard bytes into an owned type.
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
/// use facet_postcard::from_slice;
///
/// #[derive(Facet, Debug, PartialEq)]
/// struct Point {
///     x: i32,
///     y: i32,
/// }
///
/// // Postcard encoding: [x=10 (zigzag), y=20 (zigzag)]
/// let bytes = &[0x14, 0x28];
/// let point: Point = from_slice(bytes).unwrap();
/// assert_eq!(point.x, 10);
/// assert_eq!(point.y, 20);
/// ```
pub fn from_slice<T>(input: &[u8]) -> Result<T, DeserializeError>
where
    T: facet_core::Facet<'static>,
{
    use facet_format::FormatDeserializer;
    let parser = PostcardParser::new(input);
    let mut de = FormatDeserializer::new_owned(parser);
    de.deserialize()
}

/// Deserialize a value from postcard bytes, allowing zero-copy borrowing.
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
/// use facet_postcard::from_slice_borrowed;
///
/// #[derive(Facet, Debug, PartialEq)]
/// struct Message<'a> {
///     id: u32,
///     data: &'a [u8],
/// }
///
/// // Postcard encoding: [id=1, data_len=3, 0xAB, 0xCD, 0xEF]
/// let bytes = &[0x01, 0x03, 0xAB, 0xCD, 0xEF];
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
    let parser = PostcardParser::new(input);
    let mut de = FormatDeserializer::new(parser);
    de.deserialize()
}

/// Deserialize postcard bytes into an existing Partial.
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
/// use facet_postcard::from_slice_into;
/// use facet_reflect::Partial;
///
/// #[derive(Facet, Debug, PartialEq)]
/// struct Point {
///     x: i32,
///     y: i32,
/// }
///
/// // Postcard encoding: [x=10 (zigzag), y=20 (zigzag)]
/// let bytes = &[0x14, 0x28];
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
    let parser = PostcardParser::new(input);
    let mut de = FormatDeserializer::new_owned(parser);

    // SAFETY: The deserializer expects Partial<'input, false> where 'input is the
    // lifetime of the postcard bytes. Since BORROW=false, no data is borrowed from the
    // input, so the actual 'facet lifetime of the Partial is independent of 'input.
    // We transmute to satisfy the type system, then transmute back after deserialization.
    #[allow(unsafe_code)]
    let partial: facet_reflect::Partial<'_, false> = unsafe {
        core::mem::transmute::<
            facet_reflect::Partial<'facet, false>,
            facet_reflect::Partial<'_, false>,
        >(partial)
    };

    let partial = de.deserialize_into(partial)?;

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

/// Deserialize postcard bytes into an existing Partial, allowing zero-copy borrowing.
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
/// use facet_postcard::from_slice_into_borrowed;
/// use facet_reflect::Partial;
///
/// #[derive(Facet, Debug, PartialEq)]
/// struct Message<'a> {
///     id: u32,
///     data: &'a [u8],
/// }
///
/// // Postcard encoding: [id=1, data_len=3, 0xAB, 0xCD, 0xEF]
/// let bytes = &[0x01, 0x03, 0xAB, 0xCD, 0xEF];
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
    let parser = PostcardParser::new(input);
    let mut de = FormatDeserializer::new(parser);
    de.deserialize_into(partial)
}
