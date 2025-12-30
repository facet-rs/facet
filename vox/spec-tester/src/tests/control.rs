//! Control message conformance tests.
//!
//! Tests for spec rules related to control channel (channel 0).

use crate::harness::{Frame, Peer};
use crate::protocol::*;
use crate::testcase::TestResult;
use rapace_spec_tester_macros::conformance;

/// Helper to complete handshake.
async fn do_handshake(peer: &mut Peer) -> Result<(), String> {
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
        supported_features: features::ATTACHED_STREAMS
            | features::CALL_ENVELOPE
            | features::RAPACE_PING,
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

    let frame = if payload.len() <= INLINE_PAYLOAD_SIZE {
        Frame::inline(desc, &payload)
    } else {
        Frame::with_payload(desc, payload)
    };

    peer.send(&frame).await.map_err(|e| e.to_string())?;
    Ok(())
}

// =============================================================================
// control.flag_set_on_channel_zero
// =============================================================================
// Rules: [verify core.control.flag-set]
//
// CONTROL flag must be set on channel 0.

#[conformance(
    name = "control.flag_set_on_channel_zero",
    rules = "core.control.flag-set"
)]
pub async fn flag_set_on_channel_zero(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// control.flag_clear_on_other_channels
// =============================================================================
// Rules: [verify core.control.flag-clear]
//
// CONTROL flag must NOT be set on channels other than 0.

#[conformance(
    name = "control.flag_clear_on_other_channels",
    rules = "core.control.flag-clear"
)]
pub async fn flag_clear_on_other_channels(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// control.ping_pong
// =============================================================================
// Rules: [verify core.ping.semantics]
//
// Receiver must respond to Ping with Pong echoing the payload.

#[conformance(name = "control.ping_pong", rules = "core.ping.semantics")]
pub async fn ping_pong(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// control.unknown_reserved_verb
// =============================================================================
// Rules: [verify core.control.unknown-reserved]
//
// Unknown control verb in 0-99 range should trigger GoAway.

#[conformance(
    name = "control.unknown_reserved_verb",
    rules = "core.control.unknown-reserved"
)]
pub async fn unknown_reserved_verb(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// control.unknown_extension_verb
// =============================================================================
// Rules: [verify core.control.unknown-extension]
//
// Unknown control verb in 100+ range should be ignored silently.

#[conformance(
    name = "control.unknown_extension_verb",
    rules = "core.control.unknown-extension"
)]
pub async fn unknown_extension_verb(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// control.goaway_last_channel_id
// =============================================================================
// Rules: [verify core.goaway.last-channel-id]
//
// GoAway.last_channel_id indicates highest channel ID sender will process.

#[conformance(
    name = "control.goaway_last_channel_id",
    rules = "core.goaway.last-channel-id"
)]
pub async fn goaway_last_channel_id(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}
