#[cfg(feature = "alloc")]
use alloc::format;
#[cfg(feature = "alloc")]
use alloc::string::String;

#[cfg(feature = "rich-diagnostics")]
use ariadne::{Color, Config, IndexType, Label, Report, ReportKind, Source};
use facet_reflect::ReflectError;
#[cfg(feature = "rich-diagnostics")]
use owo_colors::OwoColorize;

use super::{Token, TokenErrorKind, tokenizer::Span};

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

    /// Returns a human-readable error message for this JSON error.
    pub fn message(&self) -> String {
        match &self.kind {
            JsonErrorKind::UnexpectedEof(msg) => format!("Unexpected end of file: {}", msg),
            JsonErrorKind::MissingField(fld) => format!("Missing required field: {}", fld),
            JsonErrorKind::UnexpectedToken { got, wanted } => {
                format!("Unexpected token: got '{}', wanted '{}'", got, wanted)
            }
            JsonErrorKind::NumberOutOfRange(n) => format!("Number out of range: {}", n),
            JsonErrorKind::StringAsNumber(s) => format!("Expected a string but got number: {}", s),
            JsonErrorKind::UnknownField(f) => format!("Unknown field: {}", f),
            JsonErrorKind::InvalidUtf8(e) => format!("Invalid UTF-8 encoding: {}", e),
            JsonErrorKind::ReflectError(e) => format!("Error while reflecting type: {}", e),
            JsonErrorKind::SyntaxError(e) => format!("Syntax error: {}", e),
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
    UnknownField(String),
    /// A string that could not be built into valid UTF-8 Unicode
    InvalidUtf8(String),
    /// An error occurred while reflecting a type.
    ReflectError(ReflectError),
    /// An error occurred while parsing a token.
    SyntaxError(TokenErrorKind),
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
            self.pos,
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
