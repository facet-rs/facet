# Snark Playground

The playground executes Snark's WASM `RuntimeParser` path over a declarative
Tree-sitter grammar bundle. The executable grammar input is `src/grammar.json`.

Generated Tree-sitter implementation and metadata files are ignored:
`src/parser.c`, `src/parser.cc`, `src/parser.h`, `src/node-types.json`, and
`bindings/node/binding.cc` are not inputs.

## Arborium Source Bundles

Arborium language sources usually look like this:

```text
langs/group-acorn/json/def/grammar/grammar.js
langs/group-acorn/json/def/queries/highlights.scm
```

The playground normalizes `def/queries/...`, `def/test/...`, samples, and
handwritten scanner sources into the package paths Snark expects. It does not
execute `grammar.js` in the browser.

Convert an authored grammar first:

```bash
pnpm --filter @bearcove/snark-wasm grammar-js-to-json \
  ~/oss/arborium/langs/group-acorn/json/def/grammar/grammar.js \
  --out /tmp/snark-json/src/grammar.json
```

Then add the query/corpus files you want beside that generated declarative
input and load the resulting directory in the playground.

For languages with `scanner.c`, the file is reported as part of the bundle, but
browser-side external scanner execution is not wired into this playground yet.
