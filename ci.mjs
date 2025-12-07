#!/usr/bin/env node
// CI task runner for facet
// Usage: node ci.mjs <task>

import { execSync, spawnSync } from "child_process";

const TASKS = {
  "valgrind-cranelift": valgrindCranelift,
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

function capture(cmd) {
  return execSync(cmd, { encoding: "utf-8" }).trim();
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

async function valgrindCranelift() {
  // Build tests first and capture the binary path
  cmdGroup("Build tests", () => {
    run("cargo test -p facet-json --features cranelift --lib --no-run");
  });

  // Get the test binary path
  const jsonOutput = capture(
    "cargo test -p facet-json --features cranelift --lib --no-run --message-format=json 2>/dev/null"
  );

  let testBinary = null;
  for (const line of jsonOutput.split("\n")) {
    try {
      const msg = JSON.parse(line);
      if (msg.executable) {
        testBinary = msg.executable;
        break;
      }
    } catch {
      // skip non-JSON lines
    }
  }

  if (!testBinary) {
    console.error("Failed to find test binary");
    process.exit(1);
  }

  console.log(`Test binary: ${testBinary}`);

  // Run under valgrind
  // Note: We use --errors-for-leak-kinds=definite,indirect because:
  // - "still reachable" is expected for a JIT that caches compiled code
  // - "possibly lost" can be false positives from circular references in the JIT
  cmdGroup("Run valgrind", () => {
    run(
      `valgrind --leak-check=full --show-leak-kinds=all --errors-for-leak-kinds=definite,indirect --error-exitcode=1 "${testBinary}" cranelift::tests --test-threads=1`
    );
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
