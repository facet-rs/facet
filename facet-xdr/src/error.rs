//! Error types for XDR parsing and serialization.

use core::fmt;

/// XDR parsing error.
#[derive(Debug, Clone)]
pub struct XdrError {
    /// Error code
    pub code: i32,
    /// Position in input where error occurred
    pub pos: usize,
    /// Human-readable message
    pub message: String,
}

impl fmt::Display for XdrError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at position {}", self.message, self.pos)
    }
}

impl std::error::Error for XdrError {}

/// XDR error codes.
pub mod codes {
    /// Unexpected end of input
    pub const UNEXPECTED_EOF: i32 = -100;
    /// Invalid boolean value (not 0 or 1)
    pub const INVALID_BOOL: i32 = -101;
    /// Invalid optional discriminant (not 0 or 1)
    pub const INVALID_OPTIONAL: i32 = -102;
    /// Invalid enum discriminant
    pub const INVALID_VARIANT: i32 = -103;
    /// Invalid UTF-8 in string
    pub const INVALID_UTF8: i32 = -104;
    /// Unsupported type (e.g., i128/u128)
    pub const UNSUPPORTED_TYPE: i32 = -105;
    /// Position not aligned to 4 bytes
    pub const ALIGNMENT_ERROR: i32 = -106;
}

impl XdrError {
    /// Create an error from a code and position.
    pub fn from_code(code: i32, pos: usize) -> Self {
        let message = match code {
            codes::UNEXPECTED_EOF => "unexpected end of input".to_string(),
            codes::INVALID_BOOL => "invalid boolean value (must be 0 or 1)".to_string(),
            codes::INVALID_OPTIONAL => "invalid optional discriminant (must be 0 or 1)".to_string(),
            codes::INVALID_VARIANT => "invalid enum discriminant".to_string(),
            codes::INVALID_UTF8 => "invalid UTF-8 in string".to_string(),
            codes::UNSUPPORTED_TYPE => "unsupported type for XDR".to_string(),
            codes::ALIGNMENT_ERROR => "position not aligned to 4 bytes".to_string(),
            _ => format!("unknown error code {}", code),
        };
        Self { code, pos, message }
    }

    /// Create an error with a custom message.
    pub fn new(code: i32, pos: usize, message: impl Into<String>) -> Self {
        Self {
            code,
            pos,
            message: message.into(),
        }
    }
}

/// XDR serialization error.
#[derive(Debug)]
pub struct XdrSerializeError {
    /// Human-readable message
    pub message: String,
}

impl fmt::Display for XdrSerializeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for XdrSerializeError {}

impl XdrSerializeError {
    /// Create a new serialization error.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}
