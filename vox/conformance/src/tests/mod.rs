//! Conformance test modules.
//!
//! Each module contains tests for a specific area of the spec.
//! Tests are organized by the spec document they validate.

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

/// All test categories.
pub const CATEGORIES: &[&str] = &[
    "handshake",
    "frame",
    "channel",
    "call",
    "control",
    "error",
    "cancel",
    "flow",
    "stream",
    "tunnel",
    "transport",
    "method",
];

/// Run a test case by fully-qualified name (e.g., "handshake.valid_hello_exchange").
///
/// First checks inventory for macro-registered tests, then falls back to manual registration.
pub fn run(name: &str) -> TestResult {
    // First, check inventory for macro-registered tests
    for test in inventory::iter::<ConformanceTest> {
        if test.name == name {
            let mut peer = Peer::new();
            return (test.func)(&mut peer);
        }
    }

    // Fall back to manual registration
    let parts: Vec<&str> = name.splitn(2, '.').collect();
    if parts.len() != 2 {
        return TestResult::fail(format!(
            "invalid test name '{}': expected 'category.test_name'",
            name
        ));
    }

    let (category, test_name) = (parts[0], parts[1]);

    match category {
        "handshake" => handshake::run(test_name),
        "frame" => frame::run(test_name),
        "channel" => channel::run(test_name),
        "call" => call::run(test_name),
        "control" => control::run(test_name),
        "error" => error::run(test_name),
        "cancel" => cancel::run(test_name),
        "flow" => flow::run(test_name),
        "stream" => stream::run(test_name),
        "tunnel" => tunnel::run(test_name),
        "transport" => transport::run(test_name),
        "method" => method::run(test_name),
        _ => TestResult::fail(format!("unknown category: {}", category)),
    }
}

/// List all test cases with their rules.
///
/// Includes both inventory-registered tests (from macros) and manually registered tests.
pub fn list_all() -> Vec<(String, Vec<&'static str>)> {
    let mut all = Vec::new();
    let mut seen = std::collections::HashSet::new();

    // First, collect inventory-registered tests
    for test in inventory::iter::<ConformanceTest> {
        all.push((test.name.to_string(), test.rules.to_vec()));
        seen.insert(test.name);
    }

    // Then add manually registered tests (if not already in inventory)
    for (name, rules) in handshake::list() {
        let full_name = format!("handshake.{}", name);
        if !seen.contains(name) {
            all.push((full_name, rules.to_vec()));
        }
    }

    for (name, rules) in frame::list() {
        let full_name = format!("frame.{}", name);
        if !seen.contains(name) {
            all.push((full_name, rules.to_vec()));
        }
    }

    for (name, rules) in channel::list() {
        let full_name = format!("channel.{}", name);
        if !seen.contains(name) {
            all.push((full_name, rules.to_vec()));
        }
    }

    for (name, rules) in call::list() {
        let full_name = format!("call.{}", name);
        if !seen.contains(name) {
            all.push((full_name, rules.to_vec()));
        }
    }

    for (name, rules) in control::list() {
        let full_name = format!("control.{}", name);
        if !seen.contains(name) {
            all.push((full_name, rules.to_vec()));
        }
    }

    for (name, rules) in error::list() {
        let full_name = format!("error.{}", name);
        if !seen.contains(name) {
            all.push((full_name, rules.to_vec()));
        }
    }

    for (name, rules) in cancel::list() {
        let full_name = format!("cancel.{}", name);
        if !seen.contains(name) {
            all.push((full_name, rules.to_vec()));
        }
    }

    for (name, rules) in flow::list() {
        let full_name = format!("flow.{}", name);
        if !seen.contains(name) {
            all.push((full_name, rules.to_vec()));
        }
    }

    for (name, rules) in stream::list() {
        let full_name = format!("stream.{}", name);
        if !seen.contains(name) {
            all.push((full_name, rules.to_vec()));
        }
    }

    for (name, rules) in tunnel::list() {
        let full_name = format!("tunnel.{}", name);
        if !seen.contains(name) {
            all.push((full_name, rules.to_vec()));
        }
    }

    for (name, rules) in transport::list() {
        let full_name = format!("transport.{}", name);
        if !seen.contains(name) {
            all.push((full_name, rules.to_vec()));
        }
    }

    for (name, rules) in method::list() {
        let full_name = format!("method.{}", name);
        if !seen.contains(name) {
            all.push((full_name, rules.to_vec()));
        }
    }

    all
}

/// List test cases for a specific category.
pub fn list_category(category: &str) -> Vec<(String, Vec<&'static str>)> {
    let tests = match category {
        "handshake" => handshake::list(),
        "frame" => frame::list(),
        "channel" => channel::list(),
        "call" => call::list(),
        "control" => control::list(),
        "error" => error::list(),
        "cancel" => cancel::list(),
        "flow" => flow::list(),
        "stream" => stream::list(),
        "tunnel" => tunnel::list(),
        "transport" => transport::list(),
        "method" => method::list(),
        _ => return Vec::new(),
    };

    tests
        .into_iter()
        .map(|(name, rules)| (format!("{}.{}", category, name), rules.to_vec()))
        .collect()
}
