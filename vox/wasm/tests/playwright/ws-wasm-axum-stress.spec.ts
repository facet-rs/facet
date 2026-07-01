import { test, expect } from "@playwright/test";
import { spawn, type ChildProcess } from "node:child_process";
import { createServer } from "node:net";
import { startWsServer } from "../../../typescript/tests/shared/ws-server";

// Root of the vox project
const projectRoot = new URL("../../../", import.meta.url).pathname;

// Soak parameters: many concurrent connections to the same axum server,
// hammered for a fixed wall-clock duration.
const CONNECTIONS = 8;
const DURATION_MS = 30_000;

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

test("stress: many concurrent wasm connections hammer the axum vox server", async ({ page }) => {
  // Soak duration + module init + margin.
  test.setTimeout(DURATION_MS + 60_000);

  page.on("console", (msg) => {
    console.log(`[browser ${msg.type()}] ${msg.text()}`);
  });
  page.on("pageerror", (err) => {
    console.log(`[browser pageerror] ${err.message}`);
  });
  page.on("requestfailed", (req) => {
    console.log(`[browser requestfailed] ${req.url()} - ${req.failure()?.errorText}`);
  });

  const wsUrl = `ws://127.0.0.1:${wsPort}/ws`;
  const target = `http://127.0.0.1:${vitePort}/?ws=${encodeURIComponent(wsUrl)}&stress=1&connections=${CONNECTIONS}&durationMs=${DURATION_MS}`;
  console.log(`Navigating to stress page (${target})...`);
  await page.goto(target, { waitUntil: "networkidle" });

  // Wait for the whole soak to finish (duration + generous margin).
  await page.waitForFunction(() => (window as any).testsComplete === true, {
    timeout: DURATION_MS + 30_000,
  });

  const summary = await page.evaluate(() => (window as any).stressSummary);
  console.log("Stress summary:", summary);

  expect(summary, "stress summary should be present").toBeTruthy();
  // Every worker connected, nothing errored, nothing got stuck.
  expect(summary.connected, "all connections should establish").toBe(CONNECTIONS);
  expect(summary.stuck, `stuck calls (first error: ${summary.firstError})`).toBe(0);
  expect(summary.totalErrors, `errors (first error: ${summary.firstError})`).toBe(0);
  expect(summary.allOk).toBe(true);
  // A real soak should push a lot of traffic through; guard against a no-op.
  expect(summary.totalRequests).toBeGreaterThan(CONNECTIONS * 10);
});
