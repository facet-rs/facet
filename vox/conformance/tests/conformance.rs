//! Conformance tests using libtest-mimic.
//!
//! This test harness discovers all conformance tests from the binary
//! and runs them as individual Rust tests.

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
            let name = test.name;

            let bin_path = conformance_bin.to_string();

            // Structural tests don't need stdin/stdout - they just validate constants
            // Interactive tests (handshake, channel, call, control) need a peer
            let is_structural = name.starts_with("frame.")
                || name.starts_with("error.")
                || name.starts_with("flow.")
                || name.starts_with("stream.")
                || name.starts_with("tunnel.")
                || name.starts_with("transport.")
                || name.starts_with("method.")
                || name.starts_with("cancel.deadline_field")
                || name.starts_with("cancel.reason_values");

            Trial::test(name.clone(), move || {
                if is_structural {
                    run_structural_test(&bin_path, &name)
                } else {
                    // For now, skip interactive tests that need a peer
                    // These will be run by actual implementations
                    Err(Failed::from("skipped: requires peer implementation"))
                }
            })
            .with_ignored_flag(!is_structural) // Mark interactive tests as ignored
        })
        .collect();

    libtest_mimic::run(&args, trials).exit();
}

fn run_structural_test(bin_path: &str, test_name: &str) -> Result<(), Failed> {
    let output = Command::new(bin_path)
        .args(["--case", test_name, "--format", "json"])
        .output()
        .map_err(|e| Failed::from(format!("failed to run test: {}", e)))?;

    let json_str = String::from_utf8_lossy(&output.stdout);
    let result: TestResult = facet_json::from_str(&json_str)
        .map_err(|e| Failed::from(format!("failed to parse result: {}", e)))?;

    if result.passed {
        Ok(())
    } else {
        let error = result.error.as_deref().unwrap_or("unknown error");
        Err(Failed::from(error.to_string()))
    }
}
