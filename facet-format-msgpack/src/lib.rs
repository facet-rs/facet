//! MsgPack binary format for facet using the Tier-2 JIT architecture.
//!
//! This crate provides Tier-2 JIT deserialization for the MsgPack binary format.
//! It implements `JitFormat` and `FormatJitParser` to enable direct byte-level
//! parsing without going through the event abstraction.
//!
//! **Note:** This crate is Tier-2 only. It does not implement a full `FormatParser`
//! (ParseEvent) stack. For non-JIT MsgPack support, use `facet-msgpack`.
//!
//! ## Supported Types (v1)
//!
//! - `Vec<bool>` - MsgPack booleans (0xC2/0xC3)
//! - `Vec<u8>` - MsgPack bin (0xC4/0xC5/0xC6) - **bulk copy fast path**
//! - `Vec<u32>`, `Vec<u64>`, `Vec<i32>`, `Vec<i64>` - MsgPack integers
//!
//! ## Wire Format
//!
//! This crate implements a subset of the MsgPack specification:
//!
//! | Type | Tags |
//! |------|------|
//! | Bool | `0xC2` (false), `0xC3` (true) |
//! | Unsigned | fixint (`0x00-0x7F`), `0xCC` (u8), `0xCD` (u16), `0xCE` (u32), `0xCF` (u64) |
//! | Signed | negative fixint (`0xE0-0xFF`), `0xD0` (i8), `0xD1` (i16), `0xD2` (i32), `0xD3` (i64) |
//! | Binary | `0xC4` (bin8), `0xC5` (bin16), `0xC6` (bin32) |
//! | Array | fixarray (`0x90-0x9F`), `0xDC` (array16), `0xDD` (array32) |

#![cfg_attr(not(feature = "jit"), forbid(unsafe_code))]

extern crate alloc;

mod error;
mod parser;

#[cfg(feature = "jit")]
pub mod jit;

pub use error::MsgPackError;
#[cfg(feature = "jit")]
pub use jit::MsgPackJitFormat;
pub use parser::MsgPackParser;

// Re-export DeserializeError for convenience
pub use facet_format::DeserializeError;

/// Deserialize a value from MsgPack bytes.
///
/// This uses Tier-2 JIT for supported types. Types that aren't Tier-2 compatible
/// will return an error with a detailed explanation of why (this crate is Tier-2 only).
///
/// # Supported Types (Tier-2 v1)
///
/// - `Vec<bool>`
/// - `Vec<u8>` (as MsgPack bin)
/// - `Vec<u32>`, `Vec<u64>`, `Vec<i32>`, `Vec<i64>`
///
/// # Example
///
/// ```
/// use facet_format_msgpack::from_slice;
///
/// // MsgPack encoding: [fixarray(3), true, false, true]
/// let bytes = &[0x93, 0xC3, 0xC2, 0xC3];
/// let result: Vec<bool> = from_slice(bytes).unwrap();
/// assert_eq!(result, vec![true, false, true]);
/// ```
#[cfg(feature = "jit")]
pub fn from_slice<'de, T>(input: &'de [u8]) -> Result<T, DeserializeError<MsgPackError>>
where
    T: facet_core::Facet<'de>,
{
    use facet_format::jit::{Tier2DeserializeError, try_deserialize_format_with_reason};

    let mut parser = MsgPackParser::new(input);

    // Use Tier-2 format JIT with detailed error reporting
    match try_deserialize_format_with_reason::<T, _>(&mut parser) {
        Ok(value) => Ok(value),
        Err(Tier2DeserializeError::ParserHasBufferedState) => Err(DeserializeError::Unsupported(
            "facet-format-msgpack: parser has buffered state (internal error)".into(),
        )),
        Err(Tier2DeserializeError::Incompatible(reason)) => {
            // Convert the detailed incompatibility reason to an error message
            Err(DeserializeError::Unsupported(format!(
                "facet-format-msgpack (Tier-2 only): {}",
                reason
            )))
        }
        Err(Tier2DeserializeError::Deserialize(e)) => Err(e),
    }
}

/// Deserialize a value from MsgPack bytes (non-JIT fallback).
///
/// This function is only available when the `jit` feature is disabled.
/// It will always fail because this crate is Tier-2 JIT only.
#[cfg(not(feature = "jit"))]
pub fn from_slice<'de, T>(_input: &'de [u8]) -> Result<T, DeserializeError<MsgPackError>>
where
    T: facet_core::Facet<'de>,
{
    Err(DeserializeError::Unsupported(
        "facet-format-msgpack requires the 'jit' feature".into(),
    ))
}
