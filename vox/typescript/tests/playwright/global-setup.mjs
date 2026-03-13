import { existsSync, readdirSync, statSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { spawnSync } from "node:child_process";

const projectRoot = new URL("../../../", import.meta.url).pathname;

const wasmTargets = [
  {
    name: "browser-wasm",
    crateDir: join(projectRoot, "rust/wasm-browser-tests"),
    crateArg: "rust/wasm-browser-tests",
    outDirArg: "../../typescript/tests/browser-wasm/pkg",
    outputFile: join(projectRoot, "typescript/tests/browser-wasm/pkg/wasm_browser_tests.js"),
  },
  {
    name: "browser-inprocess",
    crateDir: join(projectRoot, "rust/wasm-inprocess-tests"),
    crateArg: "rust/wasm-inprocess-tests",
    outDirArg: "../../typescript/tests/browser-inprocess/pkg",
    outputFile: join(projectRoot, "typescript/tests/browser-inprocess/pkg/wasm_inprocess_tests.js"),
  },
];

function newestMtime(path) {
  const stat = statSync(path);
  if (!stat.isDirectory()) {
    return stat.mtimeMs;
  }

  let newest = stat.mtimeMs;
  for (const entry of readdirSync(path)) {
    newest = Math.max(newest, newestMtime(join(path, entry)));
  }
  return newest;
}

function needsBuild(target) {
  if (!existsSync(target.outputFile)) {
    return true;
  }

  const outputMtime = statSync(target.outputFile).mtimeMs;
  const inputMtime = Math.max(
    newestMtime(join(target.crateDir, "Cargo.toml")),
    newestMtime(join(target.crateDir, "src")),
  );

  return inputMtime > outputMtime;
}

function wasmPackCommand() {
  const candidates = [
    process.env.CARGO_HOME ? join(process.env.CARGO_HOME, "bin", "wasm-pack") : null,
    join(homedir(), ".cargo", "bin", "wasm-pack"),
    "wasm-pack",
  ];

  for (const candidate of candidates) {
    if (candidate === null) {
      continue;
    }
    if (candidate === "wasm-pack" || existsSync(candidate)) {
      return candidate;
    }
  }

  return "wasm-pack";
}

function buildTarget(target) {
  console.log(`[playwright] building ${target.name} wasm fixture with wasm-pack`);
  const wasmPack = wasmPackCommand();
  const result = spawnSync(
    wasmPack,
    ["build", "--target", "web", target.crateArg, "--out-dir", target.outDirArg],
    {
      cwd: projectRoot,
      stdio: "inherit",
    },
  );

  if (result.error) {
    throw new Error(`failed to launch ${wasmPack} for ${target.name}: ${result.error.message}`);
  }

  if (result.status !== 0) {
    throw new Error(`wasm-pack build failed for ${target.name} with exit code ${result.status}`);
  }
}

export default async function globalSetup() {
  for (const target of wasmTargets) {
    if (needsBuild(target)) {
      buildTarget(target);
    }
  }
}
