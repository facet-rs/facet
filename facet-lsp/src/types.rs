//! Narrow LSP 3.17 type subset shared by language-server implementations.

use std::collections::BTreeMap;

use facet::Facet;

/// LSP URI string.
pub type DocumentUri = String;

/// LSP position, zero-based line and UTF-16 code-unit character.
#[derive(Clone, Copy, Debug, Facet, PartialEq, Eq, PartialOrd, Ord)]
pub struct Position {
    /// Zero-based line.
    pub line: u32,
    /// Zero-based UTF-16 character.
    pub character: u32,
}

/// Half-open LSP range.
#[derive(Clone, Copy, Debug, Facet, PartialEq, Eq)]
pub struct Range {
    /// Start position.
    pub start: Position,
    /// End position.
    pub end: Position,
}

/// Text document identifier.
#[derive(Clone, Debug, Facet, PartialEq, Eq)]
pub struct TextDocumentIdentifier {
    /// Document URI.
    pub uri: DocumentUri,
}

/// Versioned text document identifier.
#[derive(Clone, Debug, Facet, PartialEq, Eq)]
pub struct VersionedTextDocumentIdentifier {
    /// Document URI.
    pub uri: DocumentUri,
    /// Document version.
    pub version: i32,
}

/// Text document item sent on open.
#[derive(Clone, Debug, Facet, PartialEq, Eq)]
#[facet(rename_all = "camelCase")]
pub struct TextDocumentItem {
    /// Document URI.
    pub uri: DocumentUri,
    /// Language id.
    pub language_id: String,
    /// Document version.
    pub version: i32,
    /// Full document text.
    pub text: String,
}

/// `textDocument/didOpen` params.
#[derive(Clone, Debug, Facet, PartialEq, Eq)]
#[facet(rename_all = "camelCase")]
pub struct DidOpenTextDocumentParams {
    /// Opened document.
    pub text_document: TextDocumentItem,
}

/// Full-sync content change event.
#[derive(Clone, Debug, Facet, PartialEq, Eq)]
#[facet(rename_all = "camelCase")]
pub struct TextDocumentContentChangeEvent {
    /// Replacement text.
    pub text: String,
}

/// `textDocument/didChange` params.
#[derive(Clone, Debug, Facet, PartialEq, Eq)]
#[facet(rename_all = "camelCase")]
pub struct DidChangeTextDocumentParams {
    /// Changed document.
    pub text_document: VersionedTextDocumentIdentifier,
    /// Content changes.
    pub content_changes: Vec<TextDocumentContentChangeEvent>,
}

/// `textDocument/didClose` params.
#[derive(Clone, Debug, Facet, PartialEq, Eq)]
#[facet(rename_all = "camelCase")]
pub struct DidCloseTextDocumentParams {
    /// Closed document.
    pub text_document: TextDocumentIdentifier,
}

/// Position request params.
#[derive(Clone, Debug, Facet, PartialEq, Eq)]
#[facet(rename_all = "camelCase")]
pub struct TextDocumentPositionParams {
    /// Target document.
    pub text_document: TextDocumentIdentifier,
    /// Target position.
    pub position: Position,
}

/// Reference request params.
#[derive(Clone, Debug, Facet, PartialEq, Eq)]
#[facet(rename_all = "camelCase")]
pub struct ReferenceParams {
    /// Target document and position.
    pub text_document: TextDocumentIdentifier,
    /// Target position.
    pub position: Position,
    /// Reference context.
    pub context: ReferenceContext,
}

/// Reference context.
#[derive(Clone, Debug, Facet, PartialEq, Eq)]
#[facet(rename_all = "camelCase")]
pub struct ReferenceContext {
    /// Whether to include declaration.
    pub include_declaration: bool,
}

/// Rename request params.
#[derive(Clone, Debug, Facet, PartialEq, Eq)]
#[facet(rename_all = "camelCase")]
pub struct RenameParams {
    /// Target document.
    pub text_document: TextDocumentIdentifier,
    /// Target position.
    pub position: Position,
    /// New symbol name.
    pub new_name: String,
}

/// Initialize request params. Kept raw-minimal; current server does not inspect
/// client capabilities.
#[derive(Clone, Debug, Default, Facet, PartialEq, Eq)]
#[facet(rename_all = "camelCase")]
pub struct InitializeParams {
    /// Optional process id.
    pub process_id: Option<i64>,
    /// Optional root URI.
    pub root_uri: Option<DocumentUri>,
}

/// Empty initialized params.
#[derive(Clone, Debug, Default, Facet, PartialEq, Eq)]
pub struct InitializedParams {}

/// Initialize result.
#[derive(Clone, Debug, Facet, PartialEq, Eq)]
#[facet(rename_all = "camelCase")]
pub struct InitializeResult {
    /// Server capabilities.
    pub capabilities: ServerCapabilities,
    /// Server info.
    pub server_info: ServerInfo,
}

/// Server info.
#[derive(Clone, Debug, Facet, PartialEq, Eq)]
pub struct ServerInfo {
    /// Server name.
    pub name: String,
    /// Server version.
    pub version: Option<String>,
}

/// Server capabilities used by Vix.
#[derive(Clone, Debug, Facet, PartialEq, Eq)]
#[facet(rename_all = "camelCase")]
pub struct ServerCapabilities {
    /// Text document sync mode.
    pub text_document_sync: i32,
    /// Definition support.
    pub definition_provider: bool,
    /// References support.
    pub references_provider: bool,
    /// Document highlight support.
    pub document_highlight_provider: bool,
    /// Rename support.
    pub rename_provider: bool,
    /// Hover support.
    pub hover_provider: bool,
    /// Semantic token support.
    pub semantic_tokens_provider: SemanticTokensOptions,
}

/// Semantic tokens options.
#[derive(Clone, Debug, Facet, PartialEq, Eq)]
#[facet(rename_all = "camelCase")]
pub struct SemanticTokensOptions {
    /// Token legend.
    pub legend: SemanticTokensLegend,
    /// Full document semantic tokens.
    pub full: bool,
}

/// Semantic tokens legend.
#[derive(Clone, Debug, Facet, PartialEq, Eq)]
#[facet(rename_all = "camelCase")]
pub struct SemanticTokensLegend {
    /// Token type names.
    pub token_types: Vec<String>,
    /// Token modifier names.
    pub token_modifiers: Vec<String>,
}

/// Location result.
#[derive(Clone, Debug, Facet, PartialEq, Eq)]
pub struct Location {
    /// Document URI.
    pub uri: DocumentUri,
    /// Target range.
    pub range: Range,
}

/// Document highlight.
#[derive(Clone, Debug, Facet, PartialEq, Eq)]
pub struct DocumentHighlight {
    /// Highlight range.
    pub range: Range,
    /// Highlight kind.
    pub kind: Option<i32>,
}

/// Text edit.
#[derive(Clone, Debug, Facet, PartialEq, Eq)]
#[facet(rename_all = "camelCase")]
pub struct TextEdit {
    /// Replaced range.
    pub range: Range,
    /// Replacement text.
    pub new_text: String,
}

/// Workspace edit.
#[derive(Clone, Debug, Facet, PartialEq, Eq)]
pub struct WorkspaceEdit {
    /// Per-document edits.
    pub changes: BTreeMap<DocumentUri, Vec<TextEdit>>,
}

/// Hover result.
#[derive(Clone, Debug, Facet, PartialEq, Eq)]
pub struct Hover {
    /// Hover contents.
    pub contents: MarkupContent,
    /// Hover range.
    pub range: Option<Range>,
}

/// Markup content.
#[derive(Clone, Debug, Facet, PartialEq, Eq)]
pub struct MarkupContent {
    /// Markup kind.
    pub kind: String,
    /// Markup value.
    pub value: String,
}

/// Diagnostic.
#[derive(Clone, Debug, Facet, PartialEq, Eq)]
pub struct Diagnostic {
    /// Diagnostic range.
    pub range: Range,
    /// Severity, per LSP.
    pub severity: Option<i32>,
    /// Diagnostic source.
    pub source: Option<String>,
    /// Message.
    pub message: String,
}

/// Publish diagnostics params.
#[derive(Clone, Debug, Facet, PartialEq, Eq)]
pub struct PublishDiagnosticsParams {
    /// Document URI.
    pub uri: DocumentUri,
    /// Diagnostics.
    pub diagnostics: Vec<Diagnostic>,
}

/// Semantic tokens params.
#[derive(Clone, Debug, Facet, PartialEq, Eq)]
#[facet(rename_all = "camelCase")]
pub struct SemanticTokensParams {
    /// Target document.
    pub text_document: TextDocumentIdentifier,
}

/// Semantic tokens result.
#[derive(Clone, Debug, Facet, PartialEq, Eq)]
pub struct SemanticTokens {
    /// LSP 5-int delta-encoded token data.
    pub data: Vec<u32>,
}
