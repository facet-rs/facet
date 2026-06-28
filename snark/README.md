# snark

Snark is a Tree-sitter-compatible grammar package and parser runtime foundation.

The current layer imports and preserves Tree-sitter package artifacts with
provenance: `grammar.json`, `tree-sitter.json`, generated `parser.c`,
external scanner sources, query files, `node-types.json`, and test fixtures.

Raw Tree-sitter JSON types are kept as compatibility DTOs. They are not the
validated grammar IR and are not the parser runtime API. The next layer lowers
raw package data into typed tables with symbol ids, external-token ordinals,
precedence tables, conflict sets, diagnostics, and scanner/query oracles.

See `docs/methodology.md` for the fixture and oracle policy.
