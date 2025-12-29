//! Error types for ASN.1 DER/BER parsing.

extern crate alloc;

use alloc::string::String;
use core::fmt;

/// ASN.1 parsing error.
#[derive(Debug, Clone)]
pub struct Asn1Error {
    /// Error kind
    pub kind: Asn1ErrorKind,
    /// Position in input where error occurred
    pub pos: usize,
}

/// The kind of ASN.1 error.
#[derive(Debug, Clone)]
pub enum Asn1ErrorKind {
    /// Unexpected end of input
    UnexpectedEof,
    /// Unknown or unsupported tag
    UnknownTag { tag: u8 },
    /// Length mismatch
    LengthMismatch { expected: usize, got: usize },
    /// Invalid boolean value
    InvalidBool,
    /// Invalid real (float) value
    InvalidReal,
    /// Invalid UTF-8 string
    InvalidString { message: String },
    /// Sequence/content size mismatch
    SequenceSizeMismatch {
        sequence_end: usize,
        content_end: usize,
    },
    /// Unsupported ASN.1 type or shape
    Unsupported { message: String },
    /// Invalid type tag attribute
    InvalidTypeTag { message: String },
    /// Invalid discriminant for enum variant
    InvalidDiscriminant { discriminant: Option<i64> },
}

impl fmt::Display for Asn1Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            Asn1ErrorKind::UnexpectedEof => {
                write!(f, "unexpected end of input at position {}", self.pos)
            }
            Asn1ErrorKind::UnknownTag { tag } => {
                write!(f, "unknown tag 0x{:02x} at position {}", tag, self.pos)
            }
            Asn1ErrorKind::LengthMismatch { expected, got } => {
                write!(
                    f,
                    "length mismatch at position {}: expected {}, got {}",
                    self.pos, expected, got
                )
            }
            Asn1ErrorKind::InvalidBool => {
                write!(f, "invalid boolean value at position {}", self.pos)
            }
            Asn1ErrorKind::InvalidReal => write!(f, "invalid real value at position {}", self.pos),
            Asn1ErrorKind::InvalidString { message } => {
                write!(f, "invalid string at position {}: {}", self.pos, message)
            }
            Asn1ErrorKind::SequenceSizeMismatch {
                sequence_end,
                content_end,
            } => {
                write!(
                    f,
                    "sequence size mismatch: sequence ends at {}, content ends at {}",
                    sequence_end, content_end
                )
            }
            Asn1ErrorKind::Unsupported { message } => {
                write!(f, "unsupported: {}", message)
            }
            Asn1ErrorKind::InvalidTypeTag { message } => {
                write!(f, "invalid type tag: {}", message)
            }
            Asn1ErrorKind::InvalidDiscriminant { discriminant } => {
                if let Some(d) = discriminant {
                    write!(f, "invalid discriminant: {}", d)
                } else {
                    write!(f, "missing discriminant")
                }
            }
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for Asn1Error {}

impl miette::Diagnostic for Asn1Error {
    fn code<'a>(&'a self) -> Option<Box<dyn fmt::Display + 'a>> {
        let code = match &self.kind {
            Asn1ErrorKind::UnexpectedEof => "asn1::unexpected_eof",
            Asn1ErrorKind::UnknownTag { .. } => "asn1::unknown_tag",
            Asn1ErrorKind::LengthMismatch { .. } => "asn1::length_mismatch",
            Asn1ErrorKind::InvalidBool => "asn1::invalid_bool",
            Asn1ErrorKind::InvalidReal => "asn1::invalid_real",
            Asn1ErrorKind::InvalidString { .. } => "asn1::invalid_string",
            Asn1ErrorKind::SequenceSizeMismatch { .. } => "asn1::sequence_size_mismatch",
            Asn1ErrorKind::Unsupported { .. } => "asn1::unsupported",
            Asn1ErrorKind::InvalidTypeTag { .. } => "asn1::invalid_type_tag",
            Asn1ErrorKind::InvalidDiscriminant { .. } => "asn1::invalid_discriminant",
        };
        Some(Box::new(code))
    }
}

impl Asn1Error {
    /// Create a new error with the given kind at the given position.
    pub fn new(kind: Asn1ErrorKind, pos: usize) -> Self {
        Self { kind, pos }
    }

    /// Create an unexpected EOF error.
    pub fn unexpected_eof(pos: usize) -> Self {
        Self::new(Asn1ErrorKind::UnexpectedEof, pos)
    }

    /// Create an unknown tag error.
    pub fn unknown_tag(tag: u8, pos: usize) -> Self {
        Self::new(Asn1ErrorKind::UnknownTag { tag }, pos)
    }

    /// Create an unsupported error.
    pub fn unsupported(message: impl Into<String>, pos: usize) -> Self {
        Self::new(
            Asn1ErrorKind::Unsupported {
                message: message.into(),
            },
            pos,
        )
    }
}
