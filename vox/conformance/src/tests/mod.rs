//! Conformance test modules.
//!
//! Each module contains tests for a specific area of the spec.
//! Tests are organized by the spec document they validate.
//! All tests are registered via the `#[conformance]` macro and collected via inventory.

pub mod call;
pub mod cancel;
pub mod channel;
pub mod control;
pub mod error;
pub mod flow;
pub mod frame;
pub mod handshake;
pub mod method;
pub mod stream;
pub mod transport;
pub mod tunnel;

use crate::ConformanceTest;
use crate::harness::Peer;
use crate::testcase::TestResult;

/// Run a test case by name (e.g., "handshake.valid_hello_exchange").
///
/// Looks up the test in the inventory of registered tests.
pub fn run(name: &str) -> TestResult {
    for test in inventory::iter::<ConformanceTest> {
        if test.name == name {
            let mut peer = Peer::new();
            return (test.func)(&mut peer);
        }
    }

    TestResult::fail(format!("unknown test: {}", name))
}

/// List all test cases with their rules.
///
/// Returns all tests registered via the `#[conformance]` macro.
pub fn list_all() -> Vec<(String, Vec<&'static str>)> {
    inventory::iter::<ConformanceTest>
        .into_iter()
        .map(|test| (test.name.to_string(), test.rules.to_vec()))
        .collect()
}

/// List test cases for a specific category (e.g., "handshake", "call").
pub fn list_category(category: &str) -> Vec<(String, Vec<&'static str>)> {
    let prefix = format!("{}.", category);
    inventory::iter::<ConformanceTest>
        .into_iter()
        .filter(|test| test.name.starts_with(&prefix))
        .map(|test| (test.name.to_string(), test.rules.to_vec()))
        .collect()
}
