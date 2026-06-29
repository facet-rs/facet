# snark

Snark is a Tree-sitter-compatible grammar package and Weavy lowering foundation.

The current layer imports and preserves Tree-sitter package inputs with
provenance. It resolves `tree-sitter.json` grammar entries, grammar-relative
`grammar.json`, external scanner sources, configured and fallback query files,
and raw corpus/highlight fixtures.

Raw Tree-sitter JSON types are kept as compatibility DTOs. They are not the
validated grammar IR, not a semantic oracle, and not the parser runtime API.
Generated Tree-sitter implementation and metadata files such as `src/parser.c`
and `src/node-types.json` are not Snark inputs, oracle data, or implementation
references. Snark derives public node facts from the frozen `grammar.json`.

Snark's runtime direction is to validate Tree-sitter grammar semantics into
typed Snark symbols, productions, lexical rules, precedence/conflict facts,
scanner contracts, query facts, and provenance maps, then lower those facts into
a Snark dialect carried by Weavy programs. Correctness is checked against
Tree-sitter's observable corpus S-expressions and query/highlight assertions.

The crate also contains `milestone::scannerless`, a deliberately small smoke
parser for tiny scannerless grammars. It is not the semantic bridge to Weavy.
The next layers still need validated grammar IR, lexer/parser generation,
external scanner runtime support, conflict handling, incremental parsing, and
query oracles.

See `docs/methodology.md` for the fixture and oracle policy.
