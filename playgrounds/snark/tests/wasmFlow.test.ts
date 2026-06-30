import assert from "node:assert/strict";
import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import test from "node:test";

import { SnarkPlaygroundSession, initSync, parseBundle } from "../../../snark-wasm/pkg/snark_wasm.js";
import {
  discoverGrammarRoots,
  normalizeBundleFiles,
  preferredGrammarRootId,
  preferredSampleForGrammarRootId,
  projectedFilesForGrammarRootId,
  sortedFiles,
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

function runnableFilesForBundle(files: DslBundleFile[]) {
  const rootId = preferredGrammarRootId(files);
  const root = discoverGrammarRoots(files).find((candidate) => candidate.id === rootId);
  assert.ok(root, "bundle should have a grammar root");
  const grammarJson = emitGrammarJsonFromDsl(officialDsl, files, root.grammarPath);
  const projected = projectedFilesForGrammarRootId(files, rootId).map(({ path, text }) => ({
    path,
    text,
  }));
  return sortedFiles([...projected, { path: "src/grammar.json", text: grammarJson }]);
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
      path: "host/tree-sitter.json",
      text: JSON.stringify({
        grammars: [
          {
            name: "host",
            scope: "source.host",
            injections: "queries/embed.scm",
          },
        ],
        metadata: {
          version: "0.0.0",
          links: { repository: "https://example.com/host" },
        },
      }),
    },
    { path: "host/grammar.js", text: hostGrammar },
    {
      path: "host/queries/embed.scm",
      text: '((word) @injection.content\n  (#set! injection.language "text/x-child"))\n',
    },
    {
      path: "child/tree-sitter.json",
      text: JSON.stringify({
        grammars: [
          {
            name: "child",
            scope: "source.child",
            "injection-regex": "^text/x-child$",
            highlights: "queries/child-highlights.scm",
          },
        ],
        metadata: {
          version: "0.0.0",
          links: { repository: "https://example.com/child" },
        },
      }),
    },
    { path: "child/grammar.js", text: childGrammar },
    { path: "child/queries/child-highlights.scm", text: "(word) @constant\n" },
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

test("runs every vendored grammar sample through generated grammar.json and Snark WASM", () => {
  const root = new URL("../src/bundled/", import.meta.url);
  const grammarIds = readdirSync(root)
    .filter((name) => statSync(new URL(name, root)).isDirectory())
    .sort();

  const results = grammarIds.map((id) => {
    const files = bundledFiles(id);
    const sample = preferredSampleForGrammarRootId(files);
    assert.ok(sample, `${id} should have a preferred sample`);
    const response = JSON.parse(
      parseBundle(
        JSON.stringify({
          files: runnableFilesForBundle(files),
          input: sample.text,
          run_corpus: false,
        }),
      ),
    );
    return {
      id,
      sample: sample.path,
      ok: response.ok,
      language: response.language,
      errorCount: response.parse?.accepted_error_count ?? null,
      missingCount: response.parse?.accepted_missing_count ?? null,
      captures: response.highlights.length,
      diagnostics: response.diagnostics,
    };
  });

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

test("runs every non-error vendored sample through generated grammar.json and Snark WASM", () => {
  const root = new URL("../src/bundled/", import.meta.url);
  const grammarIds = readdirSync(root)
    .filter((name) => statSync(new URL(name, root)).isDirectory())
    .sort();

  const failures = grammarIds.flatMap((id) => {
    const files = bundledFiles(id);
    const runnableFiles = runnableFilesForBundle(files);
    const samples = projectedFilesForGrammarRootId(files, preferredGrammarRootId(files))
      .filter((file) => file.path.startsWith("samples/"))
      .filter((file) => !isErrorSamplePath(file.path))
      .sort((left, right) => left.path.localeCompare(right.path));

    return samples.flatMap((sample) => {
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
        return [];
      }
      return [
        {
          id,
          sample: sample.path,
          ok: response.ok,
          language: response.language,
          errorCount,
          missingCount,
          diagnostics: response.diagnostics,
        },
      ];
    });
  });

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
