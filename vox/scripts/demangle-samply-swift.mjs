#!/usr/bin/env node

import { readFileSync, writeFileSync } from "node:fs";
import { gzipSync, gunzipSync } from "node:zlib";
import { spawnSync } from "node:child_process";

function usage() {
  console.error(
    "usage: node scripts/demangle-samply-swift.mjs [input-profile.json.gz] [output-profile.json.gz]"
  );
}

function loadProfile(path) {
  const raw = readFileSync(path);
  const jsonBytes = path.endsWith(".gz") ? gunzipSync(raw) : raw;
  return JSON.parse(jsonBytes.toString("utf8"));
}

function saveProfile(path, profile) {
  const json = Buffer.from(JSON.stringify(profile));
  const output = path.endsWith(".gz") ? gzipSync(json) : json;
  writeFileSync(path, output);
}

function collectStrings(value, out = new Set()) {
  if (typeof value === "string") {
    if (value.includes("$s")) {
      out.add(value);
    }
    return out;
  }
  if (Array.isArray(value)) {
    for (const entry of value) {
      collectStrings(entry, out);
    }
    return out;
  }
  if (value && typeof value === "object") {
    for (const entry of Object.values(value)) {
      collectStrings(entry, out);
    }
  }
  return out;
}

function demangleStrings(values) {
  if (values.length === 0) {
    return new Map();
  }

  const input = `${values.join("\n")}\n`;
  const result = spawnSync("xcrun", ["swift-demangle"], {
    input,
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024,
  });

  if (result.error) {
    throw result.error;
  }
  if (result.status !== 0) {
    throw new Error(result.stderr || "swift-demangle failed");
  }

  const lines = result.stdout.endsWith("\n")
    ? result.stdout.slice(0, -1).split("\n")
    : result.stdout.split("\n");

  if (lines.length !== values.length) {
    throw new Error(
      `swift-demangle line mismatch: expected ${values.length}, got ${lines.length}`
    );
  }

  const map = new Map();
  for (let i = 0; i < values.length; i += 1) {
    map.set(values[i], lines[i]);
  }
  return map;
}

function rewriteProfile(value, replacements) {
  let rewritten = 0;
  if (Array.isArray(value)) {
    for (let i = 0; i < value.length; i += 1) {
      const entry = value[i];
      if (typeof entry === "string") {
        const replacement = replacements.get(entry);
        if (replacement && replacement !== entry) {
          value[i] = replacement;
          rewritten += 1;
        }
        continue;
      }
      rewritten += rewriteProfile(entry, replacements);
    }
    return rewritten;
  }
  if (value && typeof value === "object") {
    for (const [key, entry] of Object.entries(value)) {
      if (typeof entry === "string") {
        const replacement = replacements.get(entry);
        if (replacement && replacement !== entry) {
          value[key] = replacement;
          rewritten += 1;
        }
        continue;
      }
      rewritten += rewriteProfile(entry, replacements);
    }
  }
  return rewritten;
}

const inputPath = process.argv[2] ?? "profile.json.gz";
const outputPath = process.argv[3] ?? inputPath.replace(/(\.json)?(\.gz)?$/, ".demangled.json.gz");

if (process.argv.includes("--help") || process.argv.includes("-h")) {
  usage();
  process.exit(0);
}

const profile = loadProfile(inputPath);
const swiftStrings = [...collectStrings(profile)];
const replacements = demangleStrings(swiftStrings);
const rewritten = rewriteProfile(profile, replacements);
saveProfile(outputPath, profile);

console.error(
  `demangled ${rewritten} string table entries (${swiftStrings.length} unique) -> ${outputPath}`
);
