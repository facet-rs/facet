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

Snark validates Tree-sitter grammar semantics into typed Snark symbols,
productions, lexical rules, precedence/conflict facts, scanner contracts, query
facts, and provenance maps, then executes those facts through Weavy programs.
Correctness is checked against Tree-sitter's observable corpus S-expressions and
query/highlight assertions.

Weavy is Snark's only parser executor. The old native parser interpreters were
spike/oracle machinery and are retired; new parser behavior belongs in the
Weavy lowering/runtime path. Native copy-and-patch, host-call chains, and JIT
support are execution strategies inside Weavy, not a second Snark parser
runtime or parity target.

See `docs/methodology.md` for the fixture and oracle policy.
