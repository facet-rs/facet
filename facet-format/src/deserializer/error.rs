extern crate alloc;

use alloc::borrow::Cow;
use core::fmt;
use facet_core::Shape;
use facet_path::Path;
use facet_reflect::{ReflectError, Span};

/// Error produced by the format deserializer.
///
/// This struct contains span and path information at the top level,
/// with a `kind` field describing the specific error.
#[derive(Debug)]
pub struct DeserializeError {
    /// Source span where the error occurred (if available).
    pub span: Option<Span>,
    /// Path through the type structure where the error occurred.
    pub path: Option<Path>,
    /// The specific kind of error.
    pub kind: DeserializeErrorKind,
}

/// Specific kinds of deserialization errors.
///
/// Uses `Cow<'static, str>` to avoid allocations when possible while still
/// supporting owned strings when needed (e.g., field names from input).
#[derive(Debug)]
#[non_exhaustive]
pub enum DeserializeErrorKind {
    // ============================================================
    // Parser/Syntax errors
    // ============================================================
    /// Syntax error in the input.
    Syntax {
        /// Description of the syntax error.
        message: Cow<'static, str>,
    },

    /// Unexpected end of input.
    UnexpectedEof {
        /// What was expected before EOF.
        expected: &'static str,
    },

    /// Unexpected token.
    UnexpectedToken {
        /// The token that was found.
        got: Cow<'static, str>,
        /// What was expected instead.
        expected: &'static str,
    },

    /// Invalid UTF-8 in input.
    InvalidUtf8 {
        /// Up to 16 bytes of context around the invalid sequence.
        context: [u8; 16],
        /// Number of valid bytes in context (0-16).
        context_len: u8,
    },

    // ============================================================
    // Type/Schema errors
    // ============================================================
    /// Type mismatch during deserialization.
    TypeMismatch {
        /// The expected shape/type.
        expected: &'static Shape,
        /// Description of what was found.
        got: Cow<'static, str>,
    },

    /// Type mismatch with string descriptions (when Shape not available).
    TypeMismatchStr {
        /// Description of expected type.
        expected: &'static str,
        /// Description of what was found.
        got: Cow<'static, str>,
    },

    /// Unknown field encountered.
    UnknownField {
        /// The unknown field name.
        field: Cow<'static, str>,
        /// Optional suggestion for a similar field.
        suggestion: Option<&'static str>,
    },

    /// Missing required field.
    MissingField {
        /// The field that is missing.
        field: &'static str,
        /// The type that contains the field.
        type_name: &'static str,
    },

    /// Duplicate field in input.
    DuplicateField {
        /// The field that appeared more than once.
        field: Cow<'static, str>,
        /// Span of the first occurrence.
        first_span: Option<Span>,
    },

    // ============================================================
    // Value errors
    // ============================================================
    /// Number out of range for target type.
    NumberOutOfRange {
        /// The numeric value as a string.
        value: Cow<'static, str>,
        /// The target type that couldn't hold the value.
        target_type: &'static str,
    },

    /// Invalid value for type.
    InvalidValue {
        /// Description of why the value is invalid.
        message: Cow<'static, str>,
    },

    /// Cannot borrow string from input.
    CannotBorrow {
        /// Description of why borrowing failed.
        reason: &'static str,
    },

    // ============================================================
    // Reflection errors
    // ============================================================
    /// Reflection error from facet-reflect.
    Reflect(ReflectError),

    // ============================================================
    // Misc
    // ============================================================
    /// Unsupported type or operation.
    Unsupported {
        /// Description of what is unsupported.
        message: Cow<'static, str>,
    },

    /// I/O error (for streaming deserialization).
    Io {
        /// Description of the I/O error.
        message: Cow<'static, str>,
    },

    /// Solver error (for flattened types).
    Solver {
        /// Description of the solver error.
        message: Cow<'static, str>,
    },

    /// Validation error.
    #[cfg(feature = "validate")]
    Validation {
        /// The field that failed validation.
        field: &'static str,
        /// The validation error message.
        message: Cow<'static, str>,
    },
}

impl fmt::Display for DeserializeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.kind)?;
        if let Some(ref path) = self.path {
            write!(f, " at {path:?}")?;
        }
        Ok(())
    }
}

impl fmt::Display for DeserializeErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeserializeErrorKind::Syntax { message } => write!(f, "{message}"),
            DeserializeErrorKind::UnexpectedEof { expected } => {
                write!(f, "unexpected end of input, expected {expected}")
            }
            DeserializeErrorKind::UnexpectedToken { got, expected } => {
                write!(f, "unexpected token: got {got}, expected {expected}")
            }
            DeserializeErrorKind::InvalidUtf8 {
                context,
                context_len,
            } => {
                let len = (*context_len as usize).min(16);
                if len > 0 {
                    write!(f, "invalid UTF-8 near: {:?}", &context[..len])
                } else {
                    write!(f, "invalid UTF-8")
                }
            }
            DeserializeErrorKind::TypeMismatch { expected, got } => {
                write!(f, "type mismatch: expected {expected}, got {got}")
            }
            DeserializeErrorKind::TypeMismatchStr { expected, got } => {
                write!(f, "type mismatch: expected {expected}, got {got}")
            }
            DeserializeErrorKind::UnknownField { field, suggestion } => {
                write!(f, "unknown field `{field}`")?;
                if let Some(s) = suggestion {
                    write!(f, " (did you mean `{s}`?)")?;
                }
                Ok(())
            }
            DeserializeErrorKind::MissingField { field, type_name } => {
                write!(f, "missing field `{field}` in type `{type_name}`")
            }
            DeserializeErrorKind::DuplicateField { field, .. } => {
                write!(f, "duplicate field `{field}`")
            }
            DeserializeErrorKind::NumberOutOfRange { value, target_type } => {
                write!(f, "number `{value}` out of range for {target_type}")
            }
            DeserializeErrorKind::InvalidValue { message } => {
                write!(f, "invalid value: {message}")
            }
            DeserializeErrorKind::CannotBorrow { reason } => write!(f, "{reason}"),
            DeserializeErrorKind::Reflect(e) => write!(f, "{e}"),
            DeserializeErrorKind::Unsupported { message } => write!(f, "unsupported: {message}"),
            DeserializeErrorKind::Io { message } => write!(f, "I/O error: {message}"),
            DeserializeErrorKind::Solver { message } => write!(f, "solver error: {message}"),
            #[cfg(feature = "validate")]
            DeserializeErrorKind::Validation { field, message } => {
                write!(f, "validation failed for field `{field}`: {message}")
            }
        }
    }
}

impl std::error::Error for DeserializeError {}

impl DeserializeError {
    /// Create a new error with just a kind.
    #[inline]
    pub const fn new(kind: DeserializeErrorKind) -> Self {
        DeserializeError {
            span: None,
            path: None,
            kind,
        }
    }

    /// Create a new error with span information.
    #[inline]
    pub const fn with_span(kind: DeserializeErrorKind, span: Span) -> Self {
        DeserializeError {
            span: Some(span),
            path: None,
            kind,
        }
    }

    /// Create a new error with span and path information.
    #[inline]
    pub const fn with_context(kind: DeserializeErrorKind, span: Option<Span>, path: Path) -> Self {
        DeserializeError {
            span,
            path: Some(path),
            kind,
        }
    }

    /// Create a parser/syntax error from any Debug type.
    #[inline]
    pub fn parser<E: fmt::Debug>(err: E) -> Self {
        DeserializeError::new(DeserializeErrorKind::Syntax {
            message: Cow::Owned(alloc::format!("{err:?}")),
        })
    }

    /// Create a parser/syntax error from any Display type.
    #[inline]
    pub fn parser_display<E: fmt::Display>(err: E) -> Self {
        DeserializeError::new(DeserializeErrorKind::Syntax {
            message: Cow::Owned(alloc::format!("{err}")),
        })
    }

    /// Add span information to this error.
    #[inline]
    pub fn set_span(mut self, span: Span) -> Self {
        self.span = Some(span);
        self
    }

    /// Add path information to this error.
    #[inline]
    pub fn set_path(mut self, path: Path) -> Self {
        self.path = Some(path);
        self
    }

    /// Get the path where the error occurred, if available.
    #[inline]
    pub const fn path(&self) -> Option<&Path> {
        self.path.as_ref()
    }

    /// Get the span where the error occurred, if available.
    #[inline]
    pub const fn span(&self) -> Option<&Span> {
        self.span.as_ref()
    }

    /// Add path information to an error (consumes and returns the modified error).
    #[inline]
    pub fn with_path(mut self, new_path: Path) -> Self {
        self.path = Some(new_path);
        self
    }

    // ================================================================
    // Convenience constructors
    // ================================================================

    /// Create a type mismatch error with Shape.
    #[inline]
    pub fn type_mismatch(expected: &'static Shape, got: impl Into<Cow<'static, str>>) -> Self {
        DeserializeError::new(DeserializeErrorKind::TypeMismatch {
            expected,
            got: got.into(),
        })
    }

    /// Create a type mismatch error with string descriptions.
    #[inline]
    pub fn type_mismatch_str(expected: &'static str, got: impl Into<Cow<'static, str>>) -> Self {
        DeserializeError::new(DeserializeErrorKind::TypeMismatchStr {
            expected,
            got: got.into(),
        })
    }

    /// Create a missing field error.
    #[inline]
    pub const fn missing_field(field: &'static str, type_name: &'static str) -> Self {
        DeserializeError::new(DeserializeErrorKind::MissingField { field, type_name })
    }

    /// Create an unknown field error.
    #[inline]
    pub fn unknown_field(field: impl Into<Cow<'static, str>>) -> Self {
        DeserializeError::new(DeserializeErrorKind::UnknownField {
            field: field.into(),
            suggestion: None,
        })
    }

    /// Create an unexpected EOF error.
    #[inline]
    pub const fn unexpected_eof(expected: &'static str) -> Self {
        DeserializeError::new(DeserializeErrorKind::UnexpectedEof { expected })
    }

    /// Create an unsupported error.
    #[inline]
    pub fn unsupported(message: impl Into<Cow<'static, str>>) -> Self {
        DeserializeError::new(DeserializeErrorKind::Unsupported {
            message: message.into(),
        })
    }

    /// Create a cannot borrow error.
    #[inline]
    pub const fn cannot_borrow(reason: &'static str) -> Self {
        DeserializeError::new(DeserializeErrorKind::CannotBorrow { reason })
    }

    /// Create an invalid UTF-8 error with context bytes.
    #[inline]
    pub fn invalid_utf8(context_bytes: &[u8]) -> Self {
        let mut context = [0u8; 16];
        let len = context_bytes.len().min(16);
        context[..len].copy_from_slice(&context_bytes[..len]);
        DeserializeError::new(DeserializeErrorKind::InvalidUtf8 {
            context,
            context_len: len as u8,
        })
    }

    /// Create an invalid UTF-8 error without context.
    #[inline]
    pub const fn invalid_utf8_no_context() -> Self {
        DeserializeError::new(DeserializeErrorKind::InvalidUtf8 {
            context: [0u8; 16],
            context_len: 0,
        })
    }

    /// Create a syntax error.
    #[inline]
    pub fn syntax(message: impl Into<Cow<'static, str>>) -> Self {
        DeserializeError::new(DeserializeErrorKind::Syntax {
            message: message.into(),
        })
    }

    /// Create an invalid value error.
    #[inline]
    pub fn invalid_value(message: impl Into<Cow<'static, str>>) -> Self {
        DeserializeError::new(DeserializeErrorKind::InvalidValue {
            message: message.into(),
        })
    }

    /// Create a reflection error.
    #[inline]
    pub const fn reflect(error: ReflectError) -> Self {
        DeserializeError::new(DeserializeErrorKind::Reflect(error))
    }

    /// Check if this is an Unsupported error.
    #[inline]
    pub const fn is_unsupported(&self) -> bool {
        matches!(self.kind, DeserializeErrorKind::Unsupported { .. })
    }
}

impl From<ReflectError> for DeserializeError {
    fn from(err: ReflectError) -> Self {
        DeserializeError::new(DeserializeErrorKind::Reflect(err))
    }
}
