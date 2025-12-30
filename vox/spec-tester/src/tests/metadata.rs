//! Metadata conventions conformance tests.
//!
//! Tests for spec rules in metadata.md

use crate::harness::Peer;
use crate::testcase::TestResult;
use rapace_spec_tester_macros::conformance;

// =============================================================================
// metadata.key_reserved_prefix
// =============================================================================
// Rules: [verify metadata.key.reserved-prefix]
//
// Keys starting with `rapace.` are reserved for protocol-defined metadata.

#[conformance(
    name = "metadata.key_reserved_prefix",
    rules = "metadata.key.reserved-prefix"
)]
pub async fn key_reserved_prefix(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// metadata.key_format
// =============================================================================
// Rules: [verify metadata.key.format]
//
// Keys MUST be lowercase kebab-case matching `[a-z][a-z0-9]*(-[a-z0-9]+)*`.

#[conformance(name = "metadata.key_format", rules = "metadata.key.format")]
pub async fn key_format(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// metadata.key_lowercase
// =============================================================================
// Rules: [verify metadata.key.lowercase]
//
// Keys MUST be lowercase. Mixed-case or uppercase keys are a protocol error.

#[conformance(name = "metadata.key_lowercase", rules = "metadata.key.lowercase")]
pub async fn key_lowercase(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// metadata.key_case_sensitive
// =============================================================================
// Rules: [verify metadata.key.case-sensitive]
//
// Keys are compared as raw bytes (case-sensitive).

#[conformance(
    name = "metadata.key_case_sensitive",
    rules = "metadata.key.case-sensitive"
)]
pub async fn key_case_sensitive(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// metadata.key_duplicates
// =============================================================================
// Rules: [verify metadata.key.duplicates]
//
// Senders MUST NOT include duplicate keys.

#[conformance(name = "metadata.key_duplicates", rules = "metadata.key.duplicates")]
pub async fn key_duplicates(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// metadata.limits
// =============================================================================
// Rules: [verify metadata.limits]
//
// Implementations MUST enforce size limits.

#[conformance(name = "metadata.limits", rules = "metadata.limits")]
pub async fn limits(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// metadata.limits_reject
// =============================================================================
// Rules: [verify metadata.limits.reject]
//
// Implementations SHOULD reject messages exceeding limits with RESOURCE_EXHAUSTED.

#[conformance(name = "metadata.limits_reject", rules = "metadata.limits.reject")]
pub async fn limits_reject(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}
