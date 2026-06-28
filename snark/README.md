# snark

Snark is a Tree-sitter-compatible grammar package and parser runtime foundation.

The current layer imports and preserves Tree-sitter package artifacts with
provenance. It resolves `tree-sitter.json` grammar entries, grammar-relative
`grammar.json`, generated `parser.c`, external scanner sources, configured and
fallback query files, `node-types.json`, and raw test fixtures.

Raw Tree-sitter JSON types are kept as compatibility DTOs. They are not the
validated grammar IR, not a semantic oracle, and not the parser runtime API.
Snark now also has a scannerless parser milestone for a deliberately small
Tree-sitter JSON subset: strings, simple patterns, symbols, sequences, choices,
repetition, extras, fields, and precedence wrappers. It can produce a reduced
named-node S-expression for tiny scannerless grammars. The next layers still
need typed symbol tables, real parse table generation, external scanner runtime
support, conflict handling, incremental parsing, and query oracles.

See `docs/methodology.md` for the fixture and oracle policy.
