//! Transport conformance tests.
//!
//! Tests for spec rules related to transport layer.

use crate::harness::Peer;
use crate::testcase::TestResult;
use rapace_spec_tester_macros::conformance;

// =============================================================================
// transport.ordering_single
// =============================================================================
// Rules: [verify transport.ordering.single]
//
// Frames on a single channel are delivered in order.

#[conformance(
    name = "transport.ordering_single",
    rules = "transport.ordering.single"
)]
pub async fn ordering_single(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// transport.ordering_channel
// =============================================================================
// Rules: [verify transport.ordering.channel]
//
// No ordering guarantees across different channels.

#[conformance(
    name = "transport.ordering_channel",
    rules = "transport.ordering.channel"
)]
pub async fn ordering_channel(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// transport.reliable_delivery
// =============================================================================
// Rules: [verify transport.reliable.delivery]
//
// Transport provides reliable delivery.

#[conformance(
    name = "transport.reliable_delivery",
    rules = "transport.reliable.delivery"
)]
pub async fn reliable_delivery(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// transport.framing_boundaries
// =============================================================================
// Rules: [verify transport.framing.boundaries]
//
// Frame boundaries are preserved.

#[conformance(
    name = "transport.framing_boundaries",
    rules = "transport.framing.boundaries"
)]
pub async fn framing_boundaries(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// transport.framing_no_coalesce
// =============================================================================
// Rules: [verify transport.framing.no-coalesce]
//
// Frames must not be coalesced.

#[conformance(
    name = "transport.framing_no_coalesce",
    rules = "transport.framing.no-coalesce"
)]
pub async fn framing_no_coalesce(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// transport.stream_length
// =============================================================================
// Rules: [verify transport.stream.length-match]
//
// For stream transports, payload_len must match actual bytes.

#[conformance(
    name = "transport.stream_length_match",
    rules = "transport.stream.length-match"
)]
pub async fn stream_length_match(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// transport.stream_max_length
// =============================================================================
// Rules: [verify transport.stream.max-length]
//
// Maximum payload length is implementation-defined but at least 64KB.

#[conformance(
    name = "transport.stream_max_length",
    rules = "transport.stream.max-length"
)]
pub async fn stream_max_length(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// transport.shutdown_orderly
// =============================================================================
// Rules: [verify transport.shutdown.orderly]
//
// Orderly shutdown via GoAway.

#[conformance(
    name = "transport.shutdown_orderly",
    rules = "transport.shutdown.orderly"
)]
pub async fn shutdown_orderly(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// transport.stream_validation
// =============================================================================
// Rules: [verify transport.stream.validation]
//
// Receivers MUST enforce validation rules.

#[conformance(
    name = "transport.stream_validation",
    rules = "transport.stream.validation"
)]
pub async fn stream_validation(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// transport.stream_varint_limit
// =============================================================================
// Rules: [verify transport.stream.varint-limit]
//
// Varint length prefix must not exceed 10 bytes.

#[conformance(
    name = "transport.stream_varint_limit",
    rules = "transport.stream.varint-limit"
)]
pub async fn stream_varint_limit(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// transport.stream_varint_canonical
// =============================================================================
// Rules: [verify transport.stream.varint-canonical]
//
// Length prefix MUST be canonical (shortest encoding).

#[conformance(
    name = "transport.stream_varint_canonical",
    rules = "transport.stream.varint-canonical"
)]
pub async fn stream_varint_canonical(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// transport.stream_min_length
// =============================================================================
// Rules: [verify transport.stream.min-length]
//
// Frame length must be at least 64 bytes (descriptor size).

#[conformance(
    name = "transport.stream_min_length",
    rules = "transport.stream.min-length"
)]
pub async fn stream_min_length(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// transport.stream_size_limits
// =============================================================================
// Rules: [verify transport.stream.size-limits]
//
// Frames exceeding max_payload_size + 64 MUST be rejected before allocation.

#[conformance(
    name = "transport.stream_size_limits",
    rules = "transport.stream.size-limits"
)]
pub async fn stream_size_limits(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// transport.backpressure
// =============================================================================
// Rules: [verify transport.backpressure]
//
// Transports SHOULD propagate backpressure.

#[conformance(name = "transport.backpressure", rules = "transport.backpressure")]
pub async fn backpressure(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// transport.buffer_pool
// =============================================================================
// Rules: [verify transport.buffer-pool]
//
// Transports MUST provide a BufferPool for payload allocation.

#[conformance(name = "transport.buffer_pool", rules = "transport.buffer-pool")]
pub async fn buffer_pool(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// transport.keepalive_transport
// =============================================================================
// Rules: [verify transport.keepalive.transport]
//
// Transports SHOULD implement keepalive.

#[conformance(
    name = "transport.keepalive_transport",
    rules = "transport.keepalive.transport"
)]
pub async fn keepalive_transport(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// transport.webtransport_server_requirements
// =============================================================================
// Rules: [verify transport.webtransport.server-requirements]
//
// WebTransport server MUST serve HTTPS, handle handshake, support bidirectional streams.

#[conformance(
    name = "transport.webtransport_server_requirements",
    rules = "transport.webtransport.server-requirements"
)]
pub async fn webtransport_server_requirements(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// transport.webtransport_datagram_restrictions
// =============================================================================
// Rules: [verify transport.webtransport.datagram-restrictions]
//
// Datagrams MUST NOT be used for CALL/TUNNEL; MAY be used for unreliable STREAM.

#[conformance(
    name = "transport.webtransport_datagram_restrictions",
    rules = "transport.webtransport.datagram-restrictions"
)]
pub async fn webtransport_datagram_restrictions(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}
