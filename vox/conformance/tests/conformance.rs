//! Conformance tests using libtest-mimic.
//!
//! This test harness runs all conformance tests from the rapace-conformance binary.
//! - Structural tests (frame, method, error, flow, etc.) run directly
//! - Interactive tests (handshake, call, channel, etc.) require an implementation
//!   to communicate with via stdin/stdout

use facet::Facet;
use libtest_mimic::{Arguments, Failed, Trial};
use std::process::Command;

/// Test case from the conformance binary.
#[derive(Facet)]
struct TestCase {
    name: String,
    rules: Vec<String>,
}

/// Test result from the conformance binary.
#[derive(Facet)]
struct TestResult {
    test: String,
    passed: bool,
    error: Option<String>,
}

fn main() {
    let args = Arguments::from_args();

    // Get the path to the conformance binary
    let conformance_bin = env!("CARGO_BIN_EXE_rapace-conformance");

    // List all tests from the binary
    let output = Command::new(conformance_bin)
        .args(["--list", "--format", "json"])
        .output()
        .expect("failed to run conformance binary");

    if !output.status.success() {
        eprintln!(
            "Failed to list tests: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        std::process::exit(1);
    }

    let json_str = String::from_utf8_lossy(&output.stdout);
    let tests: Vec<TestCase> = facet_json::from_str(&json_str).expect("failed to parse test list");

    // Create a Trial for each test
    let trials: Vec<Trial> = tests
        .into_iter()
        .map(|test| {
            let name = test.name.clone();
            let bin_path = conformance_bin.to_string();

            Trial::test(name.clone(), move || run_test(&bin_path, &name))
        })
        .collect();

    libtest_mimic::run(&args, trials).exit();
}

fn run_test(bin_path: &str, test_name: &str) -> Result<(), Failed> {
    let output = Command::new(bin_path)
        .args(["--case", test_name, "--format", "json"])
        .output()
        .map_err(|e| Failed::from(format!("failed to run test: {}", e)))?;

    let json_str = String::from_utf8_lossy(&output.stdout);

    // If the test requires stdin/stdout communication, it will fail with EOF
    // That's expected for interactive tests when run without an implementation
    if !output.status.success() && json_str.is_empty() {
        // Check stderr for hints
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("UnexpectedEof") || stderr.contains("unexpected end") {
            return Err(Failed::from(
                "interactive test requires implementation to communicate via stdin/stdout",
            ));
        }
        return Err(Failed::from(format!(
            "test failed with exit code {:?}: {}",
            output.status.code(),
            stderr
        )));
    }

    let result: TestResult = facet_json::from_str(&json_str)
        .map_err(|e| Failed::from(format!("failed to parse result '{}': {}", json_str, e)))?;

    if result.passed {
        Ok(())
    } else {
        let error = result.error.as_deref().unwrap_or("unknown error");
        Err(Failed::from(error.to_string()))
    }
}
