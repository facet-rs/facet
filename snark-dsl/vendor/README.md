# Vendored Tree-sitter DSL Artifacts

`tree-sitter-cli@0.26.9` on npm ships the public `tree-sitter-cli/dsl`
type surface as `dsl.d.ts`, but it does not ship the runtime `dsl.js` used
by `tree-sitter generate`.

For this Boa spike:

- `tree-sitter-cli-0.26.9/dsl.d.ts` is copied from the official npm package.
- `tree-sitter-generate-0.26.9/dsl.js` is copied from the official
  `tree-sitter-generate` crate source for the same CLI release.

The Rust wrapper loads the official runtime DSL and only replaces the final
CLI-oriented `await import(grammarPath)` entrypoint with a CommonJS export
that Boa can evaluate for a local `grammar.js` fixture.
