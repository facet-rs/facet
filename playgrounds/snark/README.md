# Snark Playground

The playground executes Snark's WASM `RuntimeParser` path over a Tree-sitter
grammar bundle. It accepts either frozen `src/grammar.json` or authored
`grammar.js`. When `grammar.js` is present without `src/grammar.json`, the
browser shell evaluates it in a Worker with the vendored official Tree-sitter
DSL runtime and passes the resulting in-memory `src/grammar.json` to Snark.

Generated Tree-sitter implementation and metadata files are ignored:
`src/parser.c`, `src/parser.cc`, `src/parser.h`, `src/node-types.json`, and
`bindings/node/binding.cc` are not inputs.

## Authored `grammar.js` Bundles

Arborium language sources usually look like this:

```text
langs/group-acorn/json/def/grammar/grammar.js
langs/group-acorn/json/def/queries/highlights.scm
```

Normal Tree-sitter package sources usually keep `grammar.js` at the package
root, queries under `queries/`, corpus fixtures under `test/corpus/`, and
handwritten scanners under `src/scanner.c`.

Load the package/source directory directly. Browser upload path normalization
understands normal Tree-sitter package roots and Arborium `def/` source roots.
Arborium grammar inheritance such as `require("tree-sitter-javascript/grammar")`
can be resolved when the required authored grammar source is included in the
uploaded directory.

For languages with `scanner.c`, the playground only executes scanners that have
an explicit source-matched host adapter. The reduced CSS scanner from the current
fixture is wired this way; arbitrary scanner compilation/execution in the
browser is not.
