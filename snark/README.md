# snark

Snark is a Tree-sitter-compatible grammar package and parser runtime foundation.

The current layer imports and preserves Tree-sitter package artifacts with
provenance. It resolves `tree-sitter.json` grammar entries, grammar-relative
`grammar.json`, generated `parser.c`, external scanner sources, configured and
fallback query files, `node-types.json`, and raw test fixtures.

Raw Tree-sitter JSON types are kept as compatibility DTOs. They are not the
validated grammar IR, not a semantic oracle, and not the parser runtime API.
The next layer lowers raw package data into typed tables with symbol ids,
external-token tables, precedence tables, conflict sets, diagnostics, and
scanner/query oracles.

See `docs/methodology.md` for the fixture and oracle policy.
