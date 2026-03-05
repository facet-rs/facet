import { spawn, type ChildProcess } from "node:child_process";
import { join } from "node:path";

const wsServerBuildTimeoutMs = 120_000;
const wsServerStartupTimeoutMs = 5_000;

let wsServerBuildPromise: Promise<void> | null = null;

export async function startWsServer(projectRoot: string, wsPort: number): Promise<ChildProcess> {
  await buildWsServerBinary(projectRoot);

  const binaryName = process.platform === "win32" ? "ws-peer-server.exe" : "ws-peer-server";
  const binaryPath = join(projectRoot, "target", "debug", binaryName);
  const wsServer = spawn(binaryPath, [], {
    cwd: projectRoot,
    env: { ...process.env, WS_PORT: String(wsPort) },
    stdio: ["ignore", "pipe", "pipe"],
  });

  await waitForWsServerReady(wsServer, wsPort);
  return wsServer;
}

function buildWsServerBinary(projectRoot: string): Promise<void> {
  if (wsServerBuildPromise) {
    return wsServerBuildPromise;
  }

  wsServerBuildPromise = new Promise<void>((resolve, reject) => {
    const cargoBuild = spawn("cargo", ["build", "-p", "peer-server", "--bin", "ws-peer-server"], {
      cwd: projectRoot,
      stdio: ["ignore", "pipe", "pipe"],
    });

    const timeout = globalThis.setTimeout(() => {
      cargoBuild.kill("SIGTERM");
      reject(new Error(`Timeout building WS server after ${wsServerBuildTimeoutMs}ms`));
    }, wsServerBuildTimeoutMs);

    cargoBuild.stdout!.on("data", (data: Buffer) => {
      process.stdout.write(data);
    });

    cargoBuild.stderr!.on("data", (data: Buffer) => {
      process.stderr.write(data);
    });

    cargoBuild.once("error", (err) => {
      clearTimeout(timeout);
      reject(err);
    });

    cargoBuild.once("exit", (code, signal) => {
      clearTimeout(timeout);
      if (code === 0) {
        resolve();
        return;
      }

      reject(new Error(`WS server build failed (code=${code}, signal=${signal})`));
    });
  }).catch((err) => {
    wsServerBuildPromise = null;
    throw err;
  });

  return wsServerBuildPromise;
}

function waitForWsServerReady(wsServer: ChildProcess, wsPort: number): Promise<void> {
  return new Promise<void>((resolve, reject) => {
    let stdoutBuffer = "";
    let settled = false;

    const finish = (cb: () => void) => {
      if (settled) {
        return;
      }
      settled = true;
      clearTimeout(timeout);
      wsServer.stdout?.off("data", onStdout);
      wsServer.stderr?.off("data", onStderr);
      wsServer.off("error", onError);
      wsServer.off("exit", onExit);
      cb();
    };

    const fail = (err: Error) => {
      finish(() => {
        wsServer.kill("SIGTERM");
        reject(err);
      });
    };

    const timeout = globalThis.setTimeout(() => {
      fail(new Error(`Timeout starting WS server after ${wsServerStartupTimeoutMs}ms`));
    }, wsServerStartupTimeoutMs);

    const onStdout = (data: Buffer) => {
      process.stdout.write(data);
      stdoutBuffer += data.toString();

      const lines = stdoutBuffer.split(/\r?\n/);
      stdoutBuffer = lines.pop() ?? "";

      if (lines.some((line) => line.trim() === String(wsPort))) {
        finish(resolve);
      }
    };

    const onStderr = (data: Buffer) => {
      process.stderr.write(data);
    };

    const onError = (err: Error) => {
      fail(err);
    };

    const onExit = (code: number | null, signal: NodeJS.Signals | null) => {
      fail(new Error(`WS server exited unexpectedly (code=${code}, signal=${signal})`));
    };

    wsServer.stdout!.on("data", onStdout);
    wsServer.stderr!.on("data", onStderr);
    wsServer.once("error", onError);
    wsServer.once("exit", onExit);
  });
}
