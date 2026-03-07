import { test, expect } from "@playwright/test";
import { spawn, type ChildProcess } from "node:child_process";
import { createServer } from "node:net";
import { existsSync } from "node:fs";
import { join } from "node:path";

// Root of the roam project
const projectRoot = new URL("../../../", import.meta.url).pathname;

// Check if wasm pkg exists - skip tests if not built
const wasmPkgPath = join(projectRoot, "typescript/tests/browser-inprocess/pkg/wasm_inprocess_tests.js");
const wasmPkgExists = existsSync(wasmPkgPath);

// Skip all tests in this file if wasm pkg doesn't exist
test.skip(!wasmPkgExists, "Wasm pkg not built - run wasm-pack build first");

let viteServer: ChildProcess | null = null;
let vitePort = 0;

function getFreePort(): Promise<number> {
  return new Promise((resolve, reject) => {
    const srv = createServer();
    srv.listen(0, () => {
      const port = (srv.address() as { port: number }).port;
      srv.close(() => resolve(port));
    });
    srv.on("error", reject);
  });
}

test.beforeAll(async () => {
  vitePort = await getFreePort();

  // Start Vite dev server for in-process browser test app
  // No WebSocket server needed - everything is in-process!
  console.log(`Starting Vite dev server for in-process tests on port ${vitePort}...`);
  viteServer = spawn("pnpm", ["exec", "vite", "--port", String(vitePort), "--host", "127.0.0.1"], {
    cwd: `${projectRoot}typescript/tests/browser-inprocess`,
    stdio: ["ignore", "pipe", "pipe"],
  });

  await new Promise<void>((resolve, reject) => {
    const timeout = globalThis.setTimeout(
      () => reject(new Error("Timeout starting Vite server")),
      10000,
    );

    viteServer!.stdout!.on("data", (data: Buffer) => {
      // eslint-disable-next-line no-control-regex
      const text = data.toString().replace(/\x1b\[[0-9;]*m/g, "");
      console.log(`[vite stdout] ${text.trim()}`);
      if (text.includes(`127.0.0.1:${vitePort}`)) {
        clearTimeout(timeout);
        resolve();
      }
    });

    viteServer!.stderr!.on("data", (data: Buffer) => {
      console.log(`[vite stderr] ${data.toString().trim()}`);
    });

    viteServer!.on("error", (err) => {
      clearTimeout(timeout);
      reject(err);
    });

    viteServer!.on("exit", (code, signal) => {
      clearTimeout(timeout);
      reject(new Error(`Vite exited unexpectedly (code=${code}, signal=${signal})`));
    });
  });

  console.log(`Vite dev server started on port ${vitePort}`);
});

test.afterAll(async () => {
  if (viteServer) {
    viteServer.kill("SIGTERM");
    viteServer = null;
  }
});

test("In-process: Rust WASM acceptor + TS initiator in same tab", async ({ page }) => {
  page.on("console", (msg) => {
    console.log(`[browser ${msg.type()}] ${msg.text()}`);
  });

  page.on("pageerror", (err) => {
    console.log(`[browser pageerror] ${err.message}`);
  });

  page.on("requestfailed", (req) => {
    console.log(`[browser requestfailed] ${req.url()} - ${req.failure()?.errorText}`);
  });

  console.log(`Navigating to in-process test page (vite=${vitePort})...`);
  await page.goto(`http://127.0.0.1:${vitePort}/`, { waitUntil: "networkidle" });
  console.log("Navigation complete, waiting for testsComplete...");

  await page.waitForFunction(() => (window as any).testsComplete === true, { timeout: 10000 });
  console.log("testsComplete is true");

  const results = await page.evaluate(() => (window as any).testResults);
  console.log("In-process test results:", results);

  expect(results).toBeInstanceOf(Array);
  expect(results.length).toBeGreaterThan(0);

  for (const result of results) {
    expect(result.passed, `Test "${result.name}" failed: ${result.error}`).toBe(true);
  }
});
