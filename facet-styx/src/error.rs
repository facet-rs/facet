//! Error types for Styx parsing.

use std::fmt;

use styx_parse::Span;

/// Error that can occur during Styx parsing.
#[derive(Debug, Clone, PartialEq)]
pub struct StyxError {
    pub kind: StyxErrorKind,
    pub span: Option<Span>,
}

impl StyxError {
    pub fn new(kind: StyxErrorKind, span: Option<Span>) -> Self {
        Self { kind, span }
    }
}

impl fmt::Display for StyxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.kind)?;
        if let Some(span) = &self.span {
            write!(f, " at offset {}", span.start)?;
        }
        Ok(())
    }
}

impl std::error::Error for StyxError {}

/// Kind of Styx error.
#[derive(Debug, Clone, PartialEq)]
pub enum StyxErrorKind {
    /// Unexpected token.
    UnexpectedToken { got: String, expected: &'static str },
    /// Unexpected end of input.
    UnexpectedEof { expected: &'static str },
    /// Invalid scalar value for target type.
    InvalidScalar {
        value: String,
        expected: &'static str,
    },
    /// Missing required field.
    MissingField { name: String },
    /// Unknown field.
    UnknownField { name: String },
    /// Invalid escape sequence.
    InvalidEscape { sequence: String },
}

impl fmt::Display for StyxErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StyxErrorKind::UnexpectedToken { got, expected } => {
                write!(f, "unexpected token '{}', expected {}", got, expected)
            }
            StyxErrorKind::UnexpectedEof { expected } => {
                write!(f, "unexpected end of input, expected {}", expected)
            }
            StyxErrorKind::InvalidScalar { value, expected } => {
                write!(f, "invalid value '{}', expected {}", value, expected)
            }
            StyxErrorKind::MissingField { name } => {
                write!(f, "missing required field '{}'", name)
            }
            StyxErrorKind::UnknownField { name } => {
                write!(f, "unknown field '{}'", name)
            }
            StyxErrorKind::InvalidEscape { sequence } => {
                write!(f, "invalid escape sequence '{}'", sequence)
            }
        }
    }
}
