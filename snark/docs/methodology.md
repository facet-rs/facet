# Snark Methodology

Snark uses Tree-sitter as the compatibility oracle. Raw package import is only
the first boundary; lowering, scanner execution, query captures, parse trees,
error recovery, and incremental edits must be compared against upstream
Tree-sitter behavior before they become stable Snark behavior.

## Boundaries

- `grammar` owns raw `grammar.json` DTOs and later validated grammar tables.
- `tree_sitter` owns filesystem package import and provenance.
- `scanner`, `query`, and `corpus` own imported artifacts for their domains.
- `runtime_input` owns editor/runtime coordinate types.
- raw import artifacts are not runtime language objects.

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
- corpus import: named examples, inputs, expected trees, highlight assertions
- parser lowering: normalized symbols, precedence, conflicts, tokens, fields
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
