//! Error types for template parsing and evaluation
//!
//! Error types carry structured information for debugging.

#![allow(unused_assignments)]

use ariadne::{Color, Label, Report, ReportKind, Source};
use facet::Facet;
use std::sync::Arc;
use thiserror::Error;

/// A span in source code (offset, length)
// r[impl error.span]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Facet)]
pub struct SourceSpan {
    offset: usize,
    len: usize,
}

impl SourceSpan {
    /// Create a new span from offset and length
    pub fn new(offset: usize, len: usize) -> Self {
        Self { offset, len }
    }

    /// Get the offset (start position)
    pub fn offset(&self) -> usize {
        self.offset
    }

    /// Get the length
    pub fn len(&self) -> usize {
        self.len
    }

    /// Check if the span is empty
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

/// A template source file for error reporting
#[derive(Debug, Clone)]
pub struct TemplateSource {
    /// Name of the template (usually filename)
    pub name: String,
    /// The full source text
    pub source: Arc<String>,
}

impl TemplateSource {
    pub fn new(name: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            source: Arc::new(source.into()),
        }
    }

    /// Create a NamedSource for error reporting
    pub fn named_source(&self) -> NamedSource {
        NamedSource::new(self.name.clone(), (*self.source).clone())
    }
}

/// Named source for error reporting (simplified from miette)
#[derive(Debug, Clone)]
pub struct NamedSource {
    pub name: String,
    pub source: String,
}

/// Location in source code for error reporting
#[derive(Debug, Clone)]
pub struct SourceLocation {
    pub span: SourceSpan,
    pub src: NamedSource,
}

impl SourceLocation {
    pub fn new(span: SourceSpan, src: NamedSource) -> Self {
        Self { span, src }
    }

    /// Format as "file:line:col"
    pub fn display(&self) -> String {
        self.src.location(&self.span)
    }
}

impl NamedSource {
    pub fn new(name: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            source: source.into(),
        }
    }

    /// Compute (line, column) from byte offset. Line and column are 1-based.
    pub fn offset_to_line_col(&self, offset: usize) -> (usize, usize) {
        let mut line = 1;
        let mut col = 1;
        for (i, ch) in self.source.char_indices() {
            if i >= offset {
                break;
            }
            if ch == '\n' {
                line += 1;
                col = 1;
            } else {
                col += 1;
            }
        }
        (line, col)
    }

    /// Format the location as "name:line:col"
    pub fn location(&self, span: &SourceSpan) -> String {
        let (line, col) = self.offset_to_line_col(span.offset());
        format!("{}:{}:{}", self.name, line, col)
    }
}

/// All template errors
#[derive(Error, Debug, Clone)]
pub enum TemplateError {
    #[error("Syntax error: {0}")]
    Syntax(Box<SyntaxError>),

    #[error("Unknown field: {0}")]
    UnknownField(Box<UnknownFieldError>),

    #[error("Type error: {0}")]
    Type(Box<TypeError>),

    #[error("Undefined variable: {0}")]
    Undefined(Box<UndefinedError>),

    #[error("Unknown filter: {0}")]
    UnknownFilter(Box<UnknownFilterError>),

    #[error("Unknown test: {0}")]
    UnknownTest(Box<UnknownTestError>),

    #[error("Macro not found: {0}")]
    MacroNotFound(Box<MacroNotFoundError>),

    #[error("Data path not found: {0}")]
    DataPathNotFound(Box<DataPathNotFoundError>),

    #[error("Function error: {0}")]
    GlobalFn(String),
}

// Convenience From impls that auto-box
impl From<SyntaxError> for TemplateError {
    fn from(e: SyntaxError) -> Self {
        Self::Syntax(Box::new(e))
    }
}

impl From<UnknownFieldError> for TemplateError {
    fn from(e: UnknownFieldError) -> Self {
        Self::UnknownField(Box::new(e))
    }
}

impl From<TypeError> for TemplateError {
    fn from(e: TypeError) -> Self {
        Self::Type(Box::new(e))
    }
}

impl From<UndefinedError> for TemplateError {
    fn from(e: UndefinedError) -> Self {
        Self::Undefined(Box::new(e))
    }
}

impl From<UnknownFilterError> for TemplateError {
    fn from(e: UnknownFilterError) -> Self {
        Self::UnknownFilter(Box::new(e))
    }
}

impl From<UnknownTestError> for TemplateError {
    fn from(e: UnknownTestError) -> Self {
        Self::UnknownTest(Box::new(e))
    }
}

impl From<MacroNotFoundError> for TemplateError {
    fn from(e: MacroNotFoundError) -> Self {
        Self::MacroNotFound(Box::new(e))
    }
}

impl From<DataPathNotFoundError> for TemplateError {
    fn from(e: DataPathNotFoundError) -> Self {
        Self::DataPathNotFound(Box::new(e))
    }
}

/// Macro not found error
#[derive(Error, Debug, Clone)]
#[error("{}: Macro `{}::{}` not found", self.loc.display(), self.namespace, self.name)]
pub struct MacroNotFoundError {
    pub namespace: String,
    pub name: String,
    pub loc: SourceLocation,
}

/// Data path not found error
#[derive(Error, Debug, Clone)]
#[error("{}: Data path `{}` not found", self.loc.display(), self.path)]
pub struct DataPathNotFoundError {
    pub path: String,
    pub loc: SourceLocation,
}

/// Errors that can occur during rendering
#[derive(Error, Debug, Clone)]
pub enum RenderError {
    /// Template not found
    #[error("Template not found: {0}")]
    NotFound(String),

    /// Template error (parse or evaluation)
    #[error("{0}")]
    Template(#[from] TemplateError),

    /// Other errors (e.g., from template functions)
    #[error("{0}")]
    Other(String),
}

impl RenderError {
    /// Format the error with source context using ariadne (if applicable)
    pub fn format_pretty(&self) -> String {
        match self {
            RenderError::Template(e) => format_template_error_pretty(e),
            other => format!("{}", other),
        }
    }
}

/// Syntax error during parsing
// r[impl error.syntax]
#[derive(Error, Debug, Clone)]
#[error("{}: Unexpected {found}, expected {expected}", self.loc.display())]
pub struct SyntaxError {
    /// What we found
    pub found: String,
    /// What we expected
    pub expected: String,
    /// Location in source
    pub loc: SourceLocation,
}

/// Unknown field access on a type
#[derive(Error, Debug, Clone)]
#[error("{}: Type `{base_type}` has no field `{field}` (available: {})", self.loc.display(), known_fields.join(", "))]
pub struct UnknownFieldError {
    /// The type being accessed
    pub base_type: String,
    /// The field that doesn't exist
    pub field: String,
    /// Known fields on this type
    pub known_fields: Vec<String>,
    /// Location of the field access
    pub loc: SourceLocation,
}

/// Type error (e.g., iterating over non-iterable)
// r[impl error.type-mismatch]
#[derive(Error, Debug, Clone)]
#[error("{}: Expected {expected}, found {found} ({context})", self.loc.display())]
pub struct TypeError {
    /// What type was expected
    pub expected: String,
    /// What type was found
    pub found: String,
    /// Context for the error
    pub context: String,
    /// Location
    pub loc: SourceLocation,
}

/// Undefined variable
#[derive(Error, Debug, Clone)]
#[error("{}: Variable `{name}` is not defined (available: {})", self.loc.display(), available.join(", "))]
pub struct UndefinedError {
    /// The undefined variable name
    pub name: String,
    /// Variables that are available in scope
    pub available: Vec<String>,
    /// Location
    pub loc: SourceLocation,
}

/// Unknown filter
// r[impl error.undefined-filter]
#[derive(Error, Debug, Clone)]
#[error("{}: Unknown filter `{name}` (available: {})", self.loc.display(), known_filters.join(", "))]
pub struct UnknownFilterError {
    /// The filter that doesn't exist
    pub name: String,
    /// Known filters
    pub known_filters: Vec<String>,
    /// Location
    pub loc: SourceLocation,
}

/// Unknown test function
// r[impl error.undefined-test]
#[derive(Error, Debug, Clone)]
#[error("{}: Unknown test `{name}` (available: starting_with, ending_with, containing, defined, undefined, none, string, number, odd, even, empty)", self.loc.display())]
pub struct UnknownTestError {
    /// The test that doesn't exist
    pub name: String,
    /// Location
    pub loc: SourceLocation,
}

/// Unclosed delimiter (tag, block, etc.)
#[derive(Error, Debug, Clone)]
#[error("{}: Unclosed {kind}, add `{close_delim}` to close", self.loc.display())]
pub struct UnclosedError {
    /// What was left unclosed
    pub kind: String,
    /// The closing delimiter needed
    pub close_delim: String,
    /// Location where it was opened
    pub loc: SourceLocation,
}

impl From<UnclosedError> for TemplateError {
    fn from(e: UnclosedError) -> Self {
        SyntaxError {
            found: "end of input".to_string(),
            expected: e.close_delim.clone(),
            loc: e.loc,
        }
        .into()
    }
}

// ============================================================================
// Pretty error formatting with ariadne
// ============================================================================

/// Trait for errors that can be formatted with source context
pub trait PrettyError {
    /// Get the source location
    fn source_loc(&self) -> &SourceLocation;
    /// Get the error message (without location prefix)
    fn message(&self) -> String;
    /// Get an optional help message
    fn help(&self) -> Option<String> {
        None
    }
}

impl PrettyError for SyntaxError {
    fn source_loc(&self) -> &SourceLocation {
        &self.loc
    }
    fn message(&self) -> String {
        format!("Unexpected {}, expected {}", self.found, self.expected)
    }
}

impl PrettyError for UnknownFieldError {
    fn source_loc(&self) -> &SourceLocation {
        &self.loc
    }
    fn message(&self) -> String {
        format!("Type `{}` has no field `{}`", self.base_type, self.field)
    }
    fn help(&self) -> Option<String> {
        if self.known_fields.is_empty() {
            None
        } else {
            Some(format!(
                "Available fields: {}",
                self.known_fields.join(", ")
            ))
        }
    }
}

impl PrettyError for TypeError {
    fn source_loc(&self) -> &SourceLocation {
        &self.loc
    }
    fn message(&self) -> String {
        format!(
            "Expected {}, found {} ({})",
            self.expected, self.found, self.context
        )
    }
}

impl PrettyError for UndefinedError {
    fn source_loc(&self) -> &SourceLocation {
        &self.loc
    }
    fn message(&self) -> String {
        format!("Variable `{}` is not defined", self.name)
    }
    fn help(&self) -> Option<String> {
        if self.available.is_empty() {
            None
        } else {
            Some(format!("Available: {}", self.available.join(", ")))
        }
    }
}

impl PrettyError for UnknownFilterError {
    fn source_loc(&self) -> &SourceLocation {
        &self.loc
    }
    fn message(&self) -> String {
        format!("Unknown filter `{}`", self.name)
    }
    fn help(&self) -> Option<String> {
        Some(format!("Available: {}", self.known_filters.join(", ")))
    }
}

impl PrettyError for UnknownTestError {
    fn source_loc(&self) -> &SourceLocation {
        &self.loc
    }
    fn message(&self) -> String {
        format!("Unknown test `{}`", self.name)
    }
    fn help(&self) -> Option<String> {
        Some("Available: starting_with, ending_with, containing, defined, undefined, none, string, number, odd, even, empty".to_string())
    }
}

impl PrettyError for MacroNotFoundError {
    fn source_loc(&self) -> &SourceLocation {
        &self.loc
    }
    fn message(&self) -> String {
        format!("Macro `{}::{}` not found", self.namespace, self.name)
    }
}

impl PrettyError for DataPathNotFoundError {
    fn source_loc(&self) -> &SourceLocation {
        &self.loc
    }
    fn message(&self) -> String {
        format!("Data path `{}` not found", self.path)
    }
}

/// Format an error with ariadne, producing a pretty string with source context
pub fn format_error_pretty<E: PrettyError>(error: &E) -> String {
    let loc = error.source_loc();

    // Calculate the byte range for the span
    let start = loc.span.offset();
    let end = start + loc.span.len().max(1); // Ensure at least 1 char for visibility

    // Build the report - use tuple (file_id, range) as span
    let mut report =
        Report::build(ReportKind::Error, (&loc.src.name, start..end)).with_message(error.message());

    // Add the primary label
    let label = Label::new((&loc.src.name, start..end))
        .with_message(error.message())
        .with_color(Color::Red);
    report = report.with_label(label);

    // Add help if available
    if let Some(help) = error.help() {
        report = report.with_help(help);
    }

    // Render to string
    let mut output = Vec::new();
    report
        .finish()
        .write((&loc.src.name, Source::from(&loc.src.source)), &mut output)
        .expect("failed to write error report");

    String::from_utf8(output).expect("ariadne produced invalid UTF-8")
}

/// Format a TemplateError with ariadne
pub fn format_template_error_pretty(error: &TemplateError) -> String {
    match error {
        TemplateError::Syntax(e) => format_error_pretty(e.as_ref()),
        TemplateError::UnknownField(e) => format_error_pretty(e.as_ref()),
        TemplateError::Type(e) => format_error_pretty(e.as_ref()),
        TemplateError::Undefined(e) => format_error_pretty(e.as_ref()),
        TemplateError::UnknownFilter(e) => format_error_pretty(e.as_ref()),
        TemplateError::UnknownTest(e) => format_error_pretty(e.as_ref()),
        TemplateError::MacroNotFound(e) => format_error_pretty(e.as_ref()),
        TemplateError::DataPathNotFound(e) => format_error_pretty(e.as_ref()),
        TemplateError::GlobalFn(msg) => format!("Function error: {}", msg),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_type_error_pretty() {
        let source = "{% if page.extra.description %}\n  {{ page.extra.description }}\n{% endif %}";
        let error = TypeError {
            expected: "object or dict".to_string(),
            found: "none".to_string(),
            context: "accessing field `description`".to_string(),
            loc: SourceLocation::new(
                SourceSpan::new(18, 11), // "description" in page.extra.description
                NamedSource::new("section.html", source),
            ),
        };

        let pretty = format_error_pretty(&error);
        println!("Pretty error output:\n{}", pretty);

        // Should contain the filename
        assert!(pretty.contains("section.html"), "Should contain filename");
        // Should contain the error message
        assert!(
            pretty.contains("Expected object or dict"),
            "Should contain error message"
        );
        // Should contain the source line (ariadne shows it)
        assert!(
            pretty.contains("page.extra.description") || pretty.contains("│"),
            "Should contain source context or ariadne formatting"
        );
    }

    #[test]
    fn test_render_error_format_pretty_with_template() {
        let source = "{{ undefined_var }}";
        let template_error: TemplateError = UndefinedError {
            name: "undefined_var".to_string(),
            available: vec!["page".to_string(), "section".to_string()],
            loc: SourceLocation::new(
                SourceSpan::new(3, 13),
                NamedSource::new("test.html", source),
            ),
        }
        .into();
        let render_error = RenderError::Template(template_error);

        let pretty = render_error.format_pretty();
        println!("RenderError::Template pretty:\n{}", pretty);

        // Should use ariadne formatting
        assert!(
            pretty.contains("test.html")
                && (pretty.contains("│") || pretty.contains("undefined_var")),
            "RenderError::Template should use ariadne formatting"
        );
    }

    #[test]
    fn test_render_error_format_pretty_with_other() {
        let render_error = RenderError::Other("Some random error".to_string());

        let pretty = render_error.format_pretty();
        println!("RenderError::Other pretty:\n{}", pretty);

        assert_eq!(pretty, "Some random error");
    }
}
