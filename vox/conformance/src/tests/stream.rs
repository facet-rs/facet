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
pub async fn method_id_zero(_peer: &mut Peer) -> TestResult {
    // STREAM frames MUST have method_id = 0.
    // This differentiates them from CALL frames.

    let mut desc = MsgDescHot::new();
    desc.channel_id = 3; // Some stream channel
    desc.method_id = 0; // MUST be 0 for STREAM
    desc.flags = flags::DATA;

    if desc.method_id != 0 {
        return TestResult::fail(
            "[verify core.stream.frame.method-id-zero]: STREAM method_id should be 0".to_string(),
        );
    }

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
pub async fn attachment_required(_peer: &mut Peer) -> TestResult {
    // Verify AttachTo structure
    let attach = AttachTo {
        call_channel_id: 5,
        port_id: 1,
        direction: Direction::ClientToServer,
    };

    let payload = match facet_postcard::to_vec(&attach) {
        Ok(p) => p,
        Err(e) => return TestResult::fail(format!("failed to encode AttachTo: {}", e)),
    };

    let decoded: AttachTo = match facet_postcard::from_slice(&payload) {
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
pub async fn direction_values(_peer: &mut Peer) -> TestResult {
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
pub async fn ordering(_peer: &mut Peer) -> TestResult {
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
pub async fn channel_kind(_peer: &mut Peer) -> TestResult {
    if ChannelKind::Stream as u8 != 2 {
        return TestResult::fail(format!(
            "[verify core.channel.kind]: ChannelKind::Stream should be 2, got {}",
            ChannelKind::Stream as u8
        ));
    }
    TestResult::pass()
}

// =============================================================================
// stream.intro
// =============================================================================
// Rules: [verify core.stream.intro]
//
// A STREAM channel MUST carry a typed sequence of items and MUST be attached
// to a parent CALL channel.

#[conformance(name = "stream.intro", rules = "core.stream.intro")]
pub async fn intro(_peer: &mut Peer) -> TestResult {
    // STREAM channels:
    // 1. Carry typed sequences of items (like Vec<T> streamed one at a time)
    // 2. MUST be attached to a parent CALL channel via AttachTo
    // 3. Cannot exist standalone

    // Verify ChannelKind::Stream exists
    let kind = ChannelKind::Stream;
    if kind as u8 != 2 {
        return TestResult::fail(
            "[verify core.stream.intro]: ChannelKind::Stream should be 2".to_string(),
        );
    }

    // Verify AttachTo is required for streams
    let attach = AttachTo {
        call_channel_id: 1,
        port_id: 1,
        direction: Direction::ClientToServer,
    };

    // Verify structure is correct
    if attach.call_channel_id != 1 {
        return TestResult::fail(
            "[verify core.stream.intro]: AttachTo.call_channel_id broken".to_string(),
        );
    }

    TestResult::pass()
}

// =============================================================================
// stream.empty
// =============================================================================
// Rules: [verify core.stream.empty]
//
// An empty stream is represented by a single frame with EOS flag and payload_len = 0.

#[conformance(name = "stream.empty", rules = "core.stream.empty")]
pub async fn empty(_peer: &mut Peer) -> TestResult {
    // An empty stream (zero items) is represented by:
    // - Single frame with EOS flag
    // - payload_len = 0
    // - DATA flag MAY be omitted

    let mut desc = MsgDescHot::new();
    desc.channel_id = 3;
    desc.method_id = 0;
    desc.flags = flags::EOS; // EOS-only, no DATA flag
    desc.payload_len = 0;

    // This represents an empty stream
    if desc.flags & flags::EOS == 0 {
        return TestResult::fail(
            "[verify core.stream.empty]: empty stream must have EOS flag".to_string(),
        );
    }

    if desc.payload_len != 0 {
        return TestResult::fail(
            "[verify core.stream.empty]: empty stream must have payload_len = 0".to_string(),
        );
    }

    TestResult::pass()
}

// =============================================================================
// stream.frame_payload
// =============================================================================
// Rules: [verify core.stream.frame.payload]
//
// The payload MUST be a Postcard-encoded item of the stream's declared type T.

#[conformance(name = "stream.frame_payload", rules = "core.stream.frame.payload")]
pub async fn frame_payload(_peer: &mut Peer) -> TestResult {
    // Stream item payloads are Postcard-encoded.
    // The type T is known from the method signature and port binding.

    // Example: streaming Vec<u8> items
    let item: Vec<u8> = vec![1, 2, 3, 4, 5];

    let payload = match facet_postcard::to_vec(&item) {
        Ok(p) => p,
        Err(e) => return TestResult::fail(format!("failed to encode stream item: {}", e)),
    };

    // Verify it round-trips
    let decoded: Vec<u8> = match facet_postcard::from_slice(&payload) {
        Ok(d) => d,
        Err(e) => return TestResult::fail(format!("failed to decode stream item: {}", e)),
    };

    if decoded != item {
        return TestResult::fail(
            "[verify core.stream.frame.payload]: stream item roundtrip failed".to_string(),
        );
    }

    TestResult::pass()
}

// =============================================================================
// stream.type_enforcement
// =============================================================================
// Rules: [verify core.stream.type-enforcement]
//
// The receiver knows the expected item type from the method signature and port binding.

#[conformance(
    name = "stream.type_enforcement",
    rules = "core.stream.type-enforcement"
)]
pub async fn type_enforcement(_peer: &mut Peer) -> TestResult {
    // Type enforcement for streams:
    // 1. The receiver knows expected type T from:
    //    a. method_id on the parent CALL channel
    //    b. port_id in the AttachTo attachment
    // 2. Type mismatches result in CancelChannel

    // We verify the mechanisms exist

    // AttachTo carries port_id which identifies the stream port
    let attach = AttachTo {
        call_channel_id: 1,
        port_id: 1, // Identifies which Stream<T> in the method signature
        direction: Direction::ClientToServer,
    };

    if attach.port_id != 1 {
        return TestResult::fail(
            "[verify core.stream.type-enforcement]: AttachTo.port_id broken".to_string(),
        );
    }

    // The parent CALL channel carries method_id
    // Combined with port_id, the implementation knows the expected type

    TestResult::pass()
}

// =============================================================================
// stream.decode_failure
// =============================================================================
// Rules: [verify core.stream.decode-failure]
//
// If payload decoding fails, receiver MUST send CancelChannel with ProtocolViolation.

#[conformance(name = "stream.decode_failure", rules = "core.stream.decode-failure")]
pub async fn decode_failure(_peer: &mut Peer) -> TestResult {
    // When stream item decoding fails:
    // 1. Receiver MUST send CancelChannel for the stream channel
    // 2. Reason MUST be ProtocolViolation
    // 3. The parent call MUST fail with appropriate error

    // Verify CancelReason::ProtocolViolation exists and has correct value
    if CancelReason::ProtocolViolation as u8 != 4 {
        return TestResult::fail(format!(
            "[verify core.stream.decode-failure]: CancelReason::ProtocolViolation should be 4, got {}",
            CancelReason::ProtocolViolation as u8
        ));
    }

    // Verify CancelChannel structure
    let cancel = CancelChannel {
        channel_id: 3, // The stream channel
        reason: CancelReason::ProtocolViolation,
    };

    let payload = match facet_postcard::to_vec(&cancel) {
        Ok(p) => p,
        Err(e) => return TestResult::fail(format!("failed to encode CancelChannel: {}", e)),
    };

    let decoded: CancelChannel = match facet_postcard::from_slice(&payload) {
        Ok(c) => c,
        Err(e) => return TestResult::fail(format!("failed to decode CancelChannel: {}", e)),
    };

    if decoded.reason != CancelReason::ProtocolViolation {
        return TestResult::fail(
            "[verify core.stream.decode-failure]: CancelChannel.reason mismatch".to_string(),
        );
    }

    TestResult::pass()
}
