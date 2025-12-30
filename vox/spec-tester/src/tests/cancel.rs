//! Cancellation conformance tests.
//!
//! Tests for spec rules in cancellation.md

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

    let frame = if payload.len() <= INLINE_PAYLOAD_SIZE {
        Frame::inline(desc, &payload)
    } else {
        Frame::with_payload(desc, payload)
    };

    peer.send(&frame).await.map_err(|e| e.to_string())?;
    Ok(())
}

// =============================================================================
// cancel.idempotent
// =============================================================================
// Rules: [verify cancel.idempotent], [verify core.cancel.idempotent]
//
// Multiple CancelChannel messages for the same channel are harmless.

#[conformance(
    name = "cancel.cancel_idempotent",
    rules = "cancel.idempotent, core.cancel.idempotent"
)]
pub async fn cancel_idempotent(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// cancel.propagation
// =============================================================================
// Rules: [verify core.cancel.propagation], [verify cancel.impl.propagate]
//
// Canceling a CALL channel should cancel attached STREAM/TUNNEL channels.

#[conformance(
    name = "cancel.cancel_propagation",
    rules = "core.cancel.propagation, cancel.impl.propagate"
)]
pub async fn cancel_propagation(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// cancel.deadline_field
// =============================================================================
// Rules: [verify cancel.deadline.field]
//
// deadline_ns field in MsgDescHot should be honored.

#[conformance(name = "cancel.deadline_field", rules = "cancel.deadline.field")]
pub async fn deadline_field(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// cancel.reason_values
// =============================================================================
// Rules: [verify core.cancel.behavior]
//
// CancelReason enum should have correct values.

#[conformance(name = "cancel.reason_values", rules = "core.cancel.behavior")]
pub async fn reason_values(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// cancel.deadline_clock
// =============================================================================
// Rules: [verify cancel.deadline.clock]
//
// Deadlines use monotonic clock nanoseconds.

#[conformance(name = "cancel.deadline_clock", rules = "cancel.deadline.clock")]
pub async fn deadline_clock(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// cancel.deadline_expired
// =============================================================================
// Rules: [verify cancel.deadline.expired]
//
// Expired deadlines should be handled immediately.

#[conformance(name = "cancel.deadline_expired", rules = "cancel.deadline.expired")]
pub async fn deadline_expired(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// cancel.deadline_terminal
// =============================================================================
// Rules: [verify cancel.deadline.terminal]
//
// DEADLINE_EXCEEDED is a terminal error.

#[conformance(name = "cancel.deadline_terminal", rules = "cancel.deadline.terminal")]
pub async fn deadline_terminal(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// cancel.precedence
// =============================================================================
// Rules: [verify cancel.precedence]
//
// CancelChannel takes precedence over EOS.

#[conformance(name = "cancel.cancel_precedence", rules = "cancel.precedence")]
pub async fn cancel_precedence(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// cancel.ordering
// =============================================================================
// Rules: [verify cancel.ordering]
//
// Cancellation is asynchronous with no ordering guarantee.

#[conformance(name = "cancel.cancel_ordering", rules = "cancel.ordering")]
pub async fn cancel_ordering(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// cancel.ordering_handle
// =============================================================================
// Rules: [verify cancel.ordering.handle]
//
// Implementations must handle all ordering cases.

#[conformance(
    name = "cancel.cancel_ordering_handle",
    rules = "cancel.ordering.handle"
)]
pub async fn cancel_ordering_handle(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// cancel.shm_reclaim
// =============================================================================
// Rules: [verify cancel.shm.reclaim]
//
// SHM slots must be freed on cancellation.

#[conformance(name = "cancel.cancel_shm_reclaim", rules = "cancel.shm.reclaim")]
pub async fn cancel_shm_reclaim(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// cancel.impl_support
// =============================================================================
// Rules: [verify cancel.impl.support]
//
// Implementations must support CancelChannel.

#[conformance(name = "cancel.cancel_impl_support", rules = "cancel.impl.support")]
pub async fn cancel_impl_support(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// cancel.impl_idempotent
// =============================================================================
// Rules: [verify cancel.impl.idempotent]
//
// Implementation must handle CancelChannel idempotently.

#[conformance(
    name = "cancel.cancel_impl_idempotent",
    rules = "cancel.impl.idempotent"
)]
pub async fn cancel_impl_idempotent(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// cancel.deadline_exceeded
// =============================================================================
// Rules: [verify cancel.deadline.exceeded]
//
// When deadline exceeded, proper behavior is required.

#[conformance(name = "cancel.deadline_exceeded", rules = "cancel.deadline.exceeded")]
pub async fn deadline_exceeded(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// cancel.deadline_shm
// =============================================================================
// Rules: [verify cancel.deadline.shm]
//
// SHM transports use system monotonic clock directly.

#[conformance(name = "cancel.deadline_shm", rules = "cancel.deadline.shm")]
pub async fn deadline_shm(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// cancel.deadline_stream
// =============================================================================
// Rules: [verify cancel.deadline.stream]
//
// Stream transports compute remaining time.

#[conformance(name = "cancel.deadline_stream", rules = "cancel.deadline.stream")]
pub async fn deadline_stream(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// cancel.deadline_rounding
// =============================================================================
// Rules: [verify cancel.deadline.rounding]
//
// Deadline rounding uses floor division for safety.

#[conformance(name = "cancel.deadline_rounding", rules = "cancel.deadline.rounding")]
pub async fn deadline_rounding(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// cancel.impl_check_deadline
// =============================================================================
// Rules: [verify cancel.impl.check-deadline]
//
// Implementations SHOULD check deadlines before sending requests.

#[conformance(
    name = "cancel.cancel_impl_check_deadline",
    rules = "cancel.impl.check-deadline"
)]
pub async fn cancel_impl_check_deadline(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// cancel.impl_error_response
// =============================================================================
// Rules: [verify cancel.impl.error-response]
//
// Implementations SHOULD send error responses when canceling server-side.

#[conformance(
    name = "cancel.cancel_impl_error_response",
    rules = "cancel.impl.error-response"
)]
pub async fn cancel_impl_error_response(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// cancel.impl_ignore_data
// =============================================================================
// Rules: [verify cancel.impl.ignore-data]
//
// Implementations MAY ignore data frames after CancelChannel.

#[conformance(
    name = "cancel.cancel_impl_ignore_data",
    rules = "cancel.impl.ignore-data"
)]
pub async fn cancel_impl_ignore_data(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// cancel.impl_shm_free
// =============================================================================
// Rules: [verify cancel.impl.shm-free]
//
// Implementations MUST free SHM slots promptly on cancellation.

#[conformance(name = "cancel.cancel_impl_shm_free", rules = "cancel.impl.shm-free")]
pub async fn cancel_impl_shm_free(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}
