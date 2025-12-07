#!/usr/bin/env node
// CI task runner for facet
// Usage: node ci.mjs <task>

import { spawnSync } from "child_process";

const TASKS = {
  "valgrind-facet-json": valgrindFacetJson,
};

function run(cmd, opts = {}) {
  console.log(`\x1b[1;36m$ ${cmd}\x1b[0m`);
  const result = spawnSync(cmd, {
    shell: true,
    stdio: "inherit",
    ...opts,
  });
  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
  return result;
}

function cmdGroup(name, fn) {
  if (process.env.CI) {
    console.log(`::group::${name}`);
  }
  try {
    fn();
  } finally {
    if (process.env.CI) {
      console.log("::endgroup::");
    }
  }
}

async function valgrindFacetJson() {
  // Run facet-json tests under valgrind using nextest's wrapper script feature
  // The valgrind profile is defined in .config/nextest.toml
  cmdGroup("Run facet-json tests under valgrind", () => {
    run("cargo nextest run -p facet-json --features cranelift --profile valgrind");
  });
}

// Main
const task = process.argv[2];
if (!task || !TASKS[task]) {
  console.log("Available tasks:");
  for (const name of Object.keys(TASKS)) {
    console.log(`  ${name}`);
  }
  process.exit(task ? 1 : 0);
}

await TASKS[task]();
