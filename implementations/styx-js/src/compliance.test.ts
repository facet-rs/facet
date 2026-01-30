import * as fs from "node:fs";
import * as path from "node:path";
import { fileURLToPath } from "node:url";
import { execSync } from "node:child_process";
import { describe, it } from "node:test";
import assert from "node:assert";
import { parse } from "./parser.js";
import { documentToSexp, errorToSexp } from "./sexp.js";
import { ParseError } from "./types.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

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

function findCorpusPath(): string {
  const candidates = ["../../compliance/corpus", "../../../compliance/corpus"];

  for (const c of candidates) {
    const abs = path.resolve(__dirname, c);
    try {
      const stat = fs.statSync(abs);
      if (stat.isDirectory()) {
        return abs;
      }
    } catch {
      // Continue to next candidate
    }
  }

  // Try from cwd
  const fromCwd = path.resolve(process.cwd(), "../../compliance/corpus");
  try {
    const stat = fs.statSync(fromCwd);
    if (stat.isDirectory()) {
      return fromCwd;
    }
  } catch {
    // Fall through
  }

  throw new Error("Could not find compliance corpus directory");
}

function findStyxCLI(): string | null {
  const candidates = [
    "../../target/debug/styx",
    "../../target/release/styx",
    "../../../target/debug/styx",
    "../../../target/release/styx",
  ];

  for (const c of candidates) {
    const abs = path.resolve(__dirname, c);
    try {
      fs.accessSync(abs, fs.constants.X_OK);
      return abs;
    } catch {
      // Continue
    }
  }

  // Try from cwd
  for (const rel of ["../../target/debug/styx", "../../target/release/styx"]) {
    const fromCwd = path.resolve(process.cwd(), rel);
    try {
      fs.accessSync(fromCwd, fs.constants.X_OK);
      return fromCwd;
    } catch {
      // Continue
    }
  }

  return null;
}

function getJsOutput(content: string): string {
  try {
    const doc = parse(content);
    return documentToSexp(doc);
  } catch (e) {
    if (e instanceof ParseError) {
      return errorToSexp(e.message, e.span);
    }
    throw e;
  }
}

function getRustOutput(file: string, styxCLI: string): string {
  try {
    const stdout = execSync(`${styxCLI} tree --format sexp "${file}"`, {
      encoding: "utf-8",
      stdio: ["pipe", "pipe", "pipe"],
    });
    return stdout;
  } catch (e: unknown) {
    const err = e as { stderr?: string };
    if (err.stderr) {
      return extractErrorFromStderr(err.stderr);
    }
    throw e;
  }
}

function extractErrorFromStderr(stderr: string): string {
  const lines = stderr.split("\n");
  for (const line of lines) {
    if (line.startsWith("error: parse error at ")) {
      const match = line.slice(7).match(/parse error at (\d+)-(\d+): (.+)/);
      if (match) {
        const [, start, end, msg] = match;
        return `(error [${start}, ${end}] "parse error at ${start}-${end}: ${jsonEscape(msg)}")`;
      }
    }
  }
  return `(error [-1, -1] "${jsonEscape(stderr.trim())}")`;
}

function jsonEscape(s: string): string {
  let result = "";
  for (const ch of s) {
    switch (ch) {
      case '"':
        result += '\\"';
        break;
      case "\\":
        result += "\\\\";
        break;
      case "\n":
        result += "\\n";
        break;
      case "\r":
        result += "\\r";
        break;
      case "\t":
        result += "\\t";
        break;
      default:
        if (ch.charCodeAt(0) < 32) {
          result += `\\u${ch.charCodeAt(0).toString(16).padStart(4, "0")}`;
        } else {
          result += ch;
        }
    }
  }
  return result;
}

function normalizeOutput(output: string): string {
  const lines = output.split("\n");
  const result: string[] = [];
  for (const line of lines) {
    const trimmed = line.trim();
    if (trimmed.startsWith("; file:")) continue;
    if (trimmed === "") continue;
    result.push(trimmed);
  }
  return result.join("\n");
}

function parseErrorSpan(output: string): { span: [number, number]; msg: string } | null {
  const match = output.match(/\(error \[(\d+), (\d+)\] "([^"]*)"/);
  if (!match) return null;
  return {
    span: [parseInt(match[1], 10), parseInt(match[2], 10)],
    msg: match[3],
  };
}

function annotateSpan(source: string, start: number, end: number, msg: string): string {
  if (start < 0 || end < 0 || start > source.length) {
    return `  [invalid span ${start}-${end}]\n`;
  }
  if (end > source.length) {
    end = source.length;
  }

  // Find all lines that overlap with the span
  const lines: { text: string; lineStart: number; lineEnd: number }[] = [];
  let pos = 0;
  for (const lineText of source.split("\n")) {
    const lineStart = pos;
    const lineEnd = pos + lineText.length;
    // Check if this line overlaps with [start, end)
    if (lineEnd >= start && lineStart < end) {
      lines.push({ text: lineText, lineStart, lineEnd });
    }
    pos = lineEnd + 1; // +1 for the newline
    if (lineStart >= end) break;
  }

  if (lines.length === 0) {
    return `  [span ${start}-${end} not found]\n`;
  }

  let result = "";
  for (const { text, lineStart, lineEnd } of lines) {
    result += `  ${text}\n`;
    // Calculate caret positions for this line
    const caretStart = Math.max(start, lineStart) - lineStart;
    const caretEnd = Math.min(end, lineEnd) - lineStart;
    let width = caretEnd - caretStart;
    if (width < 1) width = 1;
    result += `  ${" ".repeat(caretStart)}${"^".repeat(width)}\n`;
  }
  result += `  ${msg} (${start}-${end})\n`;
  return result;
}

function annotateErrorDiff(source: string, jsOutput: string, rustOutput: string): string {
  const jsErr = parseErrorSpan(jsOutput);
  const rustErr = parseErrorSpan(rustOutput);

  if (!jsErr && !rustErr) {
    return ""; // No errors to annotate
  }

  let result = "\n";

  if (rustErr) {
    result += "Expected error:\n";
    result += annotateSpan(source, rustErr.span[0], rustErr.span[1], rustErr.msg);
    result += "\n";
  } else {
    result += "Expected: no error\n\n";
  }

  if (jsErr) {
    result += "Got error:\n";
    result += annotateSpan(source, jsErr.span[0], jsErr.span[1], jsErr.msg);
  } else {
    result += "Got: no error\n";
  }

  return result;
}

describe("Compliance", () => {
  const corpusPath = findCorpusPath();
  const styxCLI = findStyxCLI();

  if (!styxCLI) {
    it.skip("styx-cli not found - run 'cargo build' first", () => {});
    return;
  }

  const files = findStyxFiles(corpusPath);

  for (const file of files) {
    const relPath = path.relative(corpusPath, file);
    it(relPath, () => {
      const content = fs.readFileSync(file, "utf-8");
      const jsOutput = getJsOutput(content);
      const rustOutput = getRustOutput(file, styxCLI);

      const jsNorm = normalizeOutput(jsOutput);
      const rustNorm = normalizeOutput(rustOutput);

      if (jsNorm !== rustNorm) {
        const annotation = annotateErrorDiff(content, jsOutput, rustOutput);
        assert.fail(
          `Output mismatch${annotation}\n--- JS output ---\n${jsOutput}\n--- Rust output ---\n${rustOutput}`,
        );
      }
    });
  }
});
