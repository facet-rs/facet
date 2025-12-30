//! Language mapping conformance tests.
//!
//! Tests for spec rules in language-mappings.md

use crate::harness::Peer;
use crate::testcase::TestResult;
use rapace_spec_tester_macros::conformance;

// =============================================================================
// langmap.semantic
// =============================================================================
// Rules: [verify langmap.semantic]
//
// Types MUST preserve the same encoding/decoding behavior across all languages.

#[conformance(name = "langmap.semantic", rules = "langmap.semantic")]
pub async fn semantic(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// langmap.idiomatic
// =============================================================================
// Rules: [verify langmap.idiomatic]
//
// Generated code SHOULD follow target language conventions.

#[conformance(name = "langmap.idiomatic", rules = "langmap.idiomatic")]
pub async fn idiomatic(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// langmap.roundtrip
// =============================================================================
// Rules: [verify langmap.roundtrip]
//
// A value encoded in one language MUST decode identically in another.

#[conformance(name = "langmap.roundtrip", rules = "langmap.roundtrip")]
pub async fn roundtrip(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// langmap.lossy
// =============================================================================
// Rules: [verify langmap.lossy]
//
// Lossy mappings (e.g., i128 → bigint) MUST be documented.

#[conformance(name = "langmap.lossy", rules = "langmap.lossy")]
pub async fn lossy(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// langmap.java_unsigned
// =============================================================================
// Rules: [verify langmap.java.unsigned]
//
// Java lacks unsigned types, so u8/u16 use wider signed types.

#[conformance(name = "langmap.java_unsigned", rules = "langmap.java.unsigned")]
pub async fn java_unsigned(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// langmap.usize_prohibited
// =============================================================================
// Rules: [verify langmap.usize.prohibited]
//
// usize/isize are prohibited in public service APIs.

#[conformance(name = "langmap.usize_prohibited", rules = "langmap.usize.prohibited")]
pub async fn usize_prohibited(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// langmap.i128_swift
// =============================================================================
// Rules: [verify langmap.i128.swift]
//
// Swift Int64/UInt64 cannot represent full i128/u128 range.

#[conformance(name = "langmap.i128_swift", rules = "langmap.i128.swift")]
pub async fn i128_swift(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// langmap.enum_discriminant
// =============================================================================
// Rules: [verify langmap.enum.discriminant]
//
// Enum discriminants MUST be declaration order, NOT #[repr] values.

#[conformance(
    name = "langmap.enum_discriminant",
    rules = "langmap.enum.discriminant"
)]
pub async fn enum_discriminant(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}
