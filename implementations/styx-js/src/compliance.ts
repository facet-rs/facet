import * as fs from "node:fs";
import * as path from "node:path";
import { parse } from "./parser.js";
import { documentToSexp, errorToSexp } from "./sexp.js";
import { ParseError } from "./types.js";

function findStyxFiles(dir: string): string[] {
  const files: string[] = [];

  function walk(d: string) {
    const entries = fs.readdirSync(d, { withFileTypes: true });
    for (const entry of entries) {
      const fullPath = path.join(d, entry.name);
      if (entry.isDirectory()) {
        walk(fullPath);
      } else if (entry.name.endsWith(".styx")) {
        files.push(fullPath);
      }
    }
  }

  walk(dir);
  return files.sort();
}

function main() {
  const args = process.argv.slice(2);
  if (args.length !== 1) {
    console.error("Usage: node compliance.js <corpus-dir>");
    process.exit(1);
  }

  const corpusDir = args[0];
  const files = findStyxFiles(corpusDir);

  for (const file of files) {
    // Make path relative to corpus dir's grandparent for matching golden output
    const relativePath = path.relative(path.dirname(path.dirname(corpusDir)), file);
    console.log(`; file: ${relativePath}`);

    const source = fs.readFileSync(file, "utf-8");

    try {
      const doc = parse(source);
      console.log(documentToSexp(doc));
    } catch (e) {
      if (e instanceof ParseError) {
        console.log(errorToSexp(e.message, e.span));
      } else {
        throw e;
      }
    }
  }
}

main();
