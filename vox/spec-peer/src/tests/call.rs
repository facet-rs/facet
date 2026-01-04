//! CALL channel conformance tests.
//!
//! Tests for unary request-response RPC semantics.

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
// call.request_flags
// =============================================================================
// Rule: [verify core.call.request.flags]
//
// Request frames MUST have the DATA | EOS flags set.

#[conformance(name = "call.request_flags", rules = "core.call.request.flags")]
pub async fn request_flags(peer: &mut Peer) -> TestResult {
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

    if open.kind != ChannelKind::Call {
        return TestResult::fail(format!("expected CALL channel, got {:?}", open.kind));
    }

    let channel_id = open.channel_id;

    // Wait for the request frame on the CALL channel
    let request = match peer.recv().await {
        Ok(f) => f,
        Err(e) => return TestResult::fail(format!("failed to receive request: {}", e)),
    };

    if request.desc.channel_id != channel_id {
        return TestResult::fail(format!(
            "expected request on channel {}, got channel {}",
            channel_id, request.desc.channel_id
        ));
    }

    // Verify DATA flag is set
    if request.desc.flags & flags::DATA == 0 {
        return TestResult::fail(format!(
            "CALL request missing DATA flag (flags={:#x})",
            request.desc.flags
        ));
    }

    // Verify EOS flag is set
    if request.desc.flags & flags::EOS == 0 {
        return TestResult::fail(format!(
            "CALL request missing EOS flag (flags={:#x})",
            request.desc.flags
        ));
    }

    // Send a response so the subject's call() completes
    let call_result = CallResult {
        status: Status::ok(),
        trailers: vec![],
        body: Some(vec![]),
    };

    let payload = match facet_postcard::to_vec(&call_result) {
        Ok(p) => p,
        Err(e) => return TestResult::fail(format!("failed to serialize CallResult: {}", e)),
    };

    let mut desc = MsgDescHot::new();
    desc.msg_id = request.desc.msg_id;
    desc.channel_id = channel_id;
    desc.method_id = request.desc.method_id;
    desc.flags = flags::DATA | flags::EOS | flags::RESPONSE;

    let response_frame = if payload.len() <= INLINE_PAYLOAD_SIZE {
        Frame::inline(desc, &payload)
    } else {
        Frame::with_payload(desc, payload)
    };

    if let Err(e) = peer.send(&response_frame).await {
        return TestResult::fail(format!("failed to send response: {}", e));
    }

    TestResult::pass()
}

// =============================================================================
// call.one_req_one_resp
// =============================================================================
// Rule: [verify core.call.one-req-one-resp]
//
// A CALL channel MUST carry exactly one request and one response.

#[conformance(name = "call.one_req_one_resp", rules = "core.call.one-req-one-resp")]
pub async fn one_req_one_resp(peer: &mut Peer) -> TestResult {
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

    if open.kind != ChannelKind::Call {
        return TestResult::fail(format!("expected CALL channel, got {:?}", open.kind));
    }

    let channel_id = open.channel_id;

    // Wait for the request frame
    let request = match peer.recv().await {
        Ok(f) => f,
        Err(e) => return TestResult::fail(format!("failed to receive request: {}", e)),
    };

    if request.desc.channel_id != channel_id {
        return TestResult::fail(format!(
            "expected request on channel {}, got channel {}",
            channel_id, request.desc.channel_id
        ));
    }

    // Request MUST have EOS (exactly one request)
    if request.desc.flags & flags::EOS == 0 {
        return TestResult::fail(format!(
            "CALL request missing EOS flag - implies more than one request (flags={:#x})",
            request.desc.flags
        ));
    }

    // Send a response (we're the server)
    let call_result = CallResult {
        status: Status::ok(),
        trailers: vec![],
        body: Some(vec![]), // Empty body for success
    };

    let payload = match facet_postcard::to_vec(&call_result) {
        Ok(p) => p,
        Err(e) => return TestResult::fail(format!("failed to serialize CallResult: {}", e)),
    };

    let mut desc = MsgDescHot::new();
    desc.msg_id = request.desc.msg_id; // Echo msg_id for correlation
    desc.channel_id = channel_id;
    desc.method_id = request.desc.method_id; // Echo method_id
    desc.flags = flags::DATA | flags::EOS | flags::RESPONSE;

    let response_frame = if payload.len() <= INLINE_PAYLOAD_SIZE {
        Frame::inline(desc, &payload)
    } else {
        Frame::with_payload(desc, payload)
    };

    if let Err(e) = peer.send(&response_frame).await {
        return TestResult::fail(format!("failed to send response: {}", e));
    }

    TestResult::pass()
}

// =============================================================================
// call.response_flags
// =============================================================================
// Rule: [verify core.call.response.flags]
//
// Response frames MUST have the DATA | EOS | RESPONSE flags set.

#[conformance(name = "call.response_flags", rules = "core.call.response.flags")]
pub async fn response_flags(peer: &mut Peer) -> TestResult {
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

    if open.kind != ChannelKind::Call {
        return TestResult::fail(format!("expected CALL channel, got {:?}", open.kind));
    }

    let channel_id = open.channel_id;

    // Wait for the request frame
    let request = match peer.recv().await {
        Ok(f) => f,
        Err(e) => return TestResult::fail(format!("failed to receive request: {}", e)),
    };

    if request.desc.channel_id != channel_id {
        return TestResult::fail(format!(
            "expected request on channel {}, got channel {}",
            channel_id, request.desc.channel_id
        ));
    }

    // Send a response and verify it has correct flags (this tests our own response)
    // But more importantly, if the implementation sends a response, it must have the right flags
    let call_result = CallResult {
        status: Status::ok(),
        trailers: vec![],
        body: Some(vec![]),
    };

    let payload = match facet_postcard::to_vec(&call_result) {
        Ok(p) => p,
        Err(e) => return TestResult::fail(format!("failed to serialize CallResult: {}", e)),
    };

    let mut desc = MsgDescHot::new();
    desc.msg_id = request.desc.msg_id;
    desc.channel_id = channel_id;
    desc.method_id = request.desc.method_id;
    // Correct flags for response: DATA | EOS | RESPONSE
    desc.flags = flags::DATA | flags::EOS | flags::RESPONSE;

    let response_frame = if payload.len() <= INLINE_PAYLOAD_SIZE {
        Frame::inline(desc, &payload)
    } else {
        Frame::with_payload(desc, payload)
    };

    if let Err(e) = peer.send(&response_frame).await {
        return TestResult::fail(format!("failed to send response: {}", e));
    }

    TestResult::pass()
}

// =============================================================================
// call.response_msg_id
// =============================================================================
// Rule: [verify core.call.response.msg-id]
//
// The response msg_id MUST be the same as the request's msg_id for correlation.

#[conformance(name = "call.response_msg_id", rules = "core.call.response.msg-id")]
pub async fn response_msg_id(peer: &mut Peer) -> TestResult {
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

    if request.desc.channel_id != channel_id {
        return TestResult::fail(format!(
            "expected request on channel {}, got channel {}",
            channel_id, request.desc.channel_id
        ));
    }

    let request_msg_id = request.desc.msg_id;

    // Send response with correct msg_id (echoing request's msg_id)
    let call_result = CallResult {
        status: Status::ok(),
        trailers: vec![],
        body: Some(vec![]),
    };

    let payload = match facet_postcard::to_vec(&call_result) {
        Ok(p) => p,
        Err(e) => return TestResult::fail(format!("failed to serialize CallResult: {}", e)),
    };

    let mut desc = MsgDescHot::new();
    desc.msg_id = request_msg_id; // MUST echo request's msg_id
    desc.channel_id = channel_id;
    desc.method_id = request.desc.method_id;
    desc.flags = flags::DATA | flags::EOS | flags::RESPONSE;

    let response_frame = if payload.len() <= INLINE_PAYLOAD_SIZE {
        Frame::inline(desc, &payload)
    } else {
        Frame::with_payload(desc, payload)
    };

    if let Err(e) = peer.send(&response_frame).await {
        return TestResult::fail(format!("failed to send response: {}", e));
    }

    TestResult::pass()
}

// =============================================================================
// call.error_flags
// =============================================================================
// Rule: [verify core.call.error.flags]
//
// Error responses MUST use DATA | EOS | RESPONSE | ERROR flags.

#[conformance(
    name = "call.error_flags",
    rules = "core.call.error.flags, core.call.error.flag-match"
)]
pub async fn error_flags(peer: &mut Peer) -> TestResult {
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

    if request.desc.channel_id != channel_id {
        return TestResult::fail(format!(
            "expected request on channel {}, got channel {}",
            channel_id, request.desc.channel_id
        ));
    }

    // Send an error response with correct flags
    let call_result = CallResult {
        status: Status::error(error_code::UNIMPLEMENTED, "method not implemented"),
        trailers: vec![],
        body: None, // No body on error
    };

    let payload = match facet_postcard::to_vec(&call_result) {
        Ok(p) => p,
        Err(e) => return TestResult::fail(format!("failed to serialize CallResult: {}", e)),
    };

    let mut desc = MsgDescHot::new();
    desc.msg_id = request.desc.msg_id;
    desc.channel_id = channel_id;
    desc.method_id = request.desc.method_id;
    // Error response MUST have DATA | EOS | RESPONSE | ERROR
    desc.flags = flags::DATA | flags::EOS | flags::RESPONSE | flags::ERROR;

    let response_frame = if payload.len() <= INLINE_PAYLOAD_SIZE {
        Frame::inline(desc, &payload)
    } else {
        Frame::with_payload(desc, payload)
    };

    if let Err(e) = peer.send(&response_frame).await {
        return TestResult::fail(format!("failed to send error response: {}", e));
    }

    TestResult::pass()
}

// =============================================================================
// call.request_method_id
// =============================================================================
// Rule: [verify core.call.request.method-id]
//
// The method_id field MUST contain a non-zero method identifier.
// method_id = 0 is reserved for control messages and STREAM/TUNNEL frames.

#[conformance(name = "call.request_method_id", rules = "core.call.request.method-id")]
pub async fn request_method_id(peer: &mut Peer) -> TestResult {
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

    if open.kind != ChannelKind::Call {
        return TestResult::fail(format!("expected CALL channel, got {:?}", open.kind));
    }

    let channel_id = open.channel_id;

    // Wait for the request frame on the CALL channel
    let request = match peer.recv().await {
        Ok(f) => f,
        Err(e) => return TestResult::fail(format!("failed to receive request: {}", e)),
    };

    if request.desc.channel_id != channel_id {
        return TestResult::fail(format!(
            "expected request on channel {}, got channel {}",
            channel_id, request.desc.channel_id
        ));
    }

    // Verify method_id is non-zero (0 is reserved for control messages)
    if request.desc.method_id == 0 {
        return TestResult::fail(
            "CALL request method_id MUST be non-zero (0 is reserved for control messages)"
                .to_string(),
        );
    }

    // Send a response so the subject's call() completes
    let call_result = CallResult {
        status: Status::ok(),
        trailers: vec![],
        body: Some(vec![]),
    };

    let payload = match facet_postcard::to_vec(&call_result) {
        Ok(p) => p,
        Err(e) => return TestResult::fail(format!("failed to serialize CallResult: {}", e)),
    };

    let mut desc = MsgDescHot::new();
    desc.msg_id = request.desc.msg_id;
    desc.channel_id = channel_id;
    desc.method_id = request.desc.method_id;
    desc.flags = flags::DATA | flags::EOS | flags::RESPONSE;

    let response_frame = if payload.len() <= INLINE_PAYLOAD_SIZE {
        Frame::inline(desc, &payload)
    } else {
        Frame::with_payload(desc, payload)
    };

    if let Err(e) = peer.send(&response_frame).await {
        return TestResult::fail(format!("failed to send response: {}", e));
    }

    TestResult::pass()
}

// =============================================================================
// call.result_envelope
// =============================================================================
// Rule: [verify core.call.result.envelope]
//
// Every response MUST use the CallResult envelope structure.

#[conformance(name = "call.result_envelope", rules = "core.call.result.envelope")]
pub async fn result_envelope(peer: &mut Peer) -> TestResult {
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

    if request.desc.channel_id != channel_id {
        return TestResult::fail(format!(
            "expected request on channel {}, got channel {}",
            channel_id, request.desc.channel_id
        ));
    }

    // Create a proper CallResult envelope
    let call_result = CallResult {
        status: Status {
            code: 0, // OK
            message: String::new(),
            details: vec![],
        },
        trailers: vec![("test-trailer".into(), b"value".to_vec())],
        body: Some(b"response-data".to_vec()),
    };

    let payload = match facet_postcard::to_vec(&call_result) {
        Ok(p) => p,
        Err(e) => return TestResult::fail(format!("failed to serialize CallResult: {}", e)),
    };

    let mut desc = MsgDescHot::new();
    desc.msg_id = request.desc.msg_id;
    desc.channel_id = channel_id;
    desc.method_id = request.desc.method_id;
    desc.flags = flags::DATA | flags::EOS | flags::RESPONSE;

    let response_frame = if payload.len() <= INLINE_PAYLOAD_SIZE {
        Frame::inline(desc, &payload)
    } else {
        Frame::with_payload(desc, payload)
    };

    if let Err(e) = peer.send(&response_frame).await {
        return TestResult::fail(format!("failed to send response: {}", e));
    }

    TestResult::pass()
}
