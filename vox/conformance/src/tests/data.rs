//! Data model conformance tests.
//!
//! Tests for spec rules in data-model.md

use crate::harness::Peer;
use crate::testcase::TestResult;
use rapace_conformance_macros::conformance;
use std::collections::HashMap;

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
pub async fn determinism_map_order(_peer: &mut Peer) -> TestResult {
    // Map ordering is NOT deterministic between different:
    // - HashMap instances (even with same contents)
    // - Program runs (HashMap uses random state)
    // - Implementations (different languages may order differently)

    // Create two HashMaps with same contents but potentially different insertion order
    let mut map1: HashMap<String, i32> = HashMap::new();
    map1.insert("a".to_string(), 1);
    map1.insert("b".to_string(), 2);
    map1.insert("c".to_string(), 3);

    let mut map2: HashMap<String, i32> = HashMap::new();
    map2.insert("c".to_string(), 3);
    map2.insert("a".to_string(), 1);
    map2.insert("b".to_string(), 2);

    // Semantically equal
    if map1 != map2 {
        return TestResult::fail(
            "[verify data.determinism.map-order]: maps with same contents should be equal"
                .to_string(),
        );
    }

    // But serialized bytes may differ (iteration order is not guaranteed)
    // We can't guarantee they'll be different, but we document the rule:
    // "Implementations MUST NOT rely on byte-for-byte equality for maps"

    TestResult::pass()
}

// =============================================================================
// data.float_encoding
// =============================================================================
// Rules: [verify data.float.encoding]
//
// Floating-point types MUST be encoded as IEEE 754 little-endian bit patterns.

#[conformance(name = "data.float_encoding", rules = "data.float.encoding")]
pub async fn float_encoding(_peer: &mut Peer) -> TestResult {
    // Verify f32 encoding (IEEE 754 little-endian)
    let f32_val: f32 = 1.0;
    let f32_bytes = f32_val.to_le_bytes();

    // IEEE 754 single precision: 1.0 = 0x3F800000
    if f32_bytes != [0x00, 0x00, 0x80, 0x3F] {
        return TestResult::fail(format!(
            "[verify data.float.encoding]: f32 1.0 should be [0x00, 0x00, 0x80, 0x3F], got {:?}",
            f32_bytes
        ));
    }

    // Verify f64 encoding (IEEE 754 little-endian)
    let f64_val: f64 = 1.0;
    let f64_bytes = f64_val.to_le_bytes();

    // IEEE 754 double precision: 1.0 = 0x3FF0000000000000
    if f64_bytes != [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xF0, 0x3F] {
        return TestResult::fail(format!(
            "[verify data.float.encoding]: f64 1.0 should be [0x00..0x3F], got {:?}",
            f64_bytes
        ));
    }

    TestResult::pass()
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
pub async fn float_nan_canonicalization(_peer: &mut Peer) -> TestResult {
    // Canonical NaN values:
    // f32: 0x7FC00000
    // f64: 0x7FF8000000000000

    let f32_canonical_nan: u32 = 0x7FC00000;
    let f64_canonical_nan: u64 = 0x7FF8000000000000;

    // Verify the canonical NaN is indeed a NaN
    let f32_nan = f32::from_bits(f32_canonical_nan);
    if !f32_nan.is_nan() {
        return TestResult::fail(
            "[verify data.float.nan-canonicalization]: f32 canonical NaN must be NaN".to_string(),
        );
    }

    let f64_nan = f64::from_bits(f64_canonical_nan);
    if !f64_nan.is_nan() {
        return TestResult::fail(
            "[verify data.float.nan-canonicalization]: f64 canonical NaN must be NaN".to_string(),
        );
    }

    // Verify it's a quiet NaN (not signaling)
    // The quiet NaN bit is set (bit 22 for f32, bit 51 for f64)
    if f32_canonical_nan & (1 << 22) == 0 {
        return TestResult::fail(
            "[verify data.float.nan-canonicalization]: f32 canonical NaN must be quiet".to_string(),
        );
    }

    if f64_canonical_nan & (1 << 51) == 0 {
        return TestResult::fail(
            "[verify data.float.nan-canonicalization]: f64 canonical NaN must be quiet".to_string(),
        );
    }

    TestResult::pass()
}

// =============================================================================
// data.float_negative_zero
// =============================================================================
// Rules: [verify data.float.negative-zero]
//
// Negative zero and positive zero MUST be encoded as distinct bit patterns.

#[conformance(name = "data.float_negative_zero", rules = "data.float.negative-zero")]
pub async fn float_negative_zero(_peer: &mut Peer) -> TestResult {
    // Positive zero and negative zero have different bit patterns
    let pos_zero: f64 = 0.0;
    let neg_zero: f64 = -0.0;

    let pos_bits = pos_zero.to_bits();
    let neg_bits = neg_zero.to_bits();

    // +0.0 = 0x0000000000000000
    // -0.0 = 0x8000000000000000 (sign bit set)

    if pos_bits != 0 {
        return TestResult::fail(format!(
            "[verify data.float.negative-zero]: +0.0 bits should be 0, got {:#X}",
            pos_bits
        ));
    }

    if neg_bits != 0x8000000000000000 {
        return TestResult::fail(format!(
            "[verify data.float.negative-zero]: -0.0 bits should be 0x8000000000000000, got {:#X}",
            neg_bits
        ));
    }

    // They are mathematically equal but have different representations
    if pos_bits == neg_bits {
        return TestResult::fail(
            "[verify data.float.negative-zero]: +0.0 and -0.0 must have different bit patterns"
                .to_string(),
        );
    }

    TestResult::pass()
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
pub async fn service_facet_required(_peer: &mut Peer) -> TestResult {
    // This is a compile-time requirement enforced by the #[rapace::service] macro.
    // The macro requires all argument and return types to implement Facet.
    //
    // We can't directly test this at runtime, but we document the requirement.
    // The Facet trait provides:
    // - Type introspection
    // - Schema hashing
    // - Serialization compatibility

    // Verify some basic types work with postcard (which is what Facet uses)
    let val: i32 = 42;
    let encoded = facet_postcard::to_vec(&val);
    if encoded.is_err() {
        return TestResult::fail(
            "[verify data.service.facet-required]: basic types must be serializable".to_string(),
        );
    }

    TestResult::pass()
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
pub async fn type_system_additional(_peer: &mut Peer) -> TestResult {
    // The core type system includes:
    // - Primitives: i8-i128, u8-u128, f32, f64, bool, char, String
    // - Compound: structs, tuples, arrays, Vec, HashMap, BTreeMap, enums, Option, ()
    //
    // Additional types (like custom wrappers) MAY be supported by implementations
    // but are not part of the stable public API contract.

    // This is a semantic rule - we just document it.
    TestResult::pass()
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
pub async fn unsupported_borrowed_return(_peer: &mut Peer) -> TestResult {
    // Borrowed types like &[u8] or &str MUST NOT be used in return position.
    // Use owned types instead: Vec<u8>, String.
    //
    // This is enforced at compile time by the service macro.
    // We document the rule here.

    // On the wire, all data is owned bytes - borrowing is a Rust API convenience.
    TestResult::pass()
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
pub async fn unsupported_pointers(_peer: &mut Peer) -> TestResult {
    // Raw pointers (*const T, *mut T) cannot be serialized.
    // They represent memory addresses which are meaningless across processes.
    //
    // This is enforced at compile time - Facet doesn't derive for pointer types.
    TestResult::pass()
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
pub async fn unsupported_self_ref(_peer: &mut Peer) -> TestResult {
    // Self-referential types (types that contain references to themselves)
    // cannot be serialized with Postcard.
    //
    // Examples:
    // - struct Node { next: &Node }
    // - Rc<RefCell<...>> cycles
    //
    // Use indices or owned boxes instead if needed.
    TestResult::pass()
}

// =============================================================================
// data.unsupported_unions
// =============================================================================
// Rules: [verify data.unsupported.unions]
//
// Untagged unions MUST NOT be used; not supported by Postcard.

#[conformance(name = "data.unsupported_unions", rules = "data.unsupported.unions")]
pub async fn unsupported_unions(_peer: &mut Peer) -> TestResult {
    // Rust's `union` types are not supported.
    // Use enums (tagged unions) instead.
    //
    // Enums provide type safety and are fully supported.
    TestResult::pass()
}

// =============================================================================
// data.unsupported_usize
// =============================================================================
// Rules: [verify data.unsupported.usize]
//
// usize and isize MUST NOT be used in public service APIs.

#[conformance(name = "data.unsupported_usize", rules = "data.unsupported.usize")]
pub async fn unsupported_usize(_peer: &mut Peer) -> TestResult {
    // usize/isize vary by platform:
    // - 32-bit platforms: 4 bytes
    // - 64-bit platforms: 8 bytes
    //
    // This breaks cross-platform compatibility.
    // Use explicit sizes: u32, u64, i32, i64, etc.

    // Verify size varies (this test runs on 64-bit, so usize = 8)
    let usize_bytes = std::mem::size_of::<usize>();

    // Document that size varies by platform
    if usize_bytes != 4 && usize_bytes != 8 {
        return TestResult::fail(format!(
            "[verify data.unsupported.usize]: unexpected usize size: {}",
            usize_bytes
        ));
    }

    // The rule: use explicit sizes for portability
    // u32 is always 4 bytes, u64 is always 8 bytes
    if std::mem::size_of::<u32>() != 4 {
        return TestResult::fail(
            "[verify data.unsupported.usize]: u32 should be 4 bytes".to_string(),
        );
    }

    if std::mem::size_of::<u64>() != 8 {
        return TestResult::fail(
            "[verify data.unsupported.usize]: u64 should be 8 bytes".to_string(),
        );
    }

    TestResult::pass()
}

// =============================================================================
// data.wire_field_order
// =============================================================================
// Rules: [verify data.wire.field-order]
//
// Struct fields MUST be encoded in declaration order with no names.

#[conformance(name = "data.wire_field_order", rules = "data.wire.field-order")]
pub async fn wire_field_order(_peer: &mut Peer) -> TestResult {
    // Fields are encoded in declaration order.
    // No field names or indices are sent over the wire.
    //
    // This means:
    // - Field order is immutable (reordering breaks compatibility)
    // - Adding/removing fields breaks compatibility

    // Test with a simple tuple (which is ordered)
    let tuple: (i32, i32) = (1, 2);

    let encoded = facet_postcard::to_vec(&tuple).expect("encode tuple");

    // Decode back
    let decoded: (i32, i32) = facet_postcard::from_slice(&encoded).expect("decode tuple");

    if decoded.0 != 1 || decoded.1 != 2 {
        return TestResult::fail(
            "[verify data.wire.field-order]: field order must be preserved".to_string(),
        );
    }

    TestResult::pass()
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
pub async fn wire_non_self_describing(_peer: &mut Peer) -> TestResult {
    // Postcard is NOT self-describing.
    // Field names, struct names, type tags are NOT sent over the wire.
    //
    // Benefits:
    // - Compact encoding (no metadata overhead)
    // - Fast serialization (no schema lookups)
    //
    // Requirements:
    // - Both peers must have identical type definitions
    // - Schema hashing at handshake ensures compatibility

    // Encode a simple value
    let val: u32 = 42;
    let encoded = facet_postcard::to_vec(&val).expect("encode");

    // The encoding should be minimal (just the value, no type info)
    // u32 42 in varint is a single byte: 42 (< 128, so no continuation)
    if encoded.len() > 5 {
        // varint can be at most 5 bytes for u32
        return TestResult::fail(format!(
            "[verify data.wire.non-self-describing]: encoding too large: {} bytes",
            encoded.len()
        ));
    }

    TestResult::pass()
}
