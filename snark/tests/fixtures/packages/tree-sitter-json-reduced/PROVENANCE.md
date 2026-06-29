# tree-sitter-json reduced fixture provenance

- Upstream repository: <https://github.com/tree-sitter/tree-sitter-json>
- Upstream commit: `001c28d7a29832b06b0e831ec77845553c89b56d`
- Published crate checked with `cargo info tree-sitter-json`: `tree-sitter-json 0.24.8`
- License: MIT, copied as `LICENSE`

## Included files

- `tree-sitter.json`
- `src/grammar.json`
- `src/node-types.json`
- `queries/highlights.scm`
- `test/corpus/main.txt`
- `LICENSE`

## Omitted files

Generated implementation files such as `src/parser.c` and generated bindings are
omitted. Snark uses this fixture as clean-room package input and corpus oracle
data only: raw grammar metadata, query source, and expected S-expressions.
