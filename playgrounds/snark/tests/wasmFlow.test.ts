import assert from "node:assert/strict";
import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import test from "node:test";

import { SnarkPlaygroundSession, initSync, parseBundle } from "../../../snark-wasm/pkg/snark_wasm.js";
import {
  normalizeBundleFiles,
  preferredGrammarRootId,
  preferredSampleForGrammarRootId,
  projectedFilesForGrammarRootId,
  filesWithGrammarJsonUsingEmitter,
  type DslBundleFile,
} from "../src/bundlePaths.ts";
import { emitGrammarJsonFromDsl } from "../src/treeSitterDslRuntime.ts";

initSync({
  module: readFileSync(new URL("../../../snark-wasm/pkg/snark_wasm_bg.wasm", import.meta.url)),
});

const officialDsl = readFileSync(
  new URL("../../../snark-dsl/vendor/tree-sitter-generate-0.26.9/dsl.js", import.meta.url),
  "utf8",
);

function bundledFiles(id: string): DslBundleFile[] {
  const root = new URL(`../src/bundled/${id}/`, import.meta.url);
  return normalizeBundleFiles(
    walkBundledFiles(root).map((path) => ({
      path: `${id}/${path}`,
      text: readFileSync(new URL(path, root), "utf8"),
    })),
  );
}

function allBundledFiles(): DslBundleFile[] {
  const root = new URL("../src/bundled/", import.meta.url);
  return normalizeBundleFiles(
    walkBundledFiles(root).map((path) => ({
      path,
      text: readFileSync(new URL(path, root), "utf8"),
    })),
  );
}

function walkBundledFiles(root: URL, prefix = ""): string[] {
  const dir = new URL(prefix, root);
  const paths: string[] = [];
  for (const name of readdirSync(dir)) {
    const path = `${prefix}${name}`;
    const child = new URL(path, root);
    if (statSync(child).isDirectory()) {
      paths.push(...walkBundledFiles(root, `${path}/`));
    } else {
      paths.push(path);
    }
  }
  return paths;
}

async function runnableFilesForBundle(files: DslBundleFile[], rootId = preferredGrammarRootId(files)) {
  return filesWithGrammarJsonUsingEmitter(files, rootId, async (bundleFiles, grammarPath) =>
    emitGrammarJsonFromDsl(officialDsl, bundleFiles, grammarPath),
  );
}

function filesAndRootForVendoredId(id: string): { files: DslBundleFile[]; rootId: string } {
  if (id === "fences" || id === "gingembre") {
    return { files: allBundledFiles(), rootId: id };
  }
  const files = bundledFiles(id);
  return { files, rootId: preferredGrammarRootId(files) };
}

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
          {
            path: "queries/injections.scm",
            text: '((word) @injection.content\n  (#set! injection.language "text")\n  (#set! injection.combined))\n',
          },
          { path: "languages/text/src/grammar.json", text: grammarJson },
          { path: "languages/text/queries/highlights.scm", text: "(word) @variable\n" },
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
  assert.deepEqual(
    response.injections.map((injection: { language: string; text: string; combined: boolean }) => [
      injection.language,
      injection.text,
      injection.combined,
    ]),
    [
      ["text", "alpha", true],
      ["text", "beta", true],
    ],
  );
  assert.equal(response.layers.length, 1);
  assert.equal(response.layers[0].language, "text");
  assert.equal(response.layers[0].combined, true);
  assert.equal(response.layers[0].input, "alphabeta");
  assert.equal(response.layers[0].parse.sexp, "(document (word))");
  assert.deepEqual(
    response.layers[0].highlights.map((capture: { capture_name: string; text: string }) => [
      capture.capture_name,
      capture.text,
    ]),
    [
      ["variable", "alpha"],
      ["variable", "beta"],
    ],
  );
});

test("runs a grammar.js auto_close bundle through generated grammar.json and Snark WASM", () => {
  const grammarJs = `
module.exports = grammar({
  name: "tiny_auto_close",
  rules: {
    document: $ => repeat1($.element),
    element: $ => seq(
      $._start_p,
      repeat(choice($.text, $.element)),
      choice($._end_p, $._implicit_end_p),
    ),
    _start_p: $ => "<p>",
    _end_p: $ => "</p>",
    _implicit_end_p: $ => auto_close({
      tag: "p",
      open: "<p>",
      close: "</p>",
      closed_by: ["<p>"],
    }),
    text: $ => /[a-z]+/,
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
        ],
        input: "<p>one<p>two</p>",
        run_corpus: false,
      }),
    ),
  );

  assert.equal(response.ok, true, JSON.stringify(response.diagnostics, null, 2));
  assert.equal(response.language, "tiny_auto_close");
  assert.equal(response.parse.sexp, "(document (element (text)) (element (text)))");
  assert.equal(response.parse.accepted_count, 1);
  assert.equal(response.parse.failure_count, 0);
});

test("runs a node-driven auto_close bundle through generated grammar.json and Snark WASM", () => {
  const grammarJs = `
module.exports = grammar({
  name: "tiny_auto_close_nodes",
  rules: {
    document: $ => repeat1($.element),
    element: $ => seq(
      $.start_tag,
      repeat(choice($.text, $.element)),
      choice($.end_tag, $._implicit_p_end),
    ),
    start_tag: $ => seq("<", $.tag_name, ">"),
    end_tag: $ => seq("</", $.tag_name, ">"),
    _implicit_p_end: $ => auto_close({
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
    }),
    tag_name: $ => /[a-z]+/,
    text: $ => /[a-z]+/,
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
        ],
        input: "<p>one<div>two</div>",
        run_corpus: false,
      }),
    ),
  );

  assert.equal(response.ok, true, JSON.stringify(response.diagnostics, null, 2));
  assert.equal(response.language, "tiny_auto_close_nodes");
  assert.equal(
    response.parse.sexp,
    "(document (element (start_tag (tag_name)) (text)) (element (start_tag (tag_name)) (text) (end_tag (tag_name))))",
  );
  assert.equal(response.parse.accepted_count, 1);
  assert.equal(response.parse.failure_count, 0);
});

test("runs highlight query text predicates through Snark WASM", () => {
  const grammarJs = `
module.exports = grammar({
  name: "highlight_predicates",
  extras: $ => [/\\s/],
  rules: {
    document: $ => repeat1($.word),
    word: $ => /[A-Za-z_]+/,
  },
});
`;
  const grammarJson = emitGrammarJsonFromDsl(
    officialDsl,
    [{ path: "grammar.js", text: grammarJs }],
    "grammar.js",
  );
  const highlights = `
((word) @constant
  (#match? @constant "^[A-Z_][A-Z_]*$"))

((word) @function.builtin
  (#eq? @function.builtin "require"))

((word) @type.builtin
  (#any-of? @type.builtin "int" "float"))
`;

  const response = JSON.parse(
    parseBundle(
      JSON.stringify({
        files: [
          { path: "grammar.js", text: grammarJs },
          { path: "src/grammar.json", text: grammarJson },
          { path: "queries/highlights.scm", text: highlights },
        ],
        input: "FOO require int float lower Mixed",
        run_corpus: false,
      }),
    ),
  );

  assert.equal(response.ok, true);
  assert.equal(response.language, "highlight_predicates");
  assert.equal(response.parse.sexp, "(document (word) (word) (word) (word) (word) (word))");
  assert.deepEqual(
    response.highlights.map((capture: { capture_name: string; text: string }) => [
      capture.capture_name,
      capture.text,
    ]),
    [
      ["constant", "FOO"],
      ["function.builtin", "require"],
      ["type.builtin", "int"],
      ["type.builtin", "float"],
    ],
  );
});

test("projects sibling grammar roots into embedded language layers", async () => {
  const hostGrammar = `
module.exports = grammar({
  name: "host",
  extras: $ => [/\\s/],
  rules: {
    document: $ => seq($.prefix, $.word),
    prefix: $ => /x+/,
    word: $ => /[a-z]+/,
  },
});
`;
  const childGrammar = `
module.exports = grammar({
  name: "child",
  extras: $ => [/\\s/],
  rules: {
    document: $ => repeat1($.word),
    word: $ => /[a-z]+/,
  },
});
`;
  const innerGrammar = `
module.exports = grammar({
  name: "inner",
  extras: $ => [/\\s/],
  rules: {
    document: $ => repeat1($.word),
    word: $ => /[a-z]+/,
  },
});
`;
  const files = normalizeBundleFiles([
    { path: "host/grammar.js", text: hostGrammar },
    {
      path: "host/queries/injections.scm",
      text: '((word) @injection.content\n  (#set! injection.language "child")\n  (#set! injection.combined))\n',
    },
    { path: "child/grammar.js", text: childGrammar },
    { path: "child/queries/highlights.scm", text: "(word) @variable\n" },
    {
      path: "child/queries/injections.scm",
      text: '((word) @injection.content\n  (#set! injection.language "inner"))\n',
    },
    { path: "inner/grammar.js", text: innerGrammar },
    { path: "inner/queries/highlights.scm", text: "(word) @constant\n" },
  ]);

  const projected = await filesWithGrammarJsonUsingEmitter(files, "host", async (bundleFiles, grammarPath) =>
    emitGrammarJsonFromDsl(officialDsl, bundleFiles, grammarPath),
  );
  const response = JSON.parse(
    parseBundle(
      JSON.stringify({
        files: projected,
        input: "xxalpha",
        run_corpus: false,
      }),
    ),
  );

  assert.equal(response.ok, true, JSON.stringify(response.diagnostics, null, 2));
  assert.equal(response.layers.length, 1);
  assert.equal(response.layers[0].language, "child");
  assert.equal(response.layers[0].combined, true);
  assert.equal(response.layers[0].input, "alpha");
  assert.equal(response.layers[0].ranges[0].start_byte, 2);
  assert.equal(response.layers[0].parse.sexp, "(document (word))");
  assert.deepEqual(
    response.layers[0].highlights.map((capture: { capture_name: string; text: string; start_byte: number }) => [
      capture.capture_name,
      capture.text,
      capture.start_byte,
    ]),
    [["variable", "alpha", 2]],
  );
  assert.equal(response.layers[0].injections.length, 1);
  assert.equal(response.layers[0].injections[0].language, "inner");
  assert.equal(response.layers[0].injections[0].start_byte, 2);
  assert.equal(response.layers[0].layers.length, 1);
  assert.equal(response.layers[0].layers[0].language, "inner");
  assert.equal(response.layers[0].layers[0].parse.sexp, "(document (word))");
  assert.deepEqual(
    response.layers[0].layers[0].highlights.map((capture: { capture_name: string; text: string; start_byte: number }) => [
      capture.capture_name,
      capture.text,
      capture.start_byte,
    ]),
    [["constant", "alpha", 2]],
  );
});

test("projects manifest-declared mixed roots into embedded language layers", async () => {
  const manifest = JSON.stringify({
    grammars: [
      {
        name: "host",
        scope: "source.host",
        path: "grammars/host",
        injections: "queries/injections.scm",
      },
      {
        name: "child",
        scope: "source.child",
        path: "grammars/child",
        highlights: "queries/child-highlights.scm",
      },
    ],
    metadata: {
      version: "0.0.0",
      links: { repository: "https://example.com/mixed" },
    },
  });
  const files = normalizeBundleFiles([
    { path: "tree-sitter-package/tree-sitter.json", text: manifest },
    {
      path: "tree-sitter-package/grammars/host/src/grammar.json",
      text: JSON.stringify({
        name: "host",
        rules: {
          document: { type: "SYMBOL", name: "code" },
          code: {
            type: "TOKEN",
            content: { type: "PATTERN", value: "[A-Z]+" },
          },
        },
        extras: [],
        conflicts: [],
        precedences: [],
        externals: [],
        inline: [],
        supertypes: [],
      }),
    },
    {
      path: "tree-sitter-package/grammars/host/queries/injections.scm",
      text: '((code) @injection.content\n  (#set! injection.language "child"))\n',
    },
    {
      path: "tree-sitter-package/grammars/child/grammar.js",
      text: `
module.exports = grammar({
  name: "child",
  rules: {
    document: $ => $.word,
    word: $ => /[A-Z]+/,
  },
});
`,
    },
    {
      path: "tree-sitter-package/grammars/child/queries/child-highlights.scm",
      text: "(word) @constant\n",
    },
  ]);
  const projected = await filesWithGrammarJsonUsingEmitter(files, "grammars/host", async (bundleFiles, grammarPath) =>
    emitGrammarJsonFromDsl(officialDsl, bundleFiles, grammarPath),
  );

  assert.deepEqual(
    projected.map((file) => file.path),
    [
      "languages/child/grammar.js",
      "languages/child/queries/child-highlights.scm",
      "languages/child/src/grammar.json",
      "languages/child/tree-sitter.json",
      "queries/injections.scm",
      "src/grammar.json",
      "tree-sitter.json",
    ],
  );

  const response = JSON.parse(
    parseBundle(
      JSON.stringify({
        files: projected,
        input: "PRINT",
        run_corpus: false,
      }),
    ),
  );

  assert.equal(response.ok, true, JSON.stringify(response.diagnostics, null, 2));
  assert.equal(response.language, "host");
  assert.equal(response.injections.length, 1);
  assert.equal(response.injections[0].language, "child");
  assert.equal(response.layers.length, 1);
  assert.equal(response.layers[0].language, "child");
  assert.equal(response.layers[0].parse.sexp, "(document (word))");
  assert.deepEqual(
    response.layers[0].highlights.map((capture: { capture_name: string; text: string }) => [
      capture.capture_name,
      capture.text,
    ]),
    [["constant", "PRINT"]],
  );
});

test("injects scanner-free html from gingembre text chunks", async () => {
  const files = allBundledFiles();
  const projected = await filesWithGrammarJsonUsingEmitter(files, "gingembre", async (bundleFiles, grammarPath) =>
    emitGrammarJsonFromDsl(officialDsl, bundleFiles, grammarPath),
  );
  const source = "<section><p>Hello {{ name }}<p>Again</p></section>";
  const response = JSON.parse(
    parseBundle(
      JSON.stringify({
        files: projected,
        input: source,
        run_corpus: false,
      }),
    ),
  );

  assert.equal(response.ok, true, JSON.stringify(response.diagnostics, null, 2));
  assert.equal(response.language, "gingembre");
  assert.equal(response.layers.length, 1);
  const htmlLayer = response.layers[0];
  assert.equal(htmlLayer.language, "html");
  assert.equal(htmlLayer.combined, true);
  assert.equal(htmlLayer.input, "<section><p>Hello <p>Again</p></section>");
  assert.equal(htmlLayer.parse.accepted_error_count, 0);
  assert.equal(htmlLayer.parse.accepted_missing_count, 0);
  assert.match(htmlLayer.parse.sexp, /\(implicit_end_tag\)/);
  assert.deepEqual(
    htmlLayer.ranges.map((range: { start_byte: number; end_byte: number }) => [
      range.start_byte,
      range.end_byte,
    ]),
    [
      [0, 18],
      [28, 50],
    ],
  );
  assert.deepEqual(
    htmlLayer.highlights.map((capture: { capture_name: string; text: string }) => [
      capture.capture_name,
      capture.text,
    ]),
    [
      ["tag", "section"],
      ["tag", "p"],
      ["tag", "p"],
      ["tag", "p"],
      ["tag", "section"],
      ["text", "Hello "],
      ["text", "Again"],
    ],
  );
});

test("injects css inside scanner-free html inside gingembre text chunks", async () => {
  const files = [
    ...allBundledFiles(),
    {
      path: "languages/css/src/grammar.json",
      text: readFileSync(
        new URL("../../../snark/tests/fixtures/packages/tree-sitter-css-reduced/src/grammar.json", import.meta.url),
        "utf8",
      ),
    },
    {
      path: "languages/css/src/scanner.c",
      text: readFileSync(
        new URL("../../../snark/tests/fixtures/packages/tree-sitter-css-reduced/src/scanner.c", import.meta.url),
        "utf8",
      ),
    },
    {
      path: "languages/css/queries/highlights.scm",
      text: readFileSync(
        new URL("../../../snark/tests/fixtures/packages/tree-sitter-css-reduced/queries/highlights.scm", import.meta.url),
        "utf8",
      ),
    },
  ];
  const projected = await filesWithGrammarJsonUsingEmitter(files, "gingembre", async (bundleFiles, grammarPath) =>
    emitGrammarJsonFromDsl(officialDsl, bundleFiles, grammarPath),
  );
  const source = '<style>a:hover { color: red; }</style><p>Hello {{ name }}</p>';
  const response = JSON.parse(
    parseBundle(
      JSON.stringify({
        files: projected,
        input: source,
        run_corpus: false,
      }),
    ),
  );

  assert.equal(response.ok, true, JSON.stringify(response.diagnostics, null, 2));
  assert.equal(response.language, "gingembre");
  assert.equal(response.layers.length, 1);
  const htmlLayer = response.layers[0];
  assert.equal(htmlLayer.language, "html");
  assert.equal(htmlLayer.combined, true);
  assert.equal(htmlLayer.input, "<style>a:hover { color: red; }</style><p>Hello </p>");
  assert.equal(htmlLayer.parse.accepted_error_count, 0);
  assert.equal(htmlLayer.parse.accepted_missing_count, 0);
  assert.equal(htmlLayer.layers.length, 1);
  const cssLayer = htmlLayer.layers[0];
  assert.equal(cssLayer.language, "css");
  assert.equal(cssLayer.input, "a:hover { color: red; }");
  assert.equal(cssLayer.parse.accepted_error_count, 0);
  assert.equal(cssLayer.parse.accepted_missing_count, 0);
  assert.ok(
    cssLayer.highlights.some(
      (capture: { capture_name: string; text: string }) =>
        capture.capture_name === "property" && capture.text === "color",
    ),
  );
  assert.ok(
    cssLayer.highlights.some(
      (capture: { capture_name: string; text: string }) =>
        capture.capture_name === "punctuation.delimiter" && capture.text === ":",
    ),
  );
});

test("injects css from html style layers through Snark WASM", () => {
  const htmlGrammarJs = readFileSync(new URL("../src/bundled/html/grammar.js", import.meta.url), "utf8");
  const htmlGrammarJson = emitGrammarJsonFromDsl(
    officialDsl,
    [{ path: "html/grammar.js", text: htmlGrammarJs }],
    "html/grammar.js",
  );
  const response = JSON.parse(
    parseBundle(
      JSON.stringify({
        files: [
          { path: "grammar.js", text: htmlGrammarJs },
          { path: "src/grammar.json", text: htmlGrammarJson },
          {
            path: "queries/highlights.scm",
            text: readFileSync(
              new URL("../src/bundled/html/queries/highlights.scm", import.meta.url),
              "utf8",
            ),
          },
          {
            path: "queries/injections.scm",
            text: readFileSync(
              new URL("../src/bundled/html/queries/injections.scm", import.meta.url),
              "utf8",
            ),
          },
          {
            path: "languages/css/src/grammar.json",
            text: readFileSync(
              new URL(
                "../../../snark/tests/fixtures/packages/tree-sitter-css-reduced/src/grammar.json",
                import.meta.url,
              ),
              "utf8",
            ),
          },
          {
            path: "languages/css/src/scanner.c",
            text: readFileSync(
              new URL("../../../snark/tests/fixtures/packages/tree-sitter-css-reduced/src/scanner.c", import.meta.url),
              "utf8",
            ),
          },
          {
            path: "languages/css/queries/highlights.scm",
            text: readFileSync(
              new URL(
                "../../../snark/tests/fixtures/packages/tree-sitter-css-reduced/queries/highlights.scm",
                import.meta.url,
              ),
              "utf8",
            ),
          },
        ],
        input: "<style>a:hover { color: red; }</style>",
        run_corpus: false,
      }),
    ),
  );

  assert.equal(response.ok, true, JSON.stringify(response.diagnostics, null, 2));
  assert.equal(response.language, "html");
  assert.equal(response.layers.length, 1);
  const cssLayer = response.layers[0];
  assert.equal(cssLayer.language, "css");
  assert.equal(cssLayer.input, "a:hover { color: red; }");
  assert.equal(cssLayer.parse.accepted_error_count, 0);
  assert.equal(cssLayer.parse.accepted_missing_count, 0);
  assert.equal(
    cssLayer.parse.sexp,
    "(stylesheet (rule_set (selectors (pseudo_class_selector (tag_name) (class_name (identifier)))) (block (declaration (property_name) (plain_value)))))",
  );
  assert.ok(
    cssLayer.highlights.some(
      (capture: { capture_name: string; text: string }) =>
        capture.capture_name === "property" && capture.text === "color",
    ),
  );
  assert.ok(
    cssLayer.highlights.some(
      (capture: { capture_name: string; text: string }) =>
        capture.capture_name === "punctuation.delimiter" && capture.text === ":",
    ),
  );
});

test("excludes injection content children by default", async () => {
  const hostGrammar = `
module.exports = grammar({
  name: "host",
  extras: $ => [],
  rules: {
    document: $ => $.template,
    template: $ => seq("AA", $.code, "BB"),
    code: $ => "JS",
  },
});
`;
  const textGrammar = `
module.exports = grammar({
  name: "text",
  extras: $ => [],
  rules: {
    document: $ => $.word,
    word: $ => /[A-Z]+/,
  },
});
`;
  const files = normalizeBundleFiles([
    { path: "host/grammar.js", text: hostGrammar },
    {
      path: "host/queries/injections.scm",
      text: '((template) @injection.content\n  (#set! injection.language "text")\n  (#set! injection.combined))\n',
    },
    { path: "text/grammar.js", text: textGrammar },
    { path: "text/queries/highlights.scm", text: "(word) @constant\n" },
  ]);

  const projected = await filesWithGrammarJsonUsingEmitter(files, "host", async (bundleFiles, grammarPath) =>
    emitGrammarJsonFromDsl(officialDsl, bundleFiles, grammarPath),
  );
  const response = JSON.parse(
    parseBundle(
      JSON.stringify({
        files: projected,
        input: "AAJSBB",
        run_corpus: false,
      }),
    ),
  );

  assert.equal(response.ok, true, JSON.stringify(response.diagnostics, null, 2));
  assert.deepEqual(
    response.injections.map((injection: { language: string; text: string; start_byte: number; end_byte: number }) => [
      injection.language,
      injection.text,
      injection.start_byte,
      injection.end_byte,
    ]),
    [
      ["text", "AA", 0, 2],
      ["text", "BB", 4, 6],
    ],
  );
  assert.equal(response.layers.length, 1);
  assert.equal(response.layers[0].language, "text");
  assert.equal(response.layers[0].combined, true);
  assert.equal(response.layers[0].input, "AABB");
  assert.deepEqual(
    response.layers[0].highlights.map(
      (capture: { capture_name: string; text: string; start_byte: number; end_byte: number }) => [
        capture.capture_name,
        capture.text,
        capture.start_byte,
        capture.end_byte,
      ],
    ),
    [
      ["constant", "AA", 0, 2],
      ["constant", "BB", 4, 6],
    ],
  );
});

test("splits nested injections across combined layer ranges", async () => {
  const hostGrammar = `
module.exports = grammar({
  name: "host",
  extras: $ => [/\\s/],
  rules: {
    document: $ => repeat1($.word),
    word: $ => /[A-Z]+/,
  },
});
`;
  const textGrammar = `
module.exports = grammar({
  name: "text",
  extras: $ => [],
  rules: {
    document: $ => $.word,
    word: $ => /[A-Z]+/,
  },
});
`;
  const innerGrammar = `
module.exports = grammar({
  name: "inner",
  extras: $ => [],
  rules: {
    document: $ => $.word,
    word: $ => /[A-Z]+/,
  },
});
`;
  const files = normalizeBundleFiles([
    { path: "host/grammar.js", text: hostGrammar },
    {
      path: "host/queries/injections.scm",
      text: '((word) @injection.content\n  (#set! injection.language "text")\n  (#set! injection.combined))\n',
    },
    { path: "text/grammar.js", text: textGrammar },
    {
      path: "text/queries/injections.scm",
      text: '((document) @injection.content\n  (#set! injection.language "inner")\n  (#set! injection.combined))\n',
    },
    { path: "inner/grammar.js", text: innerGrammar },
    { path: "inner/queries/highlights.scm", text: "(word) @constant\n" },
  ]);

  const projected = await filesWithGrammarJsonUsingEmitter(files, "host", async (bundleFiles, grammarPath) =>
    emitGrammarJsonFromDsl(officialDsl, bundleFiles, grammarPath),
  );
  const response = JSON.parse(
    parseBundle(
      JSON.stringify({
        files: projected,
        input: "AA CCC",
        run_corpus: false,
      }),
    ),
  );

  assert.equal(response.ok, true, JSON.stringify(response.diagnostics, null, 2));
  assert.equal(response.layers.length, 1);
  const textLayer = response.layers[0];
  assert.equal(textLayer.language, "text");
  assert.equal(textLayer.combined, true);
  assert.equal(textLayer.input, "AACCC");
  assert.deepEqual(
    textLayer.injections.map((injection: { language: string; text: string; start_byte: number; end_byte: number }) => [
      injection.language,
      injection.text,
      injection.start_byte,
      injection.end_byte,
    ]),
    [
      ["inner", "AA", 0, 2],
      ["inner", "CCC", 3, 6],
    ],
  );
  assert.equal(textLayer.layers.length, 1);
  const innerLayer = textLayer.layers[0];
  assert.equal(innerLayer.language, "inner");
  assert.equal(innerLayer.combined, true);
  assert.equal(innerLayer.input, "AACCC");
  assert.deepEqual(
    innerLayer.highlights.map((capture: { capture_name: string; text: string; start_byte: number; end_byte: number }) => [
      capture.capture_name,
      capture.text,
      capture.start_byte,
      capture.end_byte,
    ]),
    [
      ["constant", "AA", 0, 2],
      ["constant", "CCC", 3, 6],
    ],
  );
});

test("resolves dynamic injection language captures through Snark WASM", async () => {
  const hostGrammar = `
module.exports = grammar({
  name: "host",
  extras: $ => [],
  rules: {
    document: $ => $.block,
    block: $ => seq($.lang, ":", $.code),
    lang: $ => /[a-z]+/,
    code: $ => /[A-Z]+/,
  },
});
`;
  const childGrammar = `
module.exports = grammar({
  name: "demo",
  extras: $ => [],
  rules: {
    document: $ => $.word,
    word: $ => /[A-Z]+/,
  },
});
`;
  const files = normalizeBundleFiles([
    { path: "host/grammar.js", text: hostGrammar },
    {
      path: "host/queries/injections.scm",
      text: "((block\n  (lang) @injection.language\n  (code) @injection.content))\n",
    },
    { path: "demo/grammar.js", text: childGrammar },
    { path: "demo/queries/highlights.scm", text: "(word) @constant\n" },
  ]);

  const projected = await filesWithGrammarJsonUsingEmitter(files, "host", async (bundleFiles, grammarPath) =>
    emitGrammarJsonFromDsl(officialDsl, bundleFiles, grammarPath),
  );
  const response = JSON.parse(
    parseBundle(
      JSON.stringify({
        files: projected,
        input: "demo:PRINT",
        run_corpus: false,
      }),
    ),
  );

  assert.equal(response.ok, true, JSON.stringify(response.diagnostics, null, 2));
  assert.deepEqual(
    response.injections.map((injection: { language: string; text: string; start_byte: number }) => [
      injection.language,
      injection.text,
      injection.start_byte,
    ]),
    [["demo", "PRINT", 5]],
  );
  assert.equal(response.layers.length, 1);
  assert.equal(response.layers[0].language, "demo");
  assert.equal(response.layers[0].input, "PRINT");
  assert.equal(response.layers[0].parse.sexp, "(document (word))");
  assert.deepEqual(
    response.layers[0].highlights.map((capture: { capture_name: string; text: string; start_byte: number }) => [
      capture.capture_name,
      capture.text,
      capture.start_byte,
    ]),
    [["constant", "PRINT", 5]],
  );
});

test("filters injected layers with query capture predicates through Snark WASM", async () => {
  const hostGrammar = `
module.exports = grammar({
  name: "host",
  extras: $ => [],
  rules: {
    document: $ => seq($.tag, ":", $.code),
    tag: $ => /[a-z]+/,
    code: $ => /[A-Z]+/,
  },
});
`;
  const htmlGrammar = `
module.exports = grammar({
  name: "html",
  extras: $ => [],
  rules: {
    document: $ => $.word,
    word: $ => /[A-Z]+/,
  },
});
`;
  const sqlGrammar = `
module.exports = grammar({
  name: "sql",
  extras: $ => [],
  rules: {
    document: $ => $.word,
    word: $ => /[A-Z]+/,
  },
});
`;
  const files = normalizeBundleFiles([
    { path: "host/grammar.js", text: hostGrammar },
    {
      path: "host/queries/injections.scm",
      text:
        '((document (tag) @_name (code) @injection.content)\n  (#match? @_name ".*(hbs|glimmer).*")\n  (#set! injection.language "html"))\n' +
        '((document (tag) @_name (code) @injection.content)\n  (#eq? @_name "sql")\n  (#set! injection.language "sql"))\n',
    },
    { path: "html/grammar.js", text: htmlGrammar },
    { path: "html/queries/highlights.scm", text: "(word) @tag\n" },
    { path: "sql/grammar.js", text: sqlGrammar },
  ]);

  const projected = await filesWithGrammarJsonUsingEmitter(files, "host", async (bundleFiles, grammarPath) =>
    emitGrammarJsonFromDsl(officialDsl, bundleFiles, grammarPath),
  );
  const response = JSON.parse(
    parseBundle(
      JSON.stringify({
        files: projected,
        input: "hbs:PRINT",
        run_corpus: false,
      }),
    ),
  );

  assert.equal(response.ok, true, JSON.stringify(response.diagnostics, null, 2));
  assert.deepEqual(
    response.injections.map((injection: { language: string; text: string }) => [
      injection.language,
      injection.text,
    ]),
    [["html", "PRINT"]],
  );
  assert.equal(response.layers.length, 1);
  assert.equal(response.layers[0].language, "html");
  assert.equal(response.layers[0].parse.sexp, "(document (word))");
  assert.deepEqual(
    response.layers[0].highlights.map((capture: { capture_name: string; text: string; start_byte: number }) => [
      capture.capture_name,
      capture.text,
      capture.start_byte,
    ]),
    [["tag", "PRINT", 4]],
  );
});

test("resolves injected layers through tree-sitter injection-regex in WASM", async () => {
  const hostGrammar = `
module.exports = grammar({
  name: "host",
  extras: $ => [/\\s/],
  rules: {
    document: $ => seq($.prefix, $.word),
    prefix: $ => /x+/,
    word: $ => /[a-z]+/,
  },
});
`;
  const childGrammar = `
module.exports = grammar({
  name: "child",
  extras: $ => [/\\s/],
  rules: {
    document: $ => repeat1($.word),
    word: $ => /[a-z]+/,
  },
});
`;
  const files = normalizeBundleFiles([
    {
      path: "tree-sitter.json",
      text: JSON.stringify({
        grammars: [
          {
            name: "host",
            scope: "source.host",
            path: "grammars/host",
            injections: "queries/embed.scm",
          },
          {
            name: "child",
            scope: "source.child",
            path: "grammars/child",
            "injection-regex": "^text/x-child$",
            highlights: "queries/child-highlights.scm",
          },
        ],
        metadata: {
          version: "0.0.0",
          links: { repository: "https://example.com/package" },
        },
      }),
    },
    { path: "grammars/host/grammar.js", text: hostGrammar },
    {
      path: "grammars/host/queries/embed.scm",
      text: '((word) @injection.content\n  (#set! injection.language "text/x-child"))\n',
    },
    { path: "grammars/child/grammar.js", text: childGrammar },
    { path: "grammars/child/queries/child-highlights.scm", text: "(word) @constant\n" },
  ]);

  const projected = await filesWithGrammarJsonUsingEmitter(files, "grammars/host", async (bundleFiles, grammarPath) =>
    emitGrammarJsonFromDsl(officialDsl, bundleFiles, grammarPath),
  );
  const response = JSON.parse(
    parseBundle(
      JSON.stringify({
        files: projected,
        input: "xxalpha",
        run_corpus: false,
      }),
    ),
  );

  assert.equal(response.ok, true, JSON.stringify(response.diagnostics, null, 2));
  assert.equal(response.layers.length, 1);
  assert.equal(response.layers[0].language, "text/x-child");
  assert.equal(response.layers[0].parse.sexp, "(document (word))");
  assert.deepEqual(
    response.layers[0].highlights.map((capture: { capture_name: string; text: string }) => [
      capture.capture_name,
      capture.text,
    ]),
    [["constant", "alpha"]],
  );
});

test("reports injected layer parse diagnostics at root source coordinates", async () => {
  const hostGrammar = `
module.exports = grammar({
  name: "host",
  extras: $ => [/\\s/],
  rules: {
    document: $ => seq($.prefix, $.word),
    prefix: $ => /x+/,
    word: $ => /[a-z]+/,
  },
});
`;
  const childGrammar = `
module.exports = grammar({
  name: "digits",
  extras: $ => [/\\s/],
  rules: {
    document: $ => $.number,
    number: $ => /[0-9]+/,
  },
});
`;
  const files = normalizeBundleFiles([
    { path: "host/grammar.js", text: hostGrammar },
    {
      path: "host/queries/injections.scm",
      text: '((word) @injection.content\n  (#set! injection.language "digits"))\n',
    },
    { path: "digits/grammar.js", text: childGrammar },
  ]);

  const projected = await filesWithGrammarJsonUsingEmitter(files, "host", async (bundleFiles, grammarPath) =>
    emitGrammarJsonFromDsl(officialDsl, bundleFiles, grammarPath),
  );
  const response = JSON.parse(
    parseBundle(
      JSON.stringify({
        files: projected,
        input: "xxalpha",
        run_corpus: false,
      }),
    ),
  );

  assert.equal(response.ok, false);
  assert.equal(response.layers.length, 1);
  assert.equal(response.layers[0].language, "digits");
  assert.equal(response.layers[0].diagnostics.length, 1);
  assert.equal(response.diagnostics[0].stage, "layer/parse");
  assert.match(response.diagnostics[0].message, /digits:/);
  assert.equal(response.diagnostics[0].primary_span.start_byte, 2);
  assert.equal(response.diagnostics[0].primary_span.start_column, 2);
});

test("reports unsupported external scanners from Snark WASM", () => {
  const grammarJs = `
module.exports = grammar({
  name: "tiny_external",
  externals: $ => [$.external_token],
  rules: {
    document: $ => $.external_token,
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
          {
            path: "src/scanner.c",
            text: "void *tree_sitter_tiny_external_external_scanner_create(void) { return 0; }",
          },
        ],
        input: "x",
        run_corpus: false,
      }),
    ),
  );

  assert.equal(response.ok, false);
  assert.equal(response.language, "tiny_external");
  assert.equal(response.diagnostics[0].stage, "scanner");
  assert.match(response.diagnostics[0].message, /external_token/);
  assert.match(response.diagnostics[0].message, /source-matched reduced CSS scanner host/);
  assert.equal(response.parse, null);
});

test("reuses a prepared Snark WASM session across inputs", () => {
  const grammarJs = `
module.exports = grammar({
  name: "tiny_session",
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
  const session = new SnarkPlaygroundSession(
    JSON.stringify({
      files: [
        { path: "grammar.js", text: grammarJs },
        { path: "src/grammar.json", text: grammarJson },
        { path: "queries/highlights.scm", text: "(word) @variable\n" },
      ],
    }),
  );

  const first = JSON.parse(
    session.parse(
      JSON.stringify({
        input: "alpha beta",
        run_corpus: false,
      }),
    ),
  );
  const second = JSON.parse(
    session.reparse(
      JSON.stringify({
        input: "gamma beta",
        run_corpus: false,
        edit: {
          start_byte: 0,
          old_end_byte: "alpha".length,
          new_end_byte: "gamma".length,
        },
      }),
    ),
  );

  assert.equal(first.ok, true);
  assert.equal(first.parse.sexp, "(document (word) (word))");
  assert.equal(first.parse.reuse_node_count, 0);
  assert.equal(second.ok, true);
  assert.equal(second.parse.sexp, "(document (word) (word))");
  assert.equal(second.parse.reuse_node_count, 1);
  assert.deepEqual(
    second.highlights.map((capture: { capture_name: string; text: string }) => [
      capture.capture_name,
      capture.text,
    ]),
    [
      ["variable", "gamma"],
      ["variable", "beta"],
    ],
  );
});

test("refreshes injected layers when reparsing a prepared Snark WASM session", async () => {
  const hostGrammar = `
module.exports = grammar({
  name: "host_session",
  extras: $ => [],
  rules: {
    document: $ => seq($.prefix, $.code),
    prefix: $ => /x+/,
    code: $ => /[a-z]+/,
  },
});
`;
  const textGrammar = `
module.exports = grammar({
  name: "text",
  extras: $ => [/\\s/],
  rules: {
    document: $ => repeat1($.word),
    word: $ => /[a-z]+/,
  },
});
`;
  const files = normalizeBundleFiles([
    { path: "host/grammar.js", text: hostGrammar },
    {
      path: "host/queries/injections.scm",
      text: '((code) @injection.content\n  (#set! injection.language "text"))\n',
    },
    { path: "text/grammar.js", text: textGrammar },
    { path: "text/queries/highlights.scm", text: "(word) @constant\n" },
  ]);
  const projected = await filesWithGrammarJsonUsingEmitter(files, "host", async (bundleFiles, grammarPath) =>
    emitGrammarJsonFromDsl(officialDsl, bundleFiles, grammarPath),
  );
  const session = new SnarkPlaygroundSession(
    JSON.stringify({
      files: projected,
    }),
  );

  const first = JSON.parse(
    session.parse(
      JSON.stringify({
        input: "xxalpha",
        run_corpus: false,
      }),
    ),
  );
  const second = JSON.parse(
    session.reparse(
      JSON.stringify({
        input: "xxbeta",
        run_corpus: false,
        edit: {
          start_byte: 2,
          old_end_byte: 7,
          new_end_byte: 6,
        },
      }),
    ),
  );

  assert.equal(first.ok, true, JSON.stringify(first.diagnostics, null, 2));
  assert.equal(first.parse.sexp, "(document (prefix) (code))");
  assert.equal(first.layers.length, 1);
  assert.equal(first.layers[0].language, "text");
  assert.equal(first.layers[0].input, "alpha");
  assert.deepEqual(
    first.layers[0].highlights.map((capture: { capture_name: string; text: string }) => [
      capture.capture_name,
      capture.text,
    ]),
    [["constant", "alpha"]],
  );

  assert.equal(second.ok, true, JSON.stringify(second.diagnostics, null, 2));
  assert.equal(second.parse.sexp, "(document (prefix) (code))");
  assert.equal(second.injections.length, 1);
  assert.equal(second.injections[0].text, "beta");
  assert.equal(second.injections[0].start_byte, 2);
  assert.equal(second.injections[0].end_byte, 6);
  assert.equal(second.layers.length, 1);
  assert.equal(second.layers[0].language, "text");
  assert.equal(second.layers[0].input, "beta");
  assert.deepEqual(
    second.layers[0].highlights.map(
      (capture: { capture_name: string; text: string; start_byte: number; end_byte: number }) => [
        capture.capture_name,
        capture.text,
        capture.start_byte,
        capture.end_byte,
      ],
    ),
    [["constant", "beta", 2, 6]],
  );
});

test("reparses vendored gingembre html layers through a prepared Snark WASM session", async () => {
  const files = allBundledFiles();
  const projected = await filesWithGrammarJsonUsingEmitter(files, "gingembre", async (bundleFiles, grammarPath) =>
    emitGrammarJsonFromDsl(officialDsl, bundleFiles, grammarPath),
  );
  const session = new SnarkPlaygroundSession(
    JSON.stringify({
      files: projected,
    }),
  );

  const firstSource = "<section><p>Hello {{ name }}<p>Again</p></section>";
  const startByte = firstSource.indexOf("name");
  assert.notEqual(startByte, -1);
  const secondSource = firstSource.replace("name", "title");

  const first = JSON.parse(
    session.parse(
      JSON.stringify({
        input: firstSource,
        run_corpus: false,
      }),
    ),
  );
  const second = JSON.parse(
    session.reparse(
      JSON.stringify({
        input: secondSource,
        run_corpus: false,
        edit: {
          start_byte: startByte,
          old_end_byte: startByte + "name".length,
          new_end_byte: startByte + "title".length,
        },
      }),
    ),
  );

  assert.equal(first.ok, true, JSON.stringify(first.diagnostics, null, 2));
  assert.equal(first.layers.length, 1);
  assert.equal(first.layers[0].language, "html");
  assert.equal(first.layers[0].input, "<section><p>Hello <p>Again</p></section>");
  assert.equal(first.layers[0].parse.accepted_error_count, 0);
  assert.equal(first.layers[0].parse.accepted_missing_count, 0);

  assert.equal(second.ok, true, JSON.stringify(second.diagnostics, null, 2));
  assert.equal(second.layers.length, 1);
  assert.equal(second.layers[0].language, "html");
  assert.equal(second.layers[0].input, "<section><p>Hello <p>Again</p></section>");
  assert.equal(second.layers[0].parse.accepted_error_count, 0);
  assert.equal(second.layers[0].parse.accepted_missing_count, 0);
  assert.deepEqual(
    second.layers[0].ranges.map((range: { start_byte: number; end_byte: number }) => [
      range.start_byte,
      range.end_byte,
    ]),
    [
      [0, 18],
      [29, 51],
    ],
  );
});

test("runs bundled corpus and highlight tests through generated grammar.json", () => {
  const grammarJs = `
module.exports = grammar({
  name: "tiny_testable",
  extras: $ => [$.comment, /\\s/],
  rules: {
    document: $ => repeat1($.word),
    word: $ => /[a-z]+/,
    comment: $ => token(/\\/\\*[^*]*\\*\\//),
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
          {
            path: "test/corpus/main.txt",
            text: "====================\nWords\n====================\n\nalpha beta\n\n---\n\n(document (word) (word))\n",
          },
          {
            path: "test/highlight/test.txt",
            text: "alpha beta\n/* ^ variable */\n      /* ^ variable */\n",
          },
        ],
        input: "",
        run_corpus: true,
      }),
    ),
  );

  assert.equal(response.ok, true, JSON.stringify(response.diagnostics, null, 2));
  assert.equal(response.parse, null);
  assert.deepEqual(response.tests, {
    requested: true,
    corpus_passed: 1,
    corpus_failed: 0,
    highlight_assertions_passed: 2,
    highlight_assertions_failed: 0,
    highlight_fixture_errors: 0,
  });
  assert.equal(response.corpus[0].passed, true);
  assert.equal(response.highlight_tests[0].passed, true);
});

test("uses first corpus case as source input when an uploaded bundle has no samples", async () => {
  const grammarJs = `
module.exports = grammar({
  name: "tiny_corpus_source",
  extras: $ => [/\\s/],
  rules: {
    document: $ => repeat1($.word),
    word: $ => /[a-z]+/,
  },
});
`;
  const files = normalizeBundleFiles([
    { path: "tree-sitter-corpus-source/grammar.js", text: grammarJs },
    {
      path: "tree-sitter-corpus-source/test/corpus/main.txt",
      text: "====================\nWords\n====================\n\nalpha beta\n\n---\n\n(document (word) (word))\n",
    },
  ]);
  const sample = preferredSampleForGrammarRootId(files);
  assert.deepEqual(sample && { path: sample.path, text: sample.text }, {
    path: "test/corpus/main.txt#Words",
    text: "alpha beta",
  });

  const runnableFiles = await runnableFilesForBundle(files);
  const response = JSON.parse(
    parseBundle(
      JSON.stringify({
        files: runnableFiles,
        input: sample?.text ?? "",
        run_corpus: true,
      }),
    ),
  );

  assert.equal(response.ok, true, JSON.stringify(response.diagnostics, null, 2));
  assert.equal(response.parse.sexp, "(document (word) (word))");
  assert.equal(response.corpus[0].passed, true);
  assert.deepEqual(response.tests, {
    requested: true,
    corpus_passed: 1,
    corpus_failed: 0,
    highlight_assertions_passed: 0,
    highlight_assertions_failed: 0,
    highlight_fixture_errors: 0,
  });
});

test("runs every vendored grammar sample through generated grammar.json and Snark WASM", async () => {
  const root = new URL("../src/bundled/", import.meta.url);
  const grammarIds = readdirSync(root)
    .filter((name) => statSync(new URL(name, root)).isDirectory())
    .sort();

  const results = [];
  for (const id of grammarIds) {
    const { files, rootId } = filesAndRootForVendoredId(id);
    const sample = preferredSampleForGrammarRootId(files, rootId);
    assert.ok(sample, `${id} should have a preferred sample`);
    const runnableFiles = await runnableFilesForBundle(files, rootId);
    const response = JSON.parse(
      parseBundle(
        JSON.stringify({
          files: runnableFiles,
          input: sample.text,
          run_corpus: false,
        }),
      ),
    );
    const result = {
      id,
      sample: sample.path,
      ok: response.ok,
      language: response.language,
      errorCount: response.parse?.accepted_error_count ?? null,
      missingCount: response.parse?.accepted_missing_count ?? null,
      captures: response.highlights.length,
      diagnostics: response.diagnostics,
    };
    results.push(result);
  }

  assert.deepEqual(
    results.map((result) => ({
      id: result.id,
      sample: result.sample,
      ok: result.ok,
      language: result.language,
      errorCount: result.errorCount,
      missingCount: result.missingCount,
    })),
    [
      { id: "capnp", sample: "samples/addressbook.capnp", ok: true, language: "capnp", errorCount: 0, missingCount: 0 },
      { id: "cedar", sample: "samples/example.cedar", ok: true, language: "cedar", errorCount: 0, missingCount: 0 },
      {
        id: "cedarschema",
        sample: "samples/example.cedarschema",
        ok: true,
        language: "cedarschema",
        errorCount: 0,
        missingCount: 0,
      },
      { id: "diff", sample: "samples/t-apply-1.patch", ok: true, language: "diff", errorCount: 0, missingCount: 0 },
      { id: "dot", sample: "samples/crazy.gv", ok: true, language: "dot", errorCount: 0, missingCount: 0 },
      { id: "fences", sample: "samples/demo.md", ok: true, language: "fences", errorCount: 0, missingCount: 0 },
      {
        id: "gingembre",
        sample: "samples/blog-index.html",
        ok: true,
        language: "gingembre",
        errorCount: 0,
        missingCount: 0,
      },
      {
        id: "gitattributes",
        sample: "samples/example.gitattributes",
        ok: true,
        language: "gitattributes",
        errorCount: 0,
        missingCount: 0,
      },
      {
        id: "graphql",
        sample: "samples/starwars_schema.graphql",
        ok: true,
        language: "graphql",
        errorCount: 0,
        missingCount: 0,
      },
      { id: "html", sample: "samples/implicit-close.html", ok: true, language: "html", errorCount: 0, missingCount: 0 },
      { id: "json", sample: "samples/package.json", ok: true, language: "json", errorCount: 0, missingCount: 0 },
      { id: "nginx", sample: "samples/basic.conf", ok: true, language: "nginx", errorCount: 0, missingCount: 0 },
      { id: "proto", sample: "samples/addressbook.proto", ok: true, language: "proto", errorCount: 0, missingCount: 0 },
      { id: "thrift", sample: "samples/tutorial.thrift", ok: true, language: "thrift", errorCount: 0, missingCount: 0 },
      { id: "yuri", sample: "samples/example.yuri", ok: true, language: "yuri", errorCount: 0, missingCount: 0 },
    ],
  );
  assert.ok(
    results.every((result) => result.captures > 0),
    JSON.stringify(results, null, 2),
  );
});

test("runs every non-error vendored sample through generated grammar.json and Snark WASM", async () => {
  const root = new URL("../src/bundled/", import.meta.url);
  const grammarIds = readdirSync(root)
    .filter((name) => statSync(new URL(name, root)).isDirectory())
    .sort();

  const failures = [];
  for (const id of grammarIds) {
    const { files, rootId } = filesAndRootForVendoredId(id);
    const runnableFiles = await runnableFilesForBundle(files, rootId);
    const samples = projectedFilesForGrammarRootId(files, rootId)
      .filter((file) => file.path.startsWith("samples/"))
      .filter((file) => !isErrorSamplePath(file.path))
      .sort((left, right) => left.path.localeCompare(right.path));

    for (const sample of samples) {
      const response = JSON.parse(
        parseBundle(
          JSON.stringify({
            files: runnableFiles,
            input: sample.text,
            run_corpus: false,
          }),
        ),
      );
      const errorCount = response.parse?.accepted_error_count ?? null;
      const missingCount = response.parse?.accepted_missing_count ?? null;
      if (response.ok && errorCount === 0 && missingCount === 0) {
        continue;
      }
      failures.push({
        id,
        sample: sample.path,
        ok: response.ok,
        language: response.language,
        errorCount,
        missingCount,
        diagnostics: response.diagnostics,
      });
    }
  }

  assert.deepEqual(failures, []);
});

const arboriumNginxDef = "/Users/amos/oss/arborium/langs/group-maple/nginx/def";

test(
  "reports Arborium nginx grammar.js dirty recovered parse through Snark WASM",
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

    assert.equal(response.ok, false);
    assert.equal(response.language, "nginx");
    assert.equal(response.diagnostics[0].stage, "parse");
    assert.match(response.diagnostics[0].message, /accepted parse contains/);
    assert.deepEqual(
      [
        response.diagnostics[0].primary_span.start_row,
        response.diagnostics[0].primary_span.start_column,
      ],
      [110, 4],
    );
    assert.ok(response.parse);
    assert.ok(response.parse.accepted_error_count > 0);
    assert.equal(response.parse.accepted_missing_count, 0);
    assert.match(response.parse.sexp, /\(ERROR/);
    assert.ok(response.highlights.length > 0);
  },
);

function isErrorSamplePath(path: string): boolean {
  return /(^|[-_/])(errors?|invalid|broken|fail)([-_.\\/]|$)/i.test(path);
}
