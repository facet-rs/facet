# tree-sitter-css Reduced Fixture

- Upstream repository: `https://github.com/tree-sitter/tree-sitter-css`
- Local source checkout: `/Users/amos/oss/tree-sitter-css`
- Upstream commit: `dda5cfc5722c429eaba1c910ca32c2c0c5bb1a3f`
- Upstream package version: `0.25.0`
- Upstream generator dependency: `tree-sitter-cli ^0.25.10`
- Upstream license: MIT, preserved in `LICENSE`
- Purpose: pinned Tree-sitter package import fixture for Snark's raw package boundary.
- Refresh recipe: copy the included file list from the pinned upstream checkout,
  then remove generated implementation files such as `src/parser.c`.

Included files:

- `tree-sitter.json`
- `LICENSE`
- `src/grammar.json`
- `src/scanner.c`
- `queries/highlights.scm`
- `test/corpus/declarations.txt`
- `test/corpus/selectors.txt`
- `test/corpus/statements.txt`
- `test/corpus/stylesheets.txt`
- `test/highlight/test_css.css`

Omitted files:

- Generated implementation files such as `src/parser.c`; Snark does not inspect
  Tree-sitter's generated parser implementation.
- Generated metadata files such as `src/node-types.json`; Snark derives public
  node facts from the frozen `grammar.json`.
- Language bindings, package-manager metadata, examples, scripts, and tests
  outside the selected parse/highlight oracle fixture slice.
