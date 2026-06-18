+++
title = "LSP Extensions"
weight = 5
slug = "lsp-extensions"
insert_anchor_links = "heading"
+++

LSP extensions allow external tools to provide domain-specific intelligence to the Styx language server.
The Styx LSP acts as a host that delegates certain operations to extension processes that understand
the semantics of specific schemas.

## Motivation

Consider a query DSL embedded in Styx:

```styx
AllProducts @query {
    from product
    where {deleted_at @null}
    select {id, handle, status}
}
```

The Styx LSP understands the syntax and structure, but cannot know valid column names for the `product` table.
An extension (e.g., the `dibs` CLI) can connect to the database, read the schema, and provide:

- **Completions**: suggest `status`, `handle`, `created_at` in the `select` block
- **Hover**: show column type, constraints, and comments
- **Inlay hints**: display column types inline: `id`: `uuid`, `handle`: `varchar(255)`
- **Diagnostics**: warn that `statsu` is not a valid column
- **Code actions**: offer to fix typos, add missing columns

## Extension declaration

> r[lsp-ext.declaration]
> A schema MAY declare LSP extension capabilities in the `meta.lsp` block.
> The `launch` field specifies how to start the extension process.
>
> ```styx
> meta {
>   id https://example.com/schemas/dibs-query
>   version 2026-01-20
>   lsp {
>     launch "dibs lsp-extension"
>   }
> }
> ```

> r[lsp-ext.declaration.launch]
> The `launch` field is either a string (shell command) or a sequence of strings (command and arguments).
>
> ```styx
> // Simple form
> lsp {launch "dibs lsp-extension"}
>
> // With explicit arguments
> lsp {launch ["dibs", "lsp-extension", "--stdio"]}
> ```

> r[lsp-ext.declaration.capabilities]
> The `capabilities` field optionally declares which LSP features the extension supports.
> If omitted, the LSP discovers capabilities during the handshake.
>
> ```styx
> lsp {
>   launch "dibs lsp-extension"
>   capabilities [completions, hover, inlay_hints, diagnostics, code_actions]
> }
> ```

## Transport

> r[lsp-ext.transport]
> Extensions communicate with the Styx LSP using [Roam](https://roam.bearcove.eu/),
> a Rust-native bidirectional RPC protocol. The extension process is spawned and communicates
> over standard input/output.

> r[lsp-ext.transport.roam]
> Roam uses Rust traits as the schema. All types implement [Facet](https://facet.rs/) for
> serialization via facet-postcard. Both sides can call methods on each other.

## Service definitions

> r[lsp-ext.service]
> The extension implements the `StyxLspExtension` service trait.
> The LSP implements the `StyxLspHost` service trait for callbacks.

### Extension service (LSP → Extension)

```rust
use facet::Facet;
use roam::service;

/// Service implemented by LSP extensions.
#[service]
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

    /// Shutdown the extension gracefully.
    async fn shutdown(&self);
}
```

### Host service (Extension → LSP)

```rust
/// Service implemented by the Styx LSP for extension callbacks.
#[service]
pub trait StyxLspHost {
    /// Get a subtree of the document at a path.
    async fn get_subtree(&self, path: Vec<String>) -> Option<Value>;

    /// Get the full document tree.
    async fn get_document(&self) -> Value;

    /// Get the raw source text.
    async fn get_source(&self) -> String;

    /// Get the schema source and URI.
    async fn get_schema(&self) -> SchemaInfo;

    /// Convert byte offset to line/character position.
    async fn offset_to_position(&self, offset: u32) -> Position;

    /// Convert line/character position to byte offset.
    async fn position_to_offset(&self, position: Position) -> u32;
}
```

## Types

> r[lsp-ext.types]
> All types derive `Facet` for Roam serialization.

### Initialization

```rust
#[derive(Facet)]
pub struct InitializeParams {
    pub styx_version: String,
    pub document_uri: String,
    pub schema_id: String,
}

#[derive(Facet)]
pub struct InitializeResult {
    pub name: String,
    pub version: String,
    pub capabilities: Vec<Capability>,
}

#[derive(Facet)]
#[repr(u8)]
pub enum Capability {
    Completions = 0,
    Hover = 1,
    InlayHints = 2,
    Diagnostics = 3,
    CodeActions = 4,
    Definition = 5,
}
```

### Positions and ranges

```rust
#[derive(Facet)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

#[derive(Facet)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

#[derive(Facet)]
pub struct Cursor {
    pub line: u32,
    pub character: u32,
    pub offset: u32,
}
```

### Completions

> r[lsp-ext.types.completions]
> Completion requests include cursor position, path in the document tree, and relevant context.

```rust
#[derive(Facet)]
pub struct CompletionParams {
    pub document_uri: String,
    pub cursor: Cursor,
    /// Path to the current location in the document tree.
    /// e.g., ["AllProducts", "@query", "select"]
    pub path: Vec<String>,
    /// Text the user has typed (for filtering).
    pub prefix: String,
    /// The subtree relevant to this completion.
    pub context: Value,
}

#[derive(Facet)]
pub struct CompletionItem {
    /// The text to insert.
    pub label: String,
    /// Short description (e.g., column type).
    pub detail: Option<String>,
    /// Longer description.
    pub documentation: Option<String>,
    /// Item kind for icon selection.
    pub kind: Option<CompletionKind>,
    /// Override sort order.
    pub sort_text: Option<String>,
    /// Text to insert if different from label.
    pub insert_text: Option<String>,
}

#[derive(Facet)]
#[repr(u8)]
pub enum CompletionKind {
    Field = 0,
    Value = 1,
    Keyword = 2,
    Type = 3,
}
```

### Hover

```rust
#[derive(Facet)]
pub struct HoverParams {
    pub document_uri: String,
    pub cursor: Cursor,
    pub path: Vec<String>,
    pub context: Value,
}

#[derive(Facet)]
pub struct HoverResult {
    /// Markdown content to display.
    pub contents: String,
    /// Range to highlight (optional).
    pub range: Option<Range>,
}
```

### Inlay hints

```rust
#[derive(Facet)]
pub struct InlayHintParams {
    pub document_uri: String,
    pub range: Range,
    pub context: Value,
}

#[derive(Facet)]
pub struct InlayHint {
    pub position: Position,
    pub label: String,
    pub kind: Option<InlayHintKind>,
    pub padding_left: bool,
    pub padding_right: bool,
}

#[derive(Facet)]
#[repr(u8)]
pub enum InlayHintKind {
    Type = 0,
    Parameter = 1,
}
```

### Diagnostics

```rust
#[derive(Facet)]
pub struct DiagnosticParams {
    pub document_uri: String,
    /// The full document tree.
    pub tree: Value,
}

#[derive(Facet)]
pub struct Diagnostic {
    pub range: Range,
    pub severity: DiagnosticSeverity,
    pub message: String,
    pub source: Option<String>,
    pub code: Option<String>,
    /// Arbitrary data for code actions.
    pub data: Option<Value>,
}

#[derive(Facet)]
#[repr(u8)]
pub enum DiagnosticSeverity {
    Error = 0,
    Warning = 1,
    Info = 2,
    Hint = 3,
}
```

### Code actions

```rust
#[derive(Facet)]
pub struct CodeActionParams {
    pub document_uri: String,
    pub range: Range,
    /// Diagnostics at this range (for context).
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Facet)]
pub struct CodeAction {
    pub title: String,
    pub kind: Option<CodeActionKind>,
    pub edit: Option<WorkspaceEdit>,
    pub is_preferred: bool,
}

#[derive(Facet)]
#[repr(u8)]
pub enum CodeActionKind {
    QuickFix = 0,
    Refactor = 1,
    Source = 2,
}

#[derive(Facet)]
pub struct WorkspaceEdit {
    pub changes: Vec<DocumentEdit>,
}

#[derive(Facet)]
pub struct DocumentEdit {
    pub uri: String,
    pub edits: Vec<TextEdit>,
}

#[derive(Facet)]
pub struct TextEdit {
    pub range: Range,
    pub new_text: String,
}
```

### Host callbacks

```rust
#[derive(Facet)]
pub struct SchemaInfo {
    pub source: String,
    pub uri: String,
}

/// Document tree value - opaque to Roam, interpreted by the extension.
/// This is the Styx document parsed into a tree structure.
pub type Value = facet_styx::Value;
```

## Security

> r[lsp-ext.security]
> Extensions can execute arbitrary code. The LSP MUST obtain user consent before launching extensions.

> r[lsp-ext.security.allowlist]
> User consent is stored in the user-wide Styx configuration at `~/.config/styx/config.styx`.
>
> ```styx
> @schema {id styx:lsp-config@1}
>
> extensions {
>     allow [
>         "dibs"
>         "another-tool"
>     ]
> }
> ```

> r[lsp-ext.security.prompt]
> When the LSP encounters a schema with an extension that is not in the allowlist:
>
> 1. Display a diagnostic: "This schema wants to use 'dibs' as an LSP extension"
> 2. Offer code actions: "Allow 'dibs'" and "Never ask for 'dibs'"
> 3. On "Allow": add to the allowlist and spawn the extension
> 4. On "Never ask": add to a denylist (not spawned, no future prompts)

> r[lsp-ext.security.denylist]
> Extensions can also be explicitly denied:
>
> ```styx
> extensions {
>     allow ["dibs"]
>     deny ["untrusted-tool"]
> }
> ```

## Lifecycle

> r[lsp-ext.lifecycle.spawn]
> Extensions are spawned lazily on first use (e.g., when completions are requested).

> r[lsp-ext.lifecycle.reuse]
> Once spawned, an extension process is reused for subsequent requests in the same session.

> r[lsp-ext.lifecycle.shutdown]
> The LSP calls the `shutdown` method when closing.
> The extension should exit gracefully. If it doesn't respond within a timeout, the LSP terminates it.

> r[lsp-ext.lifecycle.crash]
> If an extension crashes, the LSP:
>
> 1. Logs the error
> 2. Falls back to no extension (graceful degradation)
> 3. May display a diagnostic to the user
> 4. Does NOT block the main LSP functionality

## Example: Complete flow

1. User opens `queries.styx` with `@schema {cli dibs, meta {lsp {launch "dibs lsp-extension"}}}`
2. LSP detects extension, checks allowlist → "dibs" is allowed
3. User types in `select {}`, triggers completion
4. LSP spawns `dibs lsp-extension`, establishes Roam session
5. LSP calls `initialize(InitializeParams {...})`
6. Extension returns `InitializeResult { capabilities: [Completions, Hover, Diagnostics], ... }`
7. LSP calls `completions(CompletionParams { path: ["AllProducts", "@query", "select"], ... })`
8. Extension needs more context, calls back `get_subtree(["AllProducts", "@query"])`
9. LSP returns the subtree
10. Extension queries database schema, finds `product` table columns
11. Extension returns `Vec<CompletionItem>` with `id`, `handle`, `status`, `created_at`, etc.
12. LSP merges with any native completions, returns to editor
13. User sees column suggestions with types
