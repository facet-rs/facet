import officialDsl from "../../../snark-dsl/vendor/tree-sitter-generate-0.26.9/dsl.js?raw";
import type { DslBundleFile } from "./bundlePaths";
import { emitGrammarJsonFromDsl } from "./treeSitterDslRuntime";

type WorkerRequest = {
  files: DslBundleFile[];
  grammarPath: string;
};

self.onmessage = (event: MessageEvent<WorkerRequest>) => {
  try {
    const { files, grammarPath } = event.data;
    const grammarJson = emitGrammarJsonFromDsl(officialDsl, files, grammarPath);
    self.postMessage({ ok: true, grammarJson } satisfies WorkerResponse);
  } catch (error) {
    self.postMessage({
      ok: false,
      message: error instanceof Error ? error.message : String(error),
    } satisfies WorkerResponse);
  }
};

type WorkerResponse = { ok: true; grammarJson: string } | { ok: false; message: string };
