//! Error types for TOML deserialization and serialization.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt::{self, Display};

use facet_reflect::{ReflectError, Span};

/// Error type for TOML operations.
#[derive(Debug)]
pub struct TomlError {
    /// The specific kind of error
    pub kind: TomlErrorKind,
    /// Source span where the error occurred
    pub span: Option<Span>,
    /// The source input (for diagnostics)
    pub source_code: Option<String>,
}

impl Display for TomlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.kind)
    }
}

impl std::error::Error for TomlError {}

impl miette::Diagnostic for TomlError {
    fn code<'a>(&'a self) -> Option<Box<dyn Display + 'a>> {
        Some(Box::new(self.kind.code()))
    }

    fn source_code(&self) -> Option<&dyn miette::SourceCode> {
        self.source_code
            .as_ref()
            .map(|s| s as &dyn miette::SourceCode)
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = miette::LabeledSpan> + '_>> {
        // Handle MissingField with multiple spans
        if let TomlErrorKind::MissingField {
            field,
            table_start,
            table_end,
        } = &self.kind
        {
            let mut labels = Vec::new();
            if let Some(start) = table_start {
                labels.push(miette::LabeledSpan::new(
                    Some("table started here".into()),
                    start.offset,
                    start.len,
                ));
            }
            if let Some(end) = table_end {
                labels.push(miette::LabeledSpan::new(
                    Some(format!("table ended without field `{field}`")),
                    end.offset,
                    end.len,
                ));
            }
            if labels.is_empty() {
                return None;
            }
            return Some(Box::new(labels.into_iter()));
        }

        // Default: single span with label
        let span = self.span?;
        Some(Box::new(core::iter::once(miette::LabeledSpan::new(
            Some(self.kind.label()),
            span.offset,
            span.len,
        ))))
    }
}

impl TomlError {
    /// Create a new error with span information
    pub fn new(kind: TomlErrorKind, span: Span) -> Self {
        TomlError {
            kind,
            span: Some(span),
            source_code: None,
        }
    }

    /// Create an error without span information
    pub fn without_span(kind: TomlErrorKind) -> Self {
        TomlError {
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

/// Specific error kinds for TOML operations
#[derive(Debug)]
pub enum TomlErrorKind {
    /// Parse error from toml_parser
    Parse(String),
    /// Unexpected value type
    UnexpectedType {
        /// What type was expected
        expected: &'static str,
        /// What type was found
        got: &'static str,
    },
    /// Unexpected end of input
    UnexpectedEof {
        /// What was expected before EOF
        expected: &'static str,
    },
    /// Unknown field in table
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
        /// Span of the table start (opening header)
        table_start: Option<Span>,
        /// Span of the table end
        table_end: Option<Span>,
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
    /// Duplicate key in table
    DuplicateKey {
        /// The key that appeared more than once
        key: String,
    },
    /// Invalid UTF-8 in string
    InvalidUtf8,
    /// Solver error (for flattened types)
    Solver(String),
    /// Serialization error
    Serialize(String),
}

impl Display for TomlErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TomlErrorKind::Parse(msg) => write!(f, "parse error: {msg}"),
            TomlErrorKind::UnexpectedType { expected, got } => {
                write!(f, "type mismatch: expected {expected}, got {got}")
            }
            TomlErrorKind::UnexpectedEof { expected } => {
                write!(f, "unexpected end of input, expected {expected}")
            }
            TomlErrorKind::UnknownField {
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
            TomlErrorKind::MissingField { field, .. } => {
                write!(f, "missing required field `{field}`")
            }
            TomlErrorKind::InvalidValue { message } => {
                write!(f, "invalid value: {message}")
            }
            TomlErrorKind::Reflect(e) => write!(f, "reflection error: {e}"),
            TomlErrorKind::NumberOutOfRange { value, target_type } => {
                write!(f, "number `{value}` out of range for {target_type}")
            }
            TomlErrorKind::DuplicateKey { key } => {
                write!(f, "duplicate key `{key}`")
            }
            TomlErrorKind::InvalidUtf8 => write!(f, "invalid UTF-8 sequence"),
            TomlErrorKind::Solver(msg) => write!(f, "solver error: {msg}"),
            TomlErrorKind::Serialize(msg) => write!(f, "serialization error: {msg}"),
        }
    }
}

impl TomlErrorKind {
    /// Get an error code for this kind of error.
    pub fn code(&self) -> &'static str {
        match self {
            TomlErrorKind::Parse(_) => "toml::parse",
            TomlErrorKind::UnexpectedType { .. } => "toml::type_mismatch",
            TomlErrorKind::UnexpectedEof { .. } => "toml::unexpected_eof",
            TomlErrorKind::UnknownField { .. } => "toml::unknown_field",
            TomlErrorKind::MissingField { .. } => "toml::missing_field",
            TomlErrorKind::InvalidValue { .. } => "toml::invalid_value",
            TomlErrorKind::Reflect(_) => "toml::reflect",
            TomlErrorKind::NumberOutOfRange { .. } => "toml::number_out_of_range",
            TomlErrorKind::DuplicateKey { .. } => "toml::duplicate_key",
            TomlErrorKind::InvalidUtf8 => "toml::invalid_utf8",
            TomlErrorKind::Solver(_) => "toml::solver",
            TomlErrorKind::Serialize(_) => "toml::serialize",
        }
    }

    /// Get a label describing where/what the error points to.
    pub fn label(&self) -> String {
        match self {
            TomlErrorKind::Parse(msg) => format!("parse error: {msg}"),
            TomlErrorKind::UnexpectedType { expected, got } => {
                format!("expected {expected}, got {got}")
            }
            TomlErrorKind::UnexpectedEof { expected } => format!("expected {expected}"),
            TomlErrorKind::UnknownField {
                field, suggestion, ..
            } => {
                if let Some(suggested) = suggestion {
                    format!("unknown field '{field}' - did you mean '{suggested}'?")
                } else {
                    format!("unknown field '{field}'")
                }
            }
            TomlErrorKind::MissingField { field, .. } => format!("missing field '{field}'"),
            TomlErrorKind::InvalidValue { .. } => "invalid value".into(),
            TomlErrorKind::Reflect(_) => "reflection error".into(),
            TomlErrorKind::NumberOutOfRange { target_type, .. } => {
                format!("out of range for {target_type}")
            }
            TomlErrorKind::DuplicateKey { key } => format!("duplicate key '{key}'"),
            TomlErrorKind::InvalidUtf8 => "invalid UTF-8".into(),
            TomlErrorKind::Solver(_) => "solver error".into(),
            TomlErrorKind::Serialize(_) => "serialization error".into(),
        }
    }
}

impl From<ReflectError> for TomlError {
    fn from(err: ReflectError) -> Self {
        TomlError {
            kind: TomlErrorKind::Reflect(err),
            span: None,
            source_code: None,
        }
    }
}

/// Result type for TOML operations
#[allow(dead_code)]
pub type Result<T> = core::result::Result<T, TomlError>;
