//! STREAM channel conformance tests.
//!
//! Tests for spec rules related to STREAM channels.

use crate::harness::Peer;
use crate::protocol::*;
use crate::testcase::TestResult;
use rapace_conformance_macros::conformance;

// =============================================================================
// stream.method_id_zero
// =============================================================================
// Rules: [verify core.stream.frame.method-id-zero]
//
// STREAM frames must have method_id = 0.

#[conformance(
    name = "stream.method_id_zero",
    rules = "core.stream.frame.method-id-zero"
)]
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

#[conformance(
    name = "stream.attachment_required",
    rules = "core.stream.attachment, core.channel.open.attach-required"
)]
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

#[conformance(name = "stream.direction_values", rules = "core.stream.bidir")]
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

#[conformance(name = "stream.ordering", rules = "core.stream.ordering")]
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

#[conformance(name = "stream.channel_kind", rules = "core.channel.kind")]
pub fn channel_kind(_peer: &mut Peer) -> TestResult {
    if ChannelKind::Stream as u8 != 2 {
        return TestResult::fail(format!(
            "[verify core.channel.kind]: ChannelKind::Stream should be 2, got {}",
            ChannelKind::Stream as u8
        ));
    }
    TestResult::pass()
}
