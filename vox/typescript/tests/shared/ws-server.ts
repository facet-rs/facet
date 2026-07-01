import { execFile, spawn, type ChildProcess } from "node:child_process";
import { join } from "node:path";
import { promisify } from "node:util";

const execFileAsync = promisify(execFile);

const wsServerBuildTimeoutMs = 120_000;
const wsServerStartupTimeoutMs = 5_000;

// Default vox peer-server binary: a bare tokio-tungstenite accept loop. The
// axum variant (`axum-peer-server`) is selected by passing `binaryName`.
const defaultBinaryName = "ws-peer-server";

// Build promises are cached per binary name so requesting a different peer
// server (e.g. the axum variant) doesn't return a stale build.
const wsServerBuildPromises = new Map<string, Promise<void>>();
let cargoTargetDirPromise: Promise<string> | null = null;

// `projectRoot` (vox/) may be a standalone Cargo workspace or a subdirectory
// of a larger workspace (as in the facet monorepo), so the `target/` dir
// isn't necessarily directly under it. Ask cargo where it actually is.
function getCargoTargetDir(projectRoot: string): Promise<string> {
  if (!cargoTargetDirPromise) {
    cargoTargetDirPromise = execFileAsync("cargo", ["metadata", "--no-deps", "--format-version", "1"], {
      cwd: projectRoot,
      maxBuffer: 64 * 1024 * 1024,
    }).then(({ stdout }) => {
      const metadata = JSON.parse(stdout) as { target_directory: string };
      return metadata.target_directory;
    });
  }
  return cargoTargetDirPromise;
}

export async function startWsServer(
  projectRoot: string,
  wsPort: number,
  binaryName: string = defaultBinaryName,
): Promise<ChildProcess> {
  const [, targetDir] = await Promise.all([
    buildWsServerBinary(projectRoot, binaryName),
    getCargoTargetDir(projectRoot),
  ]);

  const executableName = process.platform === "win32" ? `${binaryName}.exe` : binaryName;
  const binaryPath = join(targetDir, "debug", executableName);
  const wsServer = spawn(binaryPath, [], {
    cwd: projectRoot,
    env: { ...process.env, WS_PORT: String(wsPort) },
    stdio: ["ignore", "pipe", "pipe"],
  });

  await waitForWsServerReady(wsServer, wsPort);
  return wsServer;
}

function buildWsServerBinary(projectRoot: string, binaryName: string): Promise<void> {
  const cached = wsServerBuildPromises.get(binaryName);
  if (cached) {
    return cached;
  }

  const buildPromise = new Promise<void>((resolve, reject) => {
    const cargoBuild = spawn("cargo", ["build", "-p", "peer-server", "--bin", binaryName], {
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
    wsServerBuildPromises.delete(binaryName);
    throw err;
  });

  wsServerBuildPromises.set(binaryName, buildPromise);
  return buildPromise;
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
