extern crate alloc;

use alloc::string::String;
use core::fmt;
use facet_path::Path;
use facet_reflect::{ReflectError, Span};

/// Error produced by the format deserializer.
#[derive(Debug)]
pub enum DeserializeError {
    /// Error emitted by the format-specific parser.
    // FIXME: change to be structured again
    Parser(String),
    /// Reflection error from Partial operations.
    Reflect {
        /// The underlying reflection error.
        error: ReflectError,
        /// Source span where the error occurred (if available).
        span: Option<Span>,
        /// Path through the type structure where the error occurred.
        path: Option<Path>,
    },
    /// Type mismatch during deserialization.
    TypeMismatch {
        /// The expected type or token.
        expected: &'static str,
        /// The actual type or token that was encountered.
        got: String,
        /// Source span where the mismatch occurred (if available).
        span: Option<Span>,
        /// Path through the type structure where the error occurred.
        path: Option<Path>,
    },
    /// Unsupported type or operation.
    Unsupported(String),
    /// Unknown field encountered when deny_unknown_fields is set.
    UnknownField {
        /// The unknown field name.
        field: String,
        /// Source span where the unknown field was found (if available).
        span: Option<Span>,
        /// Path through the type structure where the error occurred.
        path: Option<Path>,
    },
    /// Cannot borrow string from input (e.g., escaped string into &str).
    CannotBorrow {
        /// Description of why borrowing failed.
        message: String,
    },
    /// Required field missing from input.
    MissingField {
        /// The field that is missing.
        field: &'static str,
        /// The type that contains the field.
        type_name: &'static str,
        /// Source span where the struct was being parsed (if available).
        span: Option<Span>,
        /// Path through the type structure where the error occurred.
        path: Option<Path>,
    },
    /// Field validation failed.
    #[cfg(feature = "validate")]
    Validation {
        /// The field that failed validation.
        field: &'static str,
        /// The validation error message.
        message: String,
        /// Source span where the invalid value was found.
        span: Option<Span>,
        /// Path through the type structure where the error occurred.
        path: Option<Path>,
    },
    /// Unexpected end of input.
    UnexpectedEof {
        /// What was expected before EOF.
        expected: &'static str,
    },
}

impl fmt::Display for DeserializeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeserializeError::Parser(msg) => write!(f, "{msg}"),
            DeserializeError::Reflect { error, .. } => write!(f, "{error}"),
            DeserializeError::TypeMismatch { expected, got, .. } => {
                write!(f, "type mismatch: expected {expected}, got {got}")
            }
            DeserializeError::Unsupported(msg) => write!(f, "unsupported: {msg}"),
            DeserializeError::UnknownField { field, .. } => write!(f, "unknown field: {field}"),
            DeserializeError::CannotBorrow { message } => write!(f, "{message}"),
            DeserializeError::MissingField {
                field, type_name, ..
            } => {
                write!(f, "missing field `{field}` in type `{type_name}`")
            }
            #[cfg(feature = "validate")]
            DeserializeError::Validation { field, message, .. } => {
                write!(f, "validation failed for field `{field}`: {message}")
            }
            DeserializeError::UnexpectedEof { expected } => {
                write!(f, "unexpected end of input, expected {expected}")
            }
        }
    }
}

impl std::error::Error for DeserializeError {}

impl From<ReflectError> for DeserializeError {
    fn from(error: ReflectError) -> Self {
        DeserializeError::Reflect {
            error,
            span: None,
            path: None,
        }
    }
}

impl DeserializeError {
    /// Create a Parser error from any Debug type.
    #[inline]
    pub fn parser<E: fmt::Debug>(err: E) -> Self {
        DeserializeError::Parser(format!("{err:?}"))
    }

    /// Create a Reflect error without span or path information.
    #[inline]
    pub const fn reflect(error: ReflectError) -> Self {
        DeserializeError::Reflect {
            error,
            span: None,
            path: None,
        }
    }

    /// Create a Reflect error with span information.
    #[inline]
    pub const fn reflect_with_span(error: ReflectError, span: Span) -> Self {
        DeserializeError::Reflect {
            error,
            span: Some(span),
            path: None,
        }
    }

    /// Create a Reflect error with span and path information.
    #[inline]
    pub const fn reflect_with_context(error: ReflectError, span: Option<Span>, path: Path) -> Self {
        DeserializeError::Reflect {
            error,
            span,
            path: Some(path),
        }
    }

    /// Get the path where the error occurred, if available.
    pub const fn path(&self) -> Option<&Path> {
        match self {
            DeserializeError::Reflect { path, .. } => path.as_ref(),
            DeserializeError::TypeMismatch { path, .. } => path.as_ref(),
            DeserializeError::UnknownField { path, .. } => path.as_ref(),
            DeserializeError::MissingField { path, .. } => path.as_ref(),
            _ => None,
        }
    }

    /// Add path information to an error (consumes and returns the modified error).
    pub fn with_path(self, new_path: Path) -> Self {
        match self {
            DeserializeError::Reflect { error, span, .. } => DeserializeError::Reflect {
                error,
                span,
                path: Some(new_path),
            },
            DeserializeError::TypeMismatch {
                expected,
                got,
                span,
                ..
            } => DeserializeError::TypeMismatch {
                expected,
                got,
                span,
                path: Some(new_path),
            },
            DeserializeError::UnknownField { field, span, .. } => DeserializeError::UnknownField {
                field,
                span,
                path: Some(new_path),
            },
            DeserializeError::MissingField {
                field,
                type_name,
                span,
                ..
            } => DeserializeError::MissingField {
                field,
                type_name,
                span,
                path: Some(new_path),
            },
            // Other variants don't have path fields
            other => other,
        }
    }
}
