#![doc = include_str!("../README.md")]
//! Protocol types for Styx LSP extensions.
//!
//! This crate defines the Roam service traits and types used for communication
//! between the Styx LSP and external extensions that provide domain-specific
//! intelligence (completions, hover, diagnostics, etc.).
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────┐                      ┌─────────────────┐
//! │  Styx LSP   │◄────── Roam ────────►│    Extension    │
//! │             │                      │   (e.g. dibs)   │
//! │ implements  │                      │   implements    │
//! │ StyxLspHost │                      │ StyxLspExtension│
//! └─────────────┘                      └─────────────────┘
//! ```
//!
//! The LSP calls methods on `StyxLspExtension` to request completions, hover, etc.
//! The extension can call back to `StyxLspHost` to request additional context.
//!
//! # Generated Types
//!
//! The `#[roam::service]` macro generates:
//! - `StyxLspExtensionClient` - Call extension methods from the LSP
//! - `StyxLspExtensionDispatcher` - Dispatch incoming calls on the extension side
//! - `StyxLspHostClient` - Call LSP methods from the extension
//! - `StyxLspHostDispatcher` - Dispatch incoming calls on the LSP side
//!
use facet::Facet;
use styx_tree::Value;

// Re-export roam types needed by generated code and consumers
pub use roam;

// =============================================================================
// Service traits
// =============================================================================

/// Service implemented by LSP extensions.
///
/// The Styx LSP calls these methods to request domain-specific intelligence.
#[roam::service]
pub trait StyxLspExtension {
    /// Initialize the extension. Called once after spawn.
    async fn initialize(&self, params: InitializeParams) -> InitializeResult;

    /// Provide completion items at a cursor position.
    async fn completions(&self, params: CompletionParams) -> Vec<CompletionItem>;

    /// Provide hover information for a symbol.
    async fn hover(&self, params: HoverParams) -> Option<HoverResult>;

    /// Provide inlay hints for a range.
    async fn inlay_hints(&self, params: InlayHintParams) -> Vec<InlayHint>;

    /// Validate the document and return diagnostics.
    async fn diagnostics(&self, params: DiagnosticParams) -> Vec<Diagnostic>;

    /// Provide code actions for a range.
    async fn code_actions(&self, params: CodeActionParams) -> Vec<CodeAction>;

    /// Go to definition of a symbol.
    async fn definition(&self, params: DefinitionParams) -> Vec<Location>;

    /// Shutdown the extension gracefully.
    async fn shutdown(&self);
}

/// Service implemented by the Styx LSP for extension callbacks.
///
/// Extensions can call these methods to request additional context about
/// the document being edited.
#[roam::service]
pub trait StyxLspHost {
    /// Get a subtree of the document at a path.
    async fn get_subtree(&self, params: GetSubtreeParams) -> Option<Value>;

    /// Get the full document tree.
    async fn get_document(&self, params: GetDocumentParams) -> Option<Value>;

    /// Get the raw source text.
    async fn get_source(&self, params: GetSourceParams) -> Option<String>;

    /// Get the schema source and URI.
    async fn get_schema(&self, params: GetSchemaParams) -> Option<SchemaInfo>;

    /// Convert byte offset to line/character position.
    async fn offset_to_position(&self, params: OffsetToPositionParams) -> Option<Position>;

    /// Convert line/character position to byte offset.
    async fn position_to_offset(&self, params: PositionToOffsetParams) -> Option<u32>;
}

// =============================================================================
// Initialization
// =============================================================================

/// Parameters for extension initialization.
#[derive(Debug, Clone, Facet)]
pub struct InitializeParams {
    /// Version of the Styx LSP.
    pub styx_version: String,
    /// URI of the document being edited.
    pub document_uri: String,
    /// ID of the schema (from meta.id).
    pub schema_id: String,
}

/// Result of extension initialization.
#[derive(Debug, Clone, Facet)]
pub struct InitializeResult {
    /// Name of the extension.
    pub name: String,
    /// Version of the extension.
    pub version: String,
    /// Capabilities supported by this extension.
    pub capabilities: Vec<Capability>,
}

/// Capabilities an extension can support.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum Capability {
    Completions = 0,
    Hover = 1,
    InlayHints = 2,
    Diagnostics = 3,
    CodeActions = 4,
    Definition = 5,
}

// =============================================================================
// Positions and ranges
// =============================================================================

/// A position in a document (0-indexed line and character).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

impl Position {
    /// Create a new position.
    pub fn new(line: u32, character: u32) -> Self {
        Self { line, character }
    }

    /// Convert a byte offset to a position using document content.
    ///
    /// This is useful in `diagnostics()` where `DiagnosticParams` includes
    /// the document content, allowing offset→position conversion without
    /// an RPC call back to the host.
    pub fn from_offset(content: &str, offset: u32) -> Self {
        let offset = offset as usize;
        if offset > content.len() {
            // Past end of content - return last position
            let line = content.chars().filter(|&c| c == '\n').count() as u32;
            let last_newline = content.rfind('\n').map(|i| i + 1).unwrap_or(0);
            let character = (content.len() - last_newline) as u32;
            return Self { line, character };
        }

        let before = &content[..offset];
        let line = before.chars().filter(|&c| c == '\n').count() as u32;
        let last_newline = before.rfind('\n').map(|i| i + 1).unwrap_or(0);
        let character = (offset - last_newline) as u32;
        Self { line, character }
    }
}

/// A range in a document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

impl Range {
    /// Create a new range.
    pub fn new(start: Position, end: Position) -> Self {
        Self { start, end }
    }

    /// Convert a styx span to a range using document content.
    ///
    /// This is useful in `diagnostics()` where `DiagnosticParams` includes
    /// the document content, allowing span→range conversion without
    /// an RPC call back to the host.
    pub fn from_span(content: &str, span: &styx_tree::Span) -> Self {
        Self {
            start: Position::from_offset(content, span.start),
            end: Position::from_offset(content, span.end),
        }
    }
}

/// Cursor position with both line/character and byte offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
pub struct Cursor {
    pub line: u32,
    pub character: u32,
    pub offset: u32,
}

// =============================================================================
// Completions
// =============================================================================

/// Parameters for a completion request.
#[derive(Debug, Clone, Facet)]
#[facet(skip_all_unless_truthy)]
pub struct CompletionParams {
    /// URI of the document.
    pub document_uri: String,
    /// Cursor position.
    pub cursor: Cursor,
    /// Path to the current location in the document tree.
    /// e.g., `["AllProducts", "@query", "select"]`
    pub path: Vec<String>,
    /// Text the user has typed (for filtering).
    pub prefix: String,
    /// The subtree relevant to this completion (innermost object at cursor).
    pub context: Option<Value>,
    /// The closest enclosing tagged value (e.g., `@query{...}`).
    /// Useful for domain-specific context like finding which table a column belongs to.
    pub tagged_context: Option<Value>,
}

/// A completion item.
#[derive(Debug, Clone, Facet)]
#[facet(skip_all_unless_truthy)]
pub struct CompletionItem {
    /// The text to insert.
    pub label: String,
    /// Short description (e.g., column type).
    pub detail: Option<String>,
    /// Longer description (markdown).
    pub documentation: Option<String>,
    /// Item kind for icon selection.
    pub kind: Option<CompletionKind>,
    /// Override sort order.
    pub sort_text: Option<String>,
    /// Text to insert if different from label.
    pub insert_text: Option<String>,
}

/// Kind of completion item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum CompletionKind {
    Field = 0,
    Value = 1,
    Keyword = 2,
    Type = 3,
}

// =============================================================================
// Hover
// =============================================================================

/// Parameters for a hover request.
#[derive(Debug, Clone, Facet)]
#[facet(skip_all_unless_truthy)]
pub struct HoverParams {
    /// URI of the document.
    pub document_uri: String,
    /// Cursor position.
    pub cursor: Cursor,
    /// Path to the symbol.
    pub path: Vec<String>,
    /// Context subtree (innermost object at cursor).
    pub context: Option<Value>,
    /// The closest enclosing tagged value (e.g., `@query{...}`).
    /// Useful for domain-specific context like finding which table a column belongs to.
    pub tagged_context: Option<Value>,
}

/// Result of a hover request.
#[derive(Debug, Clone, Facet)]
#[facet(skip_all_unless_truthy)]
pub struct HoverResult {
    /// Markdown content to display.
    pub contents: String,
    /// Range to highlight (optional).
    pub range: Option<Range>,
}

// =============================================================================
// Inlay hints
// =============================================================================

/// Parameters for an inlay hints request.
#[derive(Debug, Clone, Facet)]
#[facet(skip_all_unless_truthy)]
pub struct InlayHintParams {
    /// URI of the document.
    pub document_uri: String,
    /// Range to provide hints for.
    pub range: Range,
    /// Context subtree.
    pub context: Option<Value>,
}

/// An inlay hint.
#[derive(Debug, Clone, Facet)]
#[facet(skip_all_unless_truthy)]
pub struct InlayHint {
    /// Position to display the hint.
    pub position: Position,
    /// Hint text.
    pub label: String,
    /// Kind of hint.
    pub kind: Option<InlayHintKind>,
    /// Add space before hint.
    pub padding_left: bool,
    /// Add space after hint.
    pub padding_right: bool,
}

/// Kind of inlay hint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum InlayHintKind {
    Type = 0,
    Parameter = 1,
}

// =============================================================================
// Diagnostics
// =============================================================================

/// Parameters for a diagnostics request.
#[derive(Debug, Clone, Facet)]
#[facet(skip_all_unless_truthy)]
pub struct DiagnosticParams {
    /// URI of the document.
    pub document_uri: String,
    /// The full document tree.
    pub tree: Value,
    /// The document content (for offset→position conversion).
    pub content: String,
}

/// A diagnostic (error, warning, etc.).
#[derive(Debug, Clone, Facet)]
#[facet(skip_all_unless_truthy)]
pub struct Diagnostic {
    /// Span of the diagnostic (byte offsets). The LSP host converts to line/character.
    pub span: styx_tree::Span,
    /// Severity level.
    pub severity: DiagnosticSeverity,
    /// Human-readable message.
    pub message: String,
    /// Source (extension name).
    pub source: Option<String>,
    /// Machine-readable error code.
    pub code: Option<String>,
    /// Additional data for code actions.
    pub data: Option<Value>,
}

/// Severity of a diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum DiagnosticSeverity {
    Error = 0,
    Warning = 1,
    Info = 2,
    Hint = 3,
}

// =============================================================================
// Code actions
// =============================================================================

/// Parameters for a code actions request.
#[derive(Debug, Clone, Facet)]
#[facet(skip_all_unless_truthy)]
pub struct CodeActionParams {
    /// URI of the document.
    pub document_uri: String,
    /// Span to provide actions for (byte offsets).
    pub span: styx_tree::Span,
    /// Diagnostics at this span (for context).
    pub diagnostics: Vec<Diagnostic>,
}

/// A code action (quick fix, refactoring, etc.).
#[derive(Debug, Clone, Facet)]
#[facet(skip_all_unless_truthy)]
pub struct CodeAction {
    /// Title shown to the user.
    pub title: String,
    /// Kind of action.
    pub kind: Option<CodeActionKind>,
    /// Edit to apply.
    pub edit: Option<WorkspaceEdit>,
    /// Whether this is the preferred action.
    pub is_preferred: bool,
}

/// Kind of code action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum CodeActionKind {
    QuickFix = 0,
    Refactor = 1,
    Source = 2,
}

/// A workspace edit (changes to one or more documents).
#[derive(Debug, Clone, Facet)]
pub struct WorkspaceEdit {
    pub changes: Vec<DocumentEdit>,
}

/// Edits to a single document.
#[derive(Debug, Clone, Facet)]
pub struct DocumentEdit {
    pub uri: String,
    pub edits: Vec<TextEdit>,
}

/// A text edit (replace a span with new text).
#[derive(Debug, Clone, Facet)]
pub struct TextEdit {
    /// Span to replace (byte offsets). The LSP host converts to line/character.
    pub span: styx_tree::Span,
    pub new_text: String,
}

// =============================================================================
// Go to definition
// =============================================================================

/// Parameters for a go-to-definition request.
#[derive(Debug, Clone, Facet)]
#[facet(skip_all_unless_truthy)]
pub struct DefinitionParams {
    /// URI of the document.
    pub document_uri: String,
    /// Cursor position.
    pub cursor: Cursor,
    /// Path to the symbol.
    pub path: Vec<String>,
    /// Context subtree (innermost object at cursor).
    pub context: Option<Value>,
    /// The closest enclosing tagged value (e.g., `@query{...}`).
    pub tagged_context: Option<Value>,
}

/// A location in a document (URI + span).
#[derive(Debug, Clone, Facet)]
pub struct Location {
    /// URI of the target document.
    pub uri: String,
    /// Span within the document (byte offsets). The LSP host converts to line/character.
    pub span: styx_tree::Span,
}

// =============================================================================
// Host callbacks
// =============================================================================

/// Information about the schema.
#[derive(Debug, Clone, Facet)]
pub struct SchemaInfo {
    /// Schema source text.
    pub source: String,
    /// Schema URI (file:// or styx-embedded://).
    pub uri: String,
}

/// Parameters for get_subtree.
#[derive(Debug, Clone, Facet)]
pub struct GetSubtreeParams {
    /// URI of the document.
    pub document_uri: String,
    /// Path to the subtree.
    pub path: Vec<String>,
}

/// Parameters for get_document.
#[derive(Debug, Clone, Facet)]
pub struct GetDocumentParams {
    /// URI of the document.
    pub document_uri: String,
}

/// Parameters for get_source.
#[derive(Debug, Clone, Facet)]
pub struct GetSourceParams {
    /// URI of the document.
    pub document_uri: String,
}

/// Parameters for get_schema.
#[derive(Debug, Clone, Facet)]
pub struct GetSchemaParams {
    /// URI of the document.
    pub document_uri: String,
}

/// Parameters for offset_to_position.
#[derive(Debug, Clone, Facet)]
pub struct OffsetToPositionParams {
    /// URI of the document.
    pub document_uri: String,
    /// Byte offset.
    pub offset: u32,
}

/// Parameters for position_to_offset.
#[derive(Debug, Clone, Facet)]
pub struct PositionToOffsetParams {
    /// URI of the document.
    pub document_uri: String,
    /// Position (line/character).
    pub position: Position,
}
