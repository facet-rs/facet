// Browser test client for vox Rust/Wasm
//
// This test loads the Rust-compiled-to-Wasm client and runs it against
// a Rust WebSocket server to verify cross-platform compatibility.

// The wasm module is built by wasm-pack and placed in pkg/
import init, { run_stress, run_tests, TestResults } from '../pkg/wasm_browser_tests.js';

interface StressReport {
  connections: number;
  connected: number;
  totalRequests: number;
  totalErrors: number;
  stuck: number;
  elapsedMs: number;
  firstError?: string;
  allOk: boolean;
}

// Make test results available to Playwright
declare global {
  interface Window {
    testResults: { name: string; passed: boolean; error?: string }[];
    stressSummary: StressReport | null;
    runTests: (wsUrl: string) => Promise<void>;
    runStress: (wsUrl: string, connections: number, durationMs: number) => Promise<void>;
    testsComplete: boolean;
  }
}

window.testResults = [];
window.stressSummary = null;
window.testsComplete = false;

function log(message: string) {
  const status = document.getElementById("status");
  if (status) status.textContent = message;
  console.log(message);
}

function addResult(name: string, passed: boolean, error?: string) {
  const result = { name, passed, error };
  window.testResults.push(result);

  const resultsDiv = document.getElementById("results");
  if (resultsDiv) {
    const div = document.createElement("div");
    div.className = passed ? "pass" : "fail";
    div.textContent = `${passed ? "PASS" : "FAIL"}: ${name}${error ? ` - ${error}` : ""}`;
    resultsDiv.appendChild(div);
  }
}

async function runWasmTests(wsUrl: string): Promise<void> {
  log("Initializing Wasm module...");

  try {
    await init();
    log(`Running Rust/Wasm tests against ${wsUrl}...`);

    const results: TestResults = await run_tests(wsUrl);

    // Extract results from the Wasm struct
    const count = results.count;
    for (let i = 0; i < count; i++) {
      const name = results.get_name(i) ?? "unknown";
      const passed = results.get_passed(i);
      const error = results.get_error(i) ?? undefined;
      addResult(name, passed, error);
    }

    if (results.all_passed()) {
      log("All Rust/Wasm tests passed!");
    } else {
      log("Some Rust/Wasm tests failed.");
    }
  } catch (e) {
    const error = e instanceof Error ? e.message : String(e);
    log(`Error: ${error}`);
    addResult("wasm_init", false, error);
  }

  window.testsComplete = true;
}

async function runWasmStress(wsUrl: string, connections: number, durationMs: number): Promise<void> {
  log("Initializing Wasm module...");

  try {
    await init();
    log(`Stress: ${connections} connections against ${wsUrl} for ${durationMs}ms...`);

    const summary = await run_stress(wsUrl, connections, durationMs);
    window.stressSummary = {
      connections: summary.connections,
      connected: summary.connected,
      totalRequests: summary.total_requests,
      totalErrors: summary.total_errors,
      stuck: summary.stuck,
      elapsedMs: summary.elapsed_ms,
      firstError: summary.get_first_error() ?? undefined,
      allOk: summary.all_ok(),
    };
    summary.free();

    log(
      `Stress done: connected=${window.stressSummary.connected}/${window.stressSummary.connections}, ` +
        `requests=${window.stressSummary.totalRequests}, errors=${window.stressSummary.totalErrors}, ` +
        `stuck=${window.stressSummary.stuck}`,
    );
  } catch (e) {
    const error = e instanceof Error ? e.message : String(e);
    log(`Error: ${error}`);
    window.stressSummary = {
      connections,
      connected: 0,
      totalRequests: 0,
      totalErrors: 1,
      stuck: 0,
      elapsedMs: 0,
      firstError: error,
      allOk: false,
    };
  }

  window.testsComplete = true;
}

window.runTests = runWasmTests;
window.runStress = runWasmStress;

// Auto-run based on URL params: `?stress=1` runs the soak test, otherwise the
// one-shot conformance run.
const urlParams = new URLSearchParams(window.location.search);
const wsUrl = urlParams.get("ws");
if (wsUrl) {
  if (urlParams.get("stress")) {
    const connections = Number(urlParams.get("connections") ?? "8");
    const durationMs = Number(urlParams.get("durationMs") ?? "30000");
    runWasmStress(wsUrl, connections, durationMs);
  } else {
    runWasmTests(wsUrl);
  }
}
