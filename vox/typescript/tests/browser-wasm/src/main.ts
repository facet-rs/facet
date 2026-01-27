// Browser test client for roam Rust/Wasm
//
// This test loads the Rust-compiled-to-Wasm client and runs it against
// a Rust WebSocket server to verify cross-platform compatibility.

// The wasm module is built by wasm-pack and placed in pkg/
import init, { run_tests, TestResults } from '../pkg/wasm_browser_tests.js';

// Make test results available to Playwright
declare global {
  interface Window {
    testResults: { name: string; passed: boolean; error?: string }[];
    runTests: (wsUrl: string) => Promise<void>;
    testsComplete: boolean;
  }
}

window.testResults = [];
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

window.runTests = runWasmTests;

// Auto-run if ws= is in the URL params
const urlParams = new URLSearchParams(window.location.search);
const wsUrl = urlParams.get("ws");
if (wsUrl) {
  runWasmTests(wsUrl);
}
