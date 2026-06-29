#!/usr/bin/env node
import { builtinModules, createRequire } from "node:module";
import fs from "node:fs";
import path from "node:path";
import vm from "node:vm";

const moduleCache = new Map();
const arboriumGrammarCache = new Map();

function main() {
  const { grammarPath, outPath } = parseArgs(process.argv.slice(2));
  const resolvedGrammarPath = path.resolve(grammarPath);
  const exported = loadGrammarModule(resolvedGrammarPath);
  const grammarJson = exported?.grammar ?? exported;

  if (!grammarJson || typeof grammarJson !== "object" || typeof grammarJson.name !== "string") {
    throw new Error(`${resolvedGrammarPath} did not export a Tree-sitter grammar object`);
  }

  const output = `${JSON.stringify(grammarJson, null, 2)}\n`;
  if (outPath) {
    const resolvedOutPath = path.resolve(outPath);
    fs.mkdirSync(path.dirname(resolvedOutPath), { recursive: true });
    fs.writeFileSync(resolvedOutPath, output);
  } else {
    process.stdout.write(output);
  }
}

function parseArgs(args) {
  let grammarPath = null;
  let outPath = null;

  for (let index = 0; index < args.length; index += 1) {
    const arg = args[index];
    switch (arg) {
      case "-h":
      case "--help":
        usage(0);
        break;
      case "-o":
      case "--out":
        index += 1;
        outPath = args[index] ?? null;
        if (!outPath) {
          throw new Error(`${arg} needs a path`);
        }
        break;
      default:
        if (arg.startsWith("-")) {
          throw new Error(`unknown option ${arg}`);
        }
        if (grammarPath) {
          throw new Error(`unexpected extra argument ${arg}`);
        }
        grammarPath = arg;
    }
  }

  if (!grammarPath) {
    usage(1);
  }

  return { grammarPath, outPath };
}

function usage(code) {
  const stream = code === 0 ? process.stdout : process.stderr;
  stream.write(
    [
      "usage: grammar-js-to-json <grammar.js> [--out src/grammar.json]",
      "",
      "Executes authored Tree-sitter grammar DSL and emits declarative grammar JSON.",
      "When run inside an Arborium checkout, tree-sitter-* grammar dependencies",
      "are resolved from sibling authored grammar.js sources when possible.",
      "Generated parser artifacts such as parser.c and node-types.json are not read.",
      "",
    ].join("\n"),
  );
  process.exit(code);
}

function loadGrammarModule(filename) {
  const resolved = resolveJsPath(filename);
  if (moduleCache.has(resolved)) {
    return moduleCache.get(resolved).exports;
  }

  const dirname = path.dirname(resolved);
  const module = { exports: {} };
  moduleCache.set(resolved, module);

  const context = {
    __dirname: dirname,
    __filename: resolved,
    alias,
    blank,
    choice,
    console,
    exports: module.exports,
    field,
    grammar,
    module,
    optional,
    prec,
    repeat,
    repeat1,
    require: makeRequire(resolved),
    reserved,
    RustRegex,
    seq,
    token,
  };
  context.global = context;
  context.globalThis = context;

  const source = fs.readFileSync(resolved, "utf8");
  const commonJsSource = source.replace(/(^|\n)\s*export\s+default\s+/m, "$1module.exports = ");
  if (/^\s*import\s/m.test(commonJsSource)) {
    throw new Error(
      `${resolved} uses ESM imports; this converter currently supports CommonJS grammar.js files and simple export default grammars`,
    );
  }

  vm.runInNewContext(commonJsSource, context, {
    filename: resolved,
    displayErrors: true,
  });

  return module.exports;
}

function makeRequire(parentFilename) {
  const nodeRequire = createRequire(parentFilename);
  return function localRequire(specifier) {
    if (builtinModules.includes(specifier) || specifier.startsWith("node:")) {
      return nodeRequire(specifier);
    }

    let resolved;
    try {
      resolved = nodeRequire.resolve(specifier);
    } catch (error) {
      resolved = resolveArboriumGrammarDependency(specifier, parentFilename);
      if (!resolved) {
        throw error;
      }
    }
    if (resolved.endsWith(".json")) {
      return JSON.parse(fs.readFileSync(resolved, "utf8"));
    }
    if (resolved.endsWith(".js") || resolved.endsWith(".cjs")) {
      return loadGrammarModule(resolved);
    }
    if (resolved.endsWith(".mjs")) {
      throw new Error(`cannot load ESM grammar dependency ${resolved}`);
    }
    return nodeRequire(specifier);
  };
}

function resolveArboriumGrammarDependency(specifier, parentFilename) {
  const match = /^tree-sitter-([^/]+)\/grammar(?:\.js)?$/.exec(specifier);
  if (!match) {
    return null;
  }

  const arboriumRoot = findArboriumRoot(path.dirname(parentFilename));
  if (!arboriumRoot) {
    return null;
  }

  const grammarId = match[1];
  const cacheKey = `${arboriumRoot}\0${grammarId}`;
  if (arboriumGrammarCache.has(cacheKey)) {
    return arboriumGrammarCache.get(cacheKey);
  }

  const resolved = findArboriumGrammarById(path.join(arboriumRoot, "langs"), grammarId);
  arboriumGrammarCache.set(cacheKey, resolved);
  return resolved;
}

function findArboriumRoot(startDir) {
  let current = startDir;
  while (true) {
    if (fs.existsSync(path.join(current, "langs")) && fs.statSync(path.join(current, "langs")).isDirectory()) {
      return current;
    }
    const parent = path.dirname(current);
    if (parent === current) {
      return null;
    }
    current = parent;
  }
}

function findArboriumGrammarById(langsDir, grammarId) {
  if (!fs.existsSync(langsDir)) {
    return null;
  }

  const stack = [langsDir];
  while (stack.length > 0) {
    const dir = stack.pop();
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      const child = path.join(dir, entry.name);
      if (!entry.isDirectory()) {
        continue;
      }

      const grammarPath = path.join(child, "def", "grammar", "grammar.js");
      if (entry.name === grammarId && isFile(grammarPath)) {
        return grammarPath;
      }
      stack.push(child);
    }
  }
  return null;
}

function resolveJsPath(filename) {
  if (fs.existsSync(filename) && fs.statSync(filename).isFile()) {
    return filename;
  }
  if (fs.existsSync(`${filename}.js`)) {
    return `${filename}.js`;
  }
  const indexPath = path.join(filename, "index.js");
  if (fs.existsSync(indexPath)) {
    return indexPath;
  }
  throw new Error(`could not resolve JavaScript module ${filename}`);
}

function isFile(input) {
  try {
    return fs.statSync(input).isFile();
  } catch {
    return false;
  }
}

function alias(rule, value) {
  const result = {
    type: "ALIAS",
    content: normalize(rule),
    named: false,
    value: null,
  };

  if (typeof value === "string") {
    result.value = value;
    return result;
  }
  const symbol = symbolFrom(value);
  if (symbol) {
    result.named = true;
    result.value = symbol.name;
    return result;
  }

  throw new Error(`Invalid alias value ${String(value)}`);
}

function blank() {
  return { type: "BLANK" };
}

function field(name, rule) {
  if (typeof name !== "string" || !/^[A-Za-z_][A-Za-z0-9_]*$/.test(name)) {
    throw new Error(`Invalid field name ${String(name)}`);
  }
  return {
    type: "FIELD",
    name,
    content: normalize(rule),
  };
}

function choice(...members) {
  return {
    type: "CHOICE",
    members: members.map(normalize),
  };
}

function optional(rule) {
  return choice(rule, blank());
}

function prec(value, rule) {
  checkPrecedence(value);
  return {
    type: "PREC",
    value,
    content: normalize(rule),
  };
}

prec.left = function precLeft(value, rule) {
  if (rule == null) {
    rule = value;
    value = 0;
  }
  checkPrecedence(value);
  return {
    type: "PREC_LEFT",
    value,
    content: normalize(rule),
  };
};

prec.right = function precRight(value, rule) {
  if (rule == null) {
    rule = value;
    value = 0;
  }
  checkPrecedence(value);
  return {
    type: "PREC_RIGHT",
    value,
    content: normalize(rule),
  };
};

prec.dynamic = function precDynamic(value, rule) {
  checkPrecedence(value);
  return {
    type: "PREC_DYNAMIC",
    value,
    content: normalize(rule),
  };
};

function repeat(rule) {
  return {
    type: "REPEAT",
    content: normalize(rule),
  };
}

function repeat1(rule) {
  return {
    type: "REPEAT1",
    content: normalize(rule),
  };
}

function seq(...members) {
  return {
    type: "SEQ",
    members: members.map(normalize),
  };
}

function reserved(contextName, rule) {
  if (typeof contextName !== "string") {
    throw new Error(`Invalid reserved-word context ${String(contextName)}`);
  }
  return {
    type: "RESERVED",
    context_name: contextName,
    content: normalize(rule),
  };
}

function token(rule) {
  return {
    type: "TOKEN",
    content: normalize(rule),
  };
}

token.immediate = function immediateToken(rule) {
  return {
    type: "IMMEDIATE_TOKEN",
    content: normalize(rule),
  };
};

class GrammarSymbol {
  constructor(name) {
    this.type = "SYMBOL";
    this.name = String(name);
  }
}

class RustRegex {
  constructor(value) {
    this.value = String(value);
  }
}

function grammar(baseGrammarOrOptions, maybeOptions) {
  let inherits;
  let baseGrammar;
  let options;

  if (maybeOptions) {
    options = maybeOptions;
    baseGrammar = baseGrammarOrOptions?.grammar ?? baseGrammarOrOptions;
    inherits = baseGrammar?.name;
    if (!baseGrammar || typeof baseGrammar !== "object") {
      throw new Error("base grammar must be a grammar object");
    }
  } else {
    options = baseGrammarOrOptions;
    baseGrammar = {
      name: null,
      rules: {},
      extras: [normalize(/\s/)],
      conflicts: [],
      externals: [],
      inline: [],
      supertypes: [],
      precedences: [],
      reserved: {},
    };
  }

  validateGrammarName(options.name);

  let externals = baseGrammar.externals ?? [];
  if (options.externals) {
    const builder = RuleBuilder(null);
    const externalRules = options.externals.call(builder, builder, baseGrammar.externals ?? []);
    if (!Array.isArray(externalRules)) {
      throw new Error("Grammar externals must be an array");
    }
    externals = externalRules.map(normalize);
  }

  const ruleMap = {};
  for (const name of Object.keys(options.rules ?? {})) {
    ruleMap[name] = true;
  }
  for (const name of Object.keys(baseGrammar.rules ?? {})) {
    ruleMap[name] = true;
  }
  for (const external of externals) {
    if (typeof external.name === "string") {
      ruleMap[external.name] = true;
    }
  }
  const builder = RuleBuilder(ruleMap);

  const rules = { ...(baseGrammar.rules ?? {}) };
  for (const [ruleName, ruleFn] of Object.entries(options.rules ?? {})) {
    if (typeof ruleFn !== "function") {
      throw new Error(`Grammar rule ${ruleName} is not a function`);
    }
    const rule = ruleFn.call(builder, builder, rules[ruleName]);
    if (rule === undefined) {
      throw new Error(`Rule ${ruleName} returned undefined`);
    }
    rules[ruleName] = normalize(rule);
  }

  let extras = [...(baseGrammar.extras ?? [])];
  if (options.extras) {
    extras = options.extras.call(builder, builder, baseGrammar.extras ?? []);
    if (!Array.isArray(extras)) {
      throw new Error("Grammar extras must be an array");
    }
    extras = extras.map(normalize);
  }

  let word = baseGrammar.word;
  if (options.word) {
    const wordSymbol = symbolFrom(options.word.call(builder, builder));
    if (!wordSymbol) {
      throw new Error("Grammar word must be a symbol");
    }
    word = wordSymbol.name;
  }

  let conflicts = baseGrammar.conflicts ?? [];
  if (options.conflicts) {
    const baseConflicts = conflicts.map((conflict) => conflict.map((name) => new GrammarSymbol(name)));
    const conflictRules = options.conflicts.call(builder, builder, baseConflicts);
    if (!Array.isArray(conflictRules)) {
      throw new Error("Grammar conflicts must be an array");
    }
    conflicts = conflictRules.map((conflictSet) => {
      if (!Array.isArray(conflictSet)) {
        throw new Error("Grammar conflicts must be an array of arrays");
      }
      return conflictSet.map((symbol) => {
        const normalized = normalize(symbol);
        if (normalized.type !== "SYMBOL") {
          throw new Error("Grammar conflict entries must be symbols");
        }
        return normalized.name;
      });
    });
  }

  let inline = baseGrammar.inline ?? [];
  if (options.inline) {
    const baseInline = inline.map((name) => new GrammarSymbol(name));
    const inlineRules = options.inline.call(builder, builder, baseInline);
    if (!Array.isArray(inlineRules)) {
      throw new Error("Grammar inline must be an array");
    }
    inline = uniqueSymbolNames(inlineRules);
  }

  let supertypes = baseGrammar.supertypes ?? [];
  if (options.supertypes) {
    const baseSupertypes = supertypes.map((name) => new GrammarSymbol(name));
    const supertypeRules = options.supertypes.call(builder, builder, baseSupertypes);
    if (!Array.isArray(supertypeRules)) {
      throw new Error("Grammar supertypes must be an array");
    }
    supertypes = uniqueSymbolNames(supertypeRules);
  }

  let precedences = baseGrammar.precedences ?? [];
  if (options.precedences) {
    precedences = options.precedences.call(builder, builder, baseGrammar.precedences ?? []);
    if (!Array.isArray(precedences)) {
      throw new Error("Grammar precedences must be an array");
    }
    precedences = precedences.map((list) => {
      if (!Array.isArray(list)) {
        throw new Error("Grammar precedences must be an array of arrays");
      }
      return list.map(normalizePrecedenceEntry);
    });
  }

  const reservedSets = { ...(baseGrammar.reserved ?? {}) };
  if (options.reserved) {
    for (const [setName, setFn] of Object.entries(options.reserved)) {
      if (typeof setFn !== "function") {
        throw new Error(`Reserved-word set ${setName} is not a function`);
      }
      const tokens = setFn.call(builder, builder, reservedSets[setName] ?? []);
      if (!Array.isArray(tokens)) {
        throw new Error(`Reserved-word set ${setName} must return an array`);
      }
      reservedSets[setName] = tokens.map(normalize);
    }
  }

  return {
    grammar: {
      name: options.name,
      inherits,
      word,
      rules,
      extras,
      conflicts,
      precedences,
      externals,
      inline,
      supertypes,
      reserved: reservedSets,
    },
  };
}

function RuleBuilder(ruleMap) {
  return new Proxy(
    {},
    {
      get(_, propertyName) {
        const name = String(propertyName);
        const symbol = new GrammarSymbol(name);
        if (!ruleMap || Object.prototype.hasOwnProperty.call(ruleMap, name)) {
          return symbol;
        }
        const error = new ReferenceError(`Undefined symbol ${name}`);
        error.symbol = symbol;
        return error;
      },
    },
  );
}

function normalize(value) {
  if (value === undefined) {
    throw new Error("Undefined symbol");
  }
  if (typeof value === "string") {
    return { type: "STRING", value };
  }
  if (isRegExp(value)) {
    const rule = { type: "PATTERN", value: value.source };
    if (value.flags) {
      rule.flags = value.flags;
    }
    return rule;
  }
  if (value instanceof RustRegex) {
    return { type: "PATTERN", value: value.value };
  }
  if (value instanceof ReferenceError && value.symbol) {
    throw value;
  }
  if (value && typeof value.type === "string") {
    return value;
  }
  throw new TypeError(`Invalid rule: ${String(value)}`);
}

function normalizePrecedenceEntry(value) {
  if (typeof value === "string") {
    return value;
  }
  const symbol = symbolFrom(value);
  if (symbol) {
    return {
      type: "SYMBOL",
      name: symbol.name,
    };
  }
  throw new TypeError(`Invalid precedence entry: ${String(value)}`);
}

function symbolFrom(value) {
  if (value instanceof ReferenceError && value.symbol) {
    return value.symbol;
  }
  if (value instanceof GrammarSymbol) {
    return value;
  }
  if (value && value.type === "SYMBOL" && typeof value.name === "string") {
    return value;
  }
  return null;
}

function uniqueSymbolNames(symbols) {
  const names = [];
  const seen = new Set();
  for (const symbol of symbols) {
    const normalized = symbolFrom(symbol);
    if (!normalized) {
      throw new Error("Expected symbol");
    }
    if (!seen.has(normalized.name)) {
      names.push(normalized.name);
      seen.add(normalized.name);
    }
  }
  return names;
}

function validateGrammarName(name) {
  if (typeof name !== "string" || !/^[A-Za-z_]\w*$/.test(name)) {
    throw new Error("Grammar name must be a valid identifier");
  }
}

function checkPrecedence(value) {
  if (typeof value !== "number" && typeof value !== "string") {
    throw new Error(`Invalid precedence ${String(value)}`);
  }
}

function isRegExp(value) {
  return Object.prototype.toString.call(value) === "[object RegExp]";
}

try {
  main();
} catch (error) {
  console.error(error instanceof Error ? error.message : String(error));
  process.exit(1);
}
