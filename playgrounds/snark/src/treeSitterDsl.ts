import officialDsl from "../../../snark-dsl/vendor/tree-sitter-generate-0.26.9/dsl.js?raw";

export type DslBundleFile = {
  path: string;
  text: string;
};

export type ProjectedDslBundleFile = DslBundleFile & {
  sourcePath: string;
};

export type GrammarRoot = {
  id: string;
  label: string;
  grammarPath: string;
  kind: "package" | "arborium";
};

export async function filesWithGrammarJson(
  files: DslBundleFile[],
  grammarRootId = preferredGrammarRootId(files),
): Promise<DslBundleFile[]> {
  const root = grammarRootForId(files, grammarRootId);
  const rootFiles = filesForGrammarRoot(files, root).map(({ path, text }) => ({ path, text }));
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

export function discoverGrammarRoots(files: DslBundleFile[]): GrammarRoot[] {
  const normalizedPaths = files.map((file) => normalizePath(file.path));
  const hasGrammarJson = normalizedPaths.some((path) => basename(path) === "grammar.json");
  const roots = new Map<string, GrammarRoot>();
  for (const path of normalizedPaths) {
    const name = basename(path);
    if (hasGrammarJson ? name === "grammar.json" : name === "grammar.js") {
      const root = grammarRootFromPath(path);
      roots.set(root.id, root);
    }
  }
  return [...roots.values()].sort((left, right) => left.label.localeCompare(right.label));
}

export function preferredGrammarRootId(files: DslBundleFile[]) {
  return discoverGrammarRoots(files)[0]?.id ?? "";
}

export function grammarRootForId(files: DslBundleFile[], rootId: string): GrammarRoot | null {
  const roots = discoverGrammarRoots(files);
  return roots.find((root) => root.id === rootId) ?? roots[0] ?? null;
}

export function projectedFilesForGrammarRootId(
  files: DslBundleFile[],
  rootId = preferredGrammarRootId(files),
): ProjectedDslBundleFile[] {
  return filesForGrammarRoot(files, grammarRootForId(files, rootId));
}

function filesForGrammarRoot(files: DslBundleFile[], root: GrammarRoot | null): ProjectedDslBundleFile[] {
  if (!root || root.id === "") {
    return sortedFiles(files.map((file) => ({ ...file, sourcePath: file.path })));
  }

  const prefix = `${root.id}/`;
  return sortedFiles(
    files
      .filter((file) => file.path.startsWith(prefix))
      .map((file) => {
        const relative = file.path.slice(prefix.length);
        return {
          path:
            root.kind === "arborium"
              ? (normalizeArboriumDefPath(relative) ?? relative)
              : (normalizePackagePath(relative) ?? relative),
          text: file.text,
          sourcePath: file.path,
        };
      }),
  );
}

function grammarRootFromPath(path: string): GrammarRoot {
  const root = rootDirectoryForGrammarPath(path);
  return {
    id: root,
    label: root || "bundle root",
    grammarPath: path,
    kind: path.includes("/def/grammar/") || path.startsWith("def/grammar/") ? "arborium" : "package",
  };
}

function rootDirectoryForGrammarPath(path: string) {
  if (path === "grammar.json" || path === "grammar.js") {
    return "";
  }
  if (path.startsWith("def/grammar/")) {
    return "";
  }
  for (const suffix of ["/grammar/grammar.json", "/grammar/src/grammar.json", "/grammar/grammar.js"]) {
    if (path.endsWith(`/def${suffix}`)) {
      return path.slice(0, -suffix.length);
    }
  }
  for (const suffix of [
    "/src/grammar.json",
    "/grammar.json",
    "/grammar.js",
  ]) {
    if (path.endsWith(suffix)) {
      return path.slice(0, -suffix.length);
    }
  }
  return path.includes("/") ? path.slice(0, path.lastIndexOf("/")) : "";
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

function sortedFiles<T extends DslBundleFile>(files: T[]): T[] {
  return [...files].sort((left, right) => left.path.localeCompare(right.path));
}

function normalizePath(path: string) {
  let normalized = path.replace(/\\/g, "/");
  while (normalized.startsWith("./")) {
    normalized = normalized.slice(2);
  }
  return normalized;
}

function normalizeArboriumDefPath(relative: string) {
  switch (relative) {
    case "grammar/grammar.js":
      return "grammar.js";
    case "grammar/grammar.json":
    case "grammar/src/grammar.json":
      return "src/grammar.json";
    case "grammar/scanner.c":
      return "src/scanner.c";
    case "grammar/scanner.cc":
      return "src/scanner.cc";
    case "grammar/src/parser.c":
      return "src/parser.c";
    case "grammar/src/parser.cc":
      return "src/parser.cc";
    case "grammar/src/parser.h":
      return "src/parser.h";
    case "grammar/src/node-types.json":
      return "src/node-types.json";
    case "grammar/bindings/node/binding.cc":
      return "bindings/node/binding.cc";
    default:
      break;
  }
  if (
    relative.startsWith("queries/") ||
    relative.startsWith("test/corpus/") ||
    relative.startsWith("test/highlight/") ||
    relative.startsWith("test/highlights/") ||
    relative.startsWith("samples/")
  ) {
    return relative;
  }
  if (relative.startsWith("sample.")) {
    return `samples/${relative}`;
  }
  return null;
}

function normalizePackagePath(path: string) {
  if (
    [
      "grammar.json",
      "grammar.js",
      "src/grammar.json",
      "src/scanner.c",
      "src/scanner.cc",
      "src/parser.c",
      "src/parser.cc",
      "src/parser.h",
      "src/node-types.json",
      "bindings/node/binding.cc",
    ].includes(path)
  ) {
    return path === "grammar.json" ? "src/grammar.json" : path;
  }
  for (const suffix of [
    "/grammar.json",
    "/src/grammar.json",
    "/src/scanner.c",
    "/src/scanner.cc",
    "/src/parser.c",
    "/src/parser.cc",
    "/src/parser.h",
    "/src/node-types.json",
    "/bindings/node/binding.cc",
  ]) {
    if (path.endsWith(suffix)) {
      return suffix === "/grammar.json" ? "src/grammar.json" : suffix.slice(1);
    }
  }
  for (const token of ["/queries/", "/test/corpus/", "/test/highlight/", "/test/highlights/", "/samples/"]) {
    const index = path.indexOf(token);
    if (index >= 0) {
      return path.slice(index + 1);
    }
  }
  return null;
}

function basename(path: string) {
  const index = path.lastIndexOf("/");
  return index >= 0 ? path.slice(index + 1) : path;
}

type WorkerRequest = {
  dslSource: string;
  files: DslBundleFile[];
  grammarPath: string;
};

type WorkerResponse = { ok: true; grammarJson: string } | { ok: false; message: string };
