//! Error types for MsgPack Tier-2 JIT parsing.

use core::fmt;

/// MsgPack parsing error.
#[derive(Debug, Clone)]
pub struct MsgPackError {
    /// Error code from JIT
    pub code: i32,
    /// Position in input where error occurred
    pub pos: usize,
    /// Human-readable message
    pub message: String,
}

impl fmt::Display for MsgPackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at position {}", self.message, self.pos)
    }
}

impl std::error::Error for MsgPackError {}

impl miette::Diagnostic for MsgPackError {
    fn code<'a>(&'a self) -> Option<Box<dyn fmt::Display + 'a>> {
        Some(Box::new(format!("msgpack::error::{}", self.code)))
    }
}

/// MsgPack JIT error codes.
pub mod codes {
    /// Unexpected end of input
    pub const UNEXPECTED_EOF: i32 = -100;
    /// Invalid type tag for expected bool
    pub const EXPECTED_BOOL: i32 = -101;
    /// Invalid type tag for expected array
    pub const EXPECTED_ARRAY: i32 = -102;
    /// Invalid type tag for expected bin/bytes
    pub const EXPECTED_BIN: i32 = -103;
    /// Invalid type tag for expected integer
    pub const EXPECTED_INT: i32 = -104;
    /// Integer value doesn't fit in target type
    pub const INT_OVERFLOW: i32 = -105;
    /// Array/bin count doesn't fit in usize
    pub const COUNT_OVERFLOW: i32 = -106;
    /// Sequence underflow (decrement when remaining is 0)
    pub const SEQ_UNDERFLOW: i32 = -107;
    /// Unsupported operation (triggers fallback)
    pub const UNSUPPORTED: i32 = -1;
}

impl MsgPackError {
    /// Create an error from a JIT error code and position.
    pub fn from_code(code: i32, pos: usize) -> Self {
        let message = match code {
            codes::UNEXPECTED_EOF => "unexpected end of input".to_string(),
            codes::EXPECTED_BOOL => "expected bool (0xC2 or 0xC3)".to_string(),
            codes::EXPECTED_ARRAY => "expected array tag (fixarray/array16/array32)".to_string(),
            codes::EXPECTED_BIN => "expected bin tag (bin8/bin16/bin32)".to_string(),
            codes::EXPECTED_INT => "expected integer tag".to_string(),
            codes::INT_OVERFLOW => "integer value overflows target type".to_string(),
            codes::COUNT_OVERFLOW => "count too large for platform".to_string(),
            codes::SEQ_UNDERFLOW => "sequence underflow (internal error)".to_string(),
            codes::UNSUPPORTED => "unsupported operation".to_string(),
            _ => format!("unknown error code {}", code),
        };
        Self { code, pos, message }
    }
}
