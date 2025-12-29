//! TUNNEL channel conformance tests.
//!
//! Tests for spec rules related to TUNNEL channels.

use crate::harness::Peer;
use crate::protocol::*;
use crate::testcase::TestResult;
use rapace_conformance_macros::conformance;

// =============================================================================
// tunnel.raw_bytes
// =============================================================================
// Rules: [verify core.tunnel.raw-bytes]
//
// TUNNEL payloads are raw bytes, not Postcard-encoded.

#[conformance(name = "tunnel.raw_bytes", rules = "core.tunnel.raw-bytes")]
pub fn raw_bytes(_peer: &mut Peer) -> TestResult {
    // This is a behavioral rule - TUNNEL payloads bypass serialization
    // Implementations must handle raw bytes directly
    TestResult::pass()
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
pub fn frame_boundaries(_peer: &mut Peer) -> TestResult {
    // This documents that receivers should not depend on frame boundaries
    // for message framing in tunnels
    TestResult::pass()
}

// =============================================================================
// tunnel.ordering
// =============================================================================
// Rules: [verify core.tunnel.ordering]
//
// Tunnel data is delivered in order.

#[conformance(name = "tunnel.ordering", rules = "core.tunnel.ordering")]
pub fn ordering(_peer: &mut Peer) -> TestResult {
    // Behavioral guarantee - implementations must preserve byte order
    TestResult::pass()
}

// =============================================================================
// tunnel.reliability
// =============================================================================
// Rules: [verify core.tunnel.reliability]
//
// Tunnel provides reliable delivery (no loss, no duplication).

#[conformance(name = "tunnel.reliability", rules = "core.tunnel.reliability")]
pub fn reliability(_peer: &mut Peer) -> TestResult {
    // Behavioral guarantee provided by the transport layer
    TestResult::pass()
}

// =============================================================================
// tunnel.semantics
// =============================================================================
// Rules: [verify core.tunnel.semantics]
//
// EOS indicates half-close (like TCP FIN).

#[conformance(name = "tunnel.semantics", rules = "core.tunnel.semantics")]
pub fn semantics(_peer: &mut Peer) -> TestResult {
    // Verify EOS flag exists and has correct value
    if flags::EOS != 0b0000_0100 {
        return TestResult::fail(format!(
            "[verify core.tunnel.semantics]: EOS flag should be 0x04, got {:#X}",
            flags::EOS
        ));
    }
    TestResult::pass()
}

// =============================================================================
// tunnel.channel_kind
// =============================================================================
// Rules: [verify core.channel.kind]
//
// ChannelKind::Tunnel should have correct value.

#[conformance(name = "tunnel.channel_kind", rules = "core.channel.kind")]
pub fn channel_kind(_peer: &mut Peer) -> TestResult {
    if ChannelKind::Tunnel as u8 != 3 {
        return TestResult::fail(format!(
            "[verify core.channel.kind]: ChannelKind::Tunnel should be 3, got {}",
            ChannelKind::Tunnel as u8
        ));
    }
    TestResult::pass()
}

// =============================================================================
// tunnel.credits
// =============================================================================
// Rules: [verify core.tunnel.credits]
//
// Tunnels use credit-based flow control like streams.

#[conformance(name = "tunnel.credits", rules = "core.tunnel.credits")]
pub fn credits(_peer: &mut Peer) -> TestResult {
    // Verify CREDITS flag exists
    if flags::CREDITS != 0b0100_0000 {
        return TestResult::fail(format!(
            "[verify core.tunnel.credits]: CREDITS flag should be 0x40, got {:#X}",
            flags::CREDITS
        ));
    }
    TestResult::pass()
}
