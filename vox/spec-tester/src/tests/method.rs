//! Method ID conformance tests.
//!
//! Tests for spec rules related to method ID computation.

use crate::harness::Peer;
use crate::testcase::TestResult;
use rapace_spec_tester_macros::conformance;

// =============================================================================
// method.algorithm
// =============================================================================
// Rules: [verify core.method-id.algorithm]
//
// Method IDs use FNV-1a hash folded to 32 bits.

#[conformance(name = "method.algorithm", rules = "core.method-id.algorithm")]
pub async fn algorithm(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// method.input_format
// =============================================================================
// Rules: [verify core.method-id.input-format]
//
// Method ID input is "ServiceName.MethodName".

#[conformance(name = "method.input_format", rules = "core.method-id.input-format")]
pub async fn input_format(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// method.zero_reserved
// =============================================================================
// Rules: [verify core.method-id.zero-reserved]
//
// method_id = 0 is reserved for control/stream/tunnel.

#[conformance(name = "method.zero_reserved", rules = "core.method-id.zero-reserved")]
pub async fn zero_reserved(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// method.collision_detection
// =============================================================================
// Rules: [verify core.method-id.collision-detection]
//
// Implementations should detect method ID collisions at startup.

#[conformance(
    name = "method.collision_detection",
    rules = "core.method-id.collision-detection"
)]
pub async fn collision_detection(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// method.fnv1a_properties
// =============================================================================
// Rules: [verify core.method-id.algorithm]
//
// Verify FNV-1a properties: avalanche effect, bit distribution.

#[conformance(name = "method.fnv1a_properties", rules = "core.method-id.algorithm")]
pub async fn fnv1a_properties(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// method.intro
// =============================================================================
// Rules: [verify core.method-id.intro]
//
// Method IDs MUST be 32-bit identifiers computed as a hash.

#[conformance(name = "method.intro", rules = "core.method-id.intro")]
pub async fn intro(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// method.zero_enforcement
// =============================================================================
// Rules: [verify core.method-id.zero-enforcement]
//
// Code generators MUST check if method_id returns 0 and fail.
// Handshake MUST reject method registry entries with method_id = 0.

#[conformance(
    name = "method.zero_enforcement",
    rules = "core.method-id.zero-enforcement"
)]
pub async fn zero_enforcement(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}
