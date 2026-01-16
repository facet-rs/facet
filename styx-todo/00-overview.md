# Styx TODO Overview

## Completed ✅

All foundational work is done:

- **Parsing:** Lexer, event parser, tree builder, CST (Rowan-based)
- **Validation:** Duplicate keys, separator enforcement, tag validation
- **Schema:** Schema language, validator, Rust type derivation
- **Serialization:** facet-styx, serde-styx, styx-format
- **Tree-sitter:** Grammar for syntax highlighting
- **Diagnostics:** ariadne-based error rendering

### LSP Features (fully implemented)

- ✅ Diagnostics (parse errors, CST validation, schema validation)
- ✅ Semantic tokens (syntax highlighting)
- ✅ Hover (breadcrumb path, type info, doc comments, schema link)
- ✅ Auto-completion (field names with fuzzy matching, required fields first)
- ✅ Go to definition (field → schema, schema declaration → file)
- ✅ Find references (field usages across documents)
- ✅ Document links (clickable schema paths)
- ✅ Code actions (rename typo quickfix, fill required/all fields, reorder fields, separator toggle)
- ✅ Inlay hints (schema name/version after declaration)
- ✅ Document formatting (via styx-format, preserves comments)
- ✅ Document symbols (outline view)

### CLI Features

- ✅ `styx <file>` - format, convert to JSON, validate
- ✅ `styx @tree` - debug parse tree
- ✅ `styx @lsp` - start language server
- ⏳ `styx @diff` - structural diff (placeholder)

## Remaining Work

### Core

| # | Task | Priority | Effort |
|---|------|----------|--------|
| 01 | [LSP Polish](01-lsp-polish.md) | Low | Low |
| 02 | [CLI Subcommands](02-cli-subcommands.md) | Low | Low |
| 03 | [Editor Extensions](03-editor-extensions.md) | Medium | Medium |
| 04 | [Fuzzing](04-fuzzing.md) | Low | Low |
| 05 | [Future Ideas](05-future-ideas.md) | Backlog | — |

### Ecosystem

| # | Task | Priority | Effort |
|---|------|----------|--------|
| 06 | [Language Bindings](06-language-bindings.md) | Medium | High |
| 07 | [Arborium Integration](07-arborium-integration.md) | Medium | Medium |
| 08 | [MIME Type](08-mime-type.md) | Low | Low |
| 09 | [Ecosystem & Adoption](09-ecosystem.md) | Low | Ongoing |

## Recommended Order

1. **Editor Extensions** — Zed polish, VS Code, Neovim
2. **Language Bindings** — Python and JS for wider adoption
3. **Arborium Integration** — Visual editing with schema support
4. Everything else as needed
