# LSP: Polish & Minor Improvements

**Status:** TODO  
**Priority:** Medium  
**Effort:** Low

## Overview

The LSP is feature-complete. These are minor polish items.

## Potential Improvements

### Completion Enhancements
- Complete enum variants when schema expects enum type
- Complete `@true`/`@false` when schema expects `@bool`
- Complete type names inside schema files

### Hover Enhancements
- Show constraints (min/max, pattern) in hover
- Show default value if defined in schema

### Go to Definition
- Support nested field paths (currently only immediate field)
- Jump to type definition in schema files

### Code Actions
- "Extract to schema" refactoring
- "Convert quotes" (bare ↔ quoted string)

### Performance
- Incremental parsing (currently re-parses entire document)
- Cache schema files (currently re-reads on each validation)

## Notes

These are nice-to-haves. The LSP already supports all core features:
- Diagnostics, semantic tokens, hover, completion
- Go to definition, find references, document links
- Code actions, inlay hints, formatting, document symbols
