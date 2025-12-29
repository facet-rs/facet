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
pub fn run(name: &str) -> TestResult {
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
pub fn list_all() -> Vec<(String, Vec<&'static str>)> {
    let mut all = Vec::new();

    for (name, rules) in handshake::list() {
        all.push((format!("handshake.{}", name), rules.to_vec()));
    }

    for (name, rules) in frame::list() {
        all.push((format!("frame.{}", name), rules.to_vec()));
    }

    for (name, rules) in channel::list() {
        all.push((format!("channel.{}", name), rules.to_vec()));
    }

    for (name, rules) in call::list() {
        all.push((format!("call.{}", name), rules.to_vec()));
    }

    for (name, rules) in control::list() {
        all.push((format!("control.{}", name), rules.to_vec()));
    }

    for (name, rules) in error::list() {
        all.push((format!("error.{}", name), rules.to_vec()));
    }

    for (name, rules) in cancel::list() {
        all.push((format!("cancel.{}", name), rules.to_vec()));
    }

    for (name, rules) in flow::list() {
        all.push((format!("flow.{}", name), rules.to_vec()));
    }

    for (name, rules) in stream::list() {
        all.push((format!("stream.{}", name), rules.to_vec()));
    }

    for (name, rules) in tunnel::list() {
        all.push((format!("tunnel.{}", name), rules.to_vec()));
    }

    for (name, rules) in transport::list() {
        all.push((format!("transport.{}", name), rules.to_vec()));
    }

    for (name, rules) in method::list() {
        all.push((format!("method.{}", name), rules.to_vec()));
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
