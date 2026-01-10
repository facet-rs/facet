//! Error types for YAML serialization and deserialization.

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt::{self, Display};

use facet_reflect::ReflectError;

// Re-export Span from facet-reflect for consistency across format crates
pub use facet_reflect::Span;

/// Helper functions for creating Span from saphyr-parser types
pub(crate) trait SpanExt {
    /// Create a span from a saphyr-parser Span
    fn from_saphyr_span(span: &saphyr_parser::Span) -> Span {
        let start = span.start.index();
        let end = span.end.index();
        Span::new(start, end.saturating_sub(start))
    }
}

impl SpanExt for Span {}

/// Error type for YAML operations.
#[derive(Debug)]
pub struct YamlError {
    /// The specific kind of error
    pub kind: YamlErrorKind,
    /// Source span where the error occurred
    pub span: Option<Span>,
    /// The source input (for diagnostics)
    pub source_code: Option<String>,
}

impl Display for YamlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.kind)
    }
}

impl core::error::Error for YamlError {}

impl YamlError {
    /// Create a new error with span information
    pub fn new(kind: YamlErrorKind, span: Span) -> Self {
        YamlError {
            kind,
            span: Some(span),
            source_code: None,
        }
    }

    /// Create an error without span information
    pub fn without_span(kind: YamlErrorKind) -> Self {
        YamlError {
            kind,
            span: None,
            source_code: None,
        }
    }

    /// Attach source code for rich diagnostics
    pub fn with_source(mut self, source: &str) -> Self {
        self.source_code = Some(source.to_string());
        self
    }
}

/// Specific error kinds for YAML operations
#[derive(Debug)]
pub enum YamlErrorKind {
    /// YAML parser error
    Parse(String),
    /// Unexpected YAML event
    UnexpectedEvent {
        /// The event that was found
        got: String,
        /// What was expected instead
        expected: &'static str,
    },
    /// Unexpected end of input
    UnexpectedEof {
        /// What was expected before EOF
        expected: &'static str,
    },
    /// Type mismatch
    TypeMismatch {
        /// The expected type
        expected: &'static str,
        /// The actual type found
        got: &'static str,
    },
    /// Unknown field in struct
    UnknownField {
        /// The unknown field name
        field: String,
        /// List of valid field names
        expected: Vec<&'static str>,
        /// Suggested field name (if similar to an expected field)
        suggestion: Option<&'static str>,
    },
    /// Missing required field
    MissingField {
        /// The name of the missing field
        field: &'static str,
    },
    /// Invalid value for type
    InvalidValue {
        /// Description of why the value is invalid
        message: String,
    },
    /// Reflection error from facet-reflect
    Reflect(ReflectError),
    /// Number out of range
    NumberOutOfRange {
        /// The numeric value that was out of range
        value: String,
        /// The target type that couldn't hold the value
        target_type: &'static str,
    },
    /// Duplicate key in mapping
    DuplicateKey {
        /// The key that appeared more than once
        key: String,
    },
    /// Invalid UTF-8 in string
    InvalidUtf8(core::str::Utf8Error),
    /// Solver error (for flattened types)
    Solver(String),
    /// Unsupported YAML feature
    Unsupported(String),
    /// IO error during serialization
    Io(String),
}

impl Display for YamlErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            YamlErrorKind::Parse(e) => write!(f, "YAML parse error: {e}"),
            YamlErrorKind::UnexpectedEvent { got, expected } => {
                write!(f, "unexpected YAML event: got {got}, expected {expected}")
            }
            YamlErrorKind::UnexpectedEof { expected } => {
                write!(f, "unexpected end of input, expected {expected}")
            }
            YamlErrorKind::TypeMismatch { expected, got } => {
                write!(f, "type mismatch: expected {expected}, got {got}")
            }
            YamlErrorKind::UnknownField {
                field, expected, ..
            } => {
                write!(f, "unknown field `{field}`, expected one of: {expected:?}")
            }
            YamlErrorKind::MissingField { field } => {
                write!(f, "missing required field `{field}`")
            }
            YamlErrorKind::InvalidValue { message } => {
                write!(f, "invalid value: {message}")
            }
            YamlErrorKind::Reflect(e) => write!(f, "reflection error: {e}"),
            YamlErrorKind::NumberOutOfRange { value, target_type } => {
                write!(f, "number `{value}` out of range for {target_type}")
            }
            YamlErrorKind::DuplicateKey { key } => {
                write!(f, "duplicate key `{key}`")
            }
            YamlErrorKind::InvalidUtf8(e) => write!(f, "invalid UTF-8 sequence: {e}"),
            YamlErrorKind::Solver(msg) => write!(f, "solver error: {msg}"),
            YamlErrorKind::Unsupported(msg) => write!(f, "unsupported: {msg}"),
            YamlErrorKind::Io(msg) => write!(f, "IO error: {msg}"),
        }
    }
}

impl YamlErrorKind {
    /// Get an error code for this kind of error.
    pub fn code(&self) -> &'static str {
        match self {
            YamlErrorKind::Parse(_) => "yaml::parse",
            YamlErrorKind::UnexpectedEvent { .. } => "yaml::unexpected_event",
            YamlErrorKind::UnexpectedEof { .. } => "yaml::unexpected_eof",
            YamlErrorKind::TypeMismatch { .. } => "yaml::type_mismatch",
            YamlErrorKind::UnknownField { .. } => "yaml::unknown_field",
            YamlErrorKind::MissingField { .. } => "yaml::missing_field",
            YamlErrorKind::InvalidValue { .. } => "yaml::invalid_value",
            YamlErrorKind::Reflect(_) => "yaml::reflect",
            YamlErrorKind::NumberOutOfRange { .. } => "yaml::number_out_of_range",
            YamlErrorKind::DuplicateKey { .. } => "yaml::duplicate_key",
            YamlErrorKind::InvalidUtf8(_) => "yaml::invalid_utf8",
            YamlErrorKind::Solver(_) => "yaml::solver",
            YamlErrorKind::Unsupported(_) => "yaml::unsupported",
            YamlErrorKind::Io(_) => "yaml::io",
        }
    }

    /// Get a label for diagnostic display
    pub fn label(&self) -> String {
        match self {
            YamlErrorKind::Parse(_) => "parse error here".to_string(),
            YamlErrorKind::UnexpectedEvent { got, .. } => format!("unexpected {got}"),
            YamlErrorKind::UnexpectedEof { expected } => format!("expected {expected}"),
            YamlErrorKind::TypeMismatch { expected, got } => {
                format!("expected {expected}, got {got}")
            }
            YamlErrorKind::UnknownField { field, .. } => format!("unknown field `{field}`"),
            YamlErrorKind::MissingField { field } => format!("missing `{field}`"),
            YamlErrorKind::InvalidValue { message } => message.clone(),
            YamlErrorKind::Reflect(e) => format!("{e}"),
            YamlErrorKind::NumberOutOfRange { target_type, .. } => {
                format!("out of range for {target_type}")
            }
            YamlErrorKind::DuplicateKey { key } => format!("duplicate key `{key}`"),
            YamlErrorKind::InvalidUtf8(_) => "invalid UTF-8".to_string(),
            YamlErrorKind::Solver(msg) => msg.clone(),
            YamlErrorKind::Unsupported(msg) => msg.clone(),
            YamlErrorKind::Io(msg) => msg.clone(),
        }
    }
}

impl From<ReflectError> for YamlError {
    fn from(e: ReflectError) -> Self {
        YamlError::without_span(YamlErrorKind::Reflect(e))
    }
}

impl From<ReflectError> for YamlErrorKind {
    fn from(e: ReflectError) -> Self {
        YamlErrorKind::Reflect(e)
    }
}
