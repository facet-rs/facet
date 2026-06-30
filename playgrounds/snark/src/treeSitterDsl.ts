import {
  filesWithGrammarJsonUsingEmitter,
  preferredGrammarRootId,
  type DslBundleFile,
} from "./bundlePaths";

export {
  discoverGrammarRoots,
  firstSampleForGrammarRootId,
  filesWithGrammarJsonUsingEmitter,
  grammarRootForId,
  normalizeBundleFiles,
  normalizePath,
  preferredSampleForGrammarRootId,
  preferredGrammarRootId,
  projectedFilesForGrammarRootId,
  sortedSampleFiles,
  sortedFiles,
  type DslBundleFile,
  type GrammarRoot,
  type ProjectedDslBundleFile,
} from "./bundlePaths";

export async function filesWithGrammarJson(
  files: DslBundleFile[],
  grammarRootId = preferredGrammarRootId(files),
): Promise<DslBundleFile[]> {
  return filesWithGrammarJsonUsingEmitter(files, grammarRootId, emitGrammarJsonInWorker);
}

function emitGrammarJsonInWorker(files: DslBundleFile[], grammarPath: string): Promise<string> {
  return new Promise((resolve, reject) => {
    const worker = new Worker(new URL("./treeSitterDslWorker.ts", import.meta.url), { type: "module" });
    worker.onmessage = (event: MessageEvent<WorkerResponse>) => {
      worker.terminate();
      if (event.data.ok) {
        resolve(event.data.grammarJson);
      } else {
        reject(new Error(event.data.message));
      }
    };
    worker.onerror = (event) => {
      worker.terminate();
      reject(new Error(event.message));
    };
    worker.postMessage({ files, grammarPath } satisfies WorkerRequest);
  });
}

type WorkerRequest = {
  files: DslBundleFile[];
  grammarPath: string;
};

type WorkerResponse = { ok: true; grammarJson: string } | { ok: false; message: string };
