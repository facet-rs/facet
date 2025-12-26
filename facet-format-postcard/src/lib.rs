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
//! Deserialization uses a multi-tier approach:
//!
//! ```ignore
//! use facet_format_postcard::from_slice;
//!
//! let bytes = &[0x03, 0x01, 0x00, 0x01];
//! let result: Vec<bool> = from_slice(bytes).unwrap();
//! ```
//!
//! The `from_slice` function automatically selects the best deserialization tier:
//! - **Tier-2 (Format JIT)**: Fastest path for compatible types (primitives, structs, vecs, simple enums)
//! - **Tier-0 (Reflection)**: Fallback for all other types (nested enums, complex types)
//!
//! This ensures all `Facet` types can be deserialized, making this crate a complete
//! replacement for `facet-postcard`.

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

/// Deserialize a value from postcard bytes.
///
/// This tries Tier-2 JIT deserialization first, then falls back to Tier-0
/// reflection-based deserialization if the type isn't Tier-2 compatible.
///
/// # Example
///
/// ```
/// use facet_format_postcard::from_slice;
///
/// // Postcard encoding: [length=3, true, false, true]
/// let bytes = &[0x03, 0x01, 0x00, 0x01];
/// let result: Vec<bool> = from_slice(bytes).unwrap();
/// assert_eq!(result, vec![true, false, true]);
/// ```
#[cfg(feature = "jit")]
pub fn from_slice<'de, T>(input: &'de [u8]) -> Result<T, DeserializeError<PostcardError>>
where
    T: facet_core::Facet<'de>,
{
    let mut parser = PostcardParser::new(input);

    // Try Tier-2 format JIT first (fastest path)
    match facet_format::jit::try_deserialize_format::<T, _>(&mut parser) {
        Some(result) => result,
        // Fall back to Tier-0 (reflection-based deserialization)
        None => {
            use facet_format::FormatDeserializer;
            FormatDeserializer::new(parser).deserialize()
        }
    }
}

/// Deserialize a value from postcard bytes (non-JIT fallback).
///
/// This function is only available when the `jit` feature is disabled.
/// It uses Tier-0 reflection-based deserialization, which is slower than JIT
/// but works on all platforms including WASM.
#[cfg(not(feature = "jit"))]
pub fn from_slice<'de, T>(input: &'de [u8]) -> Result<T, DeserializeError<PostcardError>>
where
    T: facet_core::Facet<'de>,
{
    use facet_format::FormatDeserializer;
    let parser = PostcardParser::new(input);
    FormatDeserializer::new(parser).deserialize()
}
