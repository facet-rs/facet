# Snark Methodology

Snark uses Tree-sitter's observable test outputs as the compatibility oracle.
Raw package import is only the first boundary; lowering, scanner execution,
query captures, parse trees, error recovery, and incremental edits must be
compared against upstream Tree-sitter corpus S-expressions and query/highlight
assertions before they become stable Snark behavior.

Generated Tree-sitter implementation files, including `src/parser.c`, are not
Snark inputs, not oracle data, and not implementation references. Snark derives
its parser from grammar semantics, then checks the resulting behavior against
Tree-sitter's public test surface.

## Boundaries

- `grammar` owns raw `grammar.json` DTOs and later validated grammar tables.
- `lower` owns the validated Snark grammar IR to Weavy lowering boundary.
- `tree_sitter` owns filesystem package import and provenance.
- `scanner`, `query`, and `corpus` own imported artifacts for their domains.
- `runtime_input` owns editor/runtime coordinate types.
- `milestone` owns non-foundational proof artifacts and smoke parsers.
- raw import artifacts are not runtime language objects.
- generated Tree-sitter implementation artifacts are not imported.
- recursive scannerless milestone behavior is not Snark parser semantics.
- the pinned fixture lane proves raw import and package-layout contracts; it is
  not a substitute for semantic parse/query/scanner oracles.

## Fixture Lanes

Always-on tests should use pinned package fixtures checked into this crate.
Each fixture must record upstream repository, commit, generator version, and
which files were included or intentionally omitted.

Opt-in tests may read full local upstream checkouts such as
`SNARK_TREE_SITTER_CSS=/Users/amos/oss/tree-sitter-css`, but those tests are
additional confidence only. They must not be the only oracle for a contract.

## Oracles

For each implemented layer, compare Snark output with Tree-sitter output:

- grammar import: observed fields, rule order, externals order, package assets
- package import: manifest grammar paths, configured query order, source
  containment, and artifact provenance
- scannerless parser milestone: tiny Tree-sitter JSON subset to reduced
  named-node S-expression, explicitly quarantined from runtime semantics
- corpus import: named examples, inputs, expected trees, highlight assertions
- grammar validation: normalized Snark symbols, productions, precedence,
  conflicts, tokens, fields, aliases, externals, supertypes, and reserved words
- parser generation: Snark lexer/parser automata derived from the validated
  grammar, including recovery and incremental state facts
- Weavy lowering: Snark intrinsic programs, block identities, effect contracts,
  provenance maps, and reduced execution traces
- scanner runtime: valid-symbol inputs, accepted tokens, serialized state
- query runtime: capture names, byte ranges, predicates, injections
- incremental parsing: changed ranges, error nodes, and final tree equivalence

Use structured values for comparisons. Prefer `rediff` for value diffs and
snapshot reduced oracle outputs only when the output is intentionally stable.

## Diagnostics

Every imported source receives a `SourceId` and package-relative path. New
semantic diagnostics should carry a source id, primary byte span, code,
severity, labels, and notes. Rendered diagnostic text is a view over structured
data, not the diagnostic contract itself.

Malformed input tests should assert structured error fields first and rendered
text second.
