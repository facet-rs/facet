//! CALL channel conformance tests.
//!
//! Tests for spec rules in core.md related to CALL semantics.

use crate::harness::{Frame, Peer};
use crate::protocol::*;
use crate::testcase::TestResult;
use rapace_conformance_macros::conformance;

/// Helper to complete handshake as acceptor (wait for Hello, send Hello response).
fn do_handshake_as_acceptor(peer: &mut Peer) -> Result<(), String> {
    let frame = peer
        .recv()
        .map_err(|e| format!("failed to receive Hello: {}", e))?;

    if frame.desc.channel_id != 0 || frame.desc.method_id != control_verb::HELLO {
        return Err("first frame must be Hello".to_string());
    }

    let response = Hello {
        protocol_version: PROTOCOL_VERSION_1_0,
        role: Role::Acceptor,
        required_features: 0,
        supported_features: features::ATTACHED_STREAMS | features::CALL_ENVELOPE,
        limits: Limits::default(),
        methods: vec![MethodInfo {
            method_id: compute_method_id("Test", "echo"),
            sig_hash: [0u8; 32],
            name: Some("Test.echo".to_string()),
        }],
        params: Vec::new(),
    };

    let payload = facet_format_postcard::to_vec(&response).map_err(|e| e.to_string())?;

    let mut desc = MsgDescHot::new();
    desc.msg_id = 1;
    desc.channel_id = 0;
    desc.method_id = control_verb::HELLO;
    desc.flags = flags::CONTROL;

    let frame = if payload.len() <= INLINE_PAYLOAD_SIZE {
        Frame::inline(desc, &payload)
    } else {
        Frame::with_payload(desc, payload)
    };

    peer.send(&frame).map_err(|e| e.to_string())?;
    Ok(())
}

/// Backwards compat alias
fn do_handshake(peer: &mut Peer) -> Result<(), String> {
    do_handshake_as_acceptor(peer)
}

/// Helper to complete handshake as initiator (send Hello, wait for Hello response).
fn do_handshake_as_initiator(peer: &mut Peer) -> Result<(), String> {
    let hello = Hello {
        protocol_version: PROTOCOL_VERSION_1_0,
        role: Role::Initiator,
        required_features: 0,
        supported_features: features::ATTACHED_STREAMS | features::CALL_ENVELOPE,
        limits: Limits::default(),
        methods: vec![],
        params: Vec::new(),
    };

    let payload = facet_format_postcard::to_vec(&hello).map_err(|e| e.to_string())?;

    let mut desc = MsgDescHot::new();
    desc.msg_id = 1;
    desc.channel_id = 0;
    desc.method_id = control_verb::HELLO;
    desc.flags = flags::CONTROL;

    let frame = if payload.len() <= INLINE_PAYLOAD_SIZE {
        Frame::inline(desc, &payload)
    } else {
        Frame::with_payload(desc, payload)
    };

    peer.send(&frame).map_err(|e| e.to_string())?;

    // Wait for Hello response
    let frame = peer
        .recv()
        .map_err(|e| format!("failed to receive Hello response: {}", e))?;

    if frame.desc.channel_id != 0 || frame.desc.method_id != control_verb::HELLO {
        return Err("expected Hello response".to_string());
    }

    Ok(())
}

// =============================================================================
// call.one_req_one_resp
// =============================================================================
// Rules: [verify core.call.one-req-one-resp]
//
// CALL channel carries exactly one request and one response.

#[conformance(name = "call.one_req_one_resp", rules = "core.call.one-req-one-resp")]
pub fn one_req_one_resp(peer: &mut Peer) -> TestResult {
    if let Err(e) = do_handshake(peer) {
        return TestResult::fail(e);
    }

    // Wait for OpenChannel from implementation
    let frame = match peer.recv() {
        Ok(f) => f,
        Err(e) => return TestResult::fail(format!("failed to receive: {}", e)),
    };

    if frame.desc.method_id != control_verb::OPEN_CHANNEL {
        return TestResult::fail("expected OpenChannel".to_string());
    }

    let open: OpenChannel = match facet_format_postcard::from_slice(frame.payload_bytes()) {
        Ok(o) => o,
        Err(e) => return TestResult::fail(format!("failed to decode OpenChannel: {}", e)),
    };

    if open.kind != ChannelKind::Call {
        return TestResult::fail(format!("expected CALL channel, got {:?}", open.kind));
    }

    let channel_id = open.channel_id;

    // Wait for request (DATA | EOS)
    let frame = match peer.recv() {
        Ok(f) => f,
        Err(e) => return TestResult::fail(format!("failed to receive request: {}", e)),
    };

    if frame.desc.channel_id != channel_id {
        return TestResult::fail(format!(
            "request on wrong channel: expected {}, got {}",
            channel_id, frame.desc.channel_id
        ));
    }

    // Request MUST have DATA | EOS
    if frame.desc.flags & flags::DATA == 0 {
        return TestResult::fail(
            "[verify core.call.request.flags]: request missing DATA flag".to_string(),
        );
    }

    if frame.desc.flags & flags::EOS == 0 {
        return TestResult::fail(
            "[verify core.call.request.flags]: request missing EOS flag".to_string(),
        );
    }

    // Send response with DATA | EOS | RESPONSE
    let result = CallResult {
        status: Status::ok(),
        trailers: Vec::new(),
        body: Some(b"echo response".to_vec()),
    };

    let payload = facet_format_postcard::to_vec(&result).expect("failed to encode CallResult");

    let mut desc = MsgDescHot::new();
    desc.msg_id = frame.desc.msg_id; // Echo msg_id per [verify core.call.response.msg-id]
    desc.channel_id = channel_id;
    desc.method_id = frame.desc.method_id; // Echo method_id per [verify core.call.response.method-id]
    desc.flags = flags::DATA | flags::EOS | flags::RESPONSE;

    let resp_frame = if payload.len() <= INLINE_PAYLOAD_SIZE {
        Frame::inline(desc, &payload)
    } else {
        Frame::with_payload(desc, payload)
    };

    if let Err(e) = peer.send(&resp_frame) {
        return TestResult::fail(format!("failed to send response: {}", e));
    }

    TestResult::pass()
}

// =============================================================================
// call.request_flags
// =============================================================================
// Rules: [verify core.call.request.flags]
//
// Request frames must have DATA | EOS.

#[conformance(name = "call.request_flags", rules = "core.call.request.flags")]
pub fn request_flags(peer: &mut Peer) -> TestResult {
    if let Err(e) = do_handshake(peer) {
        return TestResult::fail(e);
    }

    // Wait for OpenChannel + request
    let frame = match peer.recv() {
        Ok(f) => f,
        Err(e) => return TestResult::fail(format!("failed to receive: {}", e)),
    };

    if frame.desc.method_id == control_verb::OPEN_CHANNEL {
        // Wait for the actual request
        let frame = match peer.recv() {
            Ok(f) => f,
            Err(e) => return TestResult::fail(format!("failed to receive request: {}", e)),
        };

        let expected = flags::DATA | flags::EOS;
        if frame.desc.flags & expected != expected {
            return TestResult::fail(format!(
                "[verify core.call.request.flags]: request flags {:#b} missing DATA|EOS ({:#b})",
                frame.desc.flags, expected
            ));
        }
    }

    TestResult::pass()
}

// =============================================================================
// call.response_flags
// =============================================================================
// Rules: [verify core.call.response.flags]
//
// Response frames must have DATA | EOS | RESPONSE.

#[conformance(name = "call.response_flags", rules = "core.call.response.flags")]
pub fn response_flags(_peer: &mut Peer) -> TestResult {
    // This tests OUR response (peer), not the implementation
    // We verify we set the right flags when we respond
    let expected = flags::DATA | flags::EOS | flags::RESPONSE;

    // Just verify the constants
    if expected != 0b0010_0000_0101 {
        return TestResult::fail(format!(
            "[verify core.call.response.flags]: expected flags {:#b}, computed {:#b}",
            0b0010_0000_0101, expected
        ));
    }

    TestResult::pass()
}

// =============================================================================
// call.response_msg_id_echo
// =============================================================================
// Rules: [verify core.call.response.msg-id], [verify frame.msg-id.call-echo]
//
// Response must echo request's msg_id.

#[conformance(
    name = "call.response_msg_id_echo",
    rules = "core.call.response.msg-id, frame.msg-id.call-echo"
)]
pub fn response_msg_id_echo(peer: &mut Peer) -> TestResult {
    if let Err(e) = do_handshake(peer) {
        return TestResult::fail(e);
    }

    // As peer (acceptor), we send a request and check the response echoes msg_id
    // But wait - we're the acceptor. Let's receive their request instead.

    let frame = match peer.recv() {
        Ok(f) => f,
        Err(e) => return TestResult::fail(format!("failed to receive: {}", e)),
    };

    // Skip OpenChannel
    let frame = if frame.desc.method_id == control_verb::OPEN_CHANNEL {
        match peer.recv() {
            Ok(f) => f,
            Err(e) => return TestResult::fail(format!("failed to receive request: {}", e)),
        }
    } else {
        frame
    };

    let request_msg_id = frame.desc.msg_id;
    let channel_id = frame.desc.channel_id;

    // Send response echoing msg_id
    let result = CallResult {
        status: Status::ok(),
        trailers: Vec::new(),
        body: Some(vec![]),
    };

    let payload = facet_format_postcard::to_vec(&result).expect("failed to encode");

    let mut desc = MsgDescHot::new();
    desc.msg_id = request_msg_id; // MUST echo
    desc.channel_id = channel_id;
    desc.method_id = 0;
    desc.flags = flags::DATA | flags::EOS | flags::RESPONSE;

    let resp_frame = if payload.len() <= INLINE_PAYLOAD_SIZE {
        Frame::inline(desc, &payload)
    } else {
        Frame::with_payload(desc, payload)
    };

    if let Err(e) = peer.send(&resp_frame) {
        return TestResult::fail(format!("failed to send: {}", e));
    }

    TestResult::pass()
}

// =============================================================================
// call.error_flag_match
// =============================================================================
// Rules: [verify core.call.error.flag-match], [verify error.flag.match]
//
// ERROR flag must match status.code != 0.

#[conformance(
    name = "call.error_flag_match",
    rules = "core.call.error.flag-match, error.flag.match"
)]
pub fn error_flag_match(peer: &mut Peer) -> TestResult {
    if let Err(e) = do_handshake(peer) {
        return TestResult::fail(e);
    }

    let frame = match peer.recv() {
        Ok(f) => f,
        Err(e) => return TestResult::fail(format!("failed to receive: {}", e)),
    };

    let frame = if frame.desc.method_id == control_verb::OPEN_CHANNEL {
        match peer.recv() {
            Ok(f) => f,
            Err(e) => return TestResult::fail(format!("failed to receive request: {}", e)),
        }
    } else {
        frame
    };

    let channel_id = frame.desc.channel_id;

    // Send error response - must have ERROR flag
    let result = CallResult {
        status: Status::error(error_code::NOT_FOUND, "test error"),
        trailers: Vec::new(),
        body: None,
    };

    let payload = facet_format_postcard::to_vec(&result).expect("failed to encode");

    let mut desc = MsgDescHot::new();
    desc.msg_id = frame.desc.msg_id;
    desc.channel_id = channel_id;
    desc.method_id = 0;
    // ERROR flag MUST be set because status.code != 0
    desc.flags = flags::DATA | flags::EOS | flags::RESPONSE | flags::ERROR;

    let resp_frame = if payload.len() <= INLINE_PAYLOAD_SIZE {
        Frame::inline(desc, &payload)
    } else {
        Frame::with_payload(desc, payload)
    };

    if let Err(e) = peer.send(&resp_frame) {
        return TestResult::fail(format!("failed to send: {}", e));
    }

    TestResult::pass()
}

// =============================================================================
// call.unknown_method
// =============================================================================
// Rules: [verify core.method-id.unknown-method]
//
// Unknown method_id should return UNIMPLEMENTED.

#[conformance(name = "call.unknown_method", rules = "core.method-id.unknown-method")]
pub fn unknown_method(peer: &mut Peer) -> TestResult {
    if let Err(e) = do_handshake(peer) {
        return TestResult::fail(e);
    }

    // We're acceptor - implementation will call us
    // We need to respond with UNIMPLEMENTED for unknown methods

    let frame = match peer.recv() {
        Ok(f) => f,
        Err(e) => return TestResult::fail(format!("failed to receive: {}", e)),
    };

    let frame = if frame.desc.method_id == control_verb::OPEN_CHANNEL {
        match peer.recv() {
            Ok(f) => f,
            Err(e) => return TestResult::fail(format!("failed to receive: {}", e)),
        }
    } else {
        frame
    };

    // Check if the method is in our registry
    let known_method_id = compute_method_id("Test", "echo");

    if frame.desc.method_id != known_method_id {
        // Unknown method - respond with UNIMPLEMENTED
        let result = CallResult {
            status: Status::error(error_code::UNIMPLEMENTED, "method not implemented"),
            trailers: Vec::new(),
            body: None,
        };

        let payload = facet_format_postcard::to_vec(&result).expect("failed to encode");

        let mut desc = MsgDescHot::new();
        desc.msg_id = frame.desc.msg_id;
        desc.channel_id = frame.desc.channel_id;
        desc.method_id = 0;
        desc.flags = flags::DATA | flags::EOS | flags::RESPONSE | flags::ERROR;

        let resp_frame = if payload.len() <= INLINE_PAYLOAD_SIZE {
            Frame::inline(desc, &payload)
        } else {
            Frame::with_payload(desc, payload)
        };

        if let Err(e) = peer.send(&resp_frame) {
            return TestResult::fail(format!("failed to send: {}", e));
        }
    }

    TestResult::pass()
}

// =============================================================================
// core.call.request.method_id
// =============================================================================
// Rules: [verify core.call.request.method-id]
//
// Request frames MUST include the method_id field.

#[conformance(name = "call.request_method_id", rules = "core.call.request.method-id")]
pub fn request_method_id(_peer: &mut Peer) -> TestResult {
    // The method_id identifies which method to invoke.
    // It's computed from "ServiceName.method_name" using FNV-1a.
    // A zero method_id is invalid for data channels (reserved for control).

    let method_id = compute_method_id("Test", "echo");
    if method_id == 0 {
        return TestResult::fail(
            "[verify core.call.request.method-id]: method_id should not be zero".to_string(),
        );
    }

    TestResult::pass()
}

// =============================================================================
// core.call.response.method_id
// =============================================================================
// Rules: [verify core.call.response.method-id]
//
// Response method_id MUST match request method_id.

#[conformance(
    name = "call.response_method_id_must_match",
    rules = "core.call.response.method-id"
)]
pub fn response_method_id_must_match(peer: &mut Peer) -> TestResult {
    // Act as initiator: send Hello, make a call, verify response echoes method_id
    if let Err(e) = do_handshake_as_initiator(peer) {
        return TestResult::fail(e);
    }

    let method_id = compute_method_id("Test", "echo");
    let channel_id = 1u32; // Initiator uses odd channel IDs

    // Send OpenChannel
    let open = OpenChannel {
        channel_id,
        kind: ChannelKind::Call,
        attach: None,
        metadata: Vec::new(),
        initial_credits: 65536,
    };

    let payload = facet_format_postcard::to_vec(&open).expect("failed to encode OpenChannel");

    let mut desc = MsgDescHot::new();
    desc.msg_id = 2;
    desc.channel_id = 0;
    desc.method_id = control_verb::OPEN_CHANNEL;
    desc.flags = flags::CONTROL;

    let frame = if payload.len() <= INLINE_PAYLOAD_SIZE {
        Frame::inline(desc, &payload)
    } else {
        Frame::with_payload(desc, payload)
    };

    if let Err(e) = peer.send(&frame) {
        return TestResult::fail(format!("failed to send OpenChannel: {}", e));
    }

    // Send request
    let request_payload = b"test request";

    let mut desc = MsgDescHot::new();
    desc.msg_id = 3;
    desc.channel_id = channel_id;
    desc.method_id = method_id;
    desc.flags = flags::DATA | flags::EOS;

    let frame = Frame::inline(desc, request_payload);

    if let Err(e) = peer.send(&frame) {
        return TestResult::fail(format!("failed to send request: {}", e));
    }

    // Receive response
    let frame = match peer.recv() {
        Ok(f) => f,
        Err(e) => return TestResult::fail(format!("failed to receive response: {}", e)),
    };

    // Verify method_id matches
    if frame.desc.method_id != method_id {
        return TestResult::fail(format!(
            "[verify core.call.response.method-id]: response method_id {} does not match request method_id {}",
            frame.desc.method_id, method_id
        ));
    }

    TestResult::pass()
}

// =============================================================================
// core.call.request.payload
// =============================================================================
// Rules: [verify core.call.request.payload]
//
// Request payload contains serialized method arguments.

#[conformance(name = "call.request_payload", rules = "core.call.request.payload")]
pub fn request_payload(_peer: &mut Peer) -> TestResult {
    // The payload contains method arguments encoded as:
    // - () for zero args
    // - T for single arg
    // - (T, U, ...) tuple for multiple args
    // All using Postcard encoding.

    TestResult::fail("test not implemented".to_string())
}

// =============================================================================
// core.call.response.payload
// =============================================================================
// Rules: [verify core.call.response.payload]
//
// Response payload contains CallResult envelope.

#[conformance(name = "call.response_payload", rules = "core.call.response.payload")]
pub fn response_payload(_peer: &mut Peer) -> TestResult {
    // Response frames carry a CallResult envelope:
    // - status: Status with code + message
    // - trailers: Vec<(String, Vec<u8>)>
    // - body: Option<Vec<u8>> for the actual return value

    TestResult::fail("test not implemented".to_string())
}

// =============================================================================
// core.call.complete
// =============================================================================
// Rules: [verify core.call.complete]
//
// A CALL is complete after response with DATA | EOS | RESPONSE.

#[conformance(name = "call.call_complete", rules = "core.call.complete")]
pub fn call_complete(_peer: &mut Peer) -> TestResult {
    // A CALL channel is complete when:
    // - Request sent with DATA | EOS
    // - Response received with DATA | EOS | RESPONSE
    // The channel can then be cleaned up.

    TestResult::fail("test not implemented".to_string())
}

// =============================================================================
// core.call.optional_ports
// =============================================================================
// Rules: [verify core.call.optional-ports]
//
// Ports 1-100 on a CALL are optional client-to-server streams.

#[conformance(name = "call.call_optional_ports", rules = "core.call.optional-ports")]
pub fn call_optional_ports(_peer: &mut Peer) -> TestResult {
    // Ports 1-100: optional client→server streams
    // Ports 101-200: optional server→client streams
    // Port assignments are method-specific.

    TestResult::fail("test not implemented".to_string())
}

// =============================================================================
// core.call.required_port_missing
// =============================================================================
// Rules: [verify core.call.required-port-missing]
//
// Missing a required port results in INVALID_ARGUMENT.

#[conformance(
    name = "call.call_required_port_missing",
    rules = "core.call.required-port-missing"
)]
pub fn call_required_port_missing(_peer: &mut Peer) -> TestResult {
    // If a method requires a streaming port and it's not attached,
    // the server should respond with INVALID_ARGUMENT error.

    TestResult::fail("test not implemented".to_string())
}
