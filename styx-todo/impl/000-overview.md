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
│  Arborium   │ facet-styx  │   styx-ls   │     styx-fmt         │
│  (editor)   │ (serde-like)│    (LSP)    │   (formatter)        │
├─────────────┼─────────────┴─────────────┴──────────────────────┤
│ tree-sitter │              Rowan CST                           │
│   grammar   │         (lossless syntax tree)                   │
├─────────────┤                  ▲                               │
│             │                  │                               │
│             │         Document Tree (styx-tree)                │
│             │                  ▲                               │
│             │                  │                               │
│             │         Event Parser (styx-parse)                │
│             │         ─────────────────────────                │
│             │         Lexer → Events → Callbacks               │
└─────────────┴──────────────────────────────────────────────────┘
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
| 006 | styx-cst (rowan) | Lossless CST for tooling |
| 007 | styx-ls | LSP server with semantic highlighting |

## Crate Structure

```
crates/
├── tree-sitter-styx/    # Phase 001 - tree-sitter grammar
├── styx-parse/          # Phase 002-003 - lexer + event parser
├── styx-tree/           # Phase 004 - document tree
├── facet-styx/          # Phase 005 + 005a - facet integration
├── styx-cst/            # Phase 006 - rowan-based CST
└── styx-ls/             # Phase 007 - LSP server
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
005 (facet) ◀── 005a    007 (lsp)                  │
                                                   │
                              ▲                    │
                              └────────────────────┘
                              (lsp can use tree-sitter for highlighting)
```

- 001 is independent (different technology)
- 002 → 003 → 004 → 005 (linear chain for facet path)
- 005a is a spec document informing 005's serializer
- 006 can start after 003 (shares lexer, different tree structure)
- 007 requires 006, can optionally integrate 001

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
