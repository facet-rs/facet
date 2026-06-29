# Snark Playground

The playground executes Snark's WASM `RuntimeParser` path over a declarative
Tree-sitter grammar bundle. The executable grammar input is `src/grammar.json`.

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

The playground does not execute `grammar.js` in the browser. Prepare a bundle
first:

```bash
pnpm --filter @bearcove/snark-wasm prepare-bundle \
  ~/oss/arborium/langs/group-acorn/json \
  --out /tmp/snark-json
```

That writes a playground-loadable directory containing `src/grammar.json`,
queries, samples, corpus fixtures when present, and handwritten scanner sources
when present. Arborium grammar inheritance such as
`require("tree-sitter-javascript/grammar")` is resolved from local Arborium
authored grammar sources when available. The lower-level converter is also
available:

```bash
pnpm --filter @bearcove/snark-wasm grammar-js-to-json \
  ~/oss/arborium/langs/group-acorn/json/def/grammar/grammar.js \
  --out /tmp/snark-json/src/grammar.json
```

For languages with `scanner.c`, the playground only executes scanners that have
an explicit source-matched host adapter. The reduced CSS scanner from the current
fixture is wired this way; arbitrary scanner compilation/execution in the
browser is not.
