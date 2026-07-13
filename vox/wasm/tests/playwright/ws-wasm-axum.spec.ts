import { test, expect } from "@playwright/test";
import { spawn, type ChildProcess } from "node:child_process";
import { createServer } from "node:net";
import { startWsServer } from "../../../typescript/tests/shared/ws-server";

// Root of the vox project
const projectRoot = new URL("../../../", import.meta.url).pathname;

let wsServer: ChildProcess | null = null;
let viteServer: ChildProcess | null = null;
let wsPort = 0;
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
  wsPort = await getFreePort();
  vitePort = await getFreePort();

  // Start the axum HTTP peer server (vox endpoint on the /ws route).
  console.log(`Starting axum peer server on port ${wsPort}...`);
  wsServer = await startWsServer(projectRoot, wsPort, "axum-peer-server");

  console.log(`axum peer server started on port ${wsPort}`);

  // Start Vite dev server for wasm browser test app
  console.log(`Starting Vite dev server for Wasm tests on port ${vitePort}...`);
  viteServer = spawn("pnpm", ["exec", "vite", "--port", String(vitePort), "--host", "127.0.0.1"], {
    cwd: `${projectRoot}wasm/tests/browser-wasm`,
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
  if (wsServer) {
    wsServer.kill("SIGTERM");
    wsServer = null;
  }
});

test("Rust/Wasm client (vox::connect_lane) can talk to an axum vox server", async ({ page }) => {
  page.on("console", (msg) => {
    console.log(`[browser ${msg.type()}] ${msg.text()}`);
  });

  page.on("pageerror", (err) => {
    console.log(`[browser pageerror] ${err.message}`);
  });

  page.on("requestfailed", (req) => {
    console.log(`[browser requestfailed] ${req.url()} - ${req.failure()?.errorText}`);
  });

  // The vox endpoint lives on the /ws route of the axum server.
  const wsUrl = `ws://127.0.0.1:${wsPort}/ws`;
  console.log(`Navigating to test page (vite=${vitePort}, ws=${wsUrl})...`);
  await page.goto(`http://127.0.0.1:${vitePort}/?ws=${encodeURIComponent(wsUrl)}`, {
    waitUntil: "networkidle",
  });
  console.log("Navigation complete, waiting for testsComplete...");

  await page.waitForFunction(() => (window as any).testsComplete === true, { timeout: 10000 });
  console.log("testsComplete is true");

  const results = await page.evaluate(() => (window as any).testResults);
  console.log("Wasm test results:", results);

  expect(results).toBeInstanceOf(Array);
  expect(results.length).toBeGreaterThan(0);

  for (const result of results) {
    expect(result.passed, `Test "${result.name}" failed: ${result.error}`).toBe(true);
  }
});
