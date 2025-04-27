#[cfg(feature = "rich-diagnostics")]
use alloc::format;
#[cfg(feature = "alloc")]
use alloc::string::String;

#[cfg(feature = "rich-diagnostics")]
use ariadne::{Color, Config, IndexType, Label, Report, ReportKind, Source};
use facet_reflect::ReflectError;
use owo_colors::OwoColorize;

use super::{Token, TokenErrorKind, tokenizer::Span};
use facet_core::Def;
use facet_core::Shape;

/// A JSON parse error, with context. Never would've guessed huh.
pub struct JsonError<'input> {
    /// The input associated with the error.
    pub input: alloc::borrow::Cow<'input, [u8]>,

    /// Where the error occured
    pub span: Span,

    /// Where we were in the struct when the error occured
    pub path: String,

    /// The specific error that occurred while parsing the JSON.
    pub kind: JsonErrorKind,
}

impl<'input> JsonError<'input> {
    /// Creates a new `JsonParseErrorWithContext`.
    ///
    /// # Arguments
    ///
    /// * `kind` - The kind of JSON error encountered.
    /// * `input` - The original input being parsed.
    /// * `pos` - The position in the input where the error occurred.
    pub fn new(kind: JsonErrorKind, input: &'input [u8], span: Span, path: String) -> Self {
        Self {
            input: alloc::borrow::Cow::Borrowed(input),
            span,
            kind,
            path,
        }
    }

    /// Returns a wrapper type that displays a human-readable error message for this JSON error.
    pub fn message(&self) -> JsonErrorMessage<'_> {
        JsonErrorMessage(self)
    }
}

/// A wrapper type for displaying JSON error messages
pub struct JsonErrorMessage<'a>(&'a JsonError<'a>);

impl core::fmt::Display for JsonErrorMessage<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match &self.0.kind {
            JsonErrorKind::UnexpectedEof(msg) => write!(f, "Unexpected end of file: {}", msg.red()),
            JsonErrorKind::MissingField(fld) => write!(f, "Missing required field: {}", fld.red()),
            JsonErrorKind::UnexpectedToken { got, wanted } => {
                write!(
                    f,
                    "Unexpected token: got {}, wanted {}",
                    got.red(),
                    wanted.green()
                )
            }
            JsonErrorKind::NumberOutOfRange(n) => {
                write!(f, "Number out of range: {}", n.red())
            }
            JsonErrorKind::StringAsNumber(s) => {
                write!(f, "Expected a string but got number: {}", s.red())
            }
            JsonErrorKind::UnknownField { field_name, shape } => {
                write!(
                    f,
                    "Unknown field: {} for shape {}",
                    field_name.red(),
                    shape.yellow()
                )
            }
            JsonErrorKind::InvalidUtf8(e) => write!(f, "Invalid UTF-8 encoding: {}", e.red()),
            JsonErrorKind::ReflectError(e) => write!(f, "{e}"),
            JsonErrorKind::SyntaxError(e) => write!(f, "{e}"),
            JsonErrorKind::Unimplemented(s) => {
                write!(f, "Feature not yet implemented: {}", s.yellow())
            }
            JsonErrorKind::UnsupportedType { got, wanted } => {
                write!(
                    f,
                    "Unsupported type: got {}, wanted {}",
                    got.red(),
                    wanted.green()
                )
            }
            JsonErrorKind::NoSuchVariant { name, enum_shape } => match enum_shape.def {
                Def::Enum(ed) => {
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

                    write!(f, "]")
                }
                _ => {
                    write!(
                        f,
                        "Enum variant not found: {} in enum {}. No variants available (not an enum)",
                        name.red(),
                        enum_shape.yellow()
                    )
                }
            },
        }
    }
}

/// An error kind for JSON parsing.
#[derive(Debug, PartialEq, Clone)]
pub enum JsonErrorKind {
    /// The input ended unexpectedly while parsing JSON.
    UnexpectedEof(&'static str),
    /// A required struct field was missing at the end of JSON input.
    MissingField(&'static str),
    /// An unexpected token was encountered in the input.
    UnexpectedToken {
        /// The hero we got
        got: Token,

        /// The hero we wanted
        wanted: &'static str,
    },
    /// A number is out of range.
    NumberOutOfRange(f64),
    /// An unexpected String was encountered in the input.
    StringAsNumber(String),
    /// An unexpected field name was encountered in the input.
    UnknownField {
        /// The name of the field that was not recognized
        field_name: String,
        /// The shape definition where the unknown field was encountered
        shape: &'static Shape,
    },
    /// A string that could not be built into valid UTF-8 Unicode
    InvalidUtf8(String),
    /// An error occurred while reflecting a type.
    ReflectError(ReflectError),
    /// An error occurred while parsing a token.
    SyntaxError(TokenErrorKind),
    /// Some feature is not yet implemented (under development).
    Unimplemented(&'static str),
    /// An unsupported type was encountered.
    UnsupportedType {
        /// The shape we got
        got: &'static Shape,

        /// The shape we wanted
        wanted: &'static str,
    },
    /// An enum variant name that doesn't exist in the enum definition.
    NoSuchVariant {
        /// The name of the variant that was not found
        name: String,

        /// The enum shape definition where the variant was looked up
        enum_shape: &'static Shape,
    },
}

impl From<ReflectError> for JsonErrorKind {
    fn from(err: ReflectError) -> Self {
        JsonErrorKind::ReflectError(err)
    }
}

#[cfg(not(feature = "rich-diagnostics"))]
impl core::fmt::Display for JsonError<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "{} at byte {} in path {}",
            self.message(),
            self.span.start(),
            self.path
        )
    }
}

#[cfg(feature = "rich-diagnostics")]
impl core::fmt::Display for JsonError<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let Ok(input_str) = core::str::from_utf8(&self.input[..]) else {
            return write!(f, "(JSON input was invalid UTF-8)");
        };

        let source_id = "json";
        let span_start = self.span.start();
        let span_end = self.span.end();

        let mut report = Report::build(ReportKind::Error, (source_id, span_start..span_end))
            .with_message(format!("Error at {}", self.path.yellow()))
            .with_config(Config::new().with_index_type(IndexType::Byte));

        let label = Label::new((source_id, span_start..span_end))
            .with_message(self.message())
            .with_color(Color::Red);

        report = report.with_label(label);

        let source = Source::from(input_str);

        let mut writer = Vec::new();
        let cache = (source_id, &source);

        if report.finish().write(cache, &mut writer).is_err() {
            return write!(f, "Error formatting with ariadne");
        }

        if let Ok(output) = String::from_utf8(writer) {
            write!(f, "{}", output)
        } else {
            write!(f, "Error converting ariadne output to string")
        }
    }
}

impl core::fmt::Debug for JsonError<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Display::fmt(self, f)
    }
}

impl core::error::Error for JsonError<'_> {}
