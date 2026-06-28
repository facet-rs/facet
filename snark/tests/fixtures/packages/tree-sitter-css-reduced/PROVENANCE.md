# tree-sitter-css Reduced Fixture

- Upstream repository: `/Users/amos/oss/tree-sitter-css`
- Upstream commit: `dda5cfc5722c429eaba1c910ca32c2c0c5bb1a3f`
- Upstream package version: `0.25.0`
- Upstream generator dependency: `tree-sitter-cli ^0.25.10`
- Upstream license: MIT, preserved in `LICENSE`
- Purpose: pinned Tree-sitter package import oracle for Snark's raw package boundary.

Included files:

- `tree-sitter.json`
- `LICENSE`
- `src/grammar.json`
- `src/node-types.json`
- `src/scanner.c`
- `queries/highlights.scm`
- `test/highlight/test_css.css`

Omitted files:

- Generated implementation files such as `src/parser.c`; Snark does not inspect
  Tree-sitter's generated parser implementation.
- Language bindings, package-manager metadata, examples, scripts, and additional
  tests not needed by the raw package import boundary.
