import { test, expect } from "@playwright/test";
import { spawn, type ChildProcess } from "node:child_process";
import { createServer } from "node:net";
import { startTsWsServer, type TsWsServerHandle } from "./ws-ts-server";

const projectRoot = new URL("../../../", import.meta.url).pathname;

let wsServer: TsWsServerHandle | null = null;
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

  console.log(`Starting TypeScript WebSocket server on port ${wsPort}...`);
  wsServer = await startTsWsServer(wsPort);
  console.log(`TypeScript WebSocket server started on port ${wsPort}`);

  console.log(`Starting Vite dev server on port ${vitePort}...`);
  viteServer = spawn("pnpm", ["exec", "vite", "--port", String(vitePort), "--host", "127.0.0.1"], {
    cwd: `${projectRoot}typescript/tests/browser`,
    stdio: ["ignore", "pipe", "pipe"],
  });

  await new Promise<void>((resolve, reject) => {
    const timeout = globalThis.setTimeout(
      () => reject(new Error("Timeout starting Vite server")),
      10000,
    );

    viteServer!.stdout!.on("data", (data: Buffer) => {
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
    await wsServer.close();
    wsServer = null;
  }
});

test("browser can connect to TypeScript WebSocket server and call echo methods", async ({ page }) => {
  page.on("console", (msg) => {
    console.log(`[browser ${msg.type()}] ${msg.text()}`);
  });

  page.on("pageerror", (err) => {
    console.log(`[browser pageerror] ${err.message}`);
  });

  page.on("requestfailed", (req) => {
    console.log(`[browser requestfailed] ${req.url()} - ${req.failure()?.errorText}`);
  });

  await page.goto(`http://127.0.0.1:${vitePort}/?ws=ws://127.0.0.1:${wsPort}`, {
    waitUntil: "networkidle",
  });

  await page.waitForFunction(() => (window as any).testsComplete === true, { timeout: 10000 });
  const results = await page.evaluate(() => (window as any).testResults);

  expect(results).toBeInstanceOf(Array);
  expect(results.length).toBeGreaterThan(0);
  for (const result of results) {
    expect(result.passed, `Test "${result.name}" failed: ${result.error}`).toBe(true);
  }
});

test("browser reconnects and resumes an in-flight call against a TypeScript WebSocket server", async ({ page }) => {
  page.on("console", (msg) => {
    console.log(`[browser ${msg.type()}] ${msg.text()}`);
  });

  page.on("pageerror", (err) => {
    console.log(`[browser pageerror] ${err.message}`);
  });

  page.on("requestfailed", (req) => {
    console.log(`[browser requestfailed] ${req.url()} - ${req.failure()?.errorText}`);
  });

  await page.goto(
    `http://127.0.0.1:${vitePort}/?ws=ws://127.0.0.1:${wsPort}&scenario=reconnect`,
    { waitUntil: "networkidle" },
  );

  await page.waitForFunction(() => (window as any).testsComplete === true, { timeout: 10000 });
  const results = await page.evaluate(() => (window as any).testResults);

  expect(results).toBeInstanceOf(Array);
  expect(results.length).toBeGreaterThan(0);
  for (const result of results) {
    expect(result.passed, `Test "${result.name}" failed: ${result.error}`).toBe(true);
  }
});
