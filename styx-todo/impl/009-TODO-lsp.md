# Phase 009: styx-ls (Language Server)

LSP server for Styx, providing IDE features with schema-aware semantic highlighting.

## Deliverables

- `crates/styx-ls/src/main.rs` - Server entry point
- `crates/styx-ls/src/capabilities.rs` - LSP capability negotiation
- `crates/styx-ls/src/handlers.rs` - Request/notification handlers
- `crates/styx-ls/src/semantic_tokens.rs` - Semantic highlighting
- `crates/styx-ls/src/diagnostics.rs` - Error reporting
- `crates/styx-ls/src/completion.rs` - Completions
- `crates/styx-ls/src/hover.rs` - Hover information

## Dependencies

```toml
[dependencies]
tower-lsp = "0.20"
tokio = { version = "1", features = ["full"] }
styx-cst = { path = "../styx-cst" }
styx-tree = { path = "../styx-tree" }
styx-schema = { path = "../styx-schema" }
```

## LSP Capabilities

### Must Have (Phase 9a)

- **textDocument/didOpen, didChange, didClose** - Document sync
- **textDocument/publishDiagnostics** - Syntax/semantic errors
- **textDocument/semanticTokens/full** - Semantic highlighting

### Should Have (Phase 9b)

- **textDocument/completion** - Key/value completions from schema
- **textDocument/hover** - Type info and documentation from schema
- **textDocument/formatting** - Document formatting

### Nice to Have (Phase 9c)

- **textDocument/definition** - Jump to schema definition
- **textDocument/references** - Find usages of keys
- **textDocument/rename** - Rename keys
- **textDocument/codeAction** - Quick fixes
- **textDocument/foldingRange** - Code folding

## Semantic Token Types

```rust
pub enum SemanticTokenType {
    // Standard LSP types
    Namespace,    // not used
    Type,         // tag names
    Class,        // not used
    Enum,         // enum variant tags
    Interface,    // not used
    Struct,       // object type (from schema)
    TypeParameter,// not used
    Parameter,    // not used
    Variable,     // not used
    Property,     // object keys
    EnumMember,   // enum variant (unit)
    Event,        // not used
    Function,     // not used
    Method,       // not used
    Macro,        // not used
    Keyword,      // not used (styx has no keywords)
    Modifier,     // not used
    Comment,      // comments
    String,       // scalar values
    Number,       // numeric scalars (schema-aware)
    Regexp,       // not used
    Operator,     // @ = 
    Decorator,    // doc comments
}

pub enum SemanticTokenModifier {
    Declaration,  // key that introduces a name
    Definition,   // not used
    Readonly,     // not used
    Static,       // not used
    Deprecated,   // from schema
    Abstract,     // not used
    Async,        // not used
    Modification, // not used
    Documentation,// doc comments
    DefaultLibrary,// not used
}
```

## Schema-Aware Highlighting

Without schema:
- All keys → `Property`
- All scalars → `String`
- Tags → `Type`

With schema (using styx-schema):
- Keys matching schema → `Property`
- Unknown keys → `Property` + diagnostic warning
- Scalars with type info:
  - Strings → `String`
  - Numbers → `Number`
  - Booleans → `Keyword` (or custom)
- Enum tags → `EnumMember`
- Type tags → `Type`

## Document State Management

```rust
use styx_schema::Schema;

struct DocumentState {
    uri: Url,
    version: i32,
    source: String,
    cst: Parse,
    schema: Option<Schema>,
    diagnostics: Vec<Diagnostic>,
}

struct ServerState {
    documents: HashMap<Url, DocumentState>,
    schemas: HashMap<Url, Schema>,  // cached schemas
}
```

## Incremental Updates

For large documents, use rowan's incremental reparsing:

```rust
fn apply_change(state: &mut DocumentState, change: TextDocumentContentChangeEvent) {
    // Update source text
    apply_text_edit(&mut state.source, &change);
    
    // Incremental reparse (rowan supports this)
    state.cst = reparse(&state.cst, &change);
    
    // Revalidate
    state.diagnostics = validate(&state.cst, state.schema.as_ref());
}
```

## Diagnostics

```rust
use styx_schema::{validate as schema_validate};

fn compute_diagnostics(cst: &Parse, schema: Option<&Schema>) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    
    // Syntax errors from parser
    for error in cst.errors() {
        diagnostics.push(to_lsp_diagnostic(error));
    }
    
    // Semantic validation (from styx-cst)
    for diag in styx_cst::validate(cst.syntax()) {
        diagnostics.push(to_lsp_diagnostic(diag));
    }
    
    // Schema validation (from styx-schema)
    if let Some(schema) = schema {
        let result = schema_validate(&doc, schema);
        for error in result.errors {
            diagnostics.push(to_lsp_diagnostic(&error));
        }
        for warning in result.warnings {
            diagnostics.push(to_lsp_diagnostic(&warning));
        }
    }
    
    diagnostics
}
```

## Schema Discovery

Uses `styx_schema::discover_schema()` to find schemas:

```rust
fn load_schema_for_document(uri: &Url) -> Option<Schema> {
    let path = uri.to_file_path().ok()?;
    let schema_path = styx_schema::discover_schema(&path)?;
    styx_schema::load_schema_file(&schema_path).ok()
}
```

## Completion

```rust
fn completions(
    state: &DocumentState,
    position: Position,
) -> Vec<CompletionItem> {
    let offset = position_to_offset(&state.source, position);
    let node = find_node_at_offset(state.cst.syntax(), offset);
    
    match completion_context(&node) {
        Context::ObjectKey { parent_type } => {
            // Suggest keys from schema
            schema_keys(state.schema.as_ref(), parent_type)
        }
        Context::Value { expected_type } => {
            // Suggest enum variants, booleans, etc.
            value_completions(state.schema.as_ref(), expected_type)
        }
        Context::TagName => {
            // Suggest known tags from schema
            tag_completions(state.schema.as_ref())
        }
        _ => vec![],
    }
}
```

## Hover

```rust
fn hover(state: &DocumentState, position: Position) -> Option<Hover> {
    let schema = state.schema.as_ref()?;
    let node = find_node_at_offset(...);
    
    match node.kind() {
        SyntaxKind::KEY => {
            // Look up in schema
            let key_name = node.text();
            let field = schema.lookup_field(key_name)?;
            Some(Hover {
                contents: format!("**{}**: {}\n\n{}", 
                    key_name, 
                    field.ty.display(),
                    field.doc.as_deref().unwrap_or("")
                ),
                range: node.text_range(),
            })
        }
        SyntaxKind::TAG_NAME => {
            // Look up type/enum info from schema
            let tag_name = node.text();
            let variant = schema.lookup_variant(tag_name)?;
            Some(Hover {
                contents: format!("**@{}**\n\n{}", 
                    tag_name,
                    variant.doc.as_deref().unwrap_or("")
                ),
                range: node.text_range(),
            })
        }
        _ => None,
    }
}
```

## Editor Integration

### VS Code Extension

Separate package: `vscode-styx`
- Language configuration (brackets, comments, etc.)
- TextMate grammar for basic highlighting (fallback)
- LSP client configuration
- Schema file association

### Neovim

- tree-sitter integration via tree-sitter-styx
- LSP client config for styx-ls
- Schema discovery

## Testing

- Unit tests for each handler
- Integration tests with mock LSP client
- Snapshot tests for semantic tokens
- Schema validation tests (using styx-schema fixtures)
