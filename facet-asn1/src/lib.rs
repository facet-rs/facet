//! ASN.1 DER/BER serialization and deserialization for facet.
//!
//! This crate provides ASN.1 DER (Distinguished Encoding Rules) support via the
//! `FormatParser` and `FormatSerializer` traits.
//!
//! # ASN.1 Overview
//!
//! ASN.1 (Abstract Syntax Notation One) is a standard interface description language
//! for defining data structures that can be serialized and deserialized in a
//! cross-platform way. DER is a specific encoding rule that ensures canonical encoding.
//!
//! # Serialization
//!
//! ```
//! use facet::Facet;
//! use facet_asn1::to_vec;
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
//! ```ignore
//! use facet::Facet;
//! use facet_asn1::from_slice;
//!
//! #[derive(Facet)]
//! struct Point { x: i32, y: i32 }
//!
//! // DER encoding of Point { x: 10, y: 20 }
//! let bytes = &[0x30, 0x06, 0x02, 0x01, 0x0A, 0x02, 0x01, 0x14];
//! let point: Point = from_slice(bytes).unwrap();
//! ```
//!
//! # Type Mapping
//!
//! | Rust Type | ASN.1 Type |
//! |-----------|------------|
//! | `bool` | BOOLEAN |
//! | `i8`, `i16`, `i32`, `i64` | INTEGER |
//! | `u8`, `u16`, `u32`, `u64` | INTEGER |
//! | `f32`, `f64` | REAL |
//! | `String`, `&str` | UTF8String |
//! | `Vec<u8>`, `&[u8]` | OCTET STRING |
//! | struct | SEQUENCE |
//! | `Vec<T>` | SEQUENCE OF |
//! | `Option<T>` | Optional field |
//! | `()` | NULL |

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

mod error;
mod parser;
mod serializer;

pub use error::{Asn1Error, Asn1ErrorKind};
pub use parser::Asn1Parser;
pub use serializer::{Asn1SerializeError, Asn1Serializer, to_vec};

// Re-export DeserializeError for convenience
pub use facet_format::DeserializeError;

use bumpalo::Bump;

/// Deserialize a value from ASN.1 DER bytes into an owned type.
///
/// This is the recommended default for most use cases. The input does not need
/// to outlive the result, making it suitable for deserializing from temporary
/// buffers.
///
/// # Example
///
/// ```ignore
/// use facet::Facet;
/// use facet_asn1::from_slice;
///
/// #[derive(Facet, Debug, PartialEq)]
/// struct Point {
///     x: i32,
///     y: i32,
/// }
///
/// // DER encoding of Point { x: 10, y: 20 }
/// let bytes = &[0x30, 0x06, 0x02, 0x01, 0x0A, 0x02, 0x01, 0x14];
/// let point: Point = from_slice(bytes).unwrap();
/// assert_eq!(point.x, 10);
/// assert_eq!(point.y, 20);
/// ```
pub fn from_slice<T>(input: &[u8]) -> Result<T, DeserializeError>
where
    T: facet_core::Facet<'static>,
{
    use facet_format::FormatDeserializer;
    let bump = Bump::new();
    let mut parser = Asn1Parser::new(input);
    let mut de = FormatDeserializer::new_owned(&bump, &mut parser);
    de.deserialize()
}

/// Deserialize a value from ASN.1 DER bytes, allowing zero-copy borrowing.
///
/// This variant requires the input to outlive the result (`'input: 'facet`),
/// enabling zero-copy deserialization of byte slices as `&[u8]` or `Cow<[u8]>`.
///
/// Use this when you need maximum performance and can guarantee the input
/// buffer outlives the deserialized value.
pub fn from_slice_borrowed<'input, 'facet, T>(input: &'input [u8]) -> Result<T, DeserializeError>
where
    T: facet_core::Facet<'facet>,
    'input: 'facet,
{
    use facet_format::FormatDeserializer;
    let bump = Bump::new();
    let mut parser = Asn1Parser::new(input);
    let mut de = FormatDeserializer::new(&bump, &mut parser);
    de.deserialize()
}
