//! XDR (External Data Representation) format support via facet-format.
//!
//! XDR is a binary format defined in RFC 4506 for encoding structured data.
//! It is primarily used in Sun RPC (ONC RPC) protocols.
//!
//! Key characteristics:
//! - Big-endian byte order
//! - Fixed-size integers (4 bytes for i32/u32, 8 bytes for i64/u64)
//! - No support for i128/u128
//! - Strings are length-prefixed with 4-byte aligned padding
//! - Arrays have explicit length prefixes
//!
//! # Serialization
//!
//! ```
//! use facet::Facet;
//! use facet_format_xdr::to_vec;
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
//! ```
//! use facet::Facet;
//! use facet_format_xdr::from_slice;
//!
//! #[derive(Facet, Debug, PartialEq)]
//! struct Point { x: i32, y: i32 }
//!
//! // XDR encoding of Point { x: 10, y: 20 }
//! let bytes = &[0, 0, 0, 10, 0, 0, 0, 20];
//! let point: Point = from_slice(bytes).unwrap();
//! assert_eq!(point.x, 10);
//! assert_eq!(point.y, 20);
//! ```

#![forbid(unsafe_code)]

extern crate alloc;

mod error;
mod parser;
mod serializer;

pub use error::{XdrError, XdrSerializeError};
pub use parser::XdrParser;
pub use serializer::{XdrSerializer, to_vec, to_writer};

// Re-export DeserializeError for convenience
pub use facet_format::DeserializeError;

/// Deserialize a value from XDR bytes into an owned type.
///
/// This is the recommended default for most use cases.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_format_xdr::from_slice;
///
/// #[derive(Facet, Debug, PartialEq)]
/// struct Point { x: i32, y: i32 }
///
/// // XDR encoding of Point { x: 10, y: 20 }
/// let bytes = &[0, 0, 0, 10, 0, 0, 0, 20];
/// let point: Point = from_slice(bytes).unwrap();
/// assert_eq!(point.x, 10);
/// assert_eq!(point.y, 20);
/// ```
pub fn from_slice<T>(input: &[u8]) -> Result<T, DeserializeError<XdrError>>
where
    T: facet_core::Facet<'static>,
{
    use facet_format::FormatDeserializer;
    let parser = XdrParser::new(input);
    let mut de = FormatDeserializer::new_owned(parser);
    de.deserialize()
}

/// Deserialize a value from XDR bytes, allowing zero-copy borrowing.
///
/// This variant requires the input to outlive the result (`'input: 'facet`),
/// enabling zero-copy deserialization of byte slices as `&[u8]` or `Cow<[u8]>`.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_format_xdr::from_slice_borrowed;
/// use std::borrow::Cow;
///
/// #[derive(Facet, Debug, PartialEq)]
/// struct Message<'a> {
///     id: u32,
///     #[facet(sensitive)]
///     data: Cow<'a, [u8]>,
/// }
///
/// // XDR encoding of Message { id: 1, data: [0xAB, 0xCD, 0xEF] }
/// // id (4 bytes) + data length (4 bytes) + data (3 bytes) + padding (1 byte)
/// let bytes = &[0, 0, 0, 1, 0, 0, 0, 3, 0xAB, 0xCD, 0xEF, 0];
/// let msg: Message = from_slice_borrowed(bytes).unwrap();
/// assert_eq!(msg.id, 1);
/// assert_eq!(&*msg.data, &[0xAB, 0xCD, 0xEF]);
/// ```
pub fn from_slice_borrowed<'input, 'facet, T>(
    input: &'input [u8],
) -> Result<T, DeserializeError<XdrError>>
where
    T: facet_core::Facet<'facet>,
    'input: 'facet,
{
    use facet_format::FormatDeserializer;
    let parser = XdrParser::new(input);
    let mut de = FormatDeserializer::new(parser);
    de.deserialize()
}
