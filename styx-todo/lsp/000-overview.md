# Styx LSP Enhancement Roadmap

## Overview

This directory contains design documents for enhancing the Styx LSP with rich IDE features.

## Documents (in implementation order)

| # | Feature | Complexity | Status |
|---|---------|------------|--------|
| [001](001-better-validation-errors.md) | Better Validation Errors | Low | **Done** |
| [002](002-document-links.md) | Document Links | Low | **Done** |
| [003](003-goto-definition.md) | Go to Definition | Medium | **Done** |
| [004](004-hover-information.md) | Hover Information | Medium | **Done** |
| [005](005-completion.md) | Auto-Completion | High | Planned |
| [006](006-code-actions.md) | Code Actions (Quick Fixes) | Medium | Planned |

## Implementation Phases

### Phase 1: Better Errors (Quick Win)
- **001 - Better Validation Errors**
  - List valid fields in error messages
  - Add typo detection with suggestions
  - Include schema location links
  - Low effort, high impact

### Phase 2: Navigation
- **002 - Document Links**
  - Make `@ schema.styx` clickable
  - Simplest navigation feature
   
- **003 - Go to Definition**
  - Ctrl+Click on field → schema definition
  - Requires schema location tracking

### Phase 3: Information
- **004 - Hover Information**
  - Show type info and docs on hover
  - Builds on schema location tracking from 003

### Phase 4: Editing Assistance
- **005 - Completion**
  - Field name completion from schema
  - Value completion for enums/bools
  - Most complex feature

- **006 - Code Actions**
  - Quick fixes for validation errors
  - Refactoring actions

## Shared Infrastructure

Several features share common needs:

### Schema Location Tracking
Required by: 001, 002, 003, 004

The schema parser needs to preserve source locations:
```rust
pub struct SchemaField {
    pub schema: Schema,
    pub span: Option<Span>,
    pub doc: Option<String>,
}
```

Options:
- Add to facet-styx parsing
- Build separate index by re-parsing schema
- Search schema source text (pragmatic)

### Node-at-Position Lookup
Required by: 002, 003, 005, 006

Helper to find what's at cursor:
```rust
enum NodeAtOffset {
    SchemaDeclaration(String),
    FieldKey(String),
    FieldValue,
    TagName(String),
}

fn find_node_at_offset(tree: &Value, offset: usize) -> Option<NodeAtOffset>;
```

### Schema Context Loading
Required by: all features

Centralized schema loading with caching:
```rust
struct SchemaCache {
    schemas: HashMap<PathBuf, (SchemaFile, String)>,  // parsed + source
}

impl SchemaCache {
    fn get_for_document(&mut self, doc: &Value, uri: &Url) -> Option<&SchemaFile>;
}
```

## File Structure

After implementation:
```
crates/styx-lsp/src/
├── server.rs          # LSP handlers
├── schema_validation.rs
├── semantic_tokens.rs
├── navigation.rs      # NEW: go-to-def, references
├── hover.rs           # NEW: hover information
├── links.rs           # NEW: document links
├── completion.rs      # NEW: auto-completion
├── code_actions.rs    # NEW: quick fixes
└── helpers/
    ├── mod.rs
    ├── position.rs    # Position/offset conversion
    ├── tree_walk.rs   # NEW: tree traversal utilities
    └── schema.rs      # NEW: schema loading/caching
```

## Dependencies

Consider adding:
- `strsim` - String similarity for typo detection
- (already have `ariadne` for error rendering)

## Testing Strategy

1. **Unit tests** for helper functions
2. **Integration tests** with LSP test client
3. **Manual testing** in Zed

Example test setup:
```rust
#[test]
fn test_goto_definition() {
    let (service, _) = LspService::new(|client| StyxLanguageServer::new(client));
    // Send initialize, didOpen, then definition request
    // Assert response contains correct location
}
```
