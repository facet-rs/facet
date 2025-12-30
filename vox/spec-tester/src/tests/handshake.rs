//! Handshake conformance tests.
//!
//! Tests for spec rules in handshake.md

use crate::harness::{Frame, Peer};
use crate::protocol::*;
use crate::testcase::TestResult;
use rapace_spec_tester_macros::conformance;

// =============================================================================
// handshake.valid_hello_exchange
// =============================================================================
// Rules: [verify handshake.required], [verify handshake.ordering]
//
// The peer acts as ACCEPTOR. The implementation (INITIATOR) should:
// 1. Send a valid Hello
// 2. Receive our Hello response
// 3. Connection is ready

#[conformance(
    name = "handshake.valid_hello_exchange",
    rules = "handshake.required, handshake.ordering"
)]
pub async fn valid_hello_exchange(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// handshake.missing_hello
// =============================================================================
// Rules: [verify handshake.first-frame], [verify handshake.failure]
//
// The peer acts as ACCEPTOR. If implementation sends non-Hello first frame,
// peer should detect the violation.

#[conformance(
    name = "handshake.missing_hello",
    rules = "handshake.first-frame, handshake.failure"
)]
pub async fn missing_hello(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// handshake.version_mismatch
// =============================================================================
// Rules: [verify handshake.version.major]
//
// Peer sends Hello with incompatible major version.
// Implementation should reject/close.

#[conformance(name = "handshake.version_mismatch", rules = "handshake.version.major")]
pub async fn version_mismatch(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// handshake.role_conflict
// =============================================================================
// Rules: [verify handshake.role.validation]
//
// Peer sends Hello claiming to be INITIATOR (same as implementation).
// Implementation should reject.

#[conformance(name = "handshake.role_conflict", rules = "handshake.role.validation")]
pub async fn role_conflict(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// handshake.required_features_missing
// =============================================================================
// Rules: [verify handshake.features.required]
//
// Peer requires a feature the implementation doesn't support.

#[conformance(
    name = "handshake.required_features_missing",
    rules = "handshake.features.required"
)]
pub async fn required_features_missing(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// handshake.method_registry_duplicate
// =============================================================================
// Rules: [verify handshake.registry.no-duplicates]
//
// Peer sends Hello with duplicate method_id in registry.

#[conformance(
    name = "handshake.method_registry_duplicate",
    rules = "handshake.registry.no-duplicates"
)]
pub async fn method_registry_duplicate(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// handshake.method_registry_zero
// =============================================================================
// Rules: [verify handshake.registry.no-zero]
//
// Peer sends Hello with method_id=0 in registry.

#[conformance(
    name = "handshake.method_registry_zero",
    rules = "handshake.registry.no-zero"
)]
pub async fn method_registry_zero(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// handshake.explicit_required
// =============================================================================
// Rules: [verify handshake.explicit-required]
//
// Explicit handshake is a hard requirement for all compliance levels.

#[conformance(
    name = "handshake.explicit_required",
    rules = "handshake.explicit-required"
)]
pub async fn explicit_required(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// handshake.registry_cross_service
// =============================================================================
// Rules: [verify handshake.registry.cross-service]
//
// Different services with methods that hash to the same method_id are collisions.

#[conformance(
    name = "handshake.registry_cross_service",
    rules = "handshake.registry.cross-service"
)]
pub async fn registry_cross_service(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// handshake.registry_failure
// =============================================================================
// Rules: [verify handshake.registry.failure]
//
// If validation fails, send CloseChannel and close transport.

#[conformance(
    name = "handshake.registry_failure",
    rules = "handshake.registry.failure"
)]
pub async fn registry_failure(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// handshake.timeout
// =============================================================================
// Rules: [verify handshake.timeout]
//
// Implementations MUST impose a handshake timeout.

#[conformance(name = "handshake.timeout", rules = "handshake.timeout")]
pub async fn timeout(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}
