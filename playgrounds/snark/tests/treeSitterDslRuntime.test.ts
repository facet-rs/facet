import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import { emitGrammarJsonFromDsl } from "../src/treeSitterDslRuntime.ts";

const officialDsl = readFileSync(
  new URL("../../../snark-dsl/vendor/tree-sitter-generate-0.26.9/dsl.js", import.meta.url),
  "utf8",
);

test("emits grammar.json from an uploaded CommonJS grammar.js", () => {
  const grammarJson = emitGrammarJsonFromDsl(
    officialDsl,
    [
      {
        path: "grammar.js",
        text: `
module.exports = grammar({
  name: "tiny_playground",
  extras: $ => [/\\s/],
  rules: {
    document: $ => repeat1($.word),
    word: $ => token(/[a-z]+/),
  },
});
`,
      },
    ],
    "grammar.js",
  );

  const grammar = JSON.parse(grammarJson);
  assert.equal(grammar.name, "tiny_playground");
  assert.equal(grammar.rules.document.type, "REPEAT1");
  assert.deepEqual(grammar.extras, [{ type: "PATTERN", value: "\\s" }]);
  assert.deepEqual(grammar.rules.word, {
    type: "TOKEN",
    content: { type: "PATTERN", value: "[a-z]+" },
  });
});

test("resolves relative grammar.js helper modules", () => {
  const grammarJson = emitGrammarJsonFromDsl(
    officialDsl,
    [
      {
        path: "grammar.js",
        text: `
const tokens = require("./tokens");
module.exports = grammar({
  name: "tiny_with_helper",
  rules: {
    document: $ => repeat(tokens.word),
  },
});
`,
      },
      {
        path: "tokens.js",
        text: "exports.word = /[a-z]+/;",
      },
    ],
    "grammar.js",
  );

  const grammar = JSON.parse(grammarJson);
  assert.equal(grammar.name, "tiny_with_helper");
  assert.deepEqual(grammar.rules.document, {
    type: "REPEAT",
    content: { type: "PATTERN", value: "[a-z]+" },
  });
});

test("resolves inherited Arborium grammar modules by tree-sitter package name", () => {
  const grammarJson = emitGrammarJsonFromDsl(
    officialDsl,
    [
      {
        path: "group-acorn/css/def/grammar/grammar.js",
        text: `
module.exports = grammar({
  name: "css",
  rules: {
    stylesheet: $ => "a",
  },
});
`,
      },
      {
        path: "group-acorn/scss/def/grammar/grammar.js",
        text: `
const CSS = require("tree-sitter-css/grammar");
module.exports = grammar(CSS, {
  name: "scss",
  rules: {
    stylesheet: $ => "b",
  },
});
`,
      },
    ],
    "group-acorn/scss/def/grammar/grammar.js",
  );

  const grammar = JSON.parse(grammarJson);
  assert.equal(grammar.name, "scss");
  assert.deepEqual(grammar.rules.stylesheet, {
    type: "STRING",
    value: "b",
  });
});
