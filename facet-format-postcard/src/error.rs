//! Error types for postcard Tier-2 JIT parsing and serialization.

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

/// Postcard parsing error with optional source context for diagnostics.
#[derive(Debug, Clone)]
pub struct PostcardError {
    /// Error code from JIT
    pub code: i32,
    /// Position in input where error occurred
    pub pos: usize,
    /// Human-readable message
    pub message: String,
    /// Optional source bytes for diagnostics (hex dump context)
    pub source_bytes: Option<Vec<u8>>,
}

impl fmt::Display for PostcardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at position {}", self.message, self.pos)?;
        if let Some(ref bytes) = self.source_bytes {
            // Show hex dump context around error position
            let context = self.hex_context(bytes);
            if !context.is_empty() {
                write!(f, "\n{}", context)?;
            }
        }
        Ok(())
    }
}

impl PostcardError {
    /// Generate hex dump context around the error position.
    fn hex_context(&self, bytes: &[u8]) -> String {
        use alloc::format;

        if bytes.is_empty() {
            return String::new();
        }

        // Show up to 8 bytes before and after the error position
        let start = self.pos.saturating_sub(8);
        let end = (self.pos + 8).min(bytes.len());

        let mut parts = Vec::new();
        for (i, byte) in bytes[start..end].iter().enumerate() {
            let abs_pos = start + i;
            if abs_pos == self.pos {
                parts.push(format!("[{:02x}]", byte));
            } else {
                parts.push(format!("{:02x}", byte));
            }
        }

        format!(
            "  bytes: {} (position {} marked with [])",
            parts.join(" "),
            self.pos
        )
    }

    /// Attach source bytes for richer diagnostics.
    pub fn with_source(mut self, bytes: &[u8]) -> Self {
        self.source_bytes = Some(bytes.to_vec());
        self
    }
}

impl std::error::Error for PostcardError {}

#[cfg(feature = "pretty-errors")]
impl miette::Diagnostic for PostcardError {
    fn code<'a>(&'a self) -> Option<Box<dyn fmt::Display + 'a>> {
        let code_name = match self.code {
            codes::UNEXPECTED_EOF => "postcard::unexpected_eof",
            codes::INVALID_BOOL => "postcard::invalid_bool",
            codes::VARINT_OVERFLOW => "postcard::varint_overflow",
            codes::SEQ_UNDERFLOW => "postcard::seq_underflow",
            codes::INVALID_UTF8 => "postcard::invalid_utf8",
            codes::INVALID_OPTION_DISCRIMINANT => "postcard::invalid_option",
            codes::INVALID_ENUM_DISCRIMINANT => "postcard::invalid_enum",
            codes::UNSUPPORTED_OPAQUE_TYPE => "postcard::unsupported_opaque",
            codes::UNEXPECTED_END_OF_INPUT => "postcard::eof",
            codes::UNSUPPORTED => "postcard::unsupported",
            _ => "postcard::unknown",
        };
        Some(Box::new(code_name))
    }

    fn help<'a>(&'a self) -> Option<Box<dyn fmt::Display + 'a>> {
        let help = match self.code {
            codes::UNEXPECTED_EOF | codes::UNEXPECTED_END_OF_INPUT => {
                "The input data is truncated or incomplete"
            }
            codes::INVALID_BOOL => "Boolean values must be 0x00 (false) or 0x01 (true)",
            codes::VARINT_OVERFLOW => "Varint encoding used too many continuation bytes",
            codes::INVALID_UTF8 => "String data contains invalid UTF-8 bytes",
            codes::INVALID_OPTION_DISCRIMINANT => {
                "Option discriminant must be 0x00 (None) or 0x01 (Some)"
            }
            codes::INVALID_ENUM_DISCRIMINANT => "Enum variant index is out of range for this type",
            _ => return None,
        };
        Some(Box::new(help))
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
    /// Unsupported opaque type (shouldn't happen if hint_opaque_scalar is correct)
    pub const UNSUPPORTED_OPAQUE_TYPE: i32 = -107;
    /// Unexpected end of input (for fixed-length reads)
    pub const UNEXPECTED_END_OF_INPUT: i32 = -108;
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
        Self {
            code,
            pos,
            message,
            source_bytes: None,
        }
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

#[cfg(feature = "pretty-errors")]
impl miette::Diagnostic for SerializeError {}
