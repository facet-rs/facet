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
  manifestPath?: string;
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
  const normalizedFiles = files.map((file) => ({ ...file, path: normalizePath(file.path) }));
  const normalizedPaths = normalizedFiles.map((file) => file.path);
  const normalizedByPath = new Map(normalizedFiles.map((file) => [file.path, file]));
  const hasGrammarJson = normalizedPaths.some((path) => basename(path) === "grammar.json");
  const roots = new Map<string, GrammarRoot>();
  for (const file of normalizedFiles) {
    if (basename(file.path) !== "tree-sitter.json") {
      continue;
    }
    for (const root of grammarRootsFromManifest(file.path, file.text, normalizedByPath, hasGrammarJson)) {
      roots.set(root.id, root);
    }
  }
  for (const path of normalizedPaths) {
    const name = basename(path);
    if (hasGrammarJson ? name === "grammar.json" : name === "grammar.js") {
      const root = grammarRootFromPath(path);
      const manifestPath = roots.get(root.id)?.manifestPath;
      roots.set(root.id, manifestPath ? { ...root, manifestPath } : root);
    }
  }
  return [...roots.values()];
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

export async function filesWithGrammarJsonUsingEmitter(
  files: DslBundleFile[],
  grammarRootId = preferredGrammarRootId(files),
  emitGrammarJson: (files: DslBundleFile[], grammarPath: string) => Promise<string>,
): Promise<DslBundleFile[]> {
  const root = grammarRootForId(files, grammarRootId);
  const rootFiles = projectedFilesForGrammarRootId(files, grammarRootId).map(({ path, text }) => ({
    path,
    text,
  }));

  const grammarJsonByRootId = new Map<string, string>();
  const activeGrammarJson = await grammarJsonForRoot(
    files,
    grammarRootId,
    grammarJsonByRootId,
    emitGrammarJson,
  );
  const activeFiles = rootFiles.some((file) => file.path === "src/grammar.json")
    ? rootFiles
    : activeGrammarJson
      ? sortedFiles([...rootFiles, { path: "src/grammar.json", text: activeGrammarJson }])
      : rootFiles;

  const embeddedFiles: DslBundleFile[] = [];
  for (const candidate of discoverGrammarRoots(files)) {
    if (candidate.id === (root?.id ?? "")) {
      continue;
    }
    let grammarJson: string | null;
    try {
      grammarJson = await grammarJsonForRoot(
        files,
        candidate.id,
        grammarJsonByRootId,
        emitGrammarJson,
      );
    } catch {
      continue;
    }
    if (!grammarJson) {
      continue;
    }
    const languageName = grammarName(grammarJson);
    if (!languageName) {
      continue;
    }
    const projected = projectedFilesForGrammarRootId(files, candidate.id).map(({ path, text }) => ({
      path,
      text,
    }));
    const projectedWithGrammarJson = projected.some((file) => file.path === "src/grammar.json")
      ? projected
      : [...projected, { path: "src/grammar.json", text: grammarJson }];
    embeddedFiles.push(
      ...projectedWithGrammarJson.map((file) => ({
        path: `languages/${languageName}/${file.path}`,
        text: file.text,
      })),
    );
  }

  return sortedFiles([...activeFiles, ...embeddedFiles]);
}

export function firstSampleForGrammarRootId(
  files: DslBundleFile[],
  rootId = preferredGrammarRootId(files),
): ProjectedDslBundleFile | null {
  return (
    projectedFilesForGrammarRootId(files, rootId).find((file) => file.path.startsWith("samples/")) ??
    null
  );
}

export function sourceExamplesForGrammarRootId(
  files: DslBundleFile[],
  rootId = preferredGrammarRootId(files),
): ProjectedDslBundleFile[] {
  const projected = projectedFilesForGrammarRootId(files, rootId);
  const samples = projected.filter((file) => file.path.startsWith("samples/"));
  const corpusCases = projected
    .filter((file) => file.path.startsWith("test/corpus/") && file.path.endsWith(".txt"))
    .flatMap(corpusCaseExamples);
  return sortedSampleFiles([...samples, ...corpusCases]);
}

export function preferredSampleForGrammarRootId(
  files: DslBundleFile[],
  rootId = preferredGrammarRootId(files),
): ProjectedDslBundleFile | null {
  return sourceExamplesForGrammarRootId(files, rootId)[0] ?? firstSampleForGrammarRootId(files, rootId);
}

export function sortedSampleFiles<T extends DslBundleFile>(files: T[]): T[] {
  return [...files].sort((left, right) => {
    const leftError = isErrorSamplePath(left.path);
    const rightError = isErrorSamplePath(right.path);
    if (leftError !== rightError) {
      return leftError ? 1 : -1;
    }
    return left.path.localeCompare(right.path);
  });
}

export function normalizePath(path: string) {
  let normalized = path.replace(/\\/g, "/");
  while (normalized.startsWith("./")) {
    normalized = normalized.slice(2);
  }
  return normalized;
}

function isErrorSamplePath(path: string) {
  return /(^|[-_/])(errors?|invalid|broken)([-_.\\/]|$)/i.test(path);
}

function corpusCaseExamples(file: ProjectedDslBundleFile): ProjectedDslBundleFile[] {
  const lines = file.text.replace(/\r\n/g, "\n").split("\n");
  const cases: ProjectedDslBundleFile[] = [];
  for (let index = 0; index + 2 < lines.length; index += 1) {
    if (!isCorpusDivider(lines[index]) || !isCorpusDivider(lines[index + 2])) {
      continue;
    }
    const caseName = lines[index + 1]?.trim();
    if (!caseName) {
      continue;
    }
    let inputStart = index + 3;
    if (lines[inputStart] === "") {
      inputStart += 1;
    }
    let inputEnd = inputStart;
    while (inputEnd < lines.length && lines[inputEnd] !== "---") {
      inputEnd += 1;
    }
    if (inputEnd >= lines.length) {
      continue;
    }
    let trimmedInputEnd = inputEnd;
    if (trimmedInputEnd > inputStart && lines[trimmedInputEnd - 1] === "") {
      trimmedInputEnd -= 1;
    }
    cases.push({
      path: `${file.path}#${caseName}`,
      sourcePath: file.sourcePath,
      text: lines.slice(inputStart, trimmedInputEnd).join("\n"),
    });
    index = inputEnd;
  }
  return cases;
}

function isCorpusDivider(line: string) {
  return /^=+$/.test(line);
}

function filesForGrammarRoot(files: DslBundleFile[], root: GrammarRoot | null): ProjectedDslBundleFile[] {
  if (!root || root.id === "") {
    return files.map((file) => ({ ...file, sourcePath: file.path }));
  }

  const prefix = `${root.id}/`;
  const projected = files
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
    });
  if (root.manifestPath && !projected.some((file) => file.sourcePath === root.manifestPath)) {
    const manifest = files.find((file) => file.path === root.manifestPath);
    if (manifest) {
      projected.push({
        path: "tree-sitter.json",
        text: manifest.text,
        sourcePath: manifest.path,
      });
    }
  }
  return projected;
}

async function grammarJsonForRoot(
  files: DslBundleFile[],
  grammarRootId: string,
  cache: Map<string, string>,
  emitGrammarJson: (files: DslBundleFile[], grammarPath: string) => Promise<string>,
): Promise<string | null> {
  const cached = cache.get(grammarRootId);
  if (cached !== undefined) {
    return cached;
  }
  const projected = projectedFilesForGrammarRootId(files, grammarRootId).map(({ path, text }) => ({
    path,
    text,
  }));
  const existing = projected.find((file) => file.path === "src/grammar.json")?.text;
  if (existing !== undefined) {
    cache.set(grammarRootId, existing);
    return existing;
  }
  const root = grammarRootForId(files, grammarRootId);
  const grammarFile = root
    ? files.find((file) => file.path === root.grammarPath)
    : files.find((file) => file.path === "grammar.js");
  if (!grammarFile) {
    return null;
  }
  const grammarJson = await emitGrammarJson(files, grammarFile.path);
  cache.set(grammarRootId, grammarJson);
  return grammarJson;
}

function grammarName(grammarJson: string): string | null {
  let parsed: unknown;
  try {
    parsed = JSON.parse(grammarJson);
  } catch {
    return null;
  }
  const name = typeof parsed === "object" && parsed !== null ? (parsed as { name?: unknown }).name : null;
  return typeof name === "string" && name.length > 0 ? name : null;
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

function grammarRootsFromManifest(
  manifestPath: string,
  text: string,
  filesByPath: Map<string, DslBundleFile>,
  hasGrammarJson: boolean,
): GrammarRoot[] {
  let parsed: unknown;
  try {
    parsed = JSON.parse(text);
  } catch {
    return [];
  }
  const grammars =
    typeof parsed === "object" && parsed !== null && Array.isArray((parsed as { grammars?: unknown }).grammars)
      ? (parsed as { grammars: unknown[] }).grammars
      : [];
  const baseDir = dirname(manifestPath);
  const roots: GrammarRoot[] = [];
  for (const grammar of grammars) {
    if (typeof grammar !== "object" || grammar === null) {
      continue;
    }
    const entry = grammar as { name?: unknown; path?: unknown };
    const grammarBase = normalizePath(joinPath(baseDir, typeof entry.path === "string" ? entry.path : ""));
    const grammarPath = manifestGrammarPath(grammarBase, filesByPath, hasGrammarJson);
    if (!grammarPath) {
      continue;
    }
    const root = grammarRootFromPath(grammarPath);
    roots.push({
      ...root,
      label:
        typeof entry.name === "string" && entry.name.length > 0 && root.id
          ? `${entry.name} (${root.id})`
          : root.label,
      manifestPath,
    });
  }
  return roots;
}

function manifestGrammarPath(
  grammarBase: string,
  filesByPath: Map<string, DslBundleFile>,
  hasGrammarJson: boolean,
): string | null {
  for (const candidate of [joinPath(grammarBase, "src/grammar.json"), joinPath(grammarBase, "grammar.json")]) {
    if (filesByPath.has(candidate)) {
      return candidate;
    }
  }
  const grammarJs = joinPath(grammarBase, "grammar.js");
  if (!hasGrammarJson && filesByPath.has(grammarJs)) {
    return grammarJs;
  }
  return null;
}

function dirname(path: string) {
  const index = path.lastIndexOf("/");
  return index >= 0 ? path.slice(0, index) : "";
}

function joinPath(left: string, right: string) {
  if (!left) {
    return right;
  }
  if (!right || right === ".") {
    return left;
  }
  return `${left}/${right}`;
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
    case "grammar/tree-sitter.json":
      return "tree-sitter.json";
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
  if (relative.startsWith("examples/")) {
    return `samples/${relative.slice("examples/".length)}`;
  }
  if (relative.startsWith("sample.")) {
    return `samples/${relative}`;
  }
  if (relative.startsWith("example.")) {
    return `samples/${relative}`;
  }
  return null;
}

function normalizePackagePath(path: string) {
  if (
    [
      "tree-sitter.json",
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
    "/tree-sitter.json",
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
  if (path.startsWith("examples/")) {
    return `samples/${path.slice("examples/".length)}`;
  }
  for (const token of ["/queries/", "/test/corpus/", "/test/highlight/", "/test/highlights/", "/samples/"]) {
    const index = path.indexOf(token);
    if (index >= 0) {
      return path.slice(index + 1);
    }
  }
  const exampleIndex = path.indexOf("/examples/");
  if (exampleIndex >= 0) {
    return `samples/${path.slice(exampleIndex + "/examples/".length)}`;
  }
  const name = basename(path);
  if (name.startsWith("sample.") || name.startsWith("example.")) {
    return `samples/${name}`;
  }
  return null;
}

function basename(path: string) {
  const index = path.lastIndexOf("/");
  return index >= 0 ? path.slice(index + 1) : path;
}
