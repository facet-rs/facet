//! Data model conformance tests.
//!
//! Tests for spec rules in data-model.md

use crate::harness::Peer;
use crate::testcase::TestResult;
use rapace_spec_tester_macros::conformance;

// =============================================================================
// data.determinism_map_order
// =============================================================================
// Rules: [verify data.determinism.map-order]
//
// Map encoding is NOT canonical. Implementations MUST NOT rely on byte-for-byte equality.

#[conformance(
    name = "data.determinism_map_order",
    rules = "data.determinism.map-order"
)]
pub async fn determinism_map_order(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// data.float_encoding
// =============================================================================
// Rules: [verify data.float.encoding]
//
// Floating-point types MUST be encoded as IEEE 754 little-endian bit patterns.

#[conformance(name = "data.float_encoding", rules = "data.float.encoding")]
pub async fn float_encoding(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// data.float_nan_canonicalization
// =============================================================================
// Rules: [verify data.float.nan-canonicalization]
//
// All NaN values MUST be canonicalized to quiet NaN with all-zero payload.

#[conformance(
    name = "data.float_nan_canonicalization",
    rules = "data.float.nan-canonicalization"
)]
pub async fn float_nan_canonicalization(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// data.float_negative_zero
// =============================================================================
// Rules: [verify data.float.negative-zero]
//
// Negative zero and positive zero MUST be encoded as distinct bit patterns.

#[conformance(name = "data.float_negative_zero", rules = "data.float.negative-zero")]
pub async fn float_negative_zero(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// data.service_facet_required
// =============================================================================
// Rules: [verify data.service.facet-required]
//
// All argument and return types MUST implement Facet.

#[conformance(
    name = "data.service_facet_required",
    rules = "data.service.facet-required"
)]
pub async fn service_facet_required(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// data.type_system_additional
// =============================================================================
// Rules: [verify data.type-system.additional]
//
// Additional types MAY be supported but are not part of the stable API.

#[conformance(
    name = "data.type_system_additional",
    rules = "data.type-system.additional"
)]
pub async fn type_system_additional(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// data.unsupported_borrowed_return
// =============================================================================
// Rules: [verify data.unsupported.borrowed-return]
//
// Borrowed types in return position MUST NOT be used.

#[conformance(
    name = "data.unsupported_borrowed_return",
    rules = "data.unsupported.borrowed-return"
)]
pub async fn unsupported_borrowed_return(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// data.unsupported_pointers
// =============================================================================
// Rules: [verify data.unsupported.pointers]
//
// Raw pointers MUST NOT be used; they are not serializable.

#[conformance(
    name = "data.unsupported_pointers",
    rules = "data.unsupported.pointers"
)]
pub async fn unsupported_pointers(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// data.unsupported_self_ref
// =============================================================================
// Rules: [verify data.unsupported.self-ref]
//
// Self-referential types MUST NOT be used; not supported by Postcard.

#[conformance(
    name = "data.unsupported_self_ref",
    rules = "data.unsupported.self-ref"
)]
pub async fn unsupported_self_ref(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// data.unsupported_unions
// =============================================================================
// Rules: [verify data.unsupported.unions]
//
// Untagged unions MUST NOT be used; not supported by Postcard.

#[conformance(name = "data.unsupported_unions", rules = "data.unsupported.unions")]
pub async fn unsupported_unions(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// data.unsupported_usize
// =============================================================================
// Rules: [verify data.unsupported.usize]
//
// usize and isize MUST NOT be used in public service APIs.

#[conformance(name = "data.unsupported_usize", rules = "data.unsupported.usize")]
pub async fn unsupported_usize(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// data.wire_field_order
// =============================================================================
// Rules: [verify data.wire.field-order]
//
// Struct fields MUST be encoded in declaration order with no names.

#[conformance(name = "data.wire_field_order", rules = "data.wire.field-order")]
pub async fn wire_field_order(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// data.wire_non_self_describing
// =============================================================================
// Rules: [verify data.wire.non-self-describing]
//
// The wire format MUST NOT encode type information.

#[conformance(
    name = "data.wire_non_self_describing",
    rules = "data.wire.non-self-describing"
)]
pub async fn wire_non_self_describing(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}
