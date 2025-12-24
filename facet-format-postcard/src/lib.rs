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
//! Deserialization uses Tier-2 JIT and requires the `jit` feature:
//!
//! ```ignore
//! use facet_format_postcard::from_slice;
//!
//! let bytes = &[0x03, 0x01, 0x00, 0x01];
//! let result: Vec<bool> = from_slice(bytes).unwrap();
//! ```
//!
//! **Note:** Deserialization is Tier-2 JIT only. For non-JIT postcard support,
//! use `facet-postcard`.

#![cfg_attr(not(feature = "jit"), forbid(unsafe_code))]

extern crate alloc;

mod error;
mod parser;
mod serialize;

#[cfg(feature = "jit")]
pub mod jit;

pub use error::{PostcardError, SerializeError};
#[cfg(feature = "jit")]
pub use jit::PostcardJitFormat;
pub use parser::PostcardParser;
pub use serialize::{Writer, to_vec, to_writer_fallible};

// Re-export DeserializeError for convenience
pub use facet_format::DeserializeError;

/// Deserialize a value from postcard bytes.
///
/// This uses Tier-2 JIT deserialization. Types that aren't Tier-2 compatible
/// will return an error (this crate is Tier-2 only).
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

    // Try Tier-2 format JIT
    match facet_format::jit::try_deserialize_format::<T, _>(&mut parser) {
        Some(result) => result,
        None => Err(DeserializeError::Unsupported(
            "Type not supported by Tier-2 JIT (facet-format-postcard is Tier-2 only)".into(),
        )),
    }
}

/// Deserialize a value from postcard bytes (non-JIT fallback).
///
/// This function is only available when the `jit` feature is disabled.
/// It will always fail because this crate is Tier-2 JIT only.
#[cfg(not(feature = "jit"))]
pub fn from_slice<'de, T>(_input: &'de [u8]) -> Result<T, DeserializeError<PostcardError>>
where
    T: facet_core::Facet<'de>,
{
    Err(DeserializeError::Unsupported(
        "facet-format-postcard requires the 'jit' feature".into(),
    ))
}
