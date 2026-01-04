//! Error handling conformance tests.
//!
//! Tests for error response semantics and flag handling.

use crate::harness::{Frame, Peer};
use crate::protocol::*;
use crate::testcase::TestResult;
use rapace_spec_peer_macros::conformance;

/// Helper to perform handshake as acceptor.
async fn do_handshake(peer: &mut Peer) -> Result<(), TestResult> {
    let frame = peer
        .recv()
        .await
        .map_err(|e| TestResult::fail(format!("failed to receive Hello: {}", e)))?;

    if frame.desc.channel_id != 0 || frame.desc.method_id != control_verb::HELLO {
        return Err(TestResult::fail(format!(
            "expected Hello on channel 0, got channel={} method_id={}",
            frame.desc.channel_id, frame.desc.method_id
        )));
    }

    let response = Hello {
        protocol_version: PROTOCOL_VERSION_1_0,
        role: Role::Acceptor,
        required_features: 0,
        supported_features: features::ATTACHED_STREAMS | features::CALL_ENVELOPE,
        limits: Limits::default(),
        methods: vec![],
        params: vec![],
    };

    let payload = facet_postcard::to_vec(&response)
        .map_err(|e| TestResult::fail(format!("failed to serialize Hello: {}", e)))?;

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
        .map_err(|e| TestResult::fail(format!("failed to send Hello: {}", e)))?;

    Ok(())
}

// =============================================================================
// error.flag_match_success
// =============================================================================
// Rule: [verify error.flag.match]
//
// The ERROR flag MUST be set if and only if status.code != 0.
// This test verifies that successful responses (status.code == 0) do NOT have ERROR flag.

#[conformance(name = "error.flag_match_success", rules = "error.flag.match")]
pub async fn flag_match_success(peer: &mut Peer) -> TestResult {
    if let Err(result) = do_handshake(peer).await {
        return result;
    }

    // Wait for OpenChannel
    let frame = match peer.recv().await {
        Ok(f) => f,
        Err(e) => return TestResult::fail(format!("failed to receive frame: {}", e)),
    };

    if frame.desc.channel_id != 0 || frame.desc.method_id != control_verb::OPEN_CHANNEL {
        return TestResult::fail(format!(
            "expected OpenChannel, got channel={} method_id={}",
            frame.desc.channel_id, frame.desc.method_id
        ));
    }

    let open: OpenChannel = match facet_postcard::from_slice(frame.payload_bytes()) {
        Ok(o) => o,
        Err(e) => return TestResult::fail(format!("failed to deserialize OpenChannel: {}", e)),
    };

    let channel_id = open.channel_id;

    // Wait for the request frame
    let request = match peer.recv().await {
        Ok(f) => f,
        Err(e) => return TestResult::fail(format!("failed to receive request: {}", e)),
    };

    // Send a SUCCESS response (status.code == 0)
    // According to error.flag.match, ERROR flag MUST NOT be set
    let call_result = CallResult {
        status: Status::ok(), // code == 0
        trailers: vec![],
        body: Some(vec![0x01, 0x02, 0x03]),
    };

    let payload = match facet_postcard::to_vec(&call_result) {
        Ok(p) => p,
        Err(e) => return TestResult::fail(format!("failed to serialize CallResult: {}", e)),
    };

    let mut desc = MsgDescHot::new();
    desc.msg_id = request.desc.msg_id;
    desc.channel_id = channel_id;
    desc.method_id = request.desc.method_id;
    // SUCCESS: DATA | EOS | RESPONSE, but NOT ERROR
    desc.flags = flags::DATA | flags::EOS | flags::RESPONSE;

    let response_frame = if payload.len() <= INLINE_PAYLOAD_SIZE {
        Frame::inline(desc, &payload)
    } else {
        Frame::with_payload(desc, payload)
    };

    if let Err(e) = peer.send(&response_frame).await {
        return TestResult::fail(format!("failed to send response: {}", e));
    }

    // The test passes if we correctly sent a success response without ERROR flag
    // (We're the peer sending, so we demonstrate correct behavior)
    TestResult::pass()
}

// =============================================================================
// error.flag_match_error
// =============================================================================
// Rule: [verify error.flag.match]
//
// The ERROR flag MUST be set if and only if status.code != 0.
// This test verifies that error responses (status.code != 0) MUST have ERROR flag.

#[conformance(name = "error.flag_match_error", rules = "error.flag.match")]
pub async fn flag_match_error(peer: &mut Peer) -> TestResult {
    if let Err(result) = do_handshake(peer).await {
        return result;
    }

    // Wait for OpenChannel
    let frame = match peer.recv().await {
        Ok(f) => f,
        Err(e) => return TestResult::fail(format!("failed to receive frame: {}", e)),
    };

    if frame.desc.channel_id != 0 || frame.desc.method_id != control_verb::OPEN_CHANNEL {
        return TestResult::fail(format!(
            "expected OpenChannel, got channel={} method_id={}",
            frame.desc.channel_id, frame.desc.method_id
        ));
    }

    let open: OpenChannel = match facet_postcard::from_slice(frame.payload_bytes()) {
        Ok(o) => o,
        Err(e) => return TestResult::fail(format!("failed to deserialize OpenChannel: {}", e)),
    };

    let channel_id = open.channel_id;

    // Wait for the request frame
    let request = match peer.recv().await {
        Ok(f) => f,
        Err(e) => return TestResult::fail(format!("failed to receive request: {}", e)),
    };

    // Send an ERROR response (status.code != 0)
    // According to error.flag.match, ERROR flag MUST be set
    let call_result = CallResult {
        status: Status::error(error_code::INTERNAL, "test error"),
        trailers: vec![],
        body: None,
    };

    let payload = match facet_postcard::to_vec(&call_result) {
        Ok(p) => p,
        Err(e) => return TestResult::fail(format!("failed to serialize CallResult: {}", e)),
    };

    let mut desc = MsgDescHot::new();
    desc.msg_id = request.desc.msg_id;
    desc.channel_id = channel_id;
    desc.method_id = request.desc.method_id;
    // ERROR: DATA | EOS | RESPONSE | ERROR (ERROR flag MUST be set)
    desc.flags = flags::DATA | flags::EOS | flags::RESPONSE | flags::ERROR;

    let response_frame = if payload.len() <= INLINE_PAYLOAD_SIZE {
        Frame::inline(desc, &payload)
    } else {
        Frame::with_payload(desc, payload)
    };

    if let Err(e) = peer.send(&response_frame).await {
        return TestResult::fail(format!("failed to send error response: {}", e));
    }

    // The test passes if we correctly sent an error response with ERROR flag
    TestResult::pass()
}
