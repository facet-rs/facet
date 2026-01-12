//! Error types for JSON deserialization.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt::{self, Display};

use facet_reflect::{ReflectError, Span};

use crate::adapter::{AdapterError, AdapterErrorKind};
use crate::scanner::ScanErrorKind;

/// Error type for JSON deserialization.
#[derive(Debug)]
pub struct JsonError {
    /// The specific kind of error
    pub kind: JsonErrorKind,
    /// Source span where the error occurred
    pub span: Option<Span>,
    /// The source input (for diagnostics)
    pub source_code: Option<String>,
}

impl Display for JsonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.kind)
    }
}

impl std::error::Error for JsonError {}

impl JsonError {
    /// Create a new error with span information
    pub const fn new(kind: JsonErrorKind, span: Span) -> Self {
        JsonError {
            kind,
            span: Some(span),
            source_code: None,
        }
    }

    /// Create an error without span information
    pub const fn without_span(kind: JsonErrorKind) -> Self {
        JsonError {
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

/// Specific error kinds for JSON deserialization
#[derive(Debug)]
pub enum JsonErrorKind {
    /// Scanner/adapter error
    Scan(ScanErrorKind),
    /// Scanner error with type context (what type was being parsed)
    ScanWithContext {
        /// The underlying scan error
        error: ScanErrorKind,
        /// The type that was being parsed
        expected_type: &'static str,
    },
    /// Unexpected token
    UnexpectedToken {
        /// The token that was found
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
        /// Span of the object start (opening brace)
        object_start: Option<Span>,
        /// Span of the object end (closing brace)
        object_end: Option<Span>,
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
    /// Duplicate key in object
    DuplicateKey {
        /// The key that appeared more than once
        key: String,
    },
    /// Invalid UTF-8 in string
    InvalidUtf8,
    /// Solver error (for flattened types)
    Solver(String),
    /// I/O error (for streaming deserialization)
    Io(String),
}

impl Display for JsonErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JsonErrorKind::Scan(e) => write!(f, "{e:?}"),
            JsonErrorKind::ScanWithContext {
                error,
                expected_type,
            } => {
                write!(f, "{error:?} (while parsing {expected_type})")
            }
            JsonErrorKind::UnexpectedToken { got, expected } => {
                write!(f, "unexpected token: got {got}, expected {expected}")
            }
            JsonErrorKind::UnexpectedEof { expected } => {
                write!(f, "unexpected end of input, expected {expected}")
            }
            JsonErrorKind::TypeMismatch { expected, got } => {
                write!(f, "type mismatch: expected {expected}, got {got}")
            }
            JsonErrorKind::UnknownField {
                field,
                expected,
                suggestion,
            } => {
                write!(f, "unknown field `{field}`, expected one of: {expected:?}")?;
                if let Some(suggested) = suggestion {
                    write!(f, " (did you mean `{suggested}`?)")?;
                }
                Ok(())
            }
            JsonErrorKind::MissingField { field, .. } => {
                write!(f, "missing required field `{field}`")
            }
            JsonErrorKind::InvalidValue { message } => {
                write!(f, "invalid value: {message}")
            }
            JsonErrorKind::Reflect(e) => write!(f, "reflection error: {e}"),
            JsonErrorKind::NumberOutOfRange { value, target_type } => {
                write!(f, "number `{value}` out of range for {target_type}")
            }
            JsonErrorKind::DuplicateKey { key } => {
                write!(f, "duplicate key `{key}`")
            }
            JsonErrorKind::InvalidUtf8 => write!(f, "invalid UTF-8 sequence"),
            JsonErrorKind::Solver(msg) => write!(f, "solver error: {msg}"),
            JsonErrorKind::Io(msg) => write!(f, "I/O error: {msg}"),
        }
    }
}

impl JsonErrorKind {
    /// Get an error code for this kind of error.
    pub const fn code(&self) -> &'static str {
        match self {
            JsonErrorKind::Scan(_) => "json::scan",
            JsonErrorKind::ScanWithContext { .. } => "json::scan",
            JsonErrorKind::UnexpectedToken { .. } => "json::unexpected_token",
            JsonErrorKind::UnexpectedEof { .. } => "json::unexpected_eof",
            JsonErrorKind::TypeMismatch { .. } => "json::type_mismatch",
            JsonErrorKind::UnknownField { .. } => "json::unknown_field",
            JsonErrorKind::MissingField { .. } => "json::missing_field",
            JsonErrorKind::InvalidValue { .. } => "json::invalid_value",
            JsonErrorKind::Reflect(_) => "json::reflect",
            JsonErrorKind::NumberOutOfRange { .. } => "json::number_out_of_range",
            JsonErrorKind::DuplicateKey { .. } => "json::duplicate_key",
            JsonErrorKind::InvalidUtf8 => "json::invalid_utf8",
            JsonErrorKind::Solver(_) => "json::solver",
            JsonErrorKind::Io(_) => "json::io",
        }
    }

    /// Get a label describing where/what the error points to.
    pub fn label(&self) -> String {
        match self {
            JsonErrorKind::Scan(e) => match e {
                ScanErrorKind::UnexpectedChar(c) => format!("unexpected '{c}'"),
                ScanErrorKind::UnexpectedEof(ctx) => format!("unexpected end of input {ctx}"),
                ScanErrorKind::InvalidUtf8 => "invalid UTF-8 here".into(),
            },
            JsonErrorKind::ScanWithContext {
                error,
                expected_type,
            } => match error {
                ScanErrorKind::UnexpectedChar(c) => {
                    format!("unexpected '{c}', expected {expected_type}")
                }
                ScanErrorKind::UnexpectedEof(_) => {
                    format!("unexpected end of input, expected {expected_type}")
                }
                ScanErrorKind::InvalidUtf8 => "invalid UTF-8 here".into(),
            },
            JsonErrorKind::UnexpectedToken { got, expected } => {
                format!("expected {expected}, got '{got}'")
            }
            JsonErrorKind::UnexpectedEof { expected } => format!("expected {expected}"),
            JsonErrorKind::TypeMismatch { expected, got } => {
                format!("expected {expected}, got {got}")
            }
            JsonErrorKind::UnknownField {
                field, suggestion, ..
            } => {
                if let Some(suggested) = suggestion {
                    format!("unknown field '{field}' - did you mean '{suggested}'?")
                } else {
                    format!("unknown field '{field}'")
                }
            }
            JsonErrorKind::MissingField { field, .. } => format!("missing field '{field}'"),
            JsonErrorKind::InvalidValue { .. } => "invalid value".into(),
            JsonErrorKind::Reflect(_) => "reflection error".into(),
            JsonErrorKind::NumberOutOfRange { target_type, .. } => {
                format!("out of range for {target_type}")
            }
            JsonErrorKind::DuplicateKey { key } => format!("duplicate key '{key}'"),
            JsonErrorKind::InvalidUtf8 => "invalid UTF-8".into(),
            JsonErrorKind::Solver(_) => "solver error".into(),
            JsonErrorKind::Io(_) => "I/O error".into(),
        }
    }
}

impl From<AdapterError> for JsonError {
    fn from(err: AdapterError) -> Self {
        let kind = match err.kind {
            AdapterErrorKind::Scan(scan_err) => JsonErrorKind::Scan(scan_err),
            AdapterErrorKind::NeedMore => JsonErrorKind::UnexpectedEof {
                expected: "more data",
            },
        };
        JsonError {
            kind,
            span: Some(err.span),
            source_code: None,
        }
    }
}

impl From<ReflectError> for JsonError {
    fn from(err: ReflectError) -> Self {
        JsonError {
            kind: JsonErrorKind::Reflect(err),
            span: None,
            source_code: None,
        }
    }
}

/// Result type for JSON deserialization
#[allow(dead_code)]
pub type Result<T> = core::result::Result<T, JsonError>;
