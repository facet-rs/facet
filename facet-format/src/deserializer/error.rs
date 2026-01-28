use facet_core::Shape;
use facet_path::Path;
use facet_reflect::{AllocError, ReflectError, ReflectErrorKind, ShapeMismatchError, Span};
use std::borrow::Cow;
use std::cell::Cell;
use std::fmt;

thread_local! {
    /// Thread-local storage for the current span during deserialization.
    /// This is set by SpanGuard before calling Partial methods,
    /// allowing the From<ReflectError> impl to capture the span automatically.
    static CURRENT_SPAN: Cell<Option<Span>> = const { Cell::new(None) };
}

/// RAII guard that sets the current span for error reporting.
///
/// When dropped, restores the previous span value.
/// The `From<ReflectError>` impl will panic if no span is set.
pub struct SpanGuard {
    prev: Option<Span>,
}

impl SpanGuard {
    /// Create a new span guard, setting the current span.
    #[inline]
    pub fn new(span: Span) -> Self {
        let prev = CURRENT_SPAN.with(|cell| cell.replace(Some(span)));
        Self { prev }
    }
}

impl Drop for SpanGuard {
    fn drop(&mut self) {
        CURRENT_SPAN.with(|cell| cell.set(self.prev));
    }
}

/// Get the current span for error reporting.
/// Panics if no span is set (i.e., no SpanGuard is active).
#[inline]
fn current_span() -> Span {
    CURRENT_SPAN.with(|cell| {
        cell.get().expect(
            "current_span called without an active SpanGuard - this is a bug in the deserializer",
        )
    })
}

/// Error produced by a format parser (JSON, TOML, etc.).
///
/// Parse errors always have a span (location in the input) but never have a path
/// (location in the type structure) because parsers don't know about the target type.
///
/// When propagated through the deserializer, this is converted to a `DeserializeError`
/// which can add path information.
#[derive(Debug)]
pub struct ParseError {
    /// Source span where the error occurred.
    pub span: Span,

    /// The specific kind of error.
    pub kind: DeserializeErrorKind,
}

impl ParseError {
    /// Create a new parse error with the given span and kind.
    #[inline]
    pub const fn new(span: Span, kind: DeserializeErrorKind) -> Self {
        Self { span, kind }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at {:?}", self.kind, self.span)
    }
}

impl std::error::Error for ParseError {}

impl From<ParseError> for DeserializeError {
    fn from(e: ParseError) -> Self {
        DeserializeError {
            span: Some(e.span),
            path: None,
            kind: e.kind,
        }
    }
}

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
    // Parser-level errors (thrown by FormatParser implementations)
    // ============================================================
    //
    // These errors occur during lexing/parsing of the input format,
    // before we even try to map values to Rust types.
    /// Unexpected character encountered by the parser.
    ///
    /// **Level:** Parser (e.g., `JsonParser`)
    ///
    /// This happens when the parser encounters a character that doesn't
    /// fit the format's grammar at the current position.
    ///
    /// ```text
    /// {"name": @invalid}
    ///          ^
    ///          unexpected character '@', expected value
    /// ```
    UnexpectedChar {
        /// The character that was found.
        ch: char,
        /// What was expected instead (e.g., "value", "digit", "string").
        expected: &'static str,
    },

    /// Unexpected end of input.
    ///
    /// **Level:** Parser (e.g., `JsonParser`)
    ///
    /// The input ended before a complete value could be parsed.
    ///
    /// ```text
    /// {"name": "Alice
    ///                ^
    ///                unexpected EOF, expected closing quote
    /// ```
    UnexpectedEof {
        /// What was expected before EOF.
        expected: &'static str,
    },

    /// Invalid UTF-8 sequence in input.
    ///
    /// **Level:** Parser (e.g., `JsonParser`)
    ///
    /// The input contains bytes that don't form valid UTF-8.
    ///
    /// ```text
    /// {"name": "hello\xff world"}
    ///                 ^^^^
    ///                 invalid UTF-8 sequence
    /// ```
    InvalidUtf8 {
        /// Up to 16 bytes of context around the invalid sequence.
        context: [u8; 16],
        /// Number of valid bytes in context (0-16).
        context_len: u8,
    },

    // ============================================================
    // Deserializer-level errors (thrown by FormatDeserializer)
    // ============================================================
    //
    // These errors occur when mapping parsed tokens to Rust types.
    // The parser successfully produced tokens, but they don't match
    // what the deserializer expected for the target type.
    /// Unexpected token from parser.
    ///
    /// **Level:** Deserializer (`FormatDeserializer`)
    ///
    /// The parser produced a valid token, but it's not what the deserializer
    /// expected at this point given the target Rust type.
    ///
    /// ```text
    /// // Deserializing into Vec<i32>
    /// {"not": "an array"}
    /// ^
    /// unexpected token: got object, expected array
    /// ```
    ///
    /// **Not to be confused with:**
    /// - `UnexpectedChar`: parser-level, about invalid syntax
    /// - `TypeMismatch`: about shape expectations, not token types
    UnexpectedToken {
        /// The token that was found (e.g., "object", "string", "null").
        got: Cow<'static, str>,
        /// What was expected instead (e.g., "array", "number").
        expected: &'static str,
    },

    /// Type mismatch: expected a shape, got something else from the parser.
    ///
    /// **Level:** Deserializer (`FormatDeserializer`)
    ///
    /// We know the target Rust type (Shape), but the parser gave us
    /// something incompatible.
    ///
    /// ```text
    /// // Deserializing into struct User { age: u32 }
    /// {"age": "not a number"}
    ///         ^^^^^^^^^^^^^^
    ///         type mismatch: expected u32, got string
    /// ```
    TypeMismatch {
        /// The expected shape/type we were trying to deserialize into.
        expected: &'static Shape,
        /// Description of what we got from the parser.
        got: Cow<'static, str>,
    },

    /// Shape mismatch: expected one Rust type, but the code path requires another.
    ///
    /// **Level:** Deserializer (`FormatDeserializer`)
    ///
    /// This is an internal routing error - the deserializer was asked to
    /// deserialize into a type that doesn't match what the current code
    /// path expects. For example, calling enum deserialization on a struct.
    ///
    /// ```text
    /// // Internal error: deserialize_enum called but shape is a struct
    /// shape mismatch: expected enum, got struct User
    /// ```
    ///
    /// **Not to be confused with:**
    /// - `TypeMismatch`: about parser output vs expected type
    /// - `UnexpectedToken`: about token types from parser
    ShapeMismatch {
        /// The shape that was expected by this code path.
        expected: &'static Shape,
        /// The actual shape that was provided.
        got: &'static Shape,
    },

    /// Unknown field in struct.
    ///
    /// **Level:** Deserializer (`FormatDeserializer`)
    ///
    /// The input contains a field name that doesn't exist in the target struct
    /// and the struct doesn't allow unknown fields (no `#[facet(deny_unknown_fields)]`
    /// or similar).
    ///
    /// ```text
    /// // Deserializing into struct User { name: String }
    /// {"name": "Alice", "age": 30}
    ///                   ^^^^^
    ///                   unknown field `age`
    /// ```
    UnknownField {
        /// The unknown field name.
        field: Cow<'static, str>,
        /// Optional suggestion for a similar field (typo correction).
        suggestion: Option<&'static str>,
    },

    /// Unknown enum variant.
    ///
    /// **Level:** Deserializer (`FormatDeserializer`)
    ///
    /// The input specifies a variant name that doesn't exist in the target enum.
    ///
    /// ```text
    /// // Deserializing into enum Status { Active, Inactive }
    /// "Pending"
    /// ^^^^^^^^^
    /// unknown variant `Pending` for enum `Status`
    /// ```
    UnknownVariant {
        /// The unknown variant name from the input.
        variant: Cow<'static, str>,

        /// The enum type.
        enum_shape: &'static Shape,
    },

    /// No variant matched for untagged enum.
    ///
    /// **Level:** Deserializer (`FormatDeserializer`)
    ///
    /// For `#[facet(untagged)]` enums, we try each variant in order.
    /// This error means none of them matched the input.
    ///
    /// ```text
    /// // Deserializing into #[facet(untagged)] enum Value { Int(i32), Str(String) }
    /// [1, 2, 3]
    /// ^^^^^^^^^
    /// no matching variant for enum `Value` with array input
    /// ```
    NoMatchingVariant {
        /// The enum type.
        enum_shape: &'static Shape,
        /// What kind of input was provided (e.g., "array", "object", "string").
        input_kind: &'static str,
    },

    /// Missing required field.
    ///
    /// **Level:** Deserializer (`FormatDeserializer`)
    ///
    /// A struct field without a default value was not provided in the input.
    ///
    /// ```text
    /// // Deserializing into struct User { name: String, email: String }
    /// {"name": "Alice"}
    ///                 ^
    ///                 missing field `email` in type `User`
    /// ```
    MissingField {
        /// The field that is missing.
        field: &'static str,
        /// The type that contains the field.
        container_shape: &'static Shape,
    },

    /// Duplicate field in input.
    ///
    /// **Level:** Deserializer (`FormatDeserializer`)
    ///
    /// The same field appears multiple times in the input.
    ///
    /// ```text
    /// {"name": "Alice", "name": "Bob"}
    ///                   ^^^^^^
    ///                   duplicate field `name` (first occurrence at offset 1)
    /// ```
    DuplicateField {
        /// The field that appeared more than once.
        field: Cow<'static, str>,
        /// Span of the first occurrence (for better diagnostics).
        first_span: Option<Span>,
    },

    // ============================================================
    // Value errors
    // ============================================================
    /// Number out of range for target type.
    ///
    /// **Level:** Deserializer (`FormatDeserializer`)
    ///
    /// The input contains a valid number, but it doesn't fit in the target type.
    ///
    /// ```text
    /// // Deserializing into u8
    /// 256
    /// ^^^
    /// number `256` out of range for u8
    /// ```
    NumberOutOfRange {
        /// The numeric value as a string.
        value: Cow<'static, str>,
        /// The target type that couldn't hold the value.
        target_type: &'static str,
    },

    /// Invalid value for the target type.
    ///
    /// **Level:** Deserializer (`FormatDeserializer`)
    ///
    /// The value is syntactically valid but semantically wrong for the target type.
    /// Used for things like invalid enum discriminants, malformed UUIDs, etc.
    ///
    /// ```text
    /// // Deserializing into Uuid
    /// "not-a-valid-uuid"
    /// ^^^^^^^^^^^^^^^^^^
    /// invalid value: expected UUID format
    /// ```
    InvalidValue {
        /// Description of why the value is invalid.
        message: Cow<'static, str>,
    },

    /// Cannot borrow string from input.
    ///
    /// **Level:** Deserializer (`FormatDeserializer`)
    ///
    /// When deserializing into `&str` or `Cow<str>`, the string in the input
    /// required processing (e.g., escape sequences) and cannot be borrowed.
    ///
    /// ```text
    /// // Deserializing into &str
    /// "hello\nworld"
    /// ^^^^^^^^^^^^^^
    /// cannot borrow: string contains escape sequences
    /// ```
    CannotBorrow {
        /// Description of why borrowing failed.
        reason: &'static str,
    },

    // ============================================================
    // Reflection errors
    // ============================================================
    /// Error from the reflection system.
    ///
    /// **Level:** Deserializer (via `facet-reflect`)
    ///
    /// These errors come from `Partial` operations like field access,
    /// variant selection, or type building.
    ///
    /// Note: The path is stored at the `DeserializeError` level, not here.
    /// When converting from `ReflectError`, the path is extracted and stored
    /// in `DeserializeError.path`.
    Reflect {
        /// The specific kind of reflection error
        kind: ReflectErrorKind,

        /// What we were trying to do
        context: &'static str,
    },

    // ============================================================
    // Infrastructure errors
    // ============================================================
    /// Feature not implemented.
    ///
    /// **Level:** Deserializer or Parser
    ///
    /// The requested operation is not yet implemented. This is used for
    /// known gaps in functionality, not for invalid input.
    ///
    /// ```text
    /// // Trying to deserialize a type that's not yet supported
    /// unsupported: multi-element tuple variants in flatten not yet supported
    /// ```
    Unsupported {
        /// Description of what is unsupported.
        message: Cow<'static, str>,
    },

    /// I/O error during streaming deserialization.
    ///
    /// **Level:** Parser
    ///
    /// For parsers that read from streams, this wraps I/O errors.
    Io {
        /// Description of the I/O error.
        message: Cow<'static, str>,
    },

    /// Error from the flatten solver.
    ///
    /// **Level:** Deserializer (via `facet-solver`)
    ///
    /// When deserializing types with `#[facet(flatten)]`, the solver
    /// determines which fields go where. This error indicates solver failure.
    Solver {
        /// Description of the solver error.
        message: Cow<'static, str>,
    },

    /// Validation error.
    ///
    /// **Level:** Deserializer (post-deserialization)
    ///
    /// After successful deserialization, validation constraints failed.
    ///
    /// ```text
    /// // With #[facet(validate = "validate_age")]
    /// {"age": -5}
    ///         ^^
    ///         validation failed for field `age`: must be non-negative
    /// ```
    Validation {
        /// The field that failed validation.
        field: &'static str,

        /// The validation error message.
        message: Cow<'static, str>,
    },

    /// Internal error indicating a logic bug in facet-format or one of the crates
    /// that relies on it (facet-json,e tc.)
    Bug {
        /// What happened?
        error: Cow<'static, str>,

        /// What were we doing?
        context: &'static str,
    },

    /// Memory allocation failed.
    ///
    /// **Level:** Deserializer (internal)
    ///
    /// Failed to allocate memory for the partial value being built.
    /// This is rare but can happen with very large types or low memory.
    Alloc {
        /// The shape we tried to allocate.
        shape: &'static Shape,

        /// What operation was being attempted.
        operation: &'static str,
    },

    /// Shape mismatch when materializing a value.
    ///
    /// **Level:** Deserializer (internal)
    ///
    /// The shape of the built value doesn't match the target type.
    /// This indicates a bug in the deserializer logic.
    Materialize {
        /// The shape that was expected (the target type).
        expected: &'static Shape,

        /// The shape that was actually found.
        actual: &'static Shape,
    },

    /// Raw capture is not supported by the current parser.
    ///
    /// **Level:** Deserializer (`FormatDeserializer`)
    ///
    /// Types like `RawJson` require capturing the raw input without parsing it.
    /// This error occurs when attempting to deserialize such a type with a parser
    /// that doesn't support raw capture (e.g., streaming parsers without buffering).
    ///
    /// ```text
    /// // Deserializing RawJson in streaming mode
    /// raw capture not supported: type `RawJson` requires raw capture, but the
    /// parser does not support it (e.g., streaming mode without buffering)
    /// ```
    RawCaptureNotSupported {
        /// The type that requires raw capture.
        shape: &'static Shape,
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
            DeserializeErrorKind::UnexpectedChar { ch, expected } => {
                write!(f, "unexpected character {ch:?}, expected {expected}")
            }
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
            DeserializeErrorKind::ShapeMismatch { expected, got } => {
                write!(f, "shape mismatch: expected {expected}, got {got}")
            }
            DeserializeErrorKind::UnknownField { field, suggestion } => {
                write!(f, "unknown field `{field}`")?;
                if let Some(s) = suggestion {
                    write!(f, " (did you mean `{s}`?)")?;
                }
                Ok(())
            }
            DeserializeErrorKind::UnknownVariant {
                variant,
                enum_shape,
            } => {
                write!(f, "unknown variant `{variant}` for enum `{enum_shape}`")
            }
            DeserializeErrorKind::NoMatchingVariant {
                enum_shape,
                input_kind,
            } => {
                write!(
                    f,
                    "no matching variant found for enum `{enum_shape}` with {input_kind} input"
                )
            }
            DeserializeErrorKind::MissingField {
                field,
                container_shape,
            } => {
                write!(f, "missing field `{field}` in type `{container_shape}`")
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
            DeserializeErrorKind::Reflect { kind, context } => {
                if context.is_empty() {
                    write!(f, "{kind}")
                } else {
                    write!(f, "{kind} (while {context})")
                }
            }
            DeserializeErrorKind::Unsupported { message } => write!(f, "unsupported: {message}"),
            DeserializeErrorKind::Io { message } => write!(f, "I/O error: {message}"),
            DeserializeErrorKind::Solver { message } => write!(f, "solver error: {message}"),
            DeserializeErrorKind::Validation { field, message } => {
                write!(f, "validation failed for field `{field}`: {message}")
            }
            DeserializeErrorKind::Bug { error, context } => {
                write!(f, "internal error: {error} while {context}")
            }
            DeserializeErrorKind::Alloc { shape, operation } => {
                write!(f, "allocation failed for {shape}: {operation}")
            }
            DeserializeErrorKind::Materialize { expected, actual } => {
                write!(
                    f,
                    "shape mismatch when materializing: expected {expected}, got {actual}"
                )
            }
            DeserializeErrorKind::RawCaptureNotSupported { shape: type_name } => {
                write!(
                    f,
                    "raw capture not supported: type `{type_name}` requires raw capture, \
                     but the parser does not support it (e.g., streaming mode without buffering)"
                )
            }
        }
    }
}

impl std::error::Error for DeserializeError {}

impl From<ReflectError> for DeserializeError {
    fn from(e: ReflectError) -> Self {
        DeserializeError {
            span: Some(current_span()),
            path: Some(e.path),
            kind: DeserializeErrorKind::Reflect {
                kind: e.kind,
                context: "",
            },
        }
    }
}

impl From<AllocError> for DeserializeError {
    fn from(e: AllocError) -> Self {
        DeserializeError {
            span: None,
            path: None,
            kind: DeserializeErrorKind::Alloc {
                shape: e.shape,
                operation: e.operation,
            },
        }
    }
}

impl From<ShapeMismatchError> for DeserializeError {
    fn from(e: ShapeMismatchError) -> Self {
        DeserializeError {
            span: None,
            path: None,
            kind: DeserializeErrorKind::Materialize {
                expected: e.expected,
                actual: e.actual,
            },
        }
    }
}

impl DeserializeErrorKind {
    /// Attach a span to this error kind, producing a full DeserializeError.
    #[inline]
    pub const fn with_span(self, span: Span) -> DeserializeError {
        DeserializeError {
            span: Some(span),
            path: None,
            kind: self,
        }
    }

    // Note: there is no "without_span" method because you should always indicate
    // where an error happened. Hope this helps.
}

impl DeserializeError {
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
}

// ============================================================
// Pretty error rendering with ariadne
// ============================================================

#[cfg(feature = "ariadne")]
mod ariadne_impl {
    use super::*;
    use ariadne::{Color, Label, Report, ReportKind, Source};
    use std::io::Write;

    impl DeserializeError {
        /// Render this error as a pretty diagnostic using ariadne.
        ///
        /// # Arguments
        /// * `filename` - The filename to show in the diagnostic (e.g., "queries.styx")
        /// * `source` - The source text that was being parsed
        ///
        /// # Returns
        /// A string containing the formatted diagnostic with colors (ANSI codes).
        pub fn to_pretty(&self, filename: &str, source: &str) -> String {
            let mut buf = Vec::new();
            self.write_pretty(&mut buf, filename, source)
                .expect("writing to Vec<u8> should never fail");
            String::from_utf8(buf).expect("ariadne output should be valid UTF-8")
        }

        /// Write this error as a pretty diagnostic to a writer.
        ///
        /// # Arguments
        /// * `writer` - Where to write the diagnostic
        /// * `filename` - The filename to show in the diagnostic
        /// * `source` - The source text that was being parsed
        pub fn write_pretty<W: Write>(
            &self,
            writer: &mut W,
            filename: &str,
            source: &str,
        ) -> std::io::Result<()> {
            let (offset, len) = match self.span {
                Some(span) => (span.offset as usize, span.len as usize),
                None => (0, 0),
            };

            // Clamp to source bounds
            let offset = offset.min(source.len());
            let end = (offset + len).min(source.len());
            let range = offset..end.max(offset + 1).min(source.len());

            let message = self.kind.to_string();

            let mut report =
                Report::build(ReportKind::Error, (filename, range.clone())).with_message(&message);

            // Add the main label pointing to the error location
            let label = Label::new((filename, range))
                .with_message(&message)
                .with_color(Color::Red);
            report = report.with_label(label);

            // Add path information as a note if available
            if let Some(ref path) = self.path {
                report = report.with_note(format!("at path: {path}"));
            }

            report
                .finish()
                .write((filename, Source::from(source)), writer)
        }

        /// Print this error as a pretty diagnostic to stderr.
        ///
        /// # Arguments
        /// * `filename` - The filename to show in the diagnostic
        /// * `source` - The source text that was being parsed
        pub fn eprint(&self, filename: &str, source: &str) {
            let _ = self.write_pretty(&mut std::io::stderr(), filename, source);
        }
    }
}
