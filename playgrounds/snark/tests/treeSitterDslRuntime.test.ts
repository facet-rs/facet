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

test("emits Snark lexical primitive nodes from grammar.js", () => {
  const grammarJson = emitGrammarJsonFromDsl(
    officialDsl,
    [
      {
        path: "grammar.js",
        text: `
module.exports = grammar({
  name: "tiny_primitives",
  rules: {
    document: $ => seq(token(until("{{", "{#")), token(nested("{#", "#}"))),
  },
});
`,
      },
    ],
    "grammar.js",
  );

  const grammar = JSON.parse(grammarJson);
  assert.deepEqual(grammar.rules.document.members[0], {
    type: "TOKEN",
    content: {
      type: "UNTIL",
      markers: ["{{", "{#"],
    },
  });
  assert.deepEqual(grammar.rules.document.members[1], {
    type: "TOKEN",
    content: {
      type: "NESTED",
      open: "{#",
      close: "#}",
    },
  });
});

test("emits Snark auto_close primitive nodes from grammar.js", () => {
  const grammarJson = emitGrammarJsonFromDsl(
    officialDsl,
    [
      {
        path: "grammar.js",
        text: `
module.exports = grammar({
  name: "tiny_auto_close",
  rules: {
    document: $ => seq("<p>", auto_close({
      tag: "implicit_end_tag",
      open_node: "start_tag",
      close_node: "end_tag",
      tag_name_node: "tag_name",
      start_prefix: "<",
      end_prefix: "</",
      rules: [
        { tag: "p", closed_by_tags: ["p", "div"] },
        { tag: "li", closed_by_tags: ["li"] },
      ],
    })),
  },
});
`,
      },
    ],
    "grammar.js",
  );

  const grammar = JSON.parse(grammarJson);
  assert.deepEqual(grammar.rules.document.members[1], {
    type: "AUTO_CLOSE",
    tag: "implicit_end_tag",
    open_node: "start_tag",
    close_node: "end_tag",
    tag_name_node: "tag_name",
    start_prefix: "<",
    end_prefix: "</",
    rules: [
      { tag: "p", closed_by_tags: ["p", "div"] },
      { tag: "li", closed_by_tags: ["li"] },
    ],
  });
});

test("resolves ESM namespace imports and named exports in grammar modules", () => {
  const grammarJson = emitGrammarJsonFromDsl(
    officialDsl,
    [
      {
        path: "grammar.js",
        text: `
import * as h from "./helpers.mjs";
export default grammar({
  name: "tiny_esm",
  rules: {
    document: $ => h.wrap($.word),
    word: $ => h.word,
  },
});
`,
      },
      {
        path: "helpers.mjs",
        text: `
export const word = /[a-z]+/;
export const wrap = rule => repeat1(rule);
`,
      },
    ],
    "grammar.js",
  );

  const grammar = JSON.parse(grammarJson);
  assert.equal(grammar.name, "tiny_esm");
  assert.deepEqual(grammar.rules.document, {
    type: "REPEAT1",
    content: { type: "SYMBOL", name: "word" },
  });
  assert.deepEqual(grammar.rules.word, {
    type: "PATTERN",
    value: "[a-z]+",
  });
});

test("resolves Arborium-style rehomed common helpers and JSON modules", () => {
  const grammarJson = emitGrammarJsonFromDsl(
    officialDsl,
    [
      {
        path: "grammar/grammar.js",
        text: `
const common = require("../common/common");
module.exports = grammar({
  name: "tiny_rehomed",
  rules: {
    document: $ => common.wrap($.word),
    word: $ => common.word,
  },
});
`,
      },
      {
        path: "grammar/common/common.js",
        text: `
const data = require("./data.json");
exports.word = /[a-z]+/;
exports.wrap = rule => process.env.SNARK_DSL_TEST ? rule : data.repeat ? repeat1(rule) : rule;
`,
      },
      {
        path: "grammar/common/data.json",
        text: '{ "repeat": true }',
      },
    ],
    "grammar/grammar.js",
  );

  const grammar = JSON.parse(grammarJson);
  assert.equal(grammar.name, "tiny_rehomed");
  assert.deepEqual(grammar.rules.document, {
    type: "REPEAT1",
    content: { type: "SYMBOL", name: "word" },
  });
});

test("resolves rehomed common helpers from a full Arborium root", () => {
  const grammarJson = emitGrammarJsonFromDsl(
    officialDsl,
    [
      {
        path: "langs/group-willow/markdown/def/grammar/grammar.js",
        text: `
const common = require("../common/common");
module.exports = grammar({
  name: "tiny_full_root",
  rules: {
    document: $ => common.wrap($.word),
    word: $ => common.word,
  },
});
`,
      },
      {
        path: "langs/group-willow/markdown/def/grammar/common/common.js",
        text: `
exports.word = /[a-z]+/;
exports.wrap = rule => repeat1(rule);
`,
      },
    ],
    "langs/group-willow/markdown/def/grammar/grammar.js",
  );

  const grammar = JSON.parse(grammarJson);
  assert.equal(grammar.name, "tiny_full_root");
  assert.deepEqual(grammar.rules.document, {
    type: "REPEAT1",
    content: { type: "SYMBOL", name: "word" },
  });
});

test("resolves ESM named imports and import aliases in grammar modules", () => {
  const grammarJson = emitGrammarJsonFromDsl(
    officialDsl,
    [
      {
        path: "grammar.js",
        text: `
import { wrap as oneOrMore, word } from "./helpers.mjs";
export default grammar({
  name: "tiny_named_esm",
  rules: {
    document: $ => oneOrMore($.word),
    word: $ => word,
  },
});
`,
      },
      {
        path: "helpers.mjs",
        text: `
export const word = /[a-z]+/;
export const wrap = rule => repeat1(rule);
`,
      },
    ],
    "grammar.js",
  );

  const grammar = JSON.parse(grammarJson);
  assert.equal(grammar.name, "tiny_named_esm");
  assert.deepEqual(grammar.rules.document, {
    type: "REPEAT1",
    content: { type: "SYMBOL", name: "word" },
  });
  assert.deepEqual(grammar.rules.word, {
    type: "PATTERN",
    value: "[a-z]+",
  });
});

test("resolves multiline ESM named import blocks in grammar modules", () => {
  const grammarJson = emitGrammarJsonFromDsl(
    officialDsl,
    [
      {
        path: "grammar.js",
        text: `
import {
  wrap,
  word,
} from "./helpers.mjs";
export default grammar({
  name: "tiny_multiline_named_esm",
  rules: {
    document: $ => wrap($.word),
    word: $ => word,
  },
});
`,
      },
      {
        path: "helpers.mjs",
        text: `
export const word = /[a-z]+/;
export const wrap = rule => repeat1(rule);
`,
      },
    ],
    "grammar.js",
  );

  const grammar = JSON.parse(grammarJson);
  assert.equal(grammar.name, "tiny_multiline_named_esm");
  assert.deepEqual(grammar.rules.document, {
    type: "REPEAT1",
    content: { type: "SYMBOL", name: "word" },
  });
  assert.deepEqual(grammar.rules.word, {
    type: "PATTERN",
    value: "[a-z]+",
  });
});

test("resolves ESM local export lists in grammar modules", () => {
  const grammarJson = emitGrammarJsonFromDsl(
    officialDsl,
    [
      {
        path: "grammar.js",
        text: `
import { oneOrMore, word } from "./helpers.mjs";
export default grammar({
  name: "tiny_local_export_list",
  rules: {
    document: $ => oneOrMore($.word),
    word: $ => word,
  },
});
`,
      },
      {
        path: "helpers.mjs",
        text: `
const word = /[a-z]+/;
const wrap = rule => repeat1(rule);
export { word, wrap as oneOrMore };
`,
      },
    ],
    "grammar.js",
  );

  const grammar = JSON.parse(grammarJson);
  assert.equal(grammar.name, "tiny_local_export_list");
  assert.deepEqual(grammar.rules.document, {
    type: "REPEAT1",
    content: { type: "SYMBOL", name: "word" },
  });
  assert.deepEqual(grammar.rules.word, {
    type: "PATTERN",
    value: "[a-z]+",
  });
});

test("resolves ESM re-export lists in grammar modules", () => {
  const grammarJson = emitGrammarJsonFromDsl(
    officialDsl,
    [
      {
        path: "grammar.js",
        text: `
import { oneOrMore, word } from "./helpers.mjs";
export default grammar({
  name: "tiny_reexport_list",
  rules: {
    document: $ => oneOrMore($.word),
    word: $ => word,
  },
});
`,
      },
      {
        path: "helpers.mjs",
        text: 'export { word, wrap as oneOrMore } from "./tokens.mjs";',
      },
      {
        path: "tokens.mjs",
        text: `
export const word = /[a-z]+/;
export const wrap = rule => repeat1(rule);
`,
      },
    ],
    "grammar.js",
  );

  const grammar = JSON.parse(grammarJson);
  assert.equal(grammar.name, "tiny_reexport_list");
  assert.deepEqual(grammar.rules.document, {
    type: "REPEAT1",
    content: { type: "SYMBOL", name: "word" },
  });
  assert.deepEqual(grammar.rules.word, {
    type: "PATTERN",
    value: "[a-z]+",
  });
});

test("resolves ESM star re-exports in grammar modules", () => {
  const grammarJson = emitGrammarJsonFromDsl(
    officialDsl,
    [
      {
        path: "grammar.js",
        text: `
import { wrap, word } from "./helpers.mjs";
export default grammar({
  name: "tiny_star_reexport",
  rules: {
    document: $ => wrap($.word),
    word: $ => word,
  },
});
`,
      },
      {
        path: "helpers.mjs",
        text: 'export * from "./tokens.mjs";',
      },
      {
        path: "tokens.mjs",
        text: `
export let word = /[a-z]+/;
export const wrap = rule => repeat1(rule);
`,
      },
    ],
    "grammar.js",
  );

  const grammar = JSON.parse(grammarJson);
  assert.equal(grammar.name, "tiny_star_reexport");
  assert.deepEqual(grammar.rules.document, {
    type: "REPEAT1",
    content: { type: "SYMBOL", name: "word" },
  });
  assert.deepEqual(grammar.rules.word, {
    type: "PATTERN",
    value: "[a-z]+",
  });
});

test("resolves ESM default plus named imports in grammar modules", () => {
  const grammarJson = emitGrammarJsonFromDsl(
    officialDsl,
    [
      {
        path: "grammar.js",
        text: `
import helper, { word } from "./helpers.mjs";
export default grammar({
  name: "tiny_default_plus_named_import",
  rules: {
    document: $ => helper.wrap($.word),
    word: $ => word,
  },
});
`,
      },
      {
        path: "helpers.mjs",
        text: `
export const word = /[a-z]+/;
export default { wrap: rule => repeat1(rule) };
`,
      },
    ],
    "grammar.js",
  );

  const grammar = JSON.parse(grammarJson);
  assert.equal(grammar.name, "tiny_default_plus_named_import");
  assert.deepEqual(grammar.rules.document, {
    type: "REPEAT1",
    content: { type: "SYMBOL", name: "word" },
  });
  assert.deepEqual(grammar.rules.word, {
    type: "PATTERN",
    value: "[a-z]+",
  });
});

test("resolves adjacent semicolonless ESM default imports", () => {
  const grammarJson = emitGrammarJsonFromDsl(
    officialDsl,
    [
      {
        path: "grammar.js",
        text: `
import first from "./first.mjs"
import second from "./second.mjs"
export default grammar({
  name: "tiny_semicolonless_esm",
  rules: {
    document: $ => seq(first.word, second.word),
  },
});
`,
      },
      {
        path: "first.mjs",
        text: "export default { word: \"a\" };",
      },
      {
        path: "second.mjs",
        text: "export default { word: \"b\" };",
      },
    ],
    "grammar.js",
  );

  const grammar = JSON.parse(grammarJson);
  assert.equal(grammar.name, "tiny_semicolonless_esm");
  assert.deepEqual(grammar.rules.document, {
    type: "SEQ",
    members: [
      { type: "STRING", value: "a" },
      { type: "STRING", value: "b" },
    ],
  });
});

test("resolves symbol aliases emitted by grammar helper modules", () => {
  const grammarJson = emitGrammarJsonFromDsl(
    officialDsl,
    [
      {
        path: "grammar.js",
        text: `
import { rename } from "./helpers.mjs";
export default grammar({
  name: "tiny_helper_alias",
  rules: {
    document: $ => rename($.word, $.renamed_word),
    word: $ => /[a-z]+/,
  },
});
`,
      },
      {
        path: "helpers.mjs",
        text: "export const rename = (rule, name) => alias(rule, name);",
      },
    ],
    "grammar.js",
  );

  const grammar = JSON.parse(grammarJson);
  assert.equal(grammar.name, "tiny_helper_alias");
  assert.deepEqual(grammar.rules.document, {
    type: "ALIAS",
    content: { type: "SYMBOL", name: "word" },
    named: true,
    value: "renamed_word",
  });
});

test("resolves JSON helper modules required by grammar.js helpers", () => {
  const grammarJson = emitGrammarJsonFromDsl(
    officialDsl,
    [
      {
        path: "grammar.js",
        text: `
const helpers = require("./helpers");
module.exports = grammar({
  name: "tiny_json_helper",
  rules: {
    document: $ => helpers.keyword("alpha"),
  },
});
`,
      },
      {
        path: "helpers.js",
        text: `
const words = require("./words.json");
exports.keyword = name => words[name];
`,
      },
      {
        path: "words.json",
        text: JSON.stringify({ alpha: "a" }),
      },
    ],
    "grammar.js",
  );

  const grammar = JSON.parse(grammarJson);
  assert.equal(grammar.name, "tiny_json_helper");
  assert.deepEqual(grammar.rules.document, {
    type: "STRING",
    value: "a",
  });
});

test("provides an empty process.env for grammar.js feature helpers", () => {
  const grammarJson = emitGrammarJsonFromDsl(
    officialDsl,
    [
      {
        path: "grammar.js",
        text: `
const features = require("./features");
module.exports = grammar({
  name: "tiny_env_helper",
  rules: {
    document: $ => features.enabled ? "yes" : "no",
  },
});
`,
      },
      {
        path: "features.js",
        text: `
exports.enabled = !process.env.DISABLE_TINY_FEATURE;
`,
      },
    ],
    "grammar.js",
  );

  const grammar = JSON.parse(grammarJson);
  assert.equal(grammar.name, "tiny_env_helper");
  assert.deepEqual(grammar.rules.document, {
    type: "STRING",
    value: "yes",
  });
});

test("provides scoped fs and path builtins for grammar.js generation helpers", () => {
  const grammarJson = emitGrammarJsonFromDsl(
    officialDsl,
    [
      {
        path: "grammar.js",
        text: `
const helper = require("./helper");
module.exports = grammar({
  name: "tiny_builtin_helper",
  rules: {
    document: $ => helper.rules($)[0],
  },
});
`,
      },
      {
        path: "helper.js",
        text: `
const fs = require("fs");
const path = require("path");
exports.rules = $ => {
  const out = path.join("src", "generated.h");
  fs.writeFileSync(out, "generated output");
  fs.appendFileSync(out, "\\nmore generated output");
  const keyword = fs.readFileSync(path.join("data", "keyword.txt"));
  return [keyword];
};
`,
      },
      {
        path: "data/keyword.txt",
        text: "ok",
      },
    ],
    "grammar.js",
  );

  const grammar = JSON.parse(grammarJson);
  assert.equal(grammar.name, "tiny_builtin_helper");
  assert.deepEqual(grammar.rules.document, {
    type: "STRING",
    value: "ok",
  });
});

test("provides scoped fs directory helpers for grammar.js generation helpers", () => {
  const grammarJson = emitGrammarJsonFromDsl(
    officialDsl,
    [
      {
        path: "grammar.js",
        text: `
const helper = require("./helper");
module.exports = grammar({
  name: "tiny_directory_helper",
  rules: {
    document: $ => seq(...helper.keywords()),
  },
});
`,
      },
      {
        path: "helper.js",
        text: `
const fs = require("fs");
const path = require("path");
exports.keywords = () => {
  const dir = path.join("data", "keywords");
  if (!fs.existsSync(dir) || !fs.statSync(dir).isDirectory()) return [];
  return fs.readdirSync(dir)
    .filter(name => path.extname(name) === ".txt")
    .filter(name => fs.statSync(path.join(dir, name)).isFile())
    .map(name => fs.readFileSync(path.join(dir, name)));
};
`,
      },
      {
        path: "data/keywords/first.txt",
        text: "alpha",
      },
      {
        path: "data/keywords/ignored.dat",
        text: "ignored",
      },
      {
        path: "data/keywords/second.txt",
        text: "beta",
      },
    ],
    "grammar.js",
  );

  const grammar = JSON.parse(grammarJson);
  assert.equal(grammar.name, "tiny_directory_helper");
  assert.deepEqual(grammar.rules.document, {
    type: "SEQ",
    members: [
      { type: "STRING", value: "alpha" },
      { type: "STRING", value: "beta" },
    ],
  });
});

test("provides CommonJS __dirname and __filename to grammar helpers", () => {
  const grammarJson = emitGrammarJsonFromDsl(
    officialDsl,
    [
      {
        path: "grammar.js",
        text: `
const helper = require("./helpers/helper");
module.exports = grammar({
  name: "tiny_commonjs_module_paths",
  rules: {
    document: $ => seq(helper.wordFromData(), helper.fileStem()),
  },
});
`,
      },
      {
        path: "helpers/helper.js",
        text: `
const fs = require("fs");
const path = require("path");
exports.wordFromData = () => fs.readFileSync(path.join(__dirname, "data", "keyword.txt"));
exports.fileStem = () => path.basename(__filename).replace(".js", "");
`,
      },
      {
        path: "helpers/data/keyword.txt",
        text: "alpha",
      },
    ],
    "grammar.js",
  );

  const grammar = JSON.parse(grammarJson);
  assert.equal(grammar.name, "tiny_commonjs_module_paths");
  assert.deepEqual(grammar.rules.document, {
    type: "SEQ",
    members: [
      { type: "STRING", value: "alpha" },
      { type: "STRING", value: "helper" },
    ],
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
