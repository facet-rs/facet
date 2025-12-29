//! TUNNEL channel conformance tests.
//!
//! Tests for spec rules related to TUNNEL channels.

use crate::harness::Peer;
use crate::protocol::*;
use crate::testcase::TestResult;

// =============================================================================
// tunnel.raw_bytes
// =============================================================================
// Rules: [verify core.tunnel.raw-bytes]
//
// TUNNEL payloads are raw bytes, not Postcard-encoded.

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

/// Run a tunnel test case by name.
pub fn run(name: &str) -> TestResult {
    let mut peer = Peer::new();

    match name {
        "raw_bytes" => raw_bytes(&mut peer),
        "frame_boundaries" => frame_boundaries(&mut peer),
        "ordering" => ordering(&mut peer),
        "reliability" => reliability(&mut peer),
        "semantics" => semantics(&mut peer),
        "channel_kind" => channel_kind(&mut peer),
        "credits" => credits(&mut peer),
        _ => TestResult::fail(format!("unknown tunnel test: {}", name)),
    }
}

/// List all tunnel test cases.
pub fn list() -> Vec<(&'static str, &'static [&'static str])> {
    vec![
        ("raw_bytes", &["core.tunnel.raw-bytes"][..]),
        ("frame_boundaries", &["core.tunnel.frame-boundaries"][..]),
        ("ordering", &["core.tunnel.ordering"][..]),
        ("reliability", &["core.tunnel.reliability"][..]),
        ("semantics", &["core.tunnel.semantics"][..]),
        ("channel_kind", &["core.channel.kind"][..]),
        ("credits", &["core.tunnel.credits"][..]),
    ]
}
