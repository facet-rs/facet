//! STREAM channel conformance tests.
//!
//! Tests for spec rules related to STREAM channels.

use crate::harness::Peer;
use crate::testcase::TestResult;
use rapace_spec_tester_macros::conformance;

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
pub async fn method_id_zero(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
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
pub async fn attachment_required(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// stream.direction_values
// =============================================================================
// Rules: [verify core.stream.bidir]
//
// Direction enum should have correct values.

#[conformance(name = "stream.direction_values", rules = "core.stream.bidir")]
pub async fn direction_values(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// stream.ordering
// =============================================================================
// Rules: [verify core.stream.ordering]
//
// Stream items are delivered in order.

#[conformance(name = "stream.ordering", rules = "core.stream.ordering")]
pub async fn ordering(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// stream.channel_kind
// =============================================================================
// Rules: [verify core.channel.kind]
//
// ChannelKind::Stream should have correct value.

#[conformance(name = "stream.channel_kind", rules = "core.channel.kind")]
pub async fn channel_kind(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// stream.intro
// =============================================================================
// Rules: [verify core.stream.intro]
//
// A STREAM channel MUST carry a typed sequence of items and MUST be attached
// to a parent CALL channel.

#[conformance(name = "stream.intro", rules = "core.stream.intro")]
pub async fn intro(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// stream.empty
// =============================================================================
// Rules: [verify core.stream.empty]
//
// An empty stream is represented by a single frame with EOS flag and payload_len = 0.

#[conformance(name = "stream.empty", rules = "core.stream.empty")]
pub async fn empty(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// stream.frame_payload
// =============================================================================
// Rules: [verify core.stream.frame.payload]
//
// The payload MUST be a Postcard-encoded item of the stream's declared type T.

#[conformance(name = "stream.frame_payload", rules = "core.stream.frame.payload")]
pub async fn frame_payload(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
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
pub async fn type_enforcement(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// stream.decode_failure
// =============================================================================
// Rules: [verify core.stream.decode-failure]
//
// If payload decoding fails, receiver MUST send CancelChannel with ProtocolViolation.

#[conformance(name = "stream.decode_failure", rules = "core.stream.decode-failure")]
pub async fn decode_failure(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}
