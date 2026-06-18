import { readdir, readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(SCRIPT_DIR, "..");
const PACKAGES_ROOT = path.join(REPO_ROOT, "typescript", "packages");
const SOURCE_EXTENSIONS = new Set([".ts", ".mts", ".cts", ".tsx"]);

async function* walk(dir) {
  const entries = await readdir(dir, { withFileTypes: true });
  for (const entry of entries) {
    const fullPath = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      yield* walk(fullPath);
      continue;
    }

    if (SOURCE_EXTENSIONS.has(path.extname(entry.name))) {
      yield fullPath;
    }
  }
}

function lineForOffset(text, offset) {
  return text.slice(0, offset).split("\n").length;
}

function packageNameForPath(filePath) {
  const relative = path.relative(PACKAGES_ROOT, filePath);
  if (relative.startsWith("..")) {
    return null;
  }
  return relative.split(path.sep)[0] ?? null;
}

function checkSpecifier({
  specifier,
  sourceFile,
  sourcePackage,
}) {
  if (specifier.startsWith("@bearcove/") && specifier.includes("/src/")) {
    return `forbidden workspace '/src/' import (${specifier})`;
  }

  if (!specifier.startsWith(".")) {
    return null;
  }

  const resolved = path.resolve(path.dirname(sourceFile), specifier);
  const targetPackage = packageNameForPath(resolved);
  if (!targetPackage || targetPackage === sourcePackage) {
    return null;
  }

  if (resolved.includes(`${path.sep}src${path.sep}`)) {
    return `forbidden cross-package '/src/' import (${specifier})`;
  }

  return null;
}

async function main() {
  const violations = [];
  const importPattern = /(?:from\s+|import\s*\()\s*["']([^"']+)["']/g;

  for await (const file of walk(PACKAGES_ROOT)) {
    const sourcePackage = packageNameForPath(file);
    if (!sourcePackage) {
      continue;
    }

    const contents = await readFile(file, "utf8");
    let match;
    while ((match = importPattern.exec(contents)) !== null) {
      const specifier = match[1];
      const issue = checkSpecifier({
        specifier,
        sourceFile: file,
        sourcePackage,
      });
      if (!issue) {
        continue;
      }

      violations.push({
        file,
        line: lineForOffset(contents, match.index),
        issue,
      });
    }
  }

  if (violations.length === 0) {
    console.log("TypeScript package boundary check passed.");
    return;
  }

  console.error("TypeScript package boundary violations:");
  for (const violation of violations) {
    const rel = path.relative(REPO_ROOT, violation.file);
    console.error(`- ${rel}:${violation.line}: ${violation.issue}`);
  }
  process.exitCode = 1;
}

await main();
