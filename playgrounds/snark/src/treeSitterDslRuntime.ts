import type { DslBundleFile } from "./bundlePaths";

export function emitGrammarJsonFromDsl(
  dslSource: string,
  files: DslBundleFile[],
  grammarPath: string,
): string {
  const prelude = officialDslPrelude(dslSource);
  const modules = new Map(files.filter((file) => file.path.endsWith(".js")).map((file) => [file.path, file.text]));
  const cache = new Map<string, { exports: unknown }>();

  const loadModule = (path: string): unknown => {
    const resolved = resolveJsPath(path, modules);
    const cached = cache.get(resolved);
    if (cached) {
      return cached.exports;
    }
    const source = modules.get(resolved);
    if (source == null) {
      throw new Error(`missing grammar module ${resolved}`);
    }

    const module = { exports: {} as unknown };
    cache.set(resolved, module);
    const dirname = resolved.includes("/") ? resolved.slice(0, resolved.lastIndexOf("/")) : "";
    const require = (specifier: string) => loadModule(resolveRequire(specifier, dirname, modules));
    const commonJsSource = source.replace(/(^|\n)\s*export\s+default\s+/m, "$1module.exports = ");
    if (/^\s*import\s/m.test(commonJsSource)) {
      throw new Error(`${resolved} uses ESM imports; upload a CommonJS grammar.js bundle or pre-bundle dependencies first`);
    }

    const fn = new Function(
      "module",
      "exports",
      "require",
      `${prelude}\n${commonJsSource}\n; return module.exports;`,
    );
    module.exports = fn(module, module.exports, require);
    return module.exports;
  };

  const exported = loadModule(grammarPath);
  const grammarObj = exportedGrammarObject(exported, grammarPath);
  normalizePatternSources(grammarObj);
  return `${JSON.stringify({ "$schema": "https://tree-sitter.github.io/tree-sitter/assets/schemas/grammar.schema.json", ...grammarObj }, null, 2)}\n`;
}

function exportedGrammarObject(exported: unknown, grammarPath: string): Record<string, unknown> {
  if (!exported || typeof exported !== "object") {
    throw new Error(`${grammarPath} did not export a Tree-sitter grammar object`);
  }
  const record = exported as Record<string, unknown>;
  const defaultExport = record.default;
  const grammar =
    grammarValue(record.grammar) ??
    (defaultExport && typeof defaultExport === "object"
      ? grammarValue((defaultExport as Record<string, unknown>).grammar)
      : null) ??
    grammarValue(defaultExport) ??
    grammarValue(record);
  if (!grammar) {
    throw new Error(`${grammarPath} did not export a Tree-sitter grammar object`);
  }
  return grammar;
}

function grammarValue(value: unknown): Record<string, unknown> | null {
  if (!value || typeof value !== "object") {
    return null;
  }
  const record = value as Record<string, unknown>;
  return typeof record.name === "string" ? record : null;
}

function officialDslPrelude(dslSource: string) {
  const marker = 'const grammarPath = getEnv("TREE_SITTER_GRAMMAR_PATH");';
  const index = dslSource.indexOf(marker);
  if (index < 0) {
    throw new Error("official Tree-sitter DSL entrypoint marker was not found");
  }
  return dslSource.slice(0, index);
}

function resolveRequire(specifier: string, dirname: string, modules: Map<string, string>) {
  if (specifier.startsWith("./") || specifier.startsWith("../")) {
    return resolveJsPath(normalizeRelativePath(dirname, specifier), modules);
  }

  const grammarMatch = /^tree-sitter-([^/]+)\/grammar(?:\.js)?$/.exec(specifier);
  if (grammarMatch) {
    const grammarId = grammarMatch[1];
    for (const candidate of [
      `node_modules/tree-sitter-${grammarId}/grammar.js`,
      `tree-sitter-${grammarId}/grammar.js`,
      `langs/${grammarId}/def/grammar/grammar.js`,
    ]) {
      if (modules.has(candidate)) {
        return candidate;
      }
    }
    for (const key of modules.keys()) {
      if (
        key.endsWith(`/node_modules/tree-sitter-${grammarId}/grammar.js`) ||
        key.endsWith(`/tree-sitter-${grammarId}/grammar.js`) ||
        key.endsWith(`/${grammarId}/def/grammar/grammar.js`)
      ) {
        return key;
      }
    }
  }

  throw new Error(`cannot resolve grammar dependency ${specifier}`);
}

function resolveJsPath(path: string, modules: Map<string, string>) {
  for (const candidate of [path, `${path}.js`, `${path}/index.js`, `${path}/grammar.js`]) {
    if (modules.has(candidate)) {
      return candidate;
    }
  }
  throw new Error(`could not resolve JavaScript module ${path}`);
}

function normalizeRelativePath(dirname: string, specifier: string) {
  const parts = (dirname ? dirname.split("/") : []).concat(specifier.split("/"));
  const out = [];
  for (const part of parts) {
    if (!part || part === ".") {
      continue;
    }
    if (part === "..") {
      out.pop();
    } else {
      out.push(part);
    }
  }
  return out.join("/");
}

function normalizePatternSources(root: unknown) {
  const stack = [root];
  while (stack.length > 0) {
    const value = stack.pop();
    if (!value || typeof value !== "object") {
      continue;
    }

    const record = value as Record<string, unknown>;
    if (record.type === "PATTERN" && typeof record.value === "string") {
      record.value = normalizePatternSourceLikeTreeSitter(record.value);
    }

    for (const key of Object.keys(record)) {
      stack.push(record[key]);
    }
  }
}

function normalizePatternSourceLikeTreeSitter(source: string) {
  let out = "";
  let escaped = false;
  let inCharacterClass = false;

  for (const ch of source) {
    if (escaped) {
      out += inCharacterClass && ch === "/" ? "/" : `\\${ch}`;
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

  if (escaped) {
    out += "\\";
  }
  return out;
}
