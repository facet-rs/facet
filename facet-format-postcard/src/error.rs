//! Error types for postcard Tier-2 JIT parsing and serialization.

extern crate alloc;

use alloc::string::String;
use core::fmt;

/// Postcard parsing error.
#[derive(Debug, Clone)]
pub struct PostcardError {
    /// Error code from JIT
    pub code: i32,
    /// Position in input where error occurred
    pub pos: usize,
    /// Human-readable message
    pub message: String,
}

impl fmt::Display for PostcardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at position {}", self.message, self.pos)
    }
}

impl std::error::Error for PostcardError {}

impl miette::Diagnostic for PostcardError {
    fn code<'a>(&'a self) -> Option<Box<dyn fmt::Display + 'a>> {
        Some(Box::new(format!("postcard::error::{}", self.code)))
    }
}

/// Postcard JIT error codes.
pub mod codes {
    /// Unexpected end of input
    pub const UNEXPECTED_EOF: i32 = -100;
    /// Invalid boolean value (not 0 or 1)
    pub const INVALID_BOOL: i32 = -101;
    /// Varint overflow (too many continuation bytes)
    pub const VARINT_OVERFLOW: i32 = -102;
    /// Sequence underflow (decrement when remaining is 0)
    pub const SEQ_UNDERFLOW: i32 = -103;
    /// Invalid UTF-8 in string
    pub const INVALID_UTF8: i32 = -104;
    /// Invalid Option discriminant (not 0x00 or 0x01)
    pub const INVALID_OPTION_DISCRIMINANT: i32 = -105;
    /// Invalid enum variant discriminant (out of range)
    pub const INVALID_ENUM_DISCRIMINANT: i32 = -106;
    /// Unsupported operation (triggers fallback)
    pub const UNSUPPORTED: i32 = -1;
}

impl PostcardError {
    /// Create an error from a JIT error code and position.
    pub fn from_code(code: i32, pos: usize) -> Self {
        let message = match code {
            codes::UNEXPECTED_EOF => "unexpected end of input".to_string(),
            codes::INVALID_BOOL => "invalid boolean value (expected 0 or 1)".to_string(),
            codes::VARINT_OVERFLOW => "varint overflow".to_string(),
            codes::SEQ_UNDERFLOW => "sequence underflow (internal error)".to_string(),
            codes::INVALID_UTF8 => "invalid UTF-8 in string".to_string(),
            codes::INVALID_OPTION_DISCRIMINANT => {
                "invalid Option discriminant (expected 0x00 or 0x01)".to_string()
            }
            codes::INVALID_ENUM_DISCRIMINANT => "invalid enum variant discriminant".to_string(),
            codes::UNSUPPORTED => "unsupported operation".to_string(),
            _ => format!("unknown error code {}", code),
        };
        Self { code, pos, message }
    }
}

/// Errors that can occur during postcard serialization.
#[derive(Debug)]
pub enum SerializeError {
    /// The output buffer is too small to hold the serialized data
    BufferTooSmall,
    /// A custom error message
    Custom(String),
}

impl fmt::Display for SerializeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SerializeError::BufferTooSmall => write!(f, "Buffer too small for serialized data"),
            SerializeError::Custom(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for SerializeError {}

impl miette::Diagnostic for SerializeError {}
