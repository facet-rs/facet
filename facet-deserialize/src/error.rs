#[cfg(feature = "rich-diagnostics")]
use ariadne::{Color, Config, IndexType, Label, Report, ReportKind, Source};

use alloc::string::String;

use facet_core::{Shape, Type, UserType};
use facet_reflect::{ReflectError, VariantError};
use owo_colors::OwoColorize;

use crate::debug::InputDebug;
use crate::{Cooked, Outcome, Span};

/// A JSON parse error, with context. Never would've guessed huh.
#[derive(Debug)]
pub struct DeserError<'input, 'shape, C = Cooked> {
    /// The input associated with the error.
    pub input: alloc::borrow::Cow<'input, [u8]>,

    /// Where the error occured
    pub span: Span<C>,

    /// The specific error that occurred while parsing the JSON.
    pub kind: DeserErrorKind<'shape>,

    /// The source identifier for error reporting
    pub source_id: &'static str,
}

impl<'input, 'shape, C> DeserError<'input, 'shape, C> {
    /// Converts the error into an owned error.
    pub fn into_owned(self) -> DeserError<'static, 'shape, C> {
        DeserError {
            input: self.input.into_owned().into(),
            span: self.span,
            kind: self.kind,
            source_id: self.source_id,
        }
    }

    /// Sets the span of this error
    pub fn with_span<D>(self, span: Span<D>) -> DeserError<'input, 'shape, D> {
        DeserError {
            input: self.input,
            span,
            kind: self.kind,
            source_id: self.source_id,
        }
    }
}

/// An error kind for JSON parsing.
#[derive(Debug, PartialEq, Clone)]
pub enum DeserErrorKind<'shape> {
    /// An unexpected byte was encountered in the input.
    UnexpectedByte {
        /// The byte that was found.
        got: u8,
        /// The expected value as a string description.
        wanted: &'static str,
    },

    /// An unexpected character was encountered in the input.
    UnexpectedChar {
        /// The character that was found.
        got: char,
        /// The expected value as a string description.
        wanted: &'static str,
    },

    /// An unexpected outcome was encountered in the input.
    UnexpectedOutcome {
        /// The outcome that was found.
        got: Outcome<'static>,
        /// The expected value as a string description.
        wanted: &'static str,
    },

    /// The input ended unexpectedly while parsing JSON.
    UnexpectedEof {
        /// The expected value as a string description.
        wanted: &'static str,
    },

    /// Indicates a value was expected to follow an element in the input.
    MissingValue {
        /// Describes what type of value was expected.
        expected: &'static str,
        /// The element that requires the missing value.
        field: String,
    },

    /// A required struct field was missing at the end of JSON input.
    MissingField(&'static str),

    /// A number is out of range.
    NumberOutOfRange(f64),

    /// An unexpected String was encountered in the input.
    StringAsNumber(String),

    /// An unexpected field name was encountered in the input.
    UnknownField {
        /// The name of the field that was not recognized
        field_name: String,

        /// The shape definition where the unknown field was encountered
        shape: &'shape Shape<'shape>,
    },

    /// A string that could not be built into valid UTF-8 Unicode
    InvalidUtf8(String),

    /// An error occurred while reflecting a type.
    ReflectError(ReflectError<'shape>),

    /// Some feature is not yet implemented (under development).
    Unimplemented(&'static str),

    /// An unsupported type was encountered.
    UnsupportedType {
        /// The shape we got
        got: &'shape Shape<'shape>,

        /// The shape we wanted
        wanted: &'static str,
    },

    /// An enum variant name that doesn't exist in the enum definition.
    NoSuchVariant {
        /// The name of the variant that was not found
        name: String,

        /// The enum shape definition where the variant was looked up
        enum_shape: &'shape Shape<'shape>,
    },

    /// An error occurred when reflecting an enum variant (index) from a user type.
    VariantError(VariantError),
}

impl<'input, 'shape, C> DeserError<'input, 'shape, C> {
    /// Creates a new deser error, preserving input and location context for accurate reporting.
    pub fn new<I>(
        kind: DeserErrorKind<'shape>,
        input: &'input I,
        span: Span<C>,
        source_id: &'static str,
    ) -> Self
    where
        I: ?Sized + 'input + InputDebug,
    {
        Self {
            input: input.as_cow(),
            span,
            kind,
            source_id,
        }
    }

    /// Constructs a reflection-related deser error, keeping contextual information intact.
    pub(crate) fn new_reflect<I>(
        e: ReflectError<'shape>,
        input: &'input I,
        span: Span<C>,
        source_id: &'static str,
    ) -> Self
    where
        I: ?Sized + 'input + InputDebug,
    {
        DeserError::new(DeserErrorKind::ReflectError(e), input, span, source_id)
    }

    /// Sets the source ID for this error
    pub fn with_source_id(mut self, source_id: &'static str) -> Self {
        self.source_id = source_id;
        self
    }

    /// Provides a human-friendly message wrapper to improve error readability.
    pub fn message(&self) -> DeserErrorMessage<'_, '_, C> {
        DeserErrorMessage(self)
    }
}

/// A wrapper type for displaying deser error messages
pub struct DeserErrorMessage<'input, 'shape, C = Cooked>(&'input DeserError<'input, 'shape, C>);

impl core::fmt::Display for DeserErrorMessage<'_, '_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match &self.0.kind {
            DeserErrorKind::UnexpectedByte { got, wanted } => write!(
                f,
                "Unexpected byte: got 0x{:02X}, wanted {}",
                got.red(),
                wanted.yellow()
            ),
            DeserErrorKind::UnexpectedChar { got, wanted } => write!(
                f,
                "Unexpected character: got '{}', wanted {}",
                got.red(),
                wanted.yellow()
            ),
            DeserErrorKind::UnexpectedOutcome { got, wanted } => {
                write!(f, "Unexpected {}, wanted {}", got.red(), wanted.yellow())
            }
            DeserErrorKind::UnexpectedEof { wanted } => {
                write!(f, "Unexpected end of file: wanted {}", wanted.red())
            }
            DeserErrorKind::MissingValue { expected, field } => {
                write!(f, "Missing {} for {}", expected.red(), field.yellow())
            }
            DeserErrorKind::MissingField(fld) => write!(f, "Missing required field: {}", fld.red()),
            DeserErrorKind::NumberOutOfRange(n) => {
                write!(f, "Number out of range: {}", n.red())
            }
            DeserErrorKind::StringAsNumber(s) => {
                write!(f, "Expected a string but got number: {}", s.red())
            }
            DeserErrorKind::UnknownField { field_name, shape } => {
                write!(
                    f,
                    "Unknown field: {} for shape {}",
                    field_name.red(),
                    shape.yellow()
                )
            }
            DeserErrorKind::InvalidUtf8(e) => write!(f, "Invalid UTF-8 encoding: {}", e.red()),
            DeserErrorKind::ReflectError(e) => write!(f, "{e}"),
            DeserErrorKind::Unimplemented(s) => {
                write!(f, "Feature not yet implemented: {}", s.yellow())
            }
            DeserErrorKind::UnsupportedType { got, wanted } => {
                write!(
                    f,
                    "Unsupported type: got {}, wanted {}",
                    got.red(),
                    wanted.green()
                )
            }
            DeserErrorKind::NoSuchVariant { name, enum_shape } => {
                if let Type::User(UserType::Enum(ed)) = enum_shape.ty {
                    write!(
                        f,
                        "Enum variant not found: {} in enum {}. Available variants: [",
                        name.red(),
                        enum_shape.yellow()
                    )?;

                    let mut first = true;
                    for variant in ed.variants.iter() {
                        if !first {
                            write!(f, ", ")?;
                        }
                        write!(f, "{}", variant.name.green())?;
                        first = false;
                    }

                    write!(f, "]")?;
                    Ok(())
                } else {
                    write!(
                        f,
                        "Enum variant not found: {} in non-enum type {}",
                        name.red(),
                        enum_shape.yellow()
                    )?;
                    Ok(())
                }
            }
            DeserErrorKind::VariantError(e) => {
                write!(f, "Variant error: {e}")
            }
        }
    }
}

#[cfg(not(feature = "rich-diagnostics"))]
impl core::fmt::Display for DeserError<'_, '_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{} at byte {}", self.message(), self.span.start(),)
    }
}

#[cfg(feature = "rich-diagnostics")]
impl core::fmt::Display for DeserError<'_, '_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Try to convert input to utf8 for source display, otherwise fallback to error
        let Ok(orig_input_str) = core::str::from_utf8(&self.input[..]) else {
            return write!(f, "(JSON input was invalid UTF-8)");
        };

        let source_id = self.source_id;
        let mut span_start = self.span.start();
        let mut span_end = self.span.end();
        use alloc::borrow::Cow;
        let mut input_str: Cow<'_, str> = Cow::Borrowed(orig_input_str);

        // --- Context-sensitive truncation logic ---
        // When the error occurs very far into a huge (often one-line) input,
        // such as minified JSON, it's annoying to display hundreds or thousands of
        // preceding and trailing characters. Instead, we seek to trim the displayed
        // "source" to just enough around the offending line/location, but only if
        // we can do this cleanly.
        //
        // Our approach:
        // - Find the full line that `span_start` is within, using memchr for newlines before and after.
        // - Only proceed if both `span_start` and `span_end` are within this line (i.e., error doesn't span lines).
        // - If there are more than 180 characters before/after the span on this line, truncate to show
        //   "...<80 chars>SPANTEXT<80 chars>..." and adjust the display offsets to ensure ariadne points
        //   to the correct span inside the trimmed display.
        //
        // Rationale: this avoids a sea of whitespace for extremely long lines (common in compact JSON).

        {
            // Find the line bounds containing span_start
            let bytes = self.input.as_ref();
            let line_start = bytes[..span_start]
                .iter()
                .rposition(|&b| b == b'\n')
                .map(|pos| pos + 1)
                .unwrap_or(0);
            let line_end = bytes[span_start..]
                .iter()
                .position(|&b| b == b'\n')
                .map(|pos| span_start + pos)
                .unwrap_or(bytes.len());

            // Check if span fits within one line
            if span_end <= line_end {
                // How much context do we have before and after the span in this line?
                let before_chars = span_start - line_start;
                let after_chars = line_end.saturating_sub(span_end);

                // Only trim if context is long enough
                if before_chars > 180 || after_chars > 180 {
                    let trim_left = if before_chars > 180 {
                        before_chars - 80
                    } else {
                        0
                    };
                    let trim_right = if after_chars > 180 {
                        after_chars - 80
                    } else {
                        0
                    };

                    let new_start = line_start + trim_left;
                    let new_end = line_end - trim_right;

                    let truncated = &orig_input_str[new_start..new_end];

                    let left_ellipsis = if trim_left > 0 { "…" } else { "" };
                    let right_ellipsis = if trim_right > 0 { "…" } else { "" };

                    let mut buf = String::with_capacity(
                        left_ellipsis.len() + truncated.len() + right_ellipsis.len(),
                    );
                    buf.push_str(left_ellipsis);
                    buf.push_str(truncated);
                    buf.push_str(right_ellipsis);

                    // Adjust span offsets to align with the trimmed string
                    span_start = span_start - new_start + left_ellipsis.len();
                    span_end = span_end - new_start + left_ellipsis.len();

                    input_str = Cow::Owned(buf);
                    // Done!
                }
            }
            // If the span goes across lines or we cannot cleanly trim, display the full input as fallback
        }

        let mut report = Report::build(ReportKind::Error, (source_id, span_start..span_end))
            .with_config(Config::new().with_index_type(IndexType::Byte));

        let label = Label::new((source_id, span_start..span_end))
            .with_message(self.message())
            .with_color(Color::Red);

        report = report.with_label(label);

        let source = Source::from(input_str);

        struct FmtWriter<'a, 'b: 'a> {
            f: &'a mut core::fmt::Formatter<'b>,
            error: Option<core::fmt::Error>,
        }

        impl core::fmt::Write for FmtWriter<'_, '_> {
            fn write_str(&mut self, s: &str) -> core::fmt::Result {
                if self.error.is_some() {
                    // Already failed, do nothing
                    return Err(core::fmt::Error);
                }
                if let Err(e) = self.f.write_str(s) {
                    self.error = Some(e);
                    Err(core::fmt::Error)
                } else {
                    Ok(())
                }
            }
        }

        struct IoWriter<'a, 'b: 'a> {
            inner: FmtWriter<'a, 'b>,
        }

        impl std::io::Write for IoWriter<'_, '_> {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                match core::str::from_utf8(buf) {
                    Ok(s) => match core::fmt::Write::write_str(&mut self.inner, s) {
                        Ok(()) => Ok(buf.len()),
                        Err(_) => Err(std::io::ErrorKind::Other.into()),
                    },
                    Err(_) => Err(std::io::ErrorKind::InvalidData.into()),
                }
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }

        let cache = (source_id, &source);

        let fmt_writer = FmtWriter { f, error: None };
        let mut io_writer = IoWriter { inner: fmt_writer };

        if report.finish().write(cache, &mut io_writer).is_err() {
            return write!(f, "Error formatting with ariadne");
        }

        // Check if our adapter ran into a formatting error
        if io_writer.inner.error.is_some() {
            return write!(f, "Error writing ariadne output to fmt::Formatter");
        }

        Ok(())
    }
}

impl core::error::Error for DeserError<'_, '_> {}
