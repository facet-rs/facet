import officialDsl from "../../../snark-dsl/vendor/tree-sitter-generate-0.26.9/dsl.js?raw";
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
    const worker = new Worker(workerUrl(), { type: "module" });
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
    worker.postMessage({ dslSource: officialDsl, files, grammarPath } satisfies WorkerRequest);
  });
}

let cachedWorkerUrl: string | null = null;

function workerUrl(): string {
  if (!cachedWorkerUrl) {
    cachedWorkerUrl = URL.createObjectURL(new Blob([workerSource()], { type: "text/javascript" }));
  }
  return cachedWorkerUrl;
}

function workerSource(): string {
  return String.raw`
self.onmessage = (event) => {
  try {
    const { dslSource, files, grammarPath } = event.data;
    const grammarJson = emitGrammarJson(dslSource, files, grammarPath);
    self.postMessage({ ok: true, grammarJson });
  } catch (error) {
    self.postMessage({ ok: false, message: error instanceof Error ? error.message : String(error) });
  }
};

function emitGrammarJson(dslSource, files, grammarPath) {
  const prelude = officialDslPrelude(dslSource);
  const modules = new Map(files.filter((file) => file.path.endsWith(".js")).map((file) => [file.path, file.text]));
  const cache = new Map();

  const loadModule = (path) => {
    const resolved = resolveJsPath(path, modules);
    if (cache.has(resolved)) return cache.get(resolved).exports;
    const source = modules.get(resolved);
    if (source == null) throw new Error("missing grammar module " + resolved);

    const module = { exports: {} };
    cache.set(resolved, module);
    const dirname = resolved.includes("/") ? resolved.slice(0, resolved.lastIndexOf("/")) : "";
    const require = (specifier) => loadModule(resolveRequire(specifier, dirname, modules));
    const commonJsSource = source.replace(/(^|\n)\s*export\s+default\s+/m, "$1module.exports = ");
    if (/^\s*import\s/m.test(commonJsSource)) {
      throw new Error(resolved + " uses ESM imports; upload a CommonJS grammar.js bundle or pre-bundle dependencies first");
    }

    const fn = new Function("module", "exports", "require", prelude + "\n" + commonJsSource + "\n; return module.exports;");
    fn(module, module.exports, require);
    return module.exports;
  };

  const exported = loadModule(grammarPath);
  const grammarObj = exported && exported.grammar
    ? exported.grammar
    : exported && exported.default && exported.default.grammar
      ? exported.default.grammar
      : exported && exported.default
        ? exported.default
        : exported;
  if (!grammarObj || typeof grammarObj !== "object" || typeof grammarObj.name !== "string") {
    throw new Error(grammarPath + " did not export a Tree-sitter grammar object");
  }
  normalizePatternSources(grammarObj);
  return JSON.stringify({ "$schema": "https://tree-sitter.github.io/tree-sitter/assets/schemas/grammar.schema.json", ...grammarObj }, null, 2) + "\n";
}

function officialDslPrelude(dslSource) {
  const marker = "const grammarPath = getEnv(\"TREE_SITTER_GRAMMAR_PATH\");";
  const index = dslSource.indexOf(marker);
  if (index < 0) throw new Error("official Tree-sitter DSL entrypoint marker was not found");
  return dslSource.slice(0, index);
}

function resolveRequire(specifier, dirname, modules) {
  if (specifier.startsWith("./") || specifier.startsWith("../")) {
    return resolveJsPath(normalizeRelativePath(dirname, specifier), modules);
  }

  const grammarMatch = /^tree-sitter-([^/]+)\/grammar(?:\.js)?$/.exec(specifier);
  if (grammarMatch) {
    const grammarId = grammarMatch[1];
    for (const candidate of [
      "node_modules/tree-sitter-" + grammarId + "/grammar.js",
      "tree-sitter-" + grammarId + "/grammar.js",
      "langs/" + grammarId + "/def/grammar/grammar.js",
    ]) {
      if (modules.has(candidate)) return candidate;
    }
    for (const key of modules.keys()) {
      if (
        key.endsWith("/node_modules/tree-sitter-" + grammarId + "/grammar.js") ||
        key.endsWith("/tree-sitter-" + grammarId + "/grammar.js") ||
        key.endsWith("/" + grammarId + "/def/grammar/grammar.js")
      ) {
        return key;
      }
    }
  }

  throw new Error("cannot resolve grammar dependency " + specifier);
}

function resolveJsPath(path, modules) {
  for (const candidate of [path, path + ".js", path + "/index.js", path + "/grammar.js"]) {
    if (modules.has(candidate)) return candidate;
  }
  throw new Error("could not resolve JavaScript module " + path);
}

function normalizeRelativePath(dirname, specifier) {
  const parts = (dirname ? dirname.split("/") : []).concat(specifier.split("/"));
  const out = [];
  for (const part of parts) {
    if (!part || part === ".") continue;
    if (part === "..") out.pop();
    else out.push(part);
  }
  return out.join("/");
}

function normalizePatternSources(root) {
  const stack = [root];
  while (stack.length > 0) {
    const value = stack.pop();
    if (!value || typeof value !== "object") continue;

    if (value.type === "PATTERN" && typeof value.value === "string") {
      value.value = normalizePatternSourceLikeTreeSitter(value.value);
    }

    for (const key of Object.keys(value)) {
      stack.push(value[key]);
    }
  }
}

function normalizePatternSourceLikeTreeSitter(source) {
  let out = "";
  let escaped = false;
  let inCharacterClass = false;

  for (const ch of source) {
    if (escaped) {
      out += inCharacterClass && ch === "/" ? "/" : "\\" + ch;
      escaped = false;
      continue;
    }

    if (ch === "\\") {
      escaped = true;
      continue;
    }

    if (ch === "[") {
      inCharacterClass = true;
    } else if (ch === "]") {
      inCharacterClass = false;
    }

    out += ch;
  }

  if (escaped) out += "\\";
  return out;
}
`;
}

type WorkerRequest = {
  dslSource: string;
  files: DslBundleFile[];
  grammarPath: string;
};

type WorkerResponse = { ok: true; grammarJson: string } | { ok: false; message: string };
