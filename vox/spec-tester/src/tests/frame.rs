//! Frame format conformance tests.
//!
//! Tests for spec rules in frame-format.md
//!
//! These tests validate that frames sent by the implementation conform to the
//! protocol specification. Each test receives frames from the implementation
//! and validates specific aspects of the frame format.

use crate::harness::{Frame, Peer};
use crate::protocol::*;
use crate::testcase::TestResult;
use rapace_spec_tester_macros::conformance;

/// Helper to complete handshake and return the received Hello frame for inspection.
async fn do_handshake_return_hello(peer: &mut Peer) -> Result<Frame, String> {
    // Receive Hello from implementation (initiator)
    let frame = peer
        .recv()
        .await
        .map_err(|e| format!("failed to receive Hello: {}", e))?;

    if frame.desc.channel_id != 0 || frame.desc.method_id != control_verb::HELLO {
        return Err("first frame must be Hello".to_string());
    }

    // Send Hello response as acceptor
    let response = Hello {
        protocol_version: PROTOCOL_VERSION_1_0,
        role: Role::Acceptor,
        required_features: 0,
        supported_features: features::ATTACHED_STREAMS | features::CALL_ENVELOPE,
        limits: Limits::default(),
        methods: Vec::new(),
        params: Vec::new(),
    };

    let payload = facet_postcard::to_vec(&response).map_err(|e| e.to_string())?;

    let mut desc = MsgDescHot::new();
    desc.msg_id = 1;
    desc.channel_id = 0;
    desc.method_id = control_verb::HELLO;
    desc.flags = flags::CONTROL;

    let response_frame = if payload.len() <= INLINE_PAYLOAD_SIZE {
        Frame::inline(desc, &payload)
    } else {
        Frame::with_payload(desc, payload)
    };

    peer.send(&response_frame)
        .await
        .map_err(|e| e.to_string())?;

    Ok(frame)
}

/// Helper to complete handshake.
async fn do_handshake(peer: &mut Peer) -> Result<(), String> {
    do_handshake_return_hello(peer).await?;
    Ok(())
}

// =============================================================================
// frame.descriptor_size
// =============================================================================
// Rules: [verify frame.desc.size], [verify frame.desc.sizeof]
//
// Validates that descriptors are exactly 64 bytes.
// We verify this by receiving a frame and checking the wire format.

#[conformance(
    name = "frame.descriptor_size",
    rules = "frame.desc.size, frame.desc.sizeof"
)]
pub async fn descriptor_size(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// frame.inline_payload_max
// =============================================================================
// Rules: [verify frame.payload.inline]
//
// Inline payloads must be ≤16 bytes.
// We verify by receiving a frame with inline payload and checking the limit.

#[conformance(name = "frame.inline_payload_max", rules = "frame.payload.inline")]
pub async fn inline_payload_max(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// frame.sentinel_inline
// =============================================================================
// Rules: [verify frame.sentinel.values]
//
// payload_slot = 0xFFFFFFFF means inline.
// We verify by receiving a frame and checking if inline payloads use this sentinel.

#[conformance(name = "frame.sentinel_inline", rules = "frame.sentinel.values")]
pub async fn sentinel_inline(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// frame.sentinel_no_deadline
// =============================================================================
// Rules: [verify frame.sentinel.values]
//
// deadline_ns = 0xFFFFFFFFFFFFFFFF means no deadline.
// We verify by receiving frames and checking the deadline field.

#[conformance(name = "frame.sentinel_no_deadline", rules = "frame.sentinel.values")]
pub async fn sentinel_no_deadline(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// frame.encoding_little_endian
// =============================================================================
// Rules: [verify frame.desc.encoding]
//
// Descriptor fields must be little-endian.
// We verify by receiving a frame and checking the raw bytes match little-endian encoding.

#[conformance(name = "frame.encoding_little_endian", rules = "frame.desc.encoding")]
pub async fn encoding_little_endian(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// frame.msg_id_control
// =============================================================================
// Rules: [verify frame.msg-id.control]
//
// Control frames use monotonic msg_id values like any other frame.
// We verify by receiving multiple control frames and checking msg_id increases.

#[conformance(name = "frame.msg_id_control", rules = "frame.msg-id.control")]
pub async fn msg_id_control(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// frame.msg_id_stream_tunnel
// =============================================================================
// Rules: [verify frame.msg-id.stream-tunnel]
//
// STREAM/TUNNEL frames MUST use monotonically increasing msg_id values.
// This test verifies msg_id increases across frames on non-control channels.

#[conformance(
    name = "frame.msg_id_stream_tunnel",
    rules = "frame.msg-id.stream-tunnel"
)]
pub async fn msg_id_stream_tunnel(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// frame.msg_id_scope
// =============================================================================
// Rules: [verify frame.msg-id.scope]
//
// msg_id is scoped per connection (not per channel).
// We verify by receiving frames across different channels and checking msg_id increases globally.

#[conformance(name = "frame.msg_id_scope", rules = "frame.msg-id.scope")]
pub async fn msg_id_scope(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}
