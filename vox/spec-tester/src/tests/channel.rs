//! Channel conformance tests.
//!
//! Tests for spec rules in core.md related to channels.

use crate::harness::{Frame, Peer};
use crate::protocol::*;
use crate::testcase::TestResult;
use rapace_spec_tester_macros::conformance;

/// Helper to complete handshake before channel tests.
async fn do_handshake(peer: &mut Peer) -> Result<(), String> {
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
        supported_features: features::ATTACHED_STREAMS
            | features::CALL_ENVELOPE
            | features::CREDIT_FLOW_CONTROL,
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
// channel.id_zero_reserved
// =============================================================================
// Rules: [verify core.channel.id.zero-reserved]
//
// Channel 0 is reserved for control messages.

#[conformance(
    name = "channel.id_zero_reserved",
    rules = "core.channel.id.zero-reserved"
)]
pub async fn id_zero_reserved(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// channel.parity_initiator_odd
// =============================================================================
// Rules: [verify core.channel.id.parity.initiator]
//
// Initiator must use odd channel IDs.

#[conformance(
    name = "channel.parity_initiator_odd",
    rules = "core.channel.id.parity.initiator"
)]
pub async fn parity_initiator_odd(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// channel.parity_acceptor_even
// =============================================================================
// Rules: [verify core.channel.id.parity.acceptor]
//
// Acceptor must use even channel IDs.

#[conformance(
    name = "channel.parity_acceptor_even",
    rules = "core.channel.id.parity.acceptor"
)]
pub async fn parity_acceptor_even(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// channel.open_required_before_data
// =============================================================================
// Rules: [verify core.channel.open]
//
// Channels must be opened before sending data.

#[conformance(
    name = "channel.open_required_before_data",
    rules = "core.channel.open"
)]
pub async fn open_required_before_data(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// channel.kind_immutable
// =============================================================================
// Rules: [verify core.channel.kind]
//
// Channel kind must not change after open.
// (This is hard to test directly - kind is set at open time)

#[conformance(name = "channel.kind_immutable", rules = "core.channel.kind")]
pub async fn kind_immutable(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// channel.id_allocation_monotonic
// =============================================================================
// Rules: [verify core.channel.id.allocation]
//
// Channel IDs must be allocated monotonically.

#[conformance(
    name = "channel.id_allocation_monotonic",
    rules = "core.channel.id.allocation"
)]
pub async fn id_allocation_monotonic(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// channel.id_no_reuse
// =============================================================================
// Rules: [verify core.channel.id.no-reuse]
//
// Channel IDs must not be reused after close.

#[conformance(name = "channel.id_no_reuse", rules = "core.channel.id.no-reuse")]
pub async fn id_no_reuse(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// channel.lifecycle
// =============================================================================
// Rules: [verify core.channel.lifecycle]
//
// Channels follow: Open -> Active -> HalfClosed -> Closed lifecycle.

#[conformance(name = "channel.lifecycle", rules = "core.channel.lifecycle")]
pub async fn lifecycle(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// channel.close_semantics
// =============================================================================
// Rules: [verify core.close.close-channel-semantics]
//
// CloseChannel is unilateral, no ack required.

#[conformance(
    name = "channel.close_semantics",
    rules = "core.close.close-channel-semantics"
)]
pub async fn close_semantics(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// channel.eos_after_send
// =============================================================================
// Rules: [verify core.eos.after-send]
//
// After sending EOS, sender MUST NOT send more DATA on that channel.

#[conformance(name = "channel.eos_after_send", rules = "core.eos.after-send")]
pub async fn eos_after_send(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// channel.flags_reserved
// =============================================================================
// Rules: [verify core.flags.reserved]
//
// Reserved flags MUST NOT be set; receivers MUST ignore unknown flags.

#[conformance(name = "channel.flags_reserved", rules = "core.flags.reserved")]
pub async fn flags_reserved(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// channel.control_reserved
// =============================================================================
// Rules: [verify core.control.reserved]
//
// Channel 0 is reserved for control messages.

#[conformance(name = "channel.control_reserved", rules = "core.control.reserved")]
pub async fn control_reserved(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// channel.goaway_after_send
// =============================================================================
// Rules: [verify core.goaway.after-send]
//
// After GoAway, sender rejects new OpenChannel with channel_id > last_channel_id.

#[conformance(name = "channel.goaway_after_send", rules = "core.goaway.after-send")]
pub async fn goaway_after_send(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// channel.open_attach_validation
// =============================================================================
// Rules: [verify core.channel.open.attach-validation]
//
// When receiving OpenChannel with attach, validate:
// - call_channel_id exists
// - port_id is declared by method
// - kind matches port's declared kind
// - direction matches expected direction

#[conformance(
    name = "channel.open_attach_validation",
    rules = "core.channel.open.attach-validation"
)]
pub async fn open_attach_validation(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// channel.open_call_validation
// =============================================================================
// Rules: [verify core.channel.open.call-validation]
//
// When receiving OpenChannel without attach (for CALL channels):
// - kind STREAM/TUNNEL without attach is protocol violation
// - max_channels exceeded returns ResourceExhausted
// - Wrong parity channel ID returns ProtocolViolation

#[conformance(
    name = "channel.open_call_validation",
    rules = "core.channel.open.call-validation"
)]
pub async fn open_call_validation(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// channel.open_cancel_on_violation
// =============================================================================
// Rules: [verify core.channel.open.cancel-on-violation]
//
// All CancelChannel responses are sent on channel 0.

#[conformance(
    name = "channel.open_cancel_on_violation",
    rules = "core.channel.open.cancel-on-violation"
)]
pub async fn open_cancel_on_violation(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// channel.open_no_pre_open
// =============================================================================
// Rules: [verify core.channel.open.no-pre-open]
//
// A peer MUST NOT open a channel on behalf of the other side.

#[conformance(
    name = "channel.open_no_pre_open",
    rules = "core.channel.open.no-pre-open"
)]
pub async fn open_no_pre_open(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// channel.open_ownership
// =============================================================================
// Rules: [verify core.channel.open.ownership]
//
// Client opens CALL and client→server ports.
// Server opens server→client ports.

#[conformance(name = "channel.open_ownership", rules = "core.channel.open.ownership")]
pub async fn open_ownership(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// channel.close_full
// =============================================================================
// Rules: [verify core.close.full]
//
// A channel is fully closed when both sides sent EOS or CancelChannel.

#[conformance(name = "channel.close_full", rules = "core.close.full")]
pub async fn close_full(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// channel.close_state_free
// =============================================================================
// Rules: [verify core.close.state-free]
//
// After full close, implementations MAY free channel state.

#[conformance(name = "channel.close_state_free", rules = "core.close.state-free")]
pub async fn close_state_free(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}
