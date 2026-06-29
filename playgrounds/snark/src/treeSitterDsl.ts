import {
  grammarRootForId,
  preferredGrammarRootId,
  projectedFilesForGrammarRootId,
  sortedFiles,
  type DslBundleFile,
} from "./bundlePaths";

export {
  discoverGrammarRoots,
  grammarRootForId,
  normalizeBundleFiles,
  normalizePath,
  preferredGrammarRootId,
  projectedFilesForGrammarRootId,
  sortedFiles,
  type DslBundleFile,
  type GrammarRoot,
  type ProjectedDslBundleFile,
} from "./bundlePaths";

export async function filesWithGrammarJson(
  files: DslBundleFile[],
  grammarRootId = preferredGrammarRootId(files),
): Promise<DslBundleFile[]> {
  const root = grammarRootForId(files, grammarRootId);
  const rootFiles = projectedFilesForGrammarRootId(files, grammarRootId).map(({ path, text }) => ({
    path,
    text,
  }));
  if (rootFiles.some((file) => file.path === "src/grammar.json")) {
    return rootFiles;
  }

  const grammarFile = root
    ? files.find((file) => file.path === root.grammarPath)
    : files.find((file) => file.path === "grammar.js");
  if (!grammarFile) {
    return rootFiles;
  }
  const grammarJson = await emitGrammarJsonInWorker(files, grammarFile.path);
  return sortedFiles([...rootFiles, { path: "src/grammar.json", text: grammarJson }]);
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
