//! Overload and draining conformance tests.
//!
//! Tests for spec rules in overload.md

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
// overload.limits_response
// =============================================================================
// Rules: [verify overload.limits.response]
//
// When limits exceeded: max_channels -> CancelChannel, max_pending -> RESOURCE_EXHAUSTED

#[conformance(name = "overload.limits_response", rules = "overload.limits.response")]
pub async fn limits_response(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// overload.goaway_existing
// =============================================================================
// Rules: [verify overload.goaway.existing]
//
// Calls on channel_id <= last_channel_id MUST proceed normally after GoAway.

#[conformance(name = "overload.goaway_existing", rules = "overload.goaway.existing")]
pub async fn goaway_existing(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// overload.goaway_new_rejected
// =============================================================================
// Rules: [verify overload.goaway.new-rejected]
//
// OpenChannel with channel_id > last_channel_id MUST receive CancelChannel.

#[conformance(
    name = "overload.goaway_new_rejected",
    rules = "overload.goaway.new-rejected"
)]
pub async fn goaway_new_rejected(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// overload.goaway_no_new
// =============================================================================
// Rules: [verify overload.goaway.no-new]
//
// After sending GoAway, peer MUST NOT open new channels.

#[conformance(name = "overload.goaway_no_new", rules = "overload.goaway.no-new")]
pub async fn goaway_no_new(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// overload.goaway_drain
// =============================================================================
// Rules: [verify overload.goaway.drain]
//
// Sender MUST close connection after grace period.

#[conformance(name = "overload.goaway_drain", rules = "overload.goaway.drain")]
pub async fn goaway_drain(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// overload.drain_grace_period
// =============================================================================
// Rules: [verify overload.drain.grace-period]
//
// Draining peer SHOULD wait grace period before closing.

#[conformance(
    name = "overload.drain_grace_period",
    rules = "overload.drain.grace-period"
)]
pub async fn drain_grace_period(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// overload.drain_after_grace
// =============================================================================
// Rules: [verify overload.drain.after-grace]
//
// After grace: cancel with DeadlineExceeded, send CloseChannel, close transport.

#[conformance(
    name = "overload.drain_after_grace",
    rules = "overload.drain.after-grace"
)]
pub async fn drain_after_grace(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// overload.retry_retryable
// =============================================================================
// Rules: [verify overload.retry.retryable]
//
// Clients MUST check rapace.retryable trailer; if 0, MUST NOT retry.

#[conformance(name = "overload.retry_retryable", rules = "overload.retry.retryable")]
pub async fn retry_retryable(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// overload.goaway_client_stop
// =============================================================================
// Rules: [verify overload.goaway.client.stop]
//
// When receiving GoAway, clients MUST stop sending new calls on this connection.

#[conformance(
    name = "overload.goaway_client_stop",
    rules = "overload.goaway.client.stop"
)]
pub async fn goaway_client_stop(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// overload.goaway_client_complete
// =============================================================================
// Rules: [verify overload.goaway.client.complete]
//
// Clients MUST allow pending in-flight calls to complete.

#[conformance(
    name = "overload.goaway_client_complete",
    rules = "overload.goaway.client.complete"
)]
pub async fn goaway_client_complete(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// overload.goaway_client_reconnect
// =============================================================================
// Rules: [verify overload.goaway.client.reconnect]
//
// Clients MUST establish a new connection proactively.

#[conformance(
    name = "overload.goaway_client_reconnect",
    rules = "overload.goaway.client.reconnect"
)]
pub async fn goaway_client_reconnect(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// overload.goaway_client_respect
// =============================================================================
// Rules: [verify overload.goaway.client.respect]
//
// Clients MUST NOT flood with retries; MUST respect the drain window.

#[conformance(
    name = "overload.goaway_client_respect",
    rules = "overload.goaway.client.respect"
)]
pub async fn goaway_client_respect(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// overload.retry_retry_after
// =============================================================================
// Rules: [verify overload.retry.retry-after]
//
// Clients MUST wait at least rapace.retry_after_ms before retrying.

#[conformance(
    name = "overload.retry_retry_after",
    rules = "overload.retry.retry-after"
)]
pub async fn retry_retry_after(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}
