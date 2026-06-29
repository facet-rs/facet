#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const GENERATED_BASENAMES = new Set(["parser.c", "parser.cc", "parser.h", "node-types.json"]);
const GENERATED_RELATIVE_PATHS = new Set(["bindings/node/binding.cc"]);

function main() {
  const { inputPath, outDir } = parseArgs(process.argv.slice(2));
  const input = path.resolve(inputPath);
  const out = path.resolve(outDir);
  const source = detectSource(input);

  ensureUsableOutputDir(out);
  fs.mkdirSync(path.join(out, "src"), { recursive: true });

  const copied = [];
  if (source.kind === "declarative-package") {
    copyFileChecked(path.join(source.root, "src", "grammar.json"), path.join(out, "src", "grammar.json"), copied);
    copyTreeIfPresent(path.join(source.root, "queries"), path.join(out, "queries"), copied);
    copyTreeIfPresent(path.join(source.root, "test", "corpus"), path.join(out, "test", "corpus"), copied);
    copyTreeIfPresent(path.join(source.root, "test", "highlight"), path.join(out, "test", "highlight"), copied);
    copyTreeIfPresent(path.join(source.root, "test", "highlights"), path.join(out, "test", "highlights"), copied);
    copyScannerSources(path.join(source.root, "src"), path.join(out, "src"), copied);
  } else {
    convertGrammarJs(source.grammarPath, path.join(out, "src", "grammar.json"));
    copied.push("src/grammar.json");
    copyTreeIfPresent(path.join(source.defRoot, "queries"), path.join(out, "queries"), copied);
    copyTreeIfPresent(path.join(source.defRoot, "test", "corpus"), path.join(out, "test", "corpus"), copied);
    copyTreeIfPresent(path.join(source.defRoot, "test", "highlight"), path.join(out, "test", "highlight"), copied);
    copyTreeIfPresent(path.join(source.defRoot, "test", "highlights"), path.join(out, "test", "highlights"), copied);
    copyScannerSources(path.join(source.defRoot, "grammar"), path.join(out, "src"), copied);
    copyArboriumSamples(source.defRoot, out, copied);
  }

  const summary = {
    source: source.kind,
    input,
    out,
    files: copied.sort(),
  };
  process.stdout.write(`${JSON.stringify(summary, null, 2)}\n`);
}

function parseArgs(args) {
  let inputPath = null;
  let outDir = null;

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
        outDir = args[index] ?? null;
        if (!outDir) {
          throw new Error(`${arg} needs a path`);
        }
        break;
      default:
        if (arg.startsWith("-")) {
          throw new Error(`unknown option ${arg}`);
        }
        if (inputPath) {
          throw new Error(`unexpected extra argument ${arg}`);
        }
        inputPath = arg;
    }
  }

  if (!inputPath || !outDir) {
    usage(1);
  }

  return { inputPath, outDir };
}

function usage(code) {
  const stream = code === 0 ? process.stdout : process.stderr;
  stream.write(
    [
      "usage: prepare-bundle <tree-sitter-package-or-arborium-lang> --out <dir>",
      "",
      "Writes a playground-loadable bundle rooted at <dir>.",
      "The output contains src/grammar.json plus allowed query, corpus, sample,",
      "and handwritten scanner files when present. Generated parser artifacts",
      "and node metadata are not copied.",
      "",
    ].join("\n"),
  );
  process.exit(code);
}

function detectSource(input) {
  const stat = mustStat(input);
  if (stat.isFile()) {
    if (path.basename(input) === "grammar.js") {
      return { kind: "arborium-source", grammarPath: input, defRoot: inferDefRootFromGrammarJs(input) };
    }
    if (path.basename(input) === "grammar.json" && path.basename(path.dirname(input)) === "src") {
      return { kind: "declarative-package", root: path.dirname(path.dirname(input)) };
    }
    throw new Error(`unsupported input file ${input}`);
  }

  const declarativeGrammar = path.join(input, "src", "grammar.json");
  if (isFile(declarativeGrammar)) {
    return { kind: "declarative-package", root: input };
  }

  const directDefGrammar = path.join(input, "grammar", "grammar.js");
  if (isFile(directDefGrammar)) {
    return { kind: "arborium-source", grammarPath: directDefGrammar, defRoot: input };
  }

  const nestedDefGrammar = path.join(input, "def", "grammar", "grammar.js");
  if (isFile(nestedDefGrammar)) {
    return { kind: "arborium-source", grammarPath: nestedDefGrammar, defRoot: path.join(input, "def") };
  }

  const grammarDirGrammar = path.join(input, "grammar.js");
  if (isFile(grammarDirGrammar)) {
    return {
      kind: "arborium-source",
      grammarPath: grammarDirGrammar,
      defRoot: inferDefRootFromGrammarJs(grammarDirGrammar),
    };
  }

  throw new Error(`${input} is not a Tree-sitter package root or Arborium source bundle`);
}

function inferDefRootFromGrammarJs(grammarPath) {
  const grammarDir = path.dirname(grammarPath);
  if (path.basename(grammarDir) === "grammar") {
    return path.dirname(grammarDir);
  }
  return grammarDir;
}

function ensureUsableOutputDir(outDir) {
  if (!fs.existsSync(outDir)) {
    fs.mkdirSync(outDir, { recursive: true });
    return;
  }
  const entries = fs.readdirSync(outDir);
  if (entries.length > 0) {
    throw new Error(`${outDir} already exists and is not empty`);
  }
}

function convertGrammarJs(grammarPath, outGrammarPath) {
  const converter = fileURLToPath(new URL("./grammar-js-to-json.mjs", import.meta.url));
  const result = spawnSync(process.execPath, [converter, grammarPath, "--out", outGrammarPath], {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  });
  if (result.status !== 0) {
    const stderr = result.stderr.trim();
    const stdout = result.stdout.trim();
    throw new Error(stderr || stdout || `grammar-js-to-json failed for ${grammarPath}`);
  }
}

function copyScannerSources(sourceDir, outSrcDir, copied) {
  for (const name of ["scanner.c", "scanner.cc"]) {
    const source = path.join(sourceDir, name);
    if (isFile(source)) {
      copyFileChecked(source, path.join(outSrcDir, name), copied);
    }
  }
}

function copyArboriumSamples(defRoot, outDir, copied) {
  for (const entry of fs.readdirSync(defRoot, { withFileTypes: true })) {
    if (entry.isFile() && entry.name.startsWith("sample.")) {
      copyFileChecked(path.join(defRoot, entry.name), path.join(outDir, "samples", entry.name), copied);
    }
  }
  copyTreeIfPresent(path.join(defRoot, "samples"), path.join(outDir, "samples"), copied);
}

function copyTreeIfPresent(sourceDir, outDir, copied) {
  if (!fs.existsSync(sourceDir)) {
    return;
  }
  const stat = fs.statSync(sourceDir);
  if (!stat.isDirectory()) {
    throw new Error(`${sourceDir} exists but is not a directory`);
  }
  for (const entry of fs.readdirSync(sourceDir, { withFileTypes: true })) {
    const source = path.join(sourceDir, entry.name);
    const dest = path.join(outDir, entry.name);
    if (entry.isDirectory()) {
      copyTreeIfPresent(source, dest, copied);
    } else if (entry.isFile()) {
      copyFileChecked(source, dest, copied);
    }
  }
}

function copyFileChecked(source, dest, copied) {
  const relativeDest = pathRelativeForBundle(dest);
  if (isGeneratedPath(relativeDest)) {
    return;
  }
  fs.mkdirSync(path.dirname(dest), { recursive: true });
  fs.copyFileSync(source, dest);
  copied.push(relativeDest);
}

function pathRelativeForBundle(dest) {
  const marker = `${path.sep}src${path.sep}`;
  const srcIndex = dest.lastIndexOf(marker);
  if (srcIndex >= 0) {
    return normalizeSlashes(dest.slice(srcIndex + 1));
  }
  for (const prefix of ["queries", "test", "samples"]) {
    const token = `${path.sep}${prefix}${path.sep}`;
    const index = dest.lastIndexOf(token);
    if (index >= 0) {
      return normalizeSlashes(dest.slice(index + 1));
    }
  }
  return normalizeSlashes(path.basename(dest));
}

function isGeneratedPath(relativePath) {
  const normalized = normalizeSlashes(relativePath);
  if (GENERATED_RELATIVE_PATHS.has(normalized)) {
    return true;
  }
  return GENERATED_BASENAMES.has(path.basename(normalized));
}

function mustStat(input) {
  try {
    return fs.statSync(input);
  } catch (error) {
    throw new Error(`cannot access ${input}: ${error.message}`);
  }
}

function isFile(input) {
  try {
    return fs.statSync(input).isFile();
  } catch {
    return false;
  }
}

function normalizeSlashes(value) {
  return value.split(path.sep).join("/");
}

try {
  main();
} catch (error) {
  console.error(error instanceof Error ? error.message : String(error));
  process.exit(1);
}
