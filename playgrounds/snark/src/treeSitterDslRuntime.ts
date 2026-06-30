import type { DslBundleFile } from "./bundlePaths";

export function emitGrammarJsonFromDsl(
  dslSource: string,
  files: DslBundleFile[],
  grammarPath: string,
): string {
  const prelude = officialDslPrelude(dslSource);
  const fileSources = new Map(files.map((file) => [file.path, file.text]));
  const modules = new Map(files.filter((file) => isDslModulePath(file.path)).map((file) => [file.path, file.text]));
  const cache = new Map<string, { exports: unknown }>();
  type SnarkRequire = ((specifier: string) => unknown) & { resolve(specifier: string): string };

  const executeModule = new Function(
    `${prelude}
${snarkDslExtensions()}
return function executeSnarkDslModule(source, module, exports, require, __default, process, __filename, __dirname) {
  eval(source);
  return module.exports;
};`,
  )() as (
    source: string,
    module: { exports: unknown },
    exports: unknown,
    require: SnarkRequire,
    __default: (value: unknown) => unknown,
    process: { env: Record<string, string> },
    __filename: string,
    __dirname: string,
  ) => unknown;

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
    if (resolved.endsWith(".json")) {
      module.exports = JSON.parse(source);
      return module.exports;
    }

    const dirname = resolved.includes("/") ? resolved.slice(0, resolved.lastIndexOf("/")) : "";
    const require = Object.assign(
      (specifier: string) => builtinModule(specifier, fileSources) ?? loadModule(resolveRequire(specifier, dirname, modules)),
      {
        resolve(specifier: string) {
          return resolveRequirePath(specifier, dirname, modules, fileSources);
        },
      },
    );
    const commonJsSource = sourceToCommonJs(source, resolved);

    module.exports = executeModule(
      commonJsSource,
      module,
      module.exports,
      require,
      defaultExportValue,
      { env: {} },
      resolved,
      dirname || ".",
    );
    return module.exports;
  };

  const exported = loadModule(grammarPath);
  const grammarObj = exportedGrammarObject(exported, grammarPath);
  normalizePatternSources(grammarObj);
  return `${JSON.stringify({ "$schema": "https://tree-sitter.github.io/tree-sitter/assets/schemas/grammar.schema.json", ...grammarObj }, null, 2)}\n`;
}

function isDslModulePath(path: string) {
  return path.endsWith(".js") || path.endsWith(".mjs") || path.endsWith(".json");
}

function sourceToCommonJs(source: string, path: string) {
  let out = source;
  let moduleIndex = 0;
  out = out.replace(
    /(^|\n)[ \t]*import\s+([A-Za-z_$][\w$]*)\s*,\s*\{([\s\S]*?)\}\s+from\s+['"]([^'"]+)['"][ \t]*;?/g,
    (_match, prefix, defaultName, names, specifier) => {
      const moduleName = `__snark_module_${moduleIndex++}`;
      return `${prefix}const ${moduleName} = require(${JSON.stringify(specifier)});const ${defaultName} = __default(${moduleName});const { ${namedImportBindings(names)} } = ${moduleName};`;
    },
  );
  out = out.replace(
    /(^|\n)[ \t]*import\s+\*\s+as\s+([A-Za-z_$][\w$]*)\s+from\s+['"]([^'"]+)['"][ \t]*;?/g,
    (_match, prefix, name, specifier) => `${prefix}const ${name} = require(${JSON.stringify(specifier)});`,
  );
  out = out.replace(
    /(^|\n)[ \t]*import\s+\{([\s\S]*?)\}\s+from\s+['"]([^'"]+)['"][ \t]*;?/g,
    (_match, prefix, names, specifier) =>
      `${prefix}const { ${namedImportBindings(names)} } = require(${JSON.stringify(specifier)});`,
  );
  out = out.replace(
    /(^|\n)[ \t]*import\s+([A-Za-z_$][\w$]*)\s+from\s+['"]([^'"]+)['"][ \t]*;?/g,
    (_match, prefix, name, specifier) => `${prefix}const ${name} = __default(require(${JSON.stringify(specifier)}));`,
  );
  out = out.replace(
    /(^|\n)\s*export\s+(const|let|var)\s+([A-Za-z_$][\w$]*)\s*=/g,
    (_match, prefix, kind, name) => `${prefix}${kind} ${name} = exports.${name} =`,
  );
  out = out.replace(
    /(^|\n)\s*export\s+function\s+([A-Za-z_$][\w$]*)\s*\(/g,
    (_match, prefix, name) => `${prefix}exports.${name} = function ${name}(`,
  );
  out = out.replace(
    /(^|\n)[ \t]*export\s+\{([\s\S]*?)\}\s+from\s+['"]([^'"]+)['"][ \t]*;?/g,
    (_match, prefix, names, specifier) => {
      const moduleName = `__snark_module_${moduleIndex++}`;
      return `${prefix}const ${moduleName} = require(${JSON.stringify(specifier)});${exportBindingsFromModule(
        names,
        moduleName,
      )}`;
    },
  );
  out = out.replace(
    /(^|\n)[ \t]*export\s+\*\s+from\s+['"]([^'"]+)['"][ \t]*;?/g,
    (_match, prefix, specifier) => {
      const moduleName = `__snark_module_${moduleIndex++}`;
      return `${prefix}const ${moduleName} = require(${JSON.stringify(specifier)});for (const key of Object.keys(${moduleName})) { if (key !== "default") exports[key] = ${moduleName}[key]; }`;
    },
  );
  out = out.replace(
    /(^|\n)[ \t]*export\s+\{([\s\S]*?)\}[ \t]*;?/g,
    (_match, prefix, names) => `${prefix}${exportLocalBindings(names)}`,
  );
  out = out.replace(/(^|\n)\s*export\s+default\s+/m, "$1module.exports.default = ");
  if (/^\s*(import|export)\s/m.test(out)) {
    throw new Error(`${path} uses unsupported ESM syntax`);
  }
  return out;
}

function namedImportBindings(names: string) {
  return names
    .split(",")
    .map((name) => name.trim())
    .filter(Boolean)
    .map((name) => {
      const alias = /^([A-Za-z_$][\w$]*)\s+as\s+([A-Za-z_$][\w$]*)$/.exec(name);
      return alias ? `${alias[1]}: ${alias[2]}` : name;
    })
    .join(", ");
}

function exportLocalBindings(names: string) {
  return exportSpecifiers(names)
    .map(({ imported, exported }) => `exports.${exported} = ${imported};`)
    .join("");
}

function exportBindingsFromModule(names: string, moduleName: string) {
  return exportSpecifiers(names)
    .map(({ imported, exported }) => {
      const value = imported === "default" ? `__default(${moduleName})` : `${moduleName}.${imported}`;
      return `exports.${exported} = ${value};`;
    })
    .join("");
}

function exportSpecifiers(names: string) {
  return names
    .split(",")
    .map((name) => name.trim())
    .filter(Boolean)
    .map((name) => {
      const alias = /^([A-Za-z_$][\w$]*|default)\s+as\s+([A-Za-z_$][\w$]*)$/.exec(name);
      return alias ? { imported: alias[1], exported: alias[2] } : { imported: name, exported: name };
    });
}

function defaultExportValue(value: unknown) {
  if (value && typeof value === "object" && "default" in value) {
    return (value as { default: unknown }).default;
  }
  return value;
}

function builtinModule(specifier: string, fileSources: Map<string, string>) {
  switch (specifier) {
    case "fs":
    case "node:fs":
      return {
        readFileSync(path: string) {
          const resolved = resolveUploadedFilePath(normalizePathForNodeBuiltin(path), fileSources);
          return fileSources.get(resolved) ?? "";
        },
        existsSync(path: string) {
          return uploadedPathExists(normalizePathForNodeBuiltin(path), fileSources);
        },
        readdirSync(path: string) {
          return uploadedDirectoryEntries(normalizePathForNodeBuiltin(path), fileSources);
        },
        statSync(path: string) {
          return uploadedPathStats(normalizePathForNodeBuiltin(path), fileSources);
        },
        writeFileSync() {},
        appendFileSync() {},
      };
    case "path":
    case "node:path":
      return {
        basename(path: string) {
          return basenamePath(path);
        },
        dirname(path: string) {
          return dirnamePath(path);
        },
        extname(path: string) {
          return extnamePath(path);
        },
        join(...parts: string[]) {
          return normalizePathForNodeBuiltin(parts.join("/"));
        },
        resolve(...parts: string[]) {
          return normalizePathForNodeBuiltin(parts.join("/"));
        },
      };
    default:
      return null;
  }
}

function normalizePathForNodeBuiltin(path: string) {
  const out = [];
  for (const part of path.replace(/\\/g, "/").split("/")) {
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

function basenamePath(path: string) {
  const normalized = normalizePathForNodeBuiltin(path);
  const index = normalized.lastIndexOf("/");
  return index >= 0 ? normalized.slice(index + 1) : normalized;
}

function dirnamePath(path: string) {
  const normalized = normalizePathForNodeBuiltin(path);
  const index = normalized.lastIndexOf("/");
  return index >= 0 ? normalized.slice(0, index) : ".";
}

function extnamePath(path: string) {
  const basename = basenamePath(path);
  const index = basename.lastIndexOf(".");
  return index > 0 ? basename.slice(index) : "";
}

function uploadedPathExists(path: string, fileSources: Map<string, string>) {
  return fileSources.has(path) || uploadedDirectoryEntries(path, fileSources).length > 0;
}

function uploadedDirectoryEntries(path: string, fileSources: Map<string, string>) {
  const prefix = path ? `${path}/` : "";
  const entries = new Set<string>();
  for (const filePath of fileSources.keys()) {
    if (path && !filePath.startsWith(prefix)) {
      continue;
    }
    const rest = path ? filePath.slice(prefix.length) : filePath;
    if (!rest) {
      continue;
    }
    entries.add(rest.split("/", 1)[0] ?? rest);
  }
  return [...entries].sort();
}

function uploadedPathStats(path: string, fileSources: Map<string, string>) {
  if (!uploadedPathExists(path, fileSources)) {
    throw new Error(`could not stat uploaded path ${path}`);
  }
  return {
    isFile() {
      return fileSources.has(path);
    },
    isDirectory() {
      return !fileSources.has(path) && uploadedDirectoryEntries(path, fileSources).length > 0;
    },
  };
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

function snarkDslExtensions() {
  return `
globalThis.until = function until(...markers) {
  return { type: "UNTIL", markers: markers.flat() };
};

globalThis.nested = function nested(open, close) {
  return { type: "NESTED", open, close };
};

globalThis.auto_close = function auto_close(options) {
  return {
    type: "AUTO_CLOSE",
    tag: options.tag,
    open: options.open,
    close: options.close,
    closed_by: options.closed_by,
    open_node: options.open_node,
    close_node: options.close_node,
    tag_name_node: options.tag_name_node,
    start_prefix: options.start_prefix,
    end_prefix: options.end_prefix,
    closed_by_tags: options.closed_by_tags,
    rules: options.rules,
  };
};
`;
}

function resolveRequire(specifier: string, dirname: string, modules: Map<string, string>) {
  if (specifier.startsWith("./") || specifier.startsWith("../")) {
    const path = normalizeRelativePath(dirname, specifier);
    const relocated =
      (dirname === "grammar" || dirname.endsWith("/grammar")) && specifier.startsWith("../")
        ? normalizeRelativePath(dirname, specifier.slice(3))
        : null;
    return resolveJsPath(path, modules, relocated ? [relocated] : []);
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

function resolveRequirePath(
  specifier: string,
  dirname: string,
  modules: Map<string, string>,
  fileSources: Map<string, string>,
) {
  if (isBuiltinModule(specifier)) {
    return specifier.startsWith("node:") ? specifier.slice("node:".length) : specifier;
  }

  if (specifier.startsWith("./") || specifier.startsWith("../")) {
    const path = normalizeRelativePath(dirname, specifier);
    const relocated =
      (dirname === "grammar" || dirname.endsWith("/grammar")) && specifier.startsWith("../")
        ? normalizeRelativePath(dirname, specifier.slice(3))
        : null;
    for (const candidate of relocated ? [path, relocated] : [path]) {
      if (fileSources.has(candidate)) {
        return candidate;
      }
    }
    return resolveJsPath(path, modules, relocated ? [relocated] : []);
  }

  return resolveRequire(specifier, dirname, modules);
}

function isBuiltinModule(specifier: string) {
  return specifier === "fs" || specifier === "node:fs" || specifier === "path" || specifier === "node:path";
}

function resolveJsPath(path: string, modules: Map<string, string>, fallbacks: string[] = []) {
  for (const candidatePath of [path, ...fallbacks]) {
    for (const candidate of [
      candidatePath,
      `${candidatePath}.js`,
      `${candidatePath}.mjs`,
      `${candidatePath}.json`,
      `${candidatePath}/index.js`,
      `${candidatePath}/index.mjs`,
      `${candidatePath}/index.json`,
      `${candidatePath}/grammar.js`,
      `${candidatePath}/grammar.mjs`,
    ]) {
      if (modules.has(candidate)) {
        return candidate;
      }
    }
  }
  throw new Error(`could not resolve JavaScript module ${path}`);
}

function resolveUploadedFilePath(path: string, fileSources: Map<string, string>) {
  if (fileSources.has(path)) {
    return path;
  }
  throw new Error(`could not resolve uploaded file ${path}`);
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
