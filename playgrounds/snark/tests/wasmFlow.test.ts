import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import test from "node:test";

import { initSync, parseBundle } from "../../../snark-wasm/pkg/snark_wasm.js";
import { emitGrammarJsonFromDsl } from "../src/treeSitterDslRuntime.ts";

initSync({
  module: readFileSync(new URL("../../../snark-wasm/pkg/snark_wasm_bg.wasm", import.meta.url)),
});

const officialDsl = readFileSync(
  new URL("../../../snark-dsl/vendor/tree-sitter-generate-0.26.9/dsl.js", import.meta.url),
  "utf8",
);

test("runs a grammar.js bundle through generated grammar.json, Snark WASM, and highlights", () => {
  const grammarJs = `
module.exports = grammar({
  name: "tiny_playground",
  extras: $ => [/\\s/],
  rules: {
    document: $ => repeat1($.word),
    word: $ => /[a-z]+/,
  },
});
`;
  const grammarJson = emitGrammarJsonFromDsl(
    officialDsl,
    [{ path: "grammar.js", text: grammarJs }],
    "grammar.js",
  );

  const response = JSON.parse(
    parseBundle(
      JSON.stringify({
        files: [
          { path: "grammar.js", text: grammarJs },
          { path: "src/grammar.json", text: grammarJson },
          { path: "queries/highlights.scm", text: "(word) @variable\n" },
        ],
        input: "alpha beta",
        run_corpus: false,
      }),
    ),
  );

  assert.equal(response.ok, true);
  assert.equal(response.language, "tiny_playground");
  assert.equal(response.parse.sexp, "(document (word) (word))");
  assert.deepEqual(
    response.highlights.map((capture: { capture_name: string; text: string }) => [
      capture.capture_name,
      capture.text,
    ]),
    [
      ["variable", "alpha"],
      ["variable", "beta"],
    ],
  );
});

const arboriumNginxDef = "/Users/amos/oss/arborium/langs/group-maple/nginx/def";

test(
  "runs the Arborium nginx grammar.js sample through Snark WASM and highlights",
  { skip: existsSync(arboriumNginxDef) ? false : `${arboriumNginxDef} is not available` },
  () => {
    const grammarJs = readFileSync(`${arboriumNginxDef}/grammar/grammar.js`, "utf8");
    const highlights = readFileSync(`${arboriumNginxDef}/queries/highlights.scm`, "utf8");
    const sample = readFileSync(`${arboriumNginxDef}/samples/nginx.conf`, "utf8");
    const grammarJson = emitGrammarJsonFromDsl(
      officialDsl,
      [{ path: "grammar.js", text: grammarJs }],
      "grammar.js",
    );

    const response = JSON.parse(
      parseBundle(
        JSON.stringify({
          files: [
            { path: "grammar.js", text: grammarJs },
            { path: "src/grammar.json", text: grammarJson },
            { path: "queries/highlights.scm", text: highlights },
          ],
          input: sample,
          run_corpus: false,
        }),
      ),
    );

    assert.equal(response.ok, true, JSON.stringify(response.diagnostics, null, 2));
    assert.equal(response.language, "nginx");
    assert.match(response.parse.sexp, /^\(conf\b/);
    assert.equal(response.highlights.length, 255);
  },
);
