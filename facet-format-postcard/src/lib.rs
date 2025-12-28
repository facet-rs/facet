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
//! use facet_format_postcard::to_vec;
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
//!
//! ```
//! use facet_format_postcard::from_slice;
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

#![cfg_attr(not(feature = "jit"), forbid(unsafe_code))]

extern crate alloc;

mod error;
mod parser;
mod serialize;

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
pub use serialize::{Writer, to_vec, to_writer_fallible};

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
/// use facet_format_postcard::from_slice;
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
pub fn from_slice<T>(input: &[u8]) -> Result<T, DeserializeError<PostcardError>>
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
/// use facet_format_postcard::from_slice_borrowed;
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
pub fn from_slice_borrowed<'input, 'facet, T>(
    input: &'input [u8],
) -> Result<T, DeserializeError<PostcardError>>
where
    T: facet_core::Facet<'facet>,
    'input: 'facet,
{
    use facet_format::FormatDeserializer;
    let parser = PostcardParser::new(input);
    let mut de = FormatDeserializer::new(parser);
    de.deserialize()
}
