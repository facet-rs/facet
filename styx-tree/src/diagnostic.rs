//! Diagnostic rendering for parser errors.

use ariadne::{Color, Label, Report, ReportKind, Source};
use styx_parse::{ParseErrorKind, Span};

/// A parser error with source location.
#[derive(Debug, Clone)]
pub struct ParseError {
    /// The kind of error.
    pub kind: ParseErrorKind,
    /// Source location.
    pub span: Span,
}

impl ParseError {
    /// Create a new parse error.
    pub fn new(kind: ParseErrorKind, span: Span) -> Self {
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
        let report = self.build_report(filename);
        let _ = report
            .finish()
            .write((filename, Source::from(source)), writer);
    }

    fn build_report<'a>(
        &self,
        filename: &'a str,
    ) -> ariadne::ReportBuilder<'static, (&'a str, std::ops::Range<usize>)> {
        let range = self.span.start as usize..self.span.end as usize;

        match &self.kind {
            // diag[impl diagnostic.parser.duplicate-key]
            ParseErrorKind::DuplicateKey { original } => {
                let original_range = original.start as usize..original.end as usize;
                Report::build(ReportKind::Error, (filename, range.clone()))
                    .with_message("duplicate key")
                    .with_label(
                        Label::new((filename, original_range))
                            .with_message("first defined here")
                            .with_color(Color::Blue),
                    )
                    .with_label(
                        Label::new((filename, range))
                            .with_message("duplicate key")
                            .with_color(Color::Red),
                    )
                    .with_help("each key must appear only once in an object")
            }

            // diag[impl diagnostic.parser.unclosed]
            ParseErrorKind::UnclosedObject => Report::build(ReportKind::Error, (filename, range.clone()))
                .with_message("unclosed object")
                .with_label(
                    Label::new((filename, range))
                        .with_message("object opened here")
                        .with_color(Color::Red),
                )
                .with_help("add a closing '}'"),

            // diag[impl diagnostic.parser.unclosed]
            ParseErrorKind::UnclosedSequence => Report::build(ReportKind::Error, (filename, range.clone()))
                .with_message("unclosed sequence")
                .with_label(
                    Label::new((filename, range))
                        .with_message("sequence opened here")
                        .with_color(Color::Red),
                )
                .with_help("add a closing ')'"),

            // diag[impl diagnostic.parser.escape]
            ParseErrorKind::InvalidEscape(seq) => Report::build(ReportKind::Error, (filename, range.clone()))
                .with_message(format!("invalid escape sequence '{}'", seq))
                .with_label(
                    Label::new((filename, range))
                        .with_message("invalid escape")
                        .with_color(Color::Red),
                )
                .with_help("valid escapes are: \\\\, \\\", \\n, \\r, \\t, \\uXXXX, \\u{X...}"),

            // diag[impl diagnostic.parser.unexpected]
            ParseErrorKind::UnexpectedToken => Report::build(ReportKind::Error, (filename, range.clone()))
                .with_message("unexpected token")
                .with_label(
                    Label::new((filename, range))
                        .with_message("unexpected")
                        .with_color(Color::Red),
                ),

            ParseErrorKind::ExpectedKey => Report::build(ReportKind::Error, (filename, range.clone()))
                .with_message("expected key")
                .with_label(
                    Label::new((filename, range))
                        .with_message("expected a key here")
                        .with_color(Color::Red),
                ),

            ParseErrorKind::ExpectedValue => Report::build(ReportKind::Error, (filename, range.clone()))
                .with_message("expected value")
                .with_label(
                    Label::new((filename, range))
                        .with_message("expected a value here")
                        .with_color(Color::Red),
                ),

            ParseErrorKind::UnexpectedEof => Report::build(ReportKind::Error, (filename, range.clone()))
                .with_message("unexpected end of input")
                .with_label(
                    Label::new((filename, range))
                        .with_message("input ends here")
                        .with_color(Color::Red),
                ),

            ParseErrorKind::InvalidTagName => Report::build(ReportKind::Error, (filename, range.clone()))
                .with_message("invalid tag name")
                .with_label(
                    Label::new((filename, range))
                        .with_message("invalid tag")
                        .with_color(Color::Red),
                )
                .with_help("tag names must match @[A-Za-z_][A-Za-z0-9_.-]*"),

            ParseErrorKind::InvalidKey => Report::build(ReportKind::Error, (filename, range.clone()))
                .with_message("invalid key")
                .with_label(
                    Label::new((filename, range))
                        .with_message("cannot be used as a key")
                        .with_color(Color::Red),
                )
                .with_help("keys must be scalars or unit, optionally tagged (no objects, sequences, or heredocs)"),

            ParseErrorKind::DanglingDocComment => Report::build(ReportKind::Error, (filename, range.clone()))
                .with_message("dangling doc comment")
                .with_label(
                    Label::new((filename, range))
                        .with_message("doc comment not followed by entry")
                        .with_color(Color::Red),
                )
                .with_help("doc comments (///) must be followed by an entry"),

            // diag[impl diagnostic.parser.toomany]
            ParseErrorKind::TooManyAtoms => Report::build(ReportKind::Error, (filename, range.clone()))
                .with_message("unexpected atom after value")
                .with_label(
                    Label::new((filename, range))
                        .with_message("unexpected third atom")
                        .with_color(Color::Red),
                )
                .with_help("did you mean `@tag{}`? whitespace is not allowed between a tag and its payload"),

            // diag[impl diagnostic.parser.reopened-path]
            ParseErrorKind::ReopenedPath { closed_path } => {
                let path_str = closed_path.join(".");
                Report::build(ReportKind::Error, (filename, range.clone()))
                    .with_message(format!("cannot reopen path `{}`", path_str))
                    .with_label(
                        Label::new((filename, range))
                            .with_message("path was closed when sibling appeared")
                            .with_color(Color::Red),
                    )
                    .with_help("sibling paths must appear contiguously; once you move to a different path, you cannot go back")
            }

            // diag[impl diagnostic.parser.nest-into-terminal]
            ParseErrorKind::NestIntoTerminal { terminal_path } => {
                let path_str = terminal_path.join(".");
                Report::build(ReportKind::Error, (filename, range.clone()))
                    .with_message(format!("cannot nest into `{}`", path_str))
                    .with_label(
                        Label::new((filename, range))
                            .with_message("path has a terminal value")
                            .with_color(Color::Red),
                    )
                    .with_help("you cannot add children to a path that already has a scalar, sequence, tag, or unit value")
            }

            // diag[impl diagnostic.parser.sequence-comma]
            ParseErrorKind::CommaInSequence => Report::build(ReportKind::Error, (filename, range.clone()))
                .with_message("unexpected comma in sequence")
                .with_label(
                    Label::new((filename, range))
                        .with_message("comma not allowed here")
                        .with_color(Color::Red),
                )
                .with_help("sequences are whitespace-separated, not comma-separated"),

            // diag[impl diagnostic.parser.missing-whitespace]
            ParseErrorKind::MissingWhitespaceBeforeBlock => Report::build(ReportKind::Error, (filename, range.clone()))
                .with_message("missing whitespace before block")
                .with_label(
                    Label::new((filename, range))
                        .with_message("add whitespace before this")
                        .with_color(Color::Red),
                )
                .with_help("bare keys must be separated from `{` or `(` by whitespace (to distinguish from tags like `@tag{}`)"),

            ParseErrorKind::TrailingContent => Report::build(ReportKind::Error, (filename, range.clone()))
                .with_message("trailing content after explicit root object")
                .with_label(
                    Label::new((filename, range))
                        .with_message("unexpected content here")
                        .with_color(Color::Red),
                )
                .with_help("an explicit root object `{...}` is the entire document; nothing can follow it"),
        }
    }
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.kind {
            ParseErrorKind::DuplicateKey { .. } => write!(f, "duplicate key"),
            ParseErrorKind::UnclosedObject => write!(f, "unclosed object"),
            ParseErrorKind::UnclosedSequence => write!(f, "unclosed sequence"),
            ParseErrorKind::InvalidEscape(seq) => write!(f, "invalid escape sequence '{}'", seq),
            ParseErrorKind::UnexpectedToken => write!(f, "unexpected token"),
            ParseErrorKind::ExpectedKey => write!(f, "expected key"),
            ParseErrorKind::ExpectedValue => write!(f, "expected value"),
            ParseErrorKind::UnexpectedEof => write!(f, "unexpected end of input"),
            ParseErrorKind::InvalidTagName => write!(f, "invalid tag name"),
            ParseErrorKind::InvalidKey => write!(f, "invalid key"),
            ParseErrorKind::DanglingDocComment => write!(f, "dangling doc comment"),
            ParseErrorKind::TooManyAtoms => write!(f, "unexpected atom after value"),
            ParseErrorKind::ReopenedPath { closed_path } => {
                write!(f, "cannot reopen path `{}`", closed_path.join("."))
            }
            ParseErrorKind::NestIntoTerminal { terminal_path } => {
                write!(f, "cannot nest into `{}`", terminal_path.join("."))
            }
            ParseErrorKind::CommaInSequence => write!(f, "unexpected comma in sequence"),
            ParseErrorKind::MissingWhitespaceBeforeBlock => {
                write!(f, "missing whitespace before block")
            }
            ParseErrorKind::TrailingContent => {
                write!(f, "trailing content after explicit root object")
            }
        }?;
        write!(f, " at offset {}", self.span.start)
    }
}

impl std::error::Error for ParseError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_with_errors(source: &str) -> Vec<ParseError> {
        let mut parser = styx_parse::Parser::new(source);
        let mut errors = Vec::new();
        while let Some(event) = parser.next_event() {
            if let styx_parse::Event::Error { span, kind } = event {
                errors.push(ParseError::new(kind, span));
            }
        }
        errors
    }

    macro_rules! assert_snapshot_stripped {
        ($value:expr) => {{
            let stripped = String::from_utf8(strip_ansi_escapes::strip(&$value)).unwrap();
            insta::assert_snapshot!(stripped);
        }};
    }

    #[test]
    fn test_duplicate_key_diagnostic() {
        let source = "a 1\na 2";
        let errors = parse_with_errors(source);
        assert_eq!(errors.len(), 1);

        assert_snapshot_stripped!(errors[0].render("test.styx", source));
    }

    #[test]
    fn test_invalid_escape_diagnostic() {
        let source = r#"name "hello\qworld""#;
        let errors = parse_with_errors(source);
        assert!(!errors.is_empty(), "expected InvalidEscape error");

        assert_snapshot_stripped!(errors[0].render("test.styx", source));
    }

    #[test]
    fn test_unclosed_object_diagnostic() {
        let source = "server {\n  host localhost";
        let errors = parse_with_errors(source);
        assert!(!errors.is_empty(), "expected UnclosedObject error");

        assert_snapshot_stripped!(errors[0].render("test.styx", source));
    }
}
