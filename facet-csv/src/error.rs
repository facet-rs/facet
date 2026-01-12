//! CSV parsing error types.

use alloc::string::String;
use core::fmt;

/// Error type for CSV parsing.
#[derive(Debug, Clone)]
pub struct CsvError {
    kind: CsvErrorKind,
    /// Source span of the error, if available.
    #[allow(dead_code)]
    span: Option<facet_reflect::Span>,
}

impl CsvError {
    /// Create a new error with the given kind.
    pub const fn new(kind: CsvErrorKind) -> Self {
        Self { kind, span: None }
    }

    /// Create a new error with the given kind and span.
    pub const fn with_span(kind: CsvErrorKind, span: facet_reflect::Span) -> Self {
        Self {
            kind,
            span: Some(span),
        }
    }

    /// Get the error kind.
    pub const fn kind(&self) -> &CsvErrorKind {
        &self.kind
    }
}

impl fmt::Display for CsvError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            CsvErrorKind::UnexpectedEof { expected } => {
                write!(f, "unexpected end of input, expected {expected}")
            }
            CsvErrorKind::InvalidValue { message } => write!(f, "invalid value: {message}"),
            CsvErrorKind::UnsupportedType { type_name } => {
                write!(f, "unsupported type for CSV: {type_name}")
            }
            CsvErrorKind::TooFewFields { expected, got } => {
                write!(f, "too few fields: expected {expected}, got {got}")
            }
            CsvErrorKind::TooManyFields { expected, got } => {
                write!(f, "too many fields: expected {expected}, got {got}")
            }
            CsvErrorKind::InvalidUtf8 { message } => {
                write!(f, "invalid UTF-8: {message}")
            }
        }
    }
}

impl std::error::Error for CsvError {}

/// Specific kinds of CSV errors.
#[derive(Debug, Clone)]
pub enum CsvErrorKind {
    /// Unexpected end of input.
    UnexpectedEof {
        /// What was expected at this point.
        expected: &'static str,
    },
    /// Invalid value.
    InvalidValue {
        /// Error message.
        message: String,
    },
    /// Unsupported type for CSV format.
    UnsupportedType {
        /// Name of the unsupported type.
        type_name: &'static str,
    },
    /// Too few fields in the CSV row.
    TooFewFields {
        /// Expected number of fields.
        expected: usize,
        /// Actual number of fields.
        got: usize,
    },
    /// Too many fields in the CSV row.
    TooManyFields {
        /// Expected number of fields.
        expected: usize,
        /// Actual number of fields.
        got: usize,
    },
    /// Invalid UTF-8 in input.
    InvalidUtf8 {
        /// The UTF-8 error details.
        message: String,
    },
}

impl From<CsvErrorKind> for CsvError {
    fn from(kind: CsvErrorKind) -> Self {
        Self::new(kind)
    }
}
