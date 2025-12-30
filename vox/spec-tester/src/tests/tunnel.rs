//! TUNNEL channel conformance tests.
//!
//! Tests for spec rules related to TUNNEL channels.

use crate::harness::Peer;
use crate::testcase::TestResult;
use rapace_spec_tester_macros::conformance;

// =============================================================================
// tunnel.raw_bytes
// =============================================================================
// Rules: [verify core.tunnel.raw-bytes]
//
// TUNNEL payloads are raw bytes, not Postcard-encoded.

#[conformance(name = "tunnel.raw_bytes", rules = "core.tunnel.raw-bytes")]
pub async fn raw_bytes(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// tunnel.frame_boundaries
// =============================================================================
// Rules: [verify core.tunnel.frame-boundaries]
//
// Frame boundaries in TUNNEL are transport artifacts, not semantic.

#[conformance(
    name = "tunnel.frame_boundaries",
    rules = "core.tunnel.frame-boundaries"
)]
pub async fn frame_boundaries(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// tunnel.ordering
// =============================================================================
// Rules: [verify core.tunnel.ordering]
//
// Tunnel data is delivered in order.

#[conformance(name = "tunnel.ordering", rules = "core.tunnel.ordering")]
pub async fn ordering(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// tunnel.reliability
// =============================================================================
// Rules: [verify core.tunnel.reliability]
//
// Tunnel provides reliable delivery (no loss, no duplication).

#[conformance(name = "tunnel.reliability", rules = "core.tunnel.reliability")]
pub async fn reliability(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// tunnel.semantics
// =============================================================================
// Rules: [verify core.tunnel.semantics]
//
// EOS indicates half-close (like TCP FIN).

#[conformance(name = "tunnel.semantics", rules = "core.tunnel.semantics")]
pub async fn semantics(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// tunnel.channel_kind
// =============================================================================
// Rules: [verify core.channel.kind]
//
// ChannelKind::Tunnel should have correct value.

#[conformance(name = "tunnel.channel_kind", rules = "core.channel.kind")]
pub async fn channel_kind(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// tunnel.credits
// =============================================================================
// Rules: [verify core.tunnel.credits]
//
// Tunnels use credit-based flow control like streams.

#[conformance(name = "tunnel.credits", rules = "core.tunnel.credits")]
pub async fn credits(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// tunnel.intro
// =============================================================================
// Rules: [verify core.tunnel.intro]
//
// A TUNNEL channel MUST carry raw bytes and MUST be attached to a parent CALL.

#[conformance(name = "tunnel.intro", rules = "core.tunnel.intro")]
pub async fn intro(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}
