//! Rich error types with intent for IDE-grade diagnostics
//!
//! Error types carry structured information, not just text.
//! This enables:
//! - Beautiful terminal output via miette
//! - Suggestions and help text
//! - Future: IDE integration, quick fixes

use miette::{Diagnostic, NamedSource, SourceSpan};
use std::sync::Arc;
use thiserror::Error;

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

    /// Create a NamedSource for miette
    pub fn named_source(&self) -> NamedSource<String> {
        NamedSource::new(&self.name, (*self.source).clone())
    }
}

/// All template errors
#[derive(Error, Debug, Diagnostic)]
pub enum TemplateError {
    #[error("Syntax error")]
    #[diagnostic(code(template::syntax))]
    Syntax(#[from] SyntaxError),

    #[error("Unknown field")]
    #[diagnostic(code(template::unknown_field))]
    UnknownField(#[from] UnknownFieldError),

    #[error("Type error")]
    #[diagnostic(code(template::type_error))]
    Type(#[from] TypeError),

    #[error("Undefined variable")]
    #[diagnostic(code(template::undefined))]
    Undefined(#[from] UndefinedError),

    #[error("Unknown filter")]
    #[diagnostic(code(template::unknown_filter))]
    UnknownFilter(#[from] UnknownFilterError),

    #[error("Unknown test")]
    #[diagnostic(code(template::unknown_test))]
    UnknownTest(#[from] UnknownTestError),
}

/// Syntax error during parsing
#[derive(Error, Debug, Diagnostic)]
#[error("Unexpected {found}")]
#[diagnostic(code(template::syntax::unexpected), help("Expected {expected}"))]
pub struct SyntaxError {
    /// What we found
    pub found: String,
    /// What we expected
    pub expected: String,
    /// Location in source
    #[label("here")]
    pub span: SourceSpan,
    /// The source code
    #[source_code]
    pub src: NamedSource<String>,
}

/// Unknown field access on a type
#[derive(Error, Debug, Diagnostic)]
#[error("Type `{base_type}` has no field `{field}`")]
#[diagnostic(
    code(template::unknown_field),
    help("Available fields: {}", known_fields.join(", "))
)]
pub struct UnknownFieldError {
    /// The type being accessed
    pub base_type: String,
    /// The field that doesn't exist
    pub field: String,
    /// Known fields on this type
    pub known_fields: Vec<String>,
    /// Location of the field access
    #[label("this field doesn't exist")]
    pub span: SourceSpan,
    /// The source code
    #[source_code]
    pub src: NamedSource<String>,
}

/// Type error (e.g., iterating over non-iterable)
#[derive(Error, Debug, Diagnostic)]
#[error("Expected {expected}, found {found}")]
#[diagnostic(code(template::type_error))]
pub struct TypeError {
    /// What type was expected
    pub expected: String,
    /// What type was found
    pub found: String,
    /// Context for the error
    pub context: String,
    /// Location
    #[label("{context}")]
    pub span: SourceSpan,
    /// The source code
    #[source_code]
    pub src: NamedSource<String>,
}

/// Undefined variable
#[derive(Error, Debug, Diagnostic)]
#[error("Variable `{name}` is not defined")]
#[diagnostic(
    code(template::undefined),
    help("Available variables: {}", available.join(", "))
)]
pub struct UndefinedError {
    /// The undefined variable name
    pub name: String,
    /// Variables that are available in scope
    pub available: Vec<String>,
    /// Location
    #[label("not found in scope")]
    pub span: SourceSpan,
    /// The source code
    #[source_code]
    pub src: NamedSource<String>,
}

/// Unknown filter
#[derive(Error, Debug, Diagnostic)]
#[error("Unknown filter `{name}`")]
#[diagnostic(
    code(template::unknown_filter),
    help("Available filters: {}", known_filters.join(", "))
)]
pub struct UnknownFilterError {
    /// The filter that doesn't exist
    pub name: String,
    /// Known filters
    pub known_filters: Vec<String>,
    /// Location
    #[label("this filter doesn't exist")]
    pub span: SourceSpan,
    /// The source code
    #[source_code]
    pub src: NamedSource<String>,
}

/// Unknown test function
#[derive(Error, Debug, Diagnostic)]
#[error("Unknown test `{name}`")]
#[diagnostic(
    code(template::unknown_test),
    help(
        "Available tests: starting_with, ending_with, containing, defined, undefined, none, string, number, odd, even, empty"
    )
)]
pub struct UnknownTestError {
    /// The test that doesn't exist
    pub name: String,
    /// Location
    #[label("this test doesn't exist")]
    pub span: SourceSpan,
    /// The source code
    #[source_code]
    pub src: NamedSource<String>,
}

/// Unclosed delimiter (tag, block, etc.)
#[derive(Error, Debug, Diagnostic)]
#[error("Unclosed {kind}")]
#[diagnostic(
    code(template::syntax::unclosed),
    help("Add `{close_delim}` to close this {kind}")
)]
pub struct UnclosedError {
    /// What was left unclosed
    pub kind: String,
    /// The closing delimiter needed
    pub close_delim: String,
    /// Where it was opened
    #[label("opened here")]
    pub open_span: SourceSpan,
    /// The source code
    #[source_code]
    pub src: NamedSource<String>,
}

impl From<UnclosedError> for TemplateError {
    fn from(e: UnclosedError) -> Self {
        TemplateError::Syntax(SyntaxError {
            found: "end of input".to_string(),
            expected: e.close_delim.clone(),
            span: e.open_span,
            src: e.src,
        })
    }
}
