//! Payload encoding conformance tests.
//!
//! Tests for spec rules in payload-encoding.md

use crate::harness::Peer;
use crate::testcase::TestResult;
use rapace_spec_tester_macros::conformance;

// =============================================================================
// payload.encoding_scope
// =============================================================================
// Rules: [verify payload.encoding.scope]
//
// Rapace MUST use Postcard for message payload encoding on CALL and STREAM channels.

#[conformance(name = "payload.encoding_scope", rules = "payload.encoding.scope")]
pub async fn encoding_scope(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// payload.encoding_tunnel_exception
// =============================================================================
// Rules: [verify payload.encoding.tunnel-exception]
//
// TUNNEL channel payloads MUST be raw bytes, NOT Postcard-encoded.

#[conformance(
    name = "payload.encoding_tunnel_exception",
    rules = "payload.encoding.tunnel-exception"
)]
pub async fn encoding_tunnel_exception(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// payload.varint_canonical
// =============================================================================
// Rules: [verify payload.varint.canonical]
//
// Varints MUST be encoded in canonical form: the shortest possible encoding.

#[conformance(name = "payload.varint_canonical", rules = "payload.varint.canonical")]
pub async fn varint_canonical(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// payload.varint_reject_noncanonical
// =============================================================================
// Rules: [verify payload.varint.reject-noncanonical]
//
// Receivers MUST reject non-canonical varints as malformed.

#[conformance(
    name = "payload.varint_reject_noncanonical",
    rules = "payload.varint.reject-noncanonical"
)]
pub async fn varint_reject_noncanonical(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// payload.float_nan
// =============================================================================
// Rules: [verify payload.float.nan]
//
// All NaN values MUST be canonicalized before encoding.

#[conformance(name = "payload.float_nan", rules = "payload.float.nan")]
pub async fn float_nan(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// payload.float_negzero
// =============================================================================
// Rules: [verify payload.float.negzero]
//
// Negative zero MUST NOT be canonicalized and MUST encode as its IEEE 754 bit pattern.

#[conformance(name = "payload.float_negzero", rules = "payload.float.negzero")]
pub async fn float_negzero(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// payload.struct_field_order
// =============================================================================
// Rules: [verify payload.struct.field-order]
//
// Fields MUST be encoded in declaration order, with no field names or tags.

#[conformance(
    name = "payload.struct_field_order",
    rules = "payload.struct.field-order"
)]
pub async fn struct_field_order(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// payload.struct_order_immutable
// =============================================================================
// Rules: [verify payload.struct.order-immutable]
//
// Field order is part of the schema. Reordering fields breaks wire compatibility.

#[conformance(
    name = "payload.struct_order_immutable",
    rules = "payload.struct.order-immutable"
)]
pub async fn struct_order_immutable(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// payload.map_nondeterministic
// =============================================================================
// Rules: [verify payload.map.nondeterministic]
//
// Map encoding is NOT deterministic. Implementations MUST NOT rely on byte-for-byte equality.

#[conformance(
    name = "payload.map_nondeterministic",
    rules = "payload.map.nondeterministic"
)]
pub async fn map_nondeterministic(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// payload.stability_frozen
// =============================================================================
// Rules: [verify payload.stability.frozen]
//
// Rapace freezes the Postcard v1 wire format as specified.

#[conformance(name = "payload.stability_frozen", rules = "payload.stability.frozen")]
pub async fn stability_frozen(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}

// =============================================================================
// payload.stability_canonical
// =============================================================================
// Rules: [verify payload.stability.canonical]
//
// This document is the canonical definition of Rapace payload encoding.

#[conformance(
    name = "payload.stability_canonical",
    rules = "payload.stability.canonical"
)]
pub async fn stability_canonical(peer: &mut Peer) -> TestResult {
    panic!("all the old tests were garbage, we're remaking them all from scratch");
}
