//! Schema types for LSP extension test files.
//!
//! These types define the structure of `.styx` test files that exercise
//! LSP extensions through real IPC (roam over stdio).
//!
//! # Example Test File
//!
//! ```styx
//! tests [
//!     @test {
//!         name "column completions are context-aware"
//!         input <<STYX
//!             @schema {id crate:dibs-queries@1}
//!             AllProducts @query {
//!                 from product
//!                 select {h|}
//!             }
//!         STYX
//!         completions {
//!             has (handle id status)
//!             not_has (locale currency)
//!         }
//!     }
//! ]
//! ```

use facet::Facet;

/// A test file containing one or more test cases.
#[derive(Debug, Clone, Facet)]
pub struct TestFile {
    /// Test cases in this file.
    pub tests: Vec<TestCase>,
}

/// A single test case.
#[derive(Debug, Clone, Facet)]
pub struct TestCase {
    /// Name of the test (for reporting).
    pub name: String,

    /// The input document. Use `|` to mark cursor position.
    pub input: String,

    /// Optional schema to use (if not embedded in input).
    #[facet(default)]
    pub schema: Option<String>,

    /// Expected completions at cursor position.
    #[facet(default)]
    pub completions: Option<CompletionExpectations>,

    /// Expected hover result at cursor position.
    #[facet(default)]
    pub hover: Option<HoverExpectations>,

    /// Expected diagnostics for the document.
    #[facet(default)]
    pub diagnostics: Option<DiagnosticExpectations>,

    /// Expected inlay hints for the document.
    #[facet(default)]
    pub inlay_hints: Option<InlayHintExpectations>,

    /// Expected definition locations at cursor position.
    #[facet(default)]
    pub definition: Option<DefinitionExpectations>,
}

/// Expectations for completion results.
#[derive(Debug, Clone, Facet)]
pub struct CompletionExpectations {
    /// Labels that must be present.
    #[facet(default)]
    pub has: Vec<String>,

    /// Labels that must NOT be present.
    #[facet(default)]
    pub not_has: Vec<String>,

    /// Detailed expectations for specific items.
    #[facet(default)]
    pub items: Vec<CompletionItemExpectation>,
}

/// Detailed expectation for a single completion item.
#[derive(Debug, Clone, Facet)]
pub struct CompletionItemExpectation {
    /// The label to match.
    pub label: String,

    /// Expected detail text.
    #[facet(default)]
    pub detail: Option<String>,

    /// Expected documentation (substring match).
    #[facet(default)]
    pub documentation: Option<String>,

    /// Expected kind.
    #[facet(default)]
    pub kind: Option<String>,
}

/// Expectations for hover results.
#[derive(Debug, Clone, Facet)]
pub struct HoverExpectations {
    /// Substrings that must be present in hover content.
    #[facet(default)]
    pub contains: Vec<String>,

    /// Substrings that must NOT be present.
    #[facet(default)]
    pub not_contains: Vec<String>,
}

/// Expectations for diagnostics.
#[derive(Debug, Clone, Facet)]
pub struct DiagnosticExpectations {
    /// Expected number of diagnostics (if specified).
    #[facet(default)]
    pub count: Option<u32>,

    /// Diagnostics that must be present.
    #[facet(default)]
    pub has: Vec<DiagnosticExpectation>,

    /// Message substrings that must NOT appear in any diagnostic.
    #[facet(default)]
    pub not_has: Vec<String>,
}

/// Expectation for a single diagnostic.
#[derive(Debug, Clone, Facet)]
pub struct DiagnosticExpectation {
    /// Substring that must appear in the message.
    pub message: String,

    /// Expected severity (error, warning, info, hint).
    #[facet(default)]
    pub severity: Option<String>,

    /// Expected line number (1-indexed for human readability).
    #[facet(default)]
    pub line: Option<u32>,
}

/// Expectations for inlay hints.
#[derive(Debug, Clone, Facet)]
pub struct InlayHintExpectations {
    /// Expected number of hints (if specified).
    #[facet(default)]
    pub count: Option<u32>,

    /// Hints that must be present.
    #[facet(default)]
    pub has: Vec<InlayHintExpectation>,
}

/// Expectation for a single inlay hint.
#[derive(Debug, Clone, Facet)]
pub struct InlayHintExpectation {
    /// The hint label text.
    pub label: String,

    /// Expected line number (1-indexed).
    #[facet(default)]
    pub line: Option<u32>,
}

/// Expectations for definition results.
#[derive(Debug, Clone, Facet)]
pub struct DefinitionExpectations {
    /// Expected number of locations (if specified).
    #[facet(default)]
    pub count: Option<u32>,

    /// Whether we expect no results (empty).
    #[facet(default)]
    pub empty: bool,

    /// Locations that must be present.
    #[facet(default)]
    pub has: Vec<DefinitionExpectation>,
}

/// Expectation for a single definition location.
#[derive(Debug, Clone, Facet)]
pub struct DefinitionExpectation {
    /// Expected line number (1-indexed).
    #[facet(default)]
    pub line: Option<u32>,

    /// Expected URI (substring match).
    #[facet(default)]
    pub uri: Option<String>,
}
