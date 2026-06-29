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

export function sortedFiles<T extends DslBundleFile>(files: T[]): T[] {
  return [...files].sort((left, right) => left.path.localeCompare(right.path));
}

export function normalizeBundleFiles(files: DslBundleFile[]): DslBundleFile[] {
  const stripped = stripCommonRoot(files);
  const context = normalizationContext(stripped.map((file) => file.path));
  return stripped.map((file) => ({ ...file, path: normalizeBundlePath(file.path, context) }));
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

export function normalizePath(path: string) {
  let normalized = path.replace(/\\/g, "/");
  while (normalized.startsWith("./")) {
    normalized = normalized.slice(2);
  }
  return normalized;
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
  if (path === "grammar.json" || path === "grammar.js" || path === "src/grammar.json") {
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
  for (const suffix of ["/src/grammar.json", "/grammar.json", "/grammar.js"]) {
    if (path.endsWith(suffix)) {
      return path.slice(0, -suffix.length);
    }
  }
  return path.includes("/") ? path.slice(0, path.lastIndexOf("/")) : "";
}

function stripCommonRoot(files: DslBundleFile[]) {
  if (files.length === 0) {
    return files;
  }
  const firstSegments = files[0].path.split("/");
  if (firstSegments.length < 2) {
    return files;
  }
  const root = firstSegments[0];
  if (!files.every((file) => file.path === root || file.path.startsWith(`${root}/`))) {
    return files;
  }
  return files.map((file) => ({ ...file, path: file.path.slice(root.length + 1) }));
}

type NormalizationContext = {
  arboriumRoots: Set<string>;
  packageRoots: Set<string>;
};

function normalizationContext(paths: string[]): NormalizationContext {
  const normalized = paths.map(normalizePath);
  return {
    arboriumRoots: new Set(normalized.flatMap(arboriumRoot)),
    packageRoots: new Set(normalized.flatMap(packageRoot)),
  };
}

function normalizeBundlePath(path: string, context: NormalizationContext) {
  const normalized = normalizePath(path);
  const arborium = arboriumDefRelative(normalized, context);
  if (arborium) {
    const mapped = normalizeArboriumDefPath(arborium);
    if (mapped) {
      return mapped;
    }
  }
  if (isAmbiguousArboriumDefPath(normalized, context)) {
    return normalized;
  }
  const packageRelative = packageRootRelative(normalized, context);
  if (packageRelative) {
    const mapped = normalizePackagePath(packageRelative);
    if (mapped) {
      return mapped;
    }
  }
  if (isAmbiguousPackagePath(normalized, context)) {
    return normalized;
  }
  return normalizePackagePath(normalized) ?? normalized;
}

function arboriumRoot(path: string) {
  if (
    path.startsWith("def/grammar/grammar.json") ||
    path.startsWith("def/grammar/src/grammar.json") ||
    path.startsWith("def/grammar/grammar.js")
  ) {
    return [""];
  }
  for (const marker of [
    "/def/grammar/grammar.json",
    "/def/grammar/src/grammar.json",
    "/def/grammar/grammar.js",
  ]) {
    const index = path.indexOf(marker);
    if (index >= 0) {
      return [path.slice(0, index)];
    }
  }
  return [];
}

function packageRoot(path: string) {
  if (path === "grammar.js" || path === "grammar.json" || path === "src/grammar.json") {
    return [""];
  }
  if (
    path.endsWith("/def/grammar/grammar.json") ||
    path.endsWith("/def/grammar/src/grammar.json") ||
    path.endsWith("/def/grammar/grammar.js")
  ) {
    return [];
  }
  if (path.endsWith("/grammar.js")) {
    return [path.slice(0, -"/grammar.js".length)];
  }
  if (path.endsWith("/src/grammar.json")) {
    return [path.slice(0, -"/src/grammar.json".length)];
  }
  if (path.endsWith("/grammar.json")) {
    return [path.slice(0, -"/grammar.json".length)];
  }
  return [];
}

function arboriumDefRelative(path: string, context: NormalizationContext) {
  if (path.startsWith("def/")) {
    return path.slice("def/".length);
  }
  const marker = "/def/";
  const index = path.indexOf(marker);
  if (index < 0 || context.arboriumRoots.size !== 1) {
    return null;
  }
  return path.slice(index + marker.length);
}

function isAmbiguousArboriumDefPath(path: string, context: NormalizationContext) {
  return path.includes("/def/") && context.arboriumRoots.size !== 1;
}

function packageRootRelative(path: string, context: NormalizationContext) {
  if (context.packageRoots.size !== 1) {
    return null;
  }
  const root = Array.from(context.packageRoots)[0];
  if (!root || !path.startsWith(`${root}/`)) {
    return null;
  }
  return path.slice(root.length + 1);
}

function isAmbiguousPackagePath(path: string, context: NormalizationContext) {
  if (context.packageRoots.size <= 1) {
    return false;
  }
  for (const root of context.packageRoots) {
    if (root && path.startsWith(`${root}/`)) {
      return true;
    }
  }
  return false;
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
