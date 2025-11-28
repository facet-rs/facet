#!/usr/bin/env node
/**
 * Build rustdoc for the test crate with our highlight.js bundle injected
 */

import { execSync } from "child_process";
import { cpSync, existsSync, mkdirSync } from "fs";
import { dirname, resolve } from "path";
import { fileURLToPath } from "url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, "..");
const TEST_CRATE = resolve(ROOT, "test-crate");
const DIST = resolve(ROOT, "dist");
const DOC_OUTPUT = resolve(TEST_CRATE, "target", "doc");

// Ensure dist exists (should have been built by build:bundle)
if (!existsSync(DIST)) {
  console.error("Error: dist/ not found. Run 'pnpm run build:bundle' first.");
  process.exit(1);
}

console.log("Building rustdoc for test-crate...");

// Build the docs with our highlight.html injected
// We need to use an absolute path for the html-in-header
const highlightHtml = resolve(ROOT, "highlight-dev.html");

try {
  execSync(
    `cargo doc --no-deps --manifest-path ${TEST_CRATE}/Cargo.toml`,
    {
      stdio: "inherit",
      env: {
        ...process.env,
        RUSTDOCFLAGS: `--html-in-header ${highlightHtml}`,
      },
    }
  );
} catch (error) {
  console.error("Failed to build rustdoc");
  process.exit(1);
}

// Copy our built assets into the doc output so they can be served
const hljsDir = resolve(DOC_OUTPUT, "hljs");
if (!existsSync(hljsDir)) {
  mkdirSync(hljsDir, { recursive: true });
}

console.log("Copying highlight.js bundle to doc output...");
cpSync(resolve(DIST, "facet-hljs.iife.js"), resolve(hljsDir, "facet-hljs.iife.js"));
cpSync(resolve(DIST, "facet-hljs.css"), resolve(hljsDir, "facet-hljs.css"));

console.log(`\nDocs built successfully at: ${DOC_OUTPUT}`);
console.log(`Open: ${DOC_OUTPUT}/hljs_test_crate/index.html`);
