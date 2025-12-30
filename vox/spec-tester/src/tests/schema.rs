//! Schema evolution conformance tests.
//!
//! Tests for spec rules in schema-evolution.md

use crate::harness::Peer;
use crate::testcase::TestResult;
use rapace_spec_tester_macros::conformance;

// =============================================================================
// schema.identifier_normalization
// =============================================================================
// Rules: [verify schema.identifier.normalization]
//
// Identifiers MUST be exact UTF-8 byte strings. Case-sensitive. No Unicode normalization.

#[conformance(
    name = "schema.identifier_normalization",
    rules = "schema.identifier.normalization"
)]
pub async fn identifier_normalization(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// schema.hash_algorithm
// =============================================================================
// Rules: [verify schema.hash.algorithm]
//
// The schema hash MUST use BLAKE3 over a canonical serialization.

#[conformance(name = "schema.hash_algorithm", rules = "schema.hash.algorithm")]
pub async fn hash_algorithm(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// schema.encoding_endianness
// =============================================================================
// Rules: [verify schema.encoding.endianness]
//
// All multi-byte integers MUST be encoded as little-endian.

#[conformance(
    name = "schema.encoding_endianness",
    rules = "schema.encoding.endianness"
)]
pub async fn encoding_endianness(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// schema.encoding_lengths
// =============================================================================
// Rules: [verify schema.encoding.lengths]
//
// String lengths and counts MUST be encoded as u32 little-endian.

#[conformance(name = "schema.encoding_lengths", rules = "schema.encoding.lengths")]
pub async fn encoding_lengths(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// schema.encoding_order
// =============================================================================
// Rules: [verify schema.encoding.order]
//
// Fields and variants MUST be serialized in declaration order.

#[conformance(name = "schema.encoding_order", rules = "schema.encoding.order")]
pub async fn encoding_order(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// schema.hash_cross_language
// =============================================================================
// Rules: [verify schema.hash.cross-language]
//
// Code generators for other languages MUST implement the same algorithm.

#[conformance(
    name = "schema.hash_cross_language",
    rules = "schema.hash.cross-language"
)]
pub async fn hash_cross_language(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// schema.compat_check
// =============================================================================
// Rules: [verify schema.compat.check]
//
// Peers MUST check compatibility based on method_id and sig_hash.

#[conformance(name = "schema.compat_check", rules = "schema.compat.check")]
pub async fn compat_check(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// schema.compat_rejection
// =============================================================================
// Rules: [verify schema.compat.rejection]
//
// Client MUST reject incompatible calls with INCOMPATIBLE_SCHEMA.

#[conformance(name = "schema.compat_rejection", rules = "schema.compat.rejection")]
pub async fn compat_rejection(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// schema.collision_detection
// =============================================================================
// Rules: [verify schema.collision.detection]
//
// Code generators MUST detect method_id collisions at build time.

#[conformance(
    name = "schema.collision_detection",
    rules = "schema.collision.detection"
)]
pub async fn collision_detection(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// schema.collision_runtime
// =============================================================================
// Rules: [verify schema.collision.runtime]
//
// Runtime collisions SHALL NOT occur if codegen is correct.

#[conformance(name = "schema.collision_runtime", rules = "schema.collision.runtime")]
pub async fn collision_runtime(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}
