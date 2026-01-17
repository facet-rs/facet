# CLI: Additional Subcommands

**Status:** Partially Done  
**Priority:** Low

## Current State

styx-cli has:
- ✅ `styx <file>` - format, convert to JSON, validate
- ✅ `styx @tree` - debug parse tree visualization
- ✅ `styx @lsp` - start language server
- ⏳ `styx @diff` - structural diff (placeholder exists)

## Remaining

### `styx @diff`

Structural diff between two styx documents.

```
styx @diff a.styx b.styx
```

Output shows:
- Added fields (green +)
- Removed fields (red -)
- Changed values (yellow ~)

Ignores formatting differences, compares semantic content.

### `styx @query` (maybe)

Extract values using path expressions.

```
styx @query file.styx "server.port"
```

Output: the value at that path, or error if not found.
