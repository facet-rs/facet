//! CALL channel conformance tests.
//!
//! Tests for spec rules in core.md related to CALL semantics.

use crate::harness::{Frame, Peer};
use crate::protocol::*;
use crate::testcase::TestResult;
use rapace_spec_tester_macros::conformance;

/// Helper to complete handshake as acceptor (wait for Hello, send Hello response).
async fn do_handshake_as_acceptor(peer: &mut Peer) -> Result<(), String> {
    let frame = peer
        .recv()
        .await
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

    let payload = facet_postcard::to_vec(&response).map_err(|e| e.to_string())?;

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

    peer.send(&frame).await.map_err(|e| e.to_string())?;
    Ok(())
}

/// Backwards compat alias
async fn do_handshake(peer: &mut Peer) -> Result<(), String> {
    do_handshake_as_acceptor(peer).await
}

/// Helper to complete handshake as initiator (send Hello, wait for Hello response).
async fn do_handshake_as_initiator(peer: &mut Peer) -> Result<(), String> {
    let hello = Hello {
        protocol_version: PROTOCOL_VERSION_1_0,
        role: Role::Initiator,
        required_features: 0,
        supported_features: features::ATTACHED_STREAMS | features::CALL_ENVELOPE,
        limits: Limits::default(),
        methods: vec![],
        params: Vec::new(),
    };

    let payload = facet_postcard::to_vec(&hello).map_err(|e| e.to_string())?;

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

    peer.send(&frame).await.map_err(|e| e.to_string())?;

    // Wait for Hello response
    let frame = peer
        .recv()
        .await
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
pub async fn one_req_one_resp(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// call.request_flags
// =============================================================================
// Rules: [verify core.call.request.flags]
//
// Request frames must have DATA | EOS.

#[conformance(name = "call.request_flags", rules = "core.call.request.flags")]
pub async fn request_flags(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// call.response_flags
// =============================================================================
// Rules: [verify core.call.response.flags]
//
// Response frames must have DATA | EOS | RESPONSE.

#[conformance(name = "call.response_flags", rules = "core.call.response.flags")]
pub async fn response_flags(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
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
pub async fn response_msg_id_echo(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
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
pub async fn error_flag_match(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// call.unknown_method
// =============================================================================
// Rules: [verify core.method-id.unknown-method]
//
// Unknown method_id should return UNIMPLEMENTED.

#[conformance(name = "call.unknown_method", rules = "core.method-id.unknown-method")]
pub async fn unknown_method(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// core.call.request.method_id
// =============================================================================
// Rules: [verify core.call.request.method-id]
//
// Request frames MUST include the method_id field.

#[conformance(name = "call.request_method_id", rules = "core.call.request.method-id")]
pub async fn request_method_id(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
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
pub async fn response_method_id_must_match(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// core.call.request.payload
// =============================================================================
// Rules: [verify core.call.request.payload]
//
// Request payload contains serialized method arguments.

#[conformance(name = "call.request_payload", rules = "core.call.request.payload")]
pub async fn request_payload(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// core.call.response.payload
// =============================================================================
// Rules: [verify core.call.response.payload]
//
// Response payload contains CallResult envelope.

#[conformance(name = "call.response_payload", rules = "core.call.response.payload")]
pub async fn response_payload(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// core.call.complete
// =============================================================================
// Rules: [verify core.call.complete]
//
// A CALL is complete after response with DATA | EOS | RESPONSE.

#[conformance(name = "call.call_complete", rules = "core.call.complete")]
pub async fn call_complete(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// core.call.optional_ports
// =============================================================================
// Rules: [verify core.call.optional-ports]
//
// Ports 1-100 on a CALL are optional client-to-server streams.

#[conformance(name = "call.call_optional_ports", rules = "core.call.optional-ports")]
pub async fn call_optional_ports(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
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
pub async fn call_required_port_missing(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}
