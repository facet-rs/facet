//! Validation error types.

use ariadne::{Color, Config, Label, Report, ReportKind, Source};
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

/// Result of validating a document against a schema.
#[derive(Debug, Clone)]
pub struct ValidationResult {
    /// Validation errors (must be empty for validation to pass).
    pub errors: Vec<ValidationError>,
    /// Validation warnings (non-fatal issues).
    pub warnings: Vec<ValidationWarning>,
}

impl ValidationResult {
    /// Create an empty (passing) result.
    pub fn ok() -> Self {
        Self {
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }

    /// Check if validation passed (no errors).
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }

    /// Add an error.
    pub fn error(&mut self, error: ValidationError) {
        self.errors.push(error);
    }

    /// Add a warning.
    pub fn warning(&mut self, warning: ValidationWarning) {
        self.warnings.push(warning);
    }

    /// Merge another result into this one.
    pub fn merge(&mut self, other: ValidationResult) {
        self.errors.extend(other.errors);
        self.warnings.extend(other.warnings);
    }

    /// Render all errors with ariadne.
    pub fn render(&self, filename: &str, source: &str) -> String {
        let mut output = Vec::new();
        self.write_report(filename, source, &mut output);
        String::from_utf8(output).unwrap_or_else(|_| {
            self.errors
                .iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join("\n")
        })
    }

    /// Write all error reports to a writer.
    pub fn write_report<W: std::io::Write>(&self, filename: &str, source: &str, mut writer: W) {
        for error in &self.errors {
            error.write_report(filename, source, &mut writer);
        }
        for warning in &self.warnings {
            warning.write_report(filename, source, &mut writer);
        }
    }
}

/// A validation error.
#[derive(Debug, Clone)]
pub struct ValidationError {
    /// Path to the error location (e.g., "server.tls.cert").
    pub path: String,
    /// Source span in the document.
    pub span: Option<Span>,
    /// Error kind.
    pub kind: ValidationErrorKind,
    /// Human-readable message.
    pub message: String,
}

impl ValidationError {
    /// Create a new validation error.
    pub fn new(
        path: impl Into<String>,
        kind: ValidationErrorKind,
        message: impl Into<String>,
    ) -> Self {
        Self {
            path: path.into(),
            span: None,
            kind,
            message: message.into(),
        }
    }

    /// Set the span.
    pub fn with_span(mut self, span: Option<Span>) -> Self {
        self.span = span;
        self
    }

    /// Get quickfix data for LSP code actions.
    /// Returns JSON data that can be used to offer quick fixes.
    pub fn quickfix_data(&self) -> Option<serde_json::Value> {
        match &self.kind {
            ValidationErrorKind::UnknownField {
                field, suggestion, ..
            } => suggestion.as_ref().map(|suggestion| {
                serde_json::json!({
                    "type": "rename_field",
                    "from": field,
                    "to": suggestion
                })
            }),
            _ => None,
        }
    }

    /// Get a rich diagnostic message suitable for LSP.
    pub fn diagnostic_message(&self) -> String {
        match &self.kind {
            ValidationErrorKind::UnknownField {
                field,
                valid_fields,
                suggestion,
            } => {
                let mut msg = format!("unknown field '{}'", field);
                if let Some(suggestion) = suggestion {
                    msg.push_str(&format!(" — did you mean '{}'?", suggestion));
                }
                if !valid_fields.is_empty() && valid_fields.len() <= 10 {
                    msg.push_str(&format!("\nvalid: {}", valid_fields.join(", ")));
                }
                msg
            }
            ValidationErrorKind::MissingField { field } => {
                format!("missing required field '{}'", field)
            }
            ValidationErrorKind::TypeMismatch { expected, got } => {
                format!("type mismatch: expected {}, got {}", expected, got)
            }
            _ => self.message.clone(),
        }
    }

    /// Render this error with ariadne.
    pub fn render(&self, filename: &str, source: &str) -> String {
        let mut output = Vec::new();
        self.write_report(filename, source, &mut output);
        String::from_utf8(output).unwrap_or_else(|_| format!("{}", self))
    }

    /// Write the error report to a writer.
    pub fn write_report<W: std::io::Write>(&self, filename: &str, source: &str, writer: W) {
        let report = self.build_report(filename);
        let _ = report
            .with_config(ariadne_config())
            .finish()
            .write((filename, Source::from(source)), writer);
    }

    /// Build an ariadne report for this error.
    fn build_report<'a>(
        &self,
        filename: &'a str,
    ) -> ariadne::ReportBuilder<'static, (&'a str, std::ops::Range<usize>)> {
        let range = self
            .span
            .map(|s| s.start as usize..s.end as usize)
            .unwrap_or(0..1);

        let path_info = if self.path.is_empty() {
            String::new()
        } else {
            format!(" at '{}'", self.path)
        };

        match &self.kind {
            ValidationErrorKind::MissingField { field } => {
                Report::build(ReportKind::Error, (filename, range.clone()))
                    .with_message(format!("missing required field '{}'", field))
                    .with_label(
                        Label::new((filename, range))
                            .with_message(format!("add field '{}' here", field))
                            .with_color(Color::Red),
                    )
                    .with_help(format!("{} <value>", field))
            }

            ValidationErrorKind::UnknownField {
                field,
                valid_fields,
                suggestion,
            } => {
                let mut builder = Report::build(ReportKind::Error, (filename, range.clone()))
                    .with_message(format!("unknown field '{}'", field))
                    .with_label(
                        Label::new((filename, range.clone()))
                            .with_message("not defined in schema")
                            .with_color(Color::Red),
                    );

                if let Some(suggestion) = suggestion {
                    builder = builder.with_help(format!("did you mean '{}'?", suggestion));
                }

                if !valid_fields.is_empty() {
                    builder =
                        builder.with_note(format!("valid fields: {}", valid_fields.join(", ")));
                }

                builder
            }

            ValidationErrorKind::TypeMismatch { expected, got } => {
                Report::build(ReportKind::Error, (filename, range.clone()))
                    .with_message(format!("type mismatch{}", path_info))
                    .with_label(
                        Label::new((filename, range))
                            .with_message(format!("expected {}, got {}", expected, got))
                            .with_color(Color::Red),
                    )
            }

            ValidationErrorKind::InvalidValue { reason } => {
                Report::build(ReportKind::Error, (filename, range.clone()))
                    .with_message(format!("invalid value{}", path_info))
                    .with_label(
                        Label::new((filename, range))
                            .with_message(reason)
                            .with_color(Color::Red),
                    )
            }

            ValidationErrorKind::UnknownType { name } => {
                Report::build(ReportKind::Error, (filename, range.clone()))
                    .with_message(format!("unknown type '{}'", name))
                    .with_label(
                        Label::new((filename, range))
                            .with_message("type not defined in schema")
                            .with_color(Color::Red),
                    )
            }

            ValidationErrorKind::InvalidVariant { expected, got } => {
                let expected_list = expected.join(", ");
                Report::build(ReportKind::Error, (filename, range.clone()))
                    .with_message(format!("invalid enum variant '@{}'", got))
                    .with_label(
                        Label::new((filename, range))
                            .with_message(format!("expected one of: {}", expected_list))
                            .with_color(Color::Red),
                    )
            }

            ValidationErrorKind::UnionMismatch { tried } => {
                let tried_list = tried.join(", ");
                Report::build(ReportKind::Error, (filename, range.clone()))
                    .with_message(format!(
                        "value doesn't match any union variant{}",
                        path_info
                    ))
                    .with_label(
                        Label::new((filename, range))
                            .with_message(format!("tried: {}", tried_list))
                            .with_color(Color::Red),
                    )
            }

            ValidationErrorKind::ExpectedObject => {
                Report::build(ReportKind::Error, (filename, range.clone()))
                    .with_message(format!("expected object{}", path_info))
                    .with_label(
                        Label::new((filename, range))
                            .with_message("expected { ... }")
                            .with_color(Color::Red),
                    )
            }

            ValidationErrorKind::ExpectedSequence => {
                Report::build(ReportKind::Error, (filename, range.clone()))
                    .with_message(format!("expected sequence{}", path_info))
                    .with_label(
                        Label::new((filename, range))
                            .with_message("expected ( ... )")
                            .with_color(Color::Red),
                    )
            }

            ValidationErrorKind::ExpectedScalar => {
                Report::build(ReportKind::Error, (filename, range.clone()))
                    .with_message(format!("expected scalar value{}", path_info))
                    .with_label(
                        Label::new((filename, range))
                            .with_message("expected a simple value")
                            .with_color(Color::Red),
                    )
            }

            ValidationErrorKind::ExpectedTagged => {
                Report::build(ReportKind::Error, (filename, range.clone()))
                    .with_message(format!("expected tagged value{}", path_info))
                    .with_label(
                        Label::new((filename, range))
                            .with_message("expected @tag or @tag{...}")
                            .with_color(Color::Red),
                    )
            }

            ValidationErrorKind::WrongTag { expected, got } => {
                Report::build(ReportKind::Error, (filename, range.clone()))
                    .with_message(format!("wrong tag{}", path_info))
                    .with_label(
                        Label::new((filename, range))
                            .with_message(format!("expected @{}, got @{}", expected, got))
                            .with_color(Color::Red),
                    )
            }

            ValidationErrorKind::SchemaError { reason } => {
                Report::build(ReportKind::Error, (filename, range.clone()))
                    .with_message("schema error")
                    .with_label(
                        Label::new((filename, range))
                            .with_message(reason)
                            .with_color(Color::Red),
                    )
            }
        }
    }
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.path.is_empty() {
            write!(f, "{}", self.message)
        } else {
            write!(f, "{}: {}", self.path, self.message)
        }
    }
}

impl std::error::Error for ValidationError {}

/// Kinds of validation errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationErrorKind {
    /// Missing required field in object.
    MissingField { field: String },
    /// Unknown field in object (when additional fields not allowed).
    UnknownField {
        field: String,
        valid_fields: Vec<String>,
        suggestion: Option<String>,
    },
    /// Type mismatch.
    TypeMismatch { expected: String, got: String },
    /// Invalid value for type.
    InvalidValue { reason: String },
    /// Unknown type reference in schema.
    UnknownType { name: String },
    /// Invalid enum variant.
    InvalidVariant { expected: Vec<String>, got: String },
    /// Union match failed (value didn't match any variant).
    UnionMismatch { tried: Vec<String> },
    /// Expected object, got something else.
    ExpectedObject,
    /// Expected sequence, got something else.
    ExpectedSequence,
    /// Expected scalar, got something else.
    ExpectedScalar,
    /// Expected tagged value.
    ExpectedTagged,
    /// Wrong tag name.
    WrongTag { expected: String, got: String },
    /// Schema error (invalid schema definition).
    SchemaError { reason: String },
}

/// A validation warning (non-fatal).
#[derive(Debug, Clone)]
pub struct ValidationWarning {
    /// Path to the warning location.
    pub path: String,
    /// Source span in the document.
    pub span: Option<Span>,
    /// Warning kind.
    pub kind: ValidationWarningKind,
    /// Human-readable message.
    pub message: String,
}

impl ValidationWarning {
    /// Create a new validation warning.
    pub fn new(
        path: impl Into<String>,
        kind: ValidationWarningKind,
        message: impl Into<String>,
    ) -> Self {
        Self {
            path: path.into(),
            span: None,
            kind,
            message: message.into(),
        }
    }

    /// Set the span.
    pub fn with_span(mut self, span: Option<Span>) -> Self {
        self.span = span;
        self
    }

    /// Write the warning report to a writer.
    pub fn write_report<W: std::io::Write>(&self, filename: &str, source: &str, writer: W) {
        let range = self
            .span
            .map(|s| s.start as usize..s.end as usize)
            .unwrap_or(0..1);

        let report = match &self.kind {
            ValidationWarningKind::Deprecated { reason } => {
                Report::build(ReportKind::Warning, (filename, range.clone()))
                    .with_message("deprecated")
                    .with_label(
                        Label::new((filename, range))
                            .with_message(reason)
                            .with_color(Color::Yellow),
                    )
            }
            ValidationWarningKind::IgnoredField { field } => {
                Report::build(ReportKind::Warning, (filename, range.clone()))
                    .with_message(format!("field '{}' will be ignored", field))
                    .with_label(
                        Label::new((filename, range))
                            .with_message("ignored")
                            .with_color(Color::Yellow),
                    )
            }
        };

        let _ = report
            .with_config(ariadne_config())
            .finish()
            .write((filename, Source::from(source)), writer);
    }
}

/// Kinds of validation warnings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationWarningKind {
    /// Deprecated field or type.
    Deprecated { reason: String },
    /// Field will be ignored.
    IgnoredField { field: String },
}
