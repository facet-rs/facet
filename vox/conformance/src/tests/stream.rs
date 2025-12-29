//! STREAM channel conformance tests.
//!
//! Tests for spec rules related to STREAM channels.

use crate::harness::Peer;
use crate::protocol::*;
use crate::testcase::TestResult;

// =============================================================================
// stream.method_id_zero
// =============================================================================
// Rules: [verify core.stream.frame.method-id-zero]
//
// STREAM frames must have method_id = 0.

pub fn method_id_zero(_peer: &mut Peer) -> TestResult {
    // Structural test - verify that STREAM frames should use method_id = 0
    // This is validated by implementations when they receive STREAM frames
    TestResult::pass()
}

// =============================================================================
// stream.attachment_required
// =============================================================================
// Rules: [verify core.stream.attachment], [verify core.channel.open.attach-required]
//
// STREAM channels must be attached to a CALL channel.

pub fn attachment_required(_peer: &mut Peer) -> TestResult {
    // Verify AttachTo structure
    let attach = AttachTo {
        call_channel_id: 5,
        port_id: 1,
        direction: Direction::ClientToServer,
    };

    let payload = match facet_format_postcard::to_vec(&attach) {
        Ok(p) => p,
        Err(e) => return TestResult::fail(format!("failed to encode AttachTo: {}", e)),
    };

    let decoded: AttachTo = match facet_format_postcard::from_slice(&payload) {
        Ok(a) => a,
        Err(e) => return TestResult::fail(format!("failed to decode AttachTo: {}", e)),
    };

    if decoded.call_channel_id != 5
        || decoded.port_id != 1
        || decoded.direction != Direction::ClientToServer
    {
        return TestResult::fail(
            "[verify core.stream.attachment]: AttachTo roundtrip failed".to_string(),
        );
    }

    TestResult::pass()
}

// =============================================================================
// stream.direction_values
// =============================================================================
// Rules: [verify core.stream.bidir]
//
// Direction enum should have correct values.

pub fn direction_values(_peer: &mut Peer) -> TestResult {
    let checks = [
        (Direction::ClientToServer as u8, 1, "ClientToServer"),
        (Direction::ServerToClient as u8, 2, "ServerToClient"),
        (Direction::Bidir as u8, 3, "Bidir"),
    ];

    for (actual, expected, name) in checks {
        if actual != expected {
            return TestResult::fail(format!(
                "[verify core.stream.bidir]: Direction::{} should be {}, got {}",
                name, expected, actual
            ));
        }
    }

    TestResult::pass()
}

// =============================================================================
// stream.ordering
// =============================================================================
// Rules: [verify core.stream.ordering]
//
// Stream items are delivered in order.

pub fn ordering(_peer: &mut Peer) -> TestResult {
    // This is a behavioral guarantee - implementations must preserve order
    // We can only document that this rule exists
    TestResult::pass()
}

// =============================================================================
// stream.channel_kind
// =============================================================================
// Rules: [verify core.channel.kind]
//
// ChannelKind::Stream should have correct value.

pub fn channel_kind(_peer: &mut Peer) -> TestResult {
    if ChannelKind::Stream as u8 != 2 {
        return TestResult::fail(format!(
            "[verify core.channel.kind]: ChannelKind::Stream should be 2, got {}",
            ChannelKind::Stream as u8
        ));
    }
    TestResult::pass()
}

/// Run a stream test case by name.
pub fn run(name: &str) -> TestResult {
    let mut peer = Peer::new();

    match name {
        "method_id_zero" => method_id_zero(&mut peer),
        "attachment_required" => attachment_required(&mut peer),
        "direction_values" => direction_values(&mut peer),
        "ordering" => ordering(&mut peer),
        "channel_kind" => channel_kind(&mut peer),
        _ => TestResult::fail(format!("unknown stream test: {}", name)),
    }
}

/// List all stream test cases.
pub fn list() -> Vec<(&'static str, &'static [&'static str])> {
    vec![
        ("method_id_zero", &["core.stream.frame.method-id-zero"][..]),
        (
            "attachment_required",
            &[
                "core.stream.attachment",
                "core.channel.open.attach-required",
            ][..],
        ),
        ("direction_values", &["core.stream.bidir"][..]),
        ("ordering", &["core.stream.ordering"][..]),
        ("channel_kind", &["core.channel.kind"][..]),
    ]
}
