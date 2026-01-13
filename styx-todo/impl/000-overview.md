# Styx Implementation Plan

This document outlines the phased implementation of Styx parsers and tooling.

## File Naming Convention

Phase files follow this naming pattern:

- `NNN-TODO-name.md` — Not yet started
- `NNN-DONE-name.md` — Completed

As each phase is implemented, rename the file from TODO to DONE.

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        Consumers                                │
├─────────────┬─────────────┬─────────────┬──────────────────────┤
│  Arborium   │ facet-styx  │   styx-ls   │     styx-cli         │
│  (editor)   │ (serde-like)│    (LSP)    │   (jq-like CLI)      │
├─────────────┼─────────────┴─────────────┴──────────────────────┤
│ tree-sitter │              Rowan CST                           │
│   grammar   │         (lossless syntax tree)                   │
├─────────────┤                  ▲                               │
│             │                  │                               │
│             │            styx-schema                           │
│             │         (schema validation)                      │
│             │                  ▲                               │
│             │                  │                               │
│             │         Document Tree (styx-tree)                │
│             │                  ▲                               │
│             │                  │                               │
│             │         Event Parser (styx-parse)                │
│             │         ─────────────────────────                │
│             │         Lexer → Events → Callbacks               │
└─────────────┴──────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│                     Editor Extensions                           │
├─────────────────┬─────────────────┬─────────────────────────────┤
│   vscode-styx   │    zed-styx     │        vim-styx             │
│  (VS Code ext)  │   (Zed ext)     │   (Vim/Neovim plugin)       │
├─────────────────┴─────────────────┴─────────────────────────────┤
│                         styx-ls (LSP)                           │
│                      tree-sitter-styx                           │
└─────────────────────────────────────────────────────────────────┘
```

## Phases

| Phase | Deliverable | Purpose |
|-------|-------------|---------|
| 001 | tree-sitter-styx | Editor syntax highlighting, arborium integration |
| 002 | styx-parse (lexer) | Tokenization with spans |
| 003 | styx-parse (events) | Event-based parser, streaming API |
| 004 | styx-tree | Document tree built from events |
| 005 | facet-styx | Deserializer using facet traits |
| 005a | Serialization rules | Canonical output format choices |
| 005b | serde_styx | Serde integration for serialization/deserialization |
| 006 | styx-cst (rowan) | Lossless CST for tooling |
| 007 | styx-schema | Schema definition and validation library |
| 008 | styx-cli | jq-like CLI tool (query, convert, validate) |
| 009 | styx-ls | LSP server with semantic highlighting |
| 010 | vscode-styx | VS Code extension |
| 011 | zed-styx | Zed extension |
| 012 | vim-styx | Vim/Neovim support |

## Crate & Package Structure

```
crates/
├── tree-sitter-styx/    # Phase 001 - tree-sitter grammar
├── styx-parse/          # Phase 002-003 - lexer + event parser
├── styx-tree/           # Phase 004 - document tree
├── styx-format/         # Formatting utilities (used by serializers)
├── facet-styx/          # Phase 005 + 005a - facet integration
├── serde_styx/          # Phase 005b - serde integration
├── styx-cst/            # Phase 006 - rowan-based CST
├── styx-schema/         # Phase 007 - schema validation
├── styx-cli/            # Phase 008 - CLI tool
└── styx-ls/             # Phase 009 - LSP server

editors/
├── vscode-styx/         # Phase 010 - VS Code extension
├── zed-styx/            # Phase 011 - Zed extension
└── vim-styx/            # Phase 012 - Vim/Neovim plugin
```

## Dependencies Between Phases

```
001 (tree-sitter) ─────────────────────────────────┐
                                                   │ (independent)
002 (lexer)                                        │
 │                                                 │
 ▼                                                 │
003 (events)──────────────────┐                    │
 │                            │                    │
 ▼                            ▼                    │
004 (tree)               006 (cst)                 │
 │                            │                    │
 ▼                            ▼                    │
005 (facet) ◀── 005a    007 (schema)               │
 │                            │                    │
 ▼                            ├──────────┐         │
005b (serde)                  │          │         │
                              ▼          ▼         │
                         008 (cli)  009 (lsp) ◀────┤
                                         │         │
                              ┌──────────┼─────────┘
                              ▼          ▼
                         010 (vscode) 011 (zed) 012 (vim)
```

- 001 is independent (different technology)
- 002 → 003 → 004 → 005 → 005b (linear chain for facet/serde path)
- 005a is a spec document informing 005's serializer
- 006 can start after 003 (shares lexer, different tree structure)
- 007 requires 004 (validates against document tree)
- 008 requires 007 (CLI uses schema validation)
- 009 requires 006 + 007, can optionally integrate 001
- 010, 011, 012 require 009 (LSP) and optionally 001 (tree-sitter)

## Testing Strategy

Each phase includes:
- Unit tests for the component
- Integration tests using shared test fixtures in `tests/fixtures/`
- Corpus tests for tree-sitter (standard approach)

## Shared Test Fixtures

```
tests/
├── fixtures/
│   ├── valid/           # Valid styx documents
│   │   ├── simple.styx
│   │   ├── nested.styx
│   │   ├── heredoc.styx
│   │   ├── raw_strings.styx
│   │   ├── tags.styx
│   │   ├── attributes.styx
│   │   └── kubernetes.styx
│   ├── invalid/         # Documents with errors
│   │   ├── unclosed_brace.styx
│   │   ├── mixed_separators.styx
│   │   ├── invalid_escape.styx
│   │   └── duplicate_keys.styx
│   └── expected/        # Expected outputs
│       ├── simple.events.json
│       ├── simple.tree.json
│       └── ...
```
