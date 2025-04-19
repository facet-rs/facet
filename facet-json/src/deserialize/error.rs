#[cfg(feature = "alloc")]
use alloc::format;
#[cfg(feature = "alloc")]
use alloc::string::String;

#[cfg(feature = "rich-diagnostics")]
use ariadne::{Color, Config, IndexType, Label, Report, ReportKind, Source};
#[cfg(feature = "rich-diagnostics")]
use owo_colors::OwoColorize;

/// A JSON parse error, with context. Never would've guessed huh.
pub struct JsonParseErrorWithContext<'input> {
    #[cfg_attr(not(feature = "rich-diagnostics"), allow(dead_code))]
    input: &'input [u8],
    pos: usize,
    /// The specific error that occurred while parsing the JSON.
    pub kind: JsonErrorKind,
    path: String,
}

impl<'input> JsonParseErrorWithContext<'input> {
    /// Creates a new `JsonParseErrorWithContext`.
    ///
    /// # Arguments
    ///
    /// * `kind` - The kind of JSON error encountered.
    /// * `input` - The original input being parsed.
    /// * `pos` - The position in the input where the error occurred.
    pub fn new(kind: JsonErrorKind, input: &'input [u8], pos: usize, path: String) -> Self {
        Self {
            input,
            pos,
            kind,
            path,
        }
    }

    /// Returns a human-readable error message for this JSON error.
    pub fn message(&self) -> String {
        match &self.kind {
            JsonErrorKind::UnexpectedEof(msg) => format!("Unexpected end of file: {}", msg),
            JsonErrorKind::MissingField(fld) => format!("Missing required field: {}", fld),
            JsonErrorKind::UnexpectedCharacter(c) => format!("Unexpected character: '{}'", c),
            JsonErrorKind::NumberOutOfRange(n) => format!("Number out of range: {}", n),
            JsonErrorKind::StringAsNumber(s) => format!("Expected a string but got number: {}", s),
            JsonErrorKind::UnknownField(f) => format!("Unknown field: {}", f),
            JsonErrorKind::InvalidUtf8(e) => format!("Invalid UTF-8 encoding: {}", e),
        }
    }
}

/// An error kind for JSON parsing.
#[derive(Debug)]
pub enum JsonErrorKind {
    /// The input ended unexpectedly while parsing JSON.
    UnexpectedEof(&'static str),
    /// A required struct field was missing at the end of JSON input.
    MissingField(&'static str),
    /// An unexpected character was encountered in the input.
    UnexpectedCharacter(char),
    /// A number is out of range.
    NumberOutOfRange(f64),
    /// An unexpected String was encountered in the input.
    StringAsNumber(String),
    /// An unexpected field name was encountered in the input.
    UnknownField(String),
    /// A string that could not be built into valid UTF-8 Unicode
    InvalidUtf8(String),
}

#[cfg(not(feature = "rich-diagnostics"))]
impl core::fmt::Display for JsonParseErrorWithContext<'_> {
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
impl core::fmt::Display for JsonParseErrorWithContext<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let Ok(input_str) = core::str::from_utf8(self.input) else {
            return write!(f, "(JSON input was invalid UTF-8)");
        };

        let source_id = "json";

        let (span_start, span_end) = match &self.kind {
            JsonErrorKind::StringAsNumber(s) => (self.pos - s.len(), self.pos),
            JsonErrorKind::UnknownField(f) => (self.pos - f.len() - 1, self.pos - 1),
            _ => {
                let span_end = if self.pos < self.input.len() {
                    self.pos + 1
                } else {
                    self.input.len()
                };
                (self.pos, span_end)
            }
        };

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

impl core::fmt::Debug for JsonParseErrorWithContext<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Display::fmt(self, f)
    }
}

impl core::error::Error for JsonParseErrorWithContext<'_> {}
