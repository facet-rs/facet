# snark

Snark is a Tree-sitter-compatible grammar package and Weavy lowering foundation.

The current layer imports and preserves Tree-sitter package artifacts with
provenance. It resolves `tree-sitter.json` grammar entries, grammar-relative
`grammar.json`, generated `parser.c`, external scanner sources, configured and
fallback query files, `node-types.json`, and raw test fixtures.

Raw Tree-sitter JSON types are kept as compatibility DTOs. They are not the
validated grammar IR, not a semantic oracle, and not the parser runtime API.
Snark's runtime direction is to validate Tree-sitter artifacts into typed
symbols, productions, lex modes, parser actions, scanner contracts, query facts,
and provenance maps, then lower those facts into a Snark dialect carried by
Weavy programs.

The crate also contains `milestone::scannerless`, a deliberately small smoke
parser for tiny scannerless grammars. It is not the semantic bridge to Weavy.
The next layers still need parser-table extraction/lowering, external scanner
runtime support, conflict handling, incremental parsing, and query oracles.

See `docs/methodology.md` for the fixture and oracle policy.
