import { test, expect } from "@playwright/test";
import { spawn, type ChildProcess } from "node:child_process";
import { setTimeout as sleep } from "node:timers/promises";
import { existsSync } from "node:fs";
import { join } from "node:path";

// Root of the roam project
const projectRoot = new URL("../../../", import.meta.url).pathname;

// Check if wasm pkg exists - skip tests if not built
const wasmPkgPath = join(projectRoot, "typescript/tests/browser-wasm/pkg/wasm_browser_tests.js");
const wasmPkgExists = existsSync(wasmPkgPath);

// Skip all tests in this file if wasm pkg doesn't exist
test.skip(!wasmPkgExists, "Wasm pkg not built - run wasm-pack build first");

let wsServer: ChildProcess | null = null;
let viteServer: ChildProcess | null = null;

async function waitForPort(port: number, timeoutMs: number = 10000): Promise<void> {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    try {
      const response = await fetch(`http://localhost:${port}/`);
      if (response.ok || response.status === 404) {
        return; // Server is up
      }
    } catch {
      // Connection refused, try again
    }
    await sleep(100);
  }
  throw new Error(`Timeout waiting for port ${port}`);
}

test.beforeAll(async () => {
  // Start Rust WebSocket Echo server
  console.log("Starting Rust WebSocket server...");
  wsServer = spawn("cargo", ["run", "-p", "peer-server", "--bin", "ws-peer-server"], {
    cwd: projectRoot,
    env: { ...process.env, WS_PORT: "9001" },
    stdio: ["ignore", "pipe", "pipe"],
  });

  // Wait for server to print port
  await new Promise<void>((resolve, reject) => {
    const timeout = globalThis.setTimeout(
      () => reject(new Error("Timeout starting WS server")),
      30000,
    );

    wsServer!.stdout!.on("data", (data: Buffer) => {
      const line = data.toString().trim();
      console.log(`[ws-server stdout] ${line}`);
      if (line === "9001") {
        clearTimeout(timeout);
        resolve();
      }
    });

    wsServer!.stderr!.on("data", (data: Buffer) => {
      console.log(`[ws-server stderr] ${data.toString().trim()}`);
    });

    wsServer!.on("error", (err) => {
      clearTimeout(timeout);
      reject(err);
    });

    wsServer!.on("exit", (code) => {
      if (code !== null && code !== 0) {
        clearTimeout(timeout);
        reject(new Error(`WS server exited with code ${code}`));
      }
    });
  });

  console.log("Rust WebSocket server started on port 9001");

  // Start Vite dev server for wasm browser test app
  console.log("Starting Vite dev server for Wasm tests...");
  viteServer = spawn("pnpm", ["exec", "vite", "--port", "3001"], {
    cwd: `${projectRoot}typescript/tests/browser-wasm`,
    stdio: ["ignore", "pipe", "pipe"],
  });

  viteServer.stdout!.on("data", (data: Buffer) => {
    console.log(`[vite stdout] ${data.toString().trim()}`);
  });

  viteServer.stderr!.on("data", (data: Buffer) => {
    console.log(`[vite stderr] ${data.toString().trim()}`);
  });

  // Wait for Vite to be ready
  await waitForPort(3001);
  console.log("Vite dev server started on port 3001");
});

test.afterAll(async () => {
  // Kill servers
  if (viteServer) {
    viteServer.kill("SIGTERM");
    viteServer = null;
  }
  if (wsServer) {
    wsServer.kill("SIGTERM");
    wsServer = null;
  }
});

test("Rust/Wasm client can connect to Rust WebSocket server and run tests", async ({ page }) => {
  // Capture console messages
  page.on("console", (msg) => {
    console.log(`[browser ${msg.type()}] ${msg.text()}`);
  });

  // Navigate to test page with WebSocket URL
  await page.goto("http://localhost:3001/?ws=ws://localhost:9001");

  // Wait for tests to complete (timeout after 30s for wasm init + tests)
  await page.waitForFunction(() => (window as any).testsComplete === true, { timeout: 30000 });

  // Get test results
  const results = await page.evaluate(() => (window as any).testResults);

  console.log("Wasm test results:", results);

  // Verify all tests passed
  expect(results).toBeInstanceOf(Array);
  expect(results.length).toBeGreaterThan(0);

  for (const result of results) {
    expect(result.passed, `Test "${result.name}" failed: ${result.error}`).toBe(true);
  }
});
