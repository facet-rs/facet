// Headless counterpart to the in-playground Benchmark panel: runs the graphql
// size ladder through the wasm session, requires clean parses and finite
// timings, and prints the ladder so `pnpm test` surfaces the actual numbers.
import assert from "node:assert/strict";
import { readdirSync, readFileSync, statSync } from "node:fs";
import test from "node:test";

import { SnarkPlaygroundSession, initSync } from "../../../snark-wasm/pkg/snark_wasm.js";
import {
  normalizeBundleFiles,
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

function graphqlBundle(): DslBundleFile[] {
  const root = new URL("../src/bundled/graphql/", import.meta.url);
  const walk = (prefix = ""): string[] => {
    const out: string[] = [];
    for (const name of readdirSync(new URL(prefix, root))) {
      const path = `${prefix}${name}`;
      if (statSync(new URL(path, root)).isDirectory()) out.push(...walk(`${path}/`));
      else out.push(path);
    }
    return out;
  };
  return normalizeBundleFiles(
    walk().map((path) => ({ path: `graphql/${path}`, text: readFileSync(new URL(path, root), "utf8") })),
  );
}

test("graphql size ladder parses cleanly and reports timings", async () => {
  const files = graphqlBundle();
  const runnable = await filesWithGrammarJsonUsingEmitter(files, "graphql", async (bundleFiles, grammarPath) =>
    emitGrammarJsonFromDsl(officialDsl, bundleFiles, grammarPath),
  );
  const session = new SnarkPlaygroundSession(JSON.stringify({ files: runnable }));
  try {
    const encoder = new TextEncoder();
    const samples = files
      .filter((file) => /\d+kb\.graphql$/.test(file.path))
      .filter((file) => file.text.length <= 128 * 1024)
      .map((file) => ({ name: file.path.split("/").pop() as string, text: file.text }))
      .sort((a, b) => a.text.length - b.text.length);
    assert.ok(samples.length >= 3, `expected a multi-rung ladder, got ${samples.length}`);

    const rows: { name: string; bytes: number; parseMs: number; xPrev: number; sizeRatio: number }[] = [];
    let prevMs: number | null = null;
    let prevBytes: number | null = null;
    for (const sample of samples) {
      let best = Infinity;
      for (let run = 0; run < 3; run += 1) {
        const resp = JSON.parse(session.parse(JSON.stringify({ input: sample.text, run_corpus: false, edit: null })));
        assert.equal(resp.ok, true, JSON.stringify(resp.diagnostics));
        assert.equal(resp.parse.accepted_error_count, 0, `${sample.name} parsed with ERROR nodes`);
        assert.ok(resp.timings && resp.timings.parse, `${sample.name} response missing parse timing`);
        best = Math.min(best, resp.timings.parse.ms);
      }
      const bytes = encoder.encode(sample.text).length;
      const xPrev = prevMs ? best / prevMs : 0;
      const sizeRatio = prevBytes ? bytes / prevBytes : 0;
      rows.push({ name: sample.name, bytes, parseMs: best, xPrev, sizeRatio });
      console.log(
        `${sample.name}\t${bytes} B\t${best.toFixed(3)} ms\t${xPrev ? `×${xPrev.toFixed(2)}` : "—"} (size ×${sizeRatio ? sizeRatio.toFixed(2) : "—"})`,
      );
      prevMs = best;
      prevBytes = bytes;
    }
    const ratios = rows
      .filter((row) => row.sizeRatio > 0 && row.xPrev > 0)
      .map((row) => row.xPrev / row.sizeRatio)
      .sort((a, b) => a - b);
    const scalingIndex = ratios[Math.floor(ratios.length / 2)];
    console.log(`scalingIndex (median xPrev/sizeRatio) = ${scalingIndex.toFixed(3)}  [1.0 = linear, >>1 = super-linear]`);

    assert.equal(Number.isFinite(scalingIndex), true);
    assert.ok(scalingIndex < 2, `expected near-linear scaling, got ${scalingIndex.toFixed(3)}`);
  } finally {
    session.free();
  }
});
