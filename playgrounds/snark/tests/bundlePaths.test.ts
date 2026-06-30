import assert from "node:assert/strict";
import test from "node:test";

import {
  discoverGrammarRoots,
  firstSampleForGrammarRootId,
  normalizeBundleFiles,
  preferredSampleForGrammarRootId,
  projectedFilesForGrammarRootId,
  type DslBundleFile,
} from "../src/bundlePaths.ts";

function file(path: string): DslBundleFile {
  return { path, text: "" };
}

test("normalizes a single Arborium def upload into Snark bundle paths", () => {
  const files = normalizeBundleFiles([
    file("tree-sitter-json/def/grammar/grammar.js"),
    file("tree-sitter-json/def/queries/highlights.scm"),
    file("tree-sitter-json/def/sample.json"),
    file("tree-sitter-json/def/samples/package.json"),
  ]);

  assert.deepEqual(
    files.map((entry) => entry.path).sort(),
    ["grammar.js", "queries/highlights.scm", "samples/package.json", "samples/sample.json"],
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
    file("tree-sitter-nginx/grammar.json"),
    file("tree-sitter-nginx/queries/highlights.scm"),
    file("tree-sitter-nginx/samples/nginx.conf"),
  ]);

  assert.deepEqual(
    files.map((entry) => entry.path).sort(),
    ["queries/highlights.scm", "samples/nginx.conf", "src/grammar.json"],
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
