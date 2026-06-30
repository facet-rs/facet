import assert from "node:assert/strict";
import test from "node:test";

import {
  discoverGrammarRoots,
  firstSampleForGrammarRootId,
  normalizeBundleFiles,
  preferredSampleForGrammarRootId,
  projectedFilesForGrammarRootId,
  sourceExamplesForGrammarRootId,
  filesWithGrammarJsonUsingEmitter,
  type DslBundleFile,
} from "../src/bundlePaths.ts";

function file(path: string): DslBundleFile {
  return { path, text: "" };
}

test("normalizes a single Arborium def upload into Snark bundle paths", () => {
  const files = normalizeBundleFiles([
    file("tree-sitter-json/def/grammar/tree-sitter.json"),
    file("tree-sitter-json/def/grammar/grammar.js"),
    file("tree-sitter-json/def/queries/highlights.scm"),
    file("tree-sitter-json/def/sample.json"),
    file("tree-sitter-json/def/samples/package.json"),
  ]);

  assert.deepEqual(
    files.map((entry) => entry.path).sort(),
    ["grammar.js", "queries/highlights.scm", "samples/package.json", "samples/sample.json", "tree-sitter.json"],
  );
  assert.deepEqual(discoverGrammarRoots(files), [
    {
      id: "",
      label: "bundle root",
      grammarPath: "grammar.js",
      kind: "package",
    },
  ]);
});

test("keeps Arborium sibling grammar roots selectable", () => {
  const files = normalizeBundleFiles([
    file("langs/group-acorn/json/def/grammar/grammar.js"),
    file("langs/group-acorn/json/def/queries/highlights.scm"),
    file("langs/group-acorn/css/def/grammar/grammar.js"),
    file("langs/group-acorn/css/def/queries/highlights.scm"),
  ]);

  const roots = discoverGrammarRoots(files);
  assert.deepEqual(
    roots.map((root) => root.id),
    ["group-acorn/json/def", "group-acorn/css/def"],
  );
  assert.deepEqual(
    projectedFilesForGrammarRootId(files, "group-acorn/json/def").map((entry) => entry.path),
    ["grammar.js", "queries/highlights.scm"],
  );
});

test("prefers grammar.json under any path over grammar.js roots", () => {
  const files = normalizeBundleFiles([
    file("packages/tree-sitter-css/grammar.js"),
    file("vendor/random/layout/tree-sitter-json/grammar.json"),
    file("vendor/random/layout/tree-sitter-json/queries/highlights.scm"),
  ]);

  const roots = discoverGrammarRoots(files);
  assert.deepEqual(roots, [
    {
      id: "vendor/random/layout/tree-sitter-json",
      label: "vendor/random/layout/tree-sitter-json",
      grammarPath: "vendor/random/layout/tree-sitter-json/grammar.json",
      kind: "package",
    },
  ]);
  assert.deepEqual(
    projectedFilesForGrammarRootId(files, roots[0]?.id).map((entry) => entry.path),
    ["src/grammar.json", "queries/highlights.scm"],
  );
});

test("normalizes package root grammar.json to src/grammar.json", () => {
  const files = normalizeBundleFiles([
    file("tree-sitter-nginx/tree-sitter.json"),
    file("tree-sitter-nginx/grammar.json"),
    file("tree-sitter-nginx/queries/highlights.scm"),
    file("tree-sitter-nginx/samples/nginx.conf"),
  ]);

  assert.deepEqual(
    files.map((entry) => entry.path).sort(),
    ["queries/highlights.scm", "samples/nginx.conf", "src/grammar.json", "tree-sitter.json"],
  );
  assert.deepEqual(discoverGrammarRoots(files), [
    {
      id: "",
      label: "bundle root",
      grammarPath: "src/grammar.json",
      kind: "package",
    },
  ]);
});

test("projects package examples as selectable samples", () => {
  const files = normalizeBundleFiles([
    file("tree-sitter-demo/grammar.js"),
    file("tree-sitter-demo/examples/basic.demo"),
    file("tree-sitter-demo/example.demo"),
  ]);

  assert.deepEqual(
    files.map((entry) => entry.path).sort(),
    ["grammar.js", "samples/basic.demo", "samples/example.demo"],
  );
  assert.deepEqual(preferredSampleForGrammarRootId(files), {
    path: "samples/basic.demo",
    sourcePath: "samples/basic.demo",
    text: "",
  });
});

test("projects Arborium examples as selectable samples", () => {
  const files = normalizeBundleFiles([
    file("tree-sitter-demo/def/grammar/grammar.js"),
    file("tree-sitter-demo/def/examples/basic.demo"),
    file("tree-sitter-demo/def/example.demo"),
  ]);

  assert.deepEqual(
    files.map((entry) => entry.path).sort(),
    ["grammar.js", "samples/basic.demo", "samples/example.demo"],
  );
  assert.deepEqual(preferredSampleForGrammarRootId(files), {
    path: "samples/basic.demo",
    sourcePath: "samples/basic.demo",
    text: "",
  });
});

test("projects package manifest into manifest-declared grammar roots", () => {
  const manifest = JSON.stringify({
    grammars: [
      {
        name: "host",
        scope: "source.host",
        path: "grammars/host",
        highlights: "queries/host-highlights.scm",
      },
      {
        name: "child",
        scope: "source.child",
        path: "grammars/child",
      },
    ],
    metadata: {
      version: "0.0.0",
      links: { repository: "https://example.com/package" },
    },
  });
  const files = normalizeBundleFiles([
    { path: "tree-sitter-package/tree-sitter.json", text: manifest },
    file("tree-sitter-package/grammars/host/src/grammar.json"),
    file("tree-sitter-package/grammars/host/queries/host-highlights.scm"),
    file("tree-sitter-package/grammars/child/src/grammar.json"),
  ]);

  const roots = discoverGrammarRoots(files);
  assert.deepEqual(
    roots.map((root) => root.id),
    ["grammars/host", "grammars/child"],
  );
  assert.deepEqual(
    projectedFilesForGrammarRootId(files, "grammars/host").map((entry) => [entry.path, entry.sourcePath]),
    [
      ["src/grammar.json", "grammars/host/src/grammar.json"],
      ["queries/host-highlights.scm", "grammars/host/queries/host-highlights.scm"],
      ["tree-sitter.json", "tree-sitter.json"],
    ],
  );
});

test("keeps grammar.json as the active grammar when a sibling grammar.js exists", () => {
  const files = normalizeBundleFiles([
    file("tree-sitter-frozen/grammar.json"),
    file("tree-sitter-frozen/grammar.js"),
    file("tree-sitter-frozen/queries/highlights.scm"),
  ]);

  const roots = discoverGrammarRoots(files);
  assert.deepEqual(roots, [
    {
      id: "",
      label: "bundle root",
      grammarPath: "src/grammar.json",
      kind: "package",
    },
  ]);
  assert.deepEqual(
    projectedFilesForGrammarRootId(files, roots[0]?.id).map((entry) => entry.path),
    ["src/grammar.json", "grammar.js", "queries/highlights.scm"],
  );
});

test("selects the first uploaded sample for a single uploaded grammar root", () => {
  const files = normalizeBundleFiles([
    file("tree-sitter-nginx/grammar.json"),
    file("tree-sitter-nginx/samples/z-last.conf"),
    file("tree-sitter-nginx/samples/a-first.conf"),
  ]);

  assert.deepEqual(firstSampleForGrammarRootId(files), {
    path: "samples/z-last.conf",
    sourcePath: "samples/z-last.conf",
    text: "",
  });
});

test("prefers non-error samples before error fixtures", () => {
  const files = normalizeBundleFiles([
    file("tree-sitter-nginx/grammar.js"),
    file("tree-sitter-nginx/samples/nginx-errors.conf"),
    file("tree-sitter-nginx/samples/nginx.conf"),
    file("tree-sitter-nginx/samples/basic.conf"),
  ]);

  assert.deepEqual(preferredSampleForGrammarRootId(files), {
    path: "samples/basic.conf",
    sourcePath: "samples/basic.conf",
    text: "",
  });
});

test("uses first corpus case as preferred source when samples are absent", () => {
  const files = normalizeBundleFiles([
    file("tree-sitter-demo/grammar.js"),
    {
      path: "tree-sitter-demo/test/corpus/main.txt",
      text: "====================\nFirst case\n====================\n\nalpha beta\n\n---\n\n(document)\n\n====================\nSecond case\n====================\n\ngamma\n\n---\n\n(document)\n",
    },
  ]);

  assert.deepEqual(sourceExamplesForGrammarRootId(files).map((entry) => [entry.path, entry.text]), [
    ["test/corpus/main.txt#First case", "alpha beta"],
    ["test/corpus/main.txt#Second case", "gamma"],
  ]);
  assert.deepEqual(preferredSampleForGrammarRootId(files), {
    path: "test/corpus/main.txt#First case",
    sourcePath: "test/corpus/main.txt",
    text: "alpha beta",
  });
});

test("keeps explicit samples ahead of corpus case source examples", () => {
  const files = normalizeBundleFiles([
    file("tree-sitter-demo/grammar.js"),
    file("tree-sitter-demo/samples/basic.demo"),
    {
      path: "tree-sitter-demo/test/corpus/main.txt",
      text: "====================\nCorpus case\n====================\n\nfrom corpus\n\n---\n\n(document)\n",
    },
  ]);

  assert.deepEqual(sourceExamplesForGrammarRootId(files).map((entry) => entry.path), [
    "samples/basic.demo",
    "test/corpus/main.txt#Corpus case",
  ]);
  assert.deepEqual(preferredSampleForGrammarRootId(files)?.path, "samples/basic.demo");
});

test("selects the first projected sample for the chosen Arborium grammar root", () => {
  const files = normalizeBundleFiles([
    file("langs/group-maple/nginx/def/grammar/grammar.js"),
    file("langs/group-maple/nginx/def/samples/nginx.conf"),
    file("langs/group-maple/hcl/def/grammar/grammar.js"),
    file("langs/group-maple/hcl/def/samples/main.hcl"),
  ]);

  assert.deepEqual(firstSampleForGrammarRootId(files, "group-maple/nginx/def"), {
    path: "samples/nginx.conf",
    sourcePath: "group-maple/nginx/def/samples/nginx.conf",
    text: "",
  });
  assert.deepEqual(firstSampleForGrammarRootId(files, "group-maple/hcl/def"), {
    path: "samples/main.hcl",
    sourcePath: "group-maple/hcl/def/samples/main.hcl",
    text: "",
  });
});

test("projects sibling grammar roots as embedded language bundles", async () => {
  const files = normalizeBundleFiles([
    { path: "host/grammar.js", text: 'grammar({ name: "host" })' },
    { path: "host/queries/injections.scm", text: "host injections" },
    { path: "child/grammar.js", text: 'grammar({ name: "child" })' },
    { path: "child/queries/highlights.scm", text: "child highlights" },
  ]);

  const projected = await filesWithGrammarJsonUsingEmitter(files, "host", async (_files, grammarPath) =>
    grammarPath === "host/grammar.js" ? '{"name":"host"}' : '{"name":"child"}',
  );

  assert.deepEqual(
    projected.map((entry) => entry.path),
    [
      "grammar.js",
      "languages/child/grammar.js",
      "languages/child/queries/highlights.scm",
      "languages/child/src/grammar.json",
      "queries/injections.scm",
      "src/grammar.json",
    ],
  );
});

test("keeps selected grammar runnable when a sibling grammar root cannot be emitted", async () => {
  const files = normalizeBundleFiles([
    { path: "host/grammar.js", text: 'grammar({ name: "host" })' },
    { path: "host/queries/injections.scm", text: "host injections" },
    { path: "broken/grammar.js", text: "not valid grammar js" },
  ]);

  const projected = await filesWithGrammarJsonUsingEmitter(files, "host", async (_files, grammarPath) => {
    if (grammarPath === "broken/grammar.js") {
      throw new Error("cannot emit sibling");
    }
    return '{"name":"host"}';
  });

  assert.deepEqual(
    projected.map((entry) => entry.path),
    ["grammar.js", "queries/injections.scm", "src/grammar.json"],
  );
});
