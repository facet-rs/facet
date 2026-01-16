//! Error types for Styx parsing.

use std::fmt;

use ariadne::{Color, Config, Label, Report, ReportKind, Source};
use facet_format::DeserializeError;
use styx_parse::Span;

/// Get ariadne config, respecting NO_COLOR env var.
fn ariadne_config() -> Config {
    let no_color = std::env::var("NO_COLOR").is_ok();
    if no_color {
        Config::default().with_color(false)
    } else {
        Config::default()
    }
}

/// Error that can occur during Styx parsing.
#[derive(Debug, Clone, PartialEq)]
pub struct StyxError {
    pub kind: StyxErrorKind,
    pub span: Option<Span>,
}

impl StyxError {
    pub fn new(kind: StyxErrorKind, span: Option<Span>) -> Self {
        Self { kind, span }
    }

    /// Render this error with ariadne.
    ///
    /// Returns a string containing the formatted error message with source context.
    pub fn render(&self, filename: &str, source: &str) -> String {
        let mut output = Vec::new();
        self.write_report(filename, source, &mut output);
        String::from_utf8(output).unwrap_or_else(|_| format!("{}", self))
    }

    /// Write the error report to a writer.
    pub fn write_report<W: std::io::Write>(&self, filename: &str, source: &str, writer: W) {
        let report = self.build_report(filename, source);
        let _ = report
            .with_config(ariadne_config())
            .finish()
            .write((filename, Source::from(source)), writer);
    }

    /// Build an ariadne report for this error.
    pub fn build_report<'a>(
        &self,
        filename: &'a str,
        _source: &str,
    ) -> ariadne::ReportBuilder<'static, (&'a str, std::ops::Range<usize>)> {
        let range = self
            .span
            .map(|s| s.start as usize..s.end as usize)
            .unwrap_or(0..1);

        match &self.kind {
            // diag[impl diagnostic.deser.invalid-value]
            StyxErrorKind::InvalidScalar { value, expected } => {
                Report::build(ReportKind::Error, filename, range.start)
                    .with_message(format!("invalid value '{}'", value))
                    .with_label(
                        Label::new((filename, range))
                            .with_message(format!("expected {}", expected))
                            .with_color(Color::Red),
                    )
            }

            // diag[impl diagnostic.deser.missing-field]
            StyxErrorKind::MissingField { name } => {
                Report::build(ReportKind::Error, filename, range.start)
                    .with_message(format!("missing required field '{}'", name))
                    .with_label(
                        Label::new((filename, range))
                            .with_message("in this object")
                            .with_color(Color::Red),
                    )
                    .with_help(format!("add the required field: {} <value>", name))
            }

            // diag[impl diagnostic.deser.unknown-field]
            StyxErrorKind::UnknownField { name } => {
                Report::build(ReportKind::Error, filename, range.start)
                    .with_message(format!("unknown field '{}'", name))
                    .with_label(
                        Label::new((filename, range))
                            .with_message("unknown field")
                            .with_color(Color::Red),
                    )
            }

            StyxErrorKind::UnexpectedToken { got, expected } => {
                Report::build(ReportKind::Error, filename, range.start)
                    .with_message(format!("unexpected token '{}'", got))
                    .with_label(
                        Label::new((filename, range))
                            .with_message(format!("expected {}", expected))
                            .with_color(Color::Red),
                    )
            }

            StyxErrorKind::UnexpectedEof { expected } => {
                Report::build(ReportKind::Error, filename, range.start)
                    .with_message("unexpected end of input")
                    .with_label(
                        Label::new((filename, range))
                            .with_message(format!("expected {}", expected))
                            .with_color(Color::Red),
                    )
            }

            StyxErrorKind::InvalidEscape { sequence } => {
                Report::build(ReportKind::Error, filename, range.start)
                    .with_message(format!("invalid escape sequence '{}'", sequence))
                    .with_label(
                        Label::new((filename, range))
                            .with_message("invalid escape")
                            .with_color(Color::Red),
                    )
                    .with_help("valid escapes are: \\\\, \\\", \\n, \\r, \\t, \\uXXXX, \\u{X...}")
            }
        }
    }
}

impl fmt::Display for StyxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.kind)?;
        if let Some(span) = &self.span {
            write!(f, " at offset {}", span.start)?;
        }
        Ok(())
    }
}

impl std::error::Error for StyxError {}

/// Kind of Styx error.
#[derive(Debug, Clone, PartialEq)]
pub enum StyxErrorKind {
    /// Unexpected token.
    UnexpectedToken { got: String, expected: &'static str },
    /// Unexpected end of input.
    UnexpectedEof { expected: &'static str },
    /// Invalid scalar value for target type.
    InvalidScalar {
        value: String,
        expected: &'static str,
    },
    /// Missing required field.
    MissingField { name: String },
    /// Unknown field.
    UnknownField { name: String },
    /// Invalid escape sequence.
    InvalidEscape { sequence: String },
}

impl fmt::Display for StyxErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StyxErrorKind::UnexpectedToken { got, expected } => {
                write!(f, "unexpected token '{}', expected {}", got, expected)
            }
            StyxErrorKind::UnexpectedEof { expected } => {
                write!(f, "unexpected end of input, expected {}", expected)
            }
            StyxErrorKind::InvalidScalar { value, expected } => {
                write!(f, "invalid value '{}', expected {}", value, expected)
            }
            StyxErrorKind::MissingField { name } => {
                write!(f, "missing required field '{}'", name)
            }
            StyxErrorKind::UnknownField { name } => {
                write!(f, "unknown field '{}'", name)
            }
            StyxErrorKind::InvalidEscape { sequence } => {
                write!(f, "invalid escape sequence '{}'", sequence)
            }
        }
    }
}

/// Convert a facet_reflect::Span to a Range<usize>.
#[allow(dead_code)]
fn reflect_span_to_range(span: &facet_reflect::Span) -> std::ops::Range<usize> {
    let start = span.offset;
    let end = start + span.len;
    start..end
}

/// Trait for rendering errors with ariadne diagnostics.
#[allow(dead_code)]
pub trait RenderError {
    /// Render this error with ariadne.
    ///
    /// Returns a string containing the formatted error message with source context.
    fn render(&self, filename: &str, source: &str) -> String;

    /// Write the error report to a writer.
    fn write_report<W: std::io::Write>(&self, filename: &str, source: &str, writer: W);
}

/// Rendering support for `DeserializeError<StyxError>`.
///
/// This allows rendering the full deserialize error (which may come from the parser
/// or from facet-format's deserializer) with ariadne diagnostics.
impl RenderError for DeserializeError<StyxError> {
    fn render(&self, filename: &str, source: &str) -> String {
        let mut output = Vec::new();
        self.write_report(filename, source, &mut output);
        String::from_utf8(output).unwrap_or_else(|_| format!("{}", self))
    }

    fn write_report<W: std::io::Write>(&self, filename: &str, source: &str, writer: W) {
        // IMPORTANT: Config must be applied BEFORE adding labels, because ariadne
        // applies filter_color when labels are added, not when the report is written.
        let report = build_deserialize_error_report(self, filename, source, ariadne_config());
        let _ = report
            .finish()
            .write((filename, Source::from(source)), writer);
    }
}

#[allow(dead_code)]
fn build_deserialize_error_report<'a>(
    err: &DeserializeError<StyxError>,
    filename: &'a str,
    source: &str,
    config: Config,
) -> ariadne::ReportBuilder<'static, (&'a str, std::ops::Range<usize>)> {
    match err {
        // Parser errors - delegate to StyxError's rendering
        // Note: StyxError::build_report doesn't take config, so we need to add it after
        DeserializeError::Parser(styx_err) => {
            styx_err.build_report(filename, source).with_config(config)
        }

        // Missing field from facet-format
        DeserializeError::MissingField {
            field,
            type_name,
            span,
            ..
        } => {
            let range = span
                .as_ref()
                .map(reflect_span_to_range)
                .unwrap_or(0..source.len().max(1));
            Report::build(ReportKind::Error, filename, range.start)
                .with_config(config)
                .with_message(format!("missing required field '{}'", field))
                .with_label(
                    Label::new((filename, range))
                        .with_message(format!("in {}", type_name))
                        .with_color(Color::Red),
                )
                .with_help(format!("add the required field: {} <value>", field))
        }

        // Unknown field from facet-format
        DeserializeError::UnknownField { field, span, .. } => {
            let range = span.as_ref().map(reflect_span_to_range).unwrap_or(0..1);
            Report::build(ReportKind::Error, filename, range.start)
                .with_config(config)
                .with_message(format!("unknown field '{}'", field))
                .with_label(
                    Label::new((filename, range))
                        .with_message("unknown field")
                        .with_color(Color::Red),
                )
        }

        // Type mismatch from facet-format
        DeserializeError::TypeMismatch {
            expected,
            got,
            span,
            ..
        } => {
            let range = span.as_ref().map(reflect_span_to_range).unwrap_or(0..1);
            Report::build(ReportKind::Error, filename, range.start)
                .with_config(config)
                .with_message(format!("type mismatch: expected {}", expected))
                .with_label(
                    Label::new((filename, range))
                        .with_message(format!("got {}", got))
                        .with_color(Color::Red),
                )
        }

        // Reflect errors from facet-format
        DeserializeError::Reflect { error, span, .. } => {
            let range = span.as_ref().map(reflect_span_to_range).unwrap_or(0..1);
            Report::build(ReportKind::Error, filename, range.start)
                .with_config(config)
                .with_message(format!("{}", error))
                .with_label(
                    Label::new((filename, range))
                        .with_message("error here")
                        .with_color(Color::Red),
                )
        }

        // Unexpected EOF
        DeserializeError::UnexpectedEof { expected } => {
            let range = source.len().saturating_sub(1)..source.len().max(1);
            Report::build(ReportKind::Error, filename, range.start)
                .with_config(config)
                .with_message("unexpected end of input")
                .with_label(
                    Label::new((filename, range))
                        .with_message(format!("expected {}", expected))
                        .with_color(Color::Red),
                )
        }

        // Unsupported operation
        DeserializeError::Unsupported(msg) => Report::build(ReportKind::Error, filename, 0)
            .with_config(config)
            .with_message(format!("unsupported: {}", msg)),

        // Cannot borrow
        DeserializeError::CannotBorrow { message } => Report::build(ReportKind::Error, filename, 0)
            .with_config(config)
            .with_message(message.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use facet::Facet;

    #[test]
    fn test_ariadne_no_color() {
        // Verify that ariadne respects our config
        let config = Config::default().with_color(false);

        let source = "test input";
        let report =
            Report::<(&str, std::ops::Range<usize>)>::build(ReportKind::Error, "test.styx", 0)
                .with_config(config)
                .with_message("test error")
                .with_label(
                    Label::new(("test.styx", 0..4))
                        .with_message("here")
                        .with_color(Color::Red),
                )
                .finish();

        let mut output = Vec::new();
        report
            .write(("test.styx", Source::from(source)), &mut output)
            .unwrap();
        let s = String::from_utf8(output).unwrap();

        // Check for ANSI escape codes
        assert!(
            !s.contains("\x1b["),
            "Output should not contain ANSI escape codes when color is disabled:\n{:?}",
            s
        );
    }

    #[test]
    fn test_ariadne_config_respects_no_color_env() {
        // Test that ariadne_config() returns correct config based on NO_COLOR
        let no_color = std::env::var("NO_COLOR").is_ok();
        eprintln!("NO_COLOR is set: {}", no_color);

        let config = ariadne_config();

        let source = "test input";
        let report =
            Report::<(&str, std::ops::Range<usize>)>::build(ReportKind::Error, "test.styx", 0)
                .with_config(config)
                .with_message("test error")
                .with_label(
                    Label::new(("test.styx", 0..4))
                        .with_message("here")
                        .with_color(Color::Red),
                )
                .finish();

        let mut output = Vec::new();
        report
            .write(("test.styx", Source::from(source)), &mut output)
            .unwrap();
        let s = String::from_utf8(output).unwrap();
        eprintln!("Output: {:?}", s);

        // Always assert - NO_COLOR should be set by nextest setup script
        assert!(no_color, "NO_COLOR should be set by nextest setup script");
        assert!(
            !s.contains("\x1b["),
            "With NO_COLOR set, output should not contain ANSI escape codes:\n{:?}",
            s
        );
    }

    #[derive(Facet, Debug)]
    struct Person {
        name: String,
        age: u32,
    }

    #[test]
    fn test_missing_field_diagnostic() {
        let source = "name Alice";
        let result: Result<Person, _> = crate::from_str(source);
        let err = result.unwrap_err();

        // Use RenderError trait - config is applied internally
        let rendered = RenderError::render(&err, "test.styx", source);

        // Check no ANSI codes when NO_COLOR is set
        let no_color = std::env::var("NO_COLOR").is_ok();
        if no_color {
            assert!(
                !rendered.contains("\x1b["),
                "Output should not contain ANSI escape codes:\n{:?}",
                rendered
            );
        }

        insta::assert_snapshot!(rendered);
    }

    #[test]
    fn test_invalid_scalar_diagnostic() {
        let source = "name Alice\nage notanumber";
        let result: Result<Person, _> = crate::from_str(source);
        let err = result.unwrap_err();

        let rendered = err.render("test.styx", source);
        insta::assert_snapshot!(rendered);
    }

    #[test]
    fn test_unknown_field_diagnostic() {
        #[derive(Facet, Debug)]
        #[facet(deny_unknown_fields)]
        struct Strict {
            name: String,
        }

        let source = "name Alice\nunknown_field value";
        let result: Result<Strict, _> = crate::from_str(source);
        let err = result.unwrap_err();

        let rendered = err.render("test.styx", source);
        insta::assert_snapshot!(rendered);
    }
}
