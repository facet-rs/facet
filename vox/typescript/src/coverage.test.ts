/**
 * Spec coverage tests for TypeScript.
 *
 * This test suite generates one test per spec rule. Each test fails unless
 * the rule is covered by either:
 * - A conformance test (from `rapace-conformance --list --format json`)
 * - An `[impl ...]` annotation in implementation code
 */

import { test } from "node:test";
import { execSync } from "node:child_process";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, extname } from "node:path";
import * as assert from "node:assert";

interface RulesManifest {
  rules: Record<string, { url: string }>;
}

interface TestCase {
  name: string;
  rules: string[];
}

// Find workspace root (where Cargo.toml is)
function findWorkspaceRoot(): string {
  let dir = process.cwd();
  for (let i = 0; i < 100; i++) {
    try {
      const cargoToml = join(dir, "Cargo.toml");
      readFileSync(cargoToml);
      return dir;
    } catch {
      const parent = join(dir, "..");
      if (parent === dir) break;
      dir = parent;
    }
  }
  throw new Error("Could not find workspace root");
}

// Get all spec rules from _rules.json
function getSpecRules(workspaceRoot: string): string[] {
  const rulesPath = join(workspaceRoot, "docs/public/_rules.json");
  try {
    const content = readFileSync(rulesPath, "utf-8");
    const manifest: RulesManifest = JSON.parse(content);
    return Object.keys(manifest.rules);
  } catch {
    console.warn(`Warning: _rules.json not found at ${rulesPath}. Run 'tracey rules -o docs/public/_rules.json docs/content/spec/**/*.md' first.`);
    return [];
  }
}

// Get covered rules from conformance harness
function getRulesFromConformanceHarness(workspaceRoot: string): Set<string> {
  const covered = new Set<string>();

  try {
    // Build the conformance binary first if needed, then run it
    const output = execSync(
      "cargo build -p rapace-conformance --quiet && cargo run -p rapace-conformance --quiet -- --list --format json",
      { cwd: workspaceRoot, encoding: "utf-8", stdio: ["pipe", "pipe", "pipe"] }
    );

    const tests: TestCase[] = JSON.parse(output);
    for (const testCase of tests) {
      for (const rule of testCase.rules) {
        covered.add(rule);
      }
    }
  } catch (e) {
    console.warn("Warning: Could not run conformance harness:", e);
  }

  return covered;
}

// Recursively scan for [impl ...] annotations in TypeScript files
function scanForImplAnnotations(dir: string, covered: Set<string>): void {
  const implRe = /\[impl ([a-z][a-z0-9._-]+)\]/g;

  try {
    const entries = readdirSync(dir);
    for (const entry of entries) {
      const path = join(dir, entry);

      // Skip node_modules, dist, hidden dirs
      if (entry.startsWith(".") || entry === "node_modules" || entry === "dist") {
        continue;
      }

      const stat = statSync(path);
      if (stat.isDirectory()) {
        scanForImplAnnotations(path, covered);
      } else if (extname(entry) === ".ts") {
        const content = readFileSync(path, "utf-8");
        let match;
        while ((match = implRe.exec(content)) !== null) {
          covered.add(match[1]);
        }
      }
    }
  } catch {
    // Ignore errors
  }
}

// Get all covered rules
function getCoveredRules(workspaceRoot: string): Set<string> {
  const covered = new Set<string>();

  // 1. Get rules from conformance harness
  const fromHarness = getRulesFromConformanceHarness(workspaceRoot);
  for (const rule of fromHarness) {
    covered.add(rule);
  }

  // 2. Scan TypeScript implementation for [impl ...] annotations
  const tsDir = join(workspaceRoot, "typescript/src");
  scanForImplAnnotations(tsDir, covered);

  return covered;
}

// Main test generation
const workspaceRoot = findWorkspaceRoot();
const specRules = getSpecRules(workspaceRoot);
const coveredRules = getCoveredRules(workspaceRoot);

// Generate a test for each spec rule
for (const rule of specRules) {
  test(`rule.${rule}`, () => {
    assert.ok(
      coveredRules.has(rule),
      `Rule '${rule}' has no conformance test or [impl ...] annotation`
    );
  });
}
