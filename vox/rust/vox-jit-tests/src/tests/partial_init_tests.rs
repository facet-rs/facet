//! Partial-initialization invariant tests.
//!
//! The design doc's safety invariants (§Safety Invariants):
//! - len/count not published before elements are initialized
//! - drop only initialized prefix on failure
//! - no borrowed references outliving input backing
//!
//! These tests verify the ORACLE (reflective interpreter) holds the invariants
//! so we have a confirmed baseline before the JIT is wired in.
//!
//! The JIT must hold the same invariants; these tests will be reused against
//! the JIT candidate once FnPtrEngine is populated.

use vox_postcard::{build_identity_plan, from_slice_with_plan};
use vox_schema::SchemaRegistry;

use crate::{corpus::encode_varint, differential::ErrorClass, fixtures::*};

// ---------------------------------------------------------------------------
// Drop safety: Vec<String> where one element has invalid UTF-8.
//
// If the runtime drops uninitialized memory instead of only initialized prefix,
// it would UB or double-free on some implementations. The reflective runtime
// gets this "for free"; JIT must replicate it.
//
// We can't directly test that only the prefix is dropped (that would require
// Miri or a custom Drop tracker), but we CAN assert that decode fails cleanly
// without panic, and that a subsequent successful decode on the same plan works.
// ---------------------------------------------------------------------------

#[test]
fn partial_init_vec_string_element_failure_is_clean() {
    let plan = build_identity_plan(<Vec<String> as facet::Facet>::SHAPE);
    let registry = SchemaRegistry::new();

    // 3 elements: first two valid, third has invalid UTF-8
    let mut bad_bytes = encode_varint(3); // count = 3
    // Element 0: "ok"
    bad_bytes.extend(encode_varint(2));
    bad_bytes.extend_from_slice(b"ok");
    // Element 1: "fine"
    bad_bytes.extend(encode_varint(4));
    bad_bytes.extend_from_slice(b"fine");
    // Element 2: invalid UTF-8
    bad_bytes.extend(encode_varint(3));
    bad_bytes.extend_from_slice(&[0xFF, 0xFE, 0xFD]);

    let result = from_slice_with_plan::<Vec<String>>(&bad_bytes, &plan, &registry);
    assert!(result.is_err(), "expected decode error, got Ok");
    let err = result.unwrap_err();
    assert_eq!(
        ErrorClass::of(&err),
        ErrorClass::InvalidUtf8,
        "wrong error class: {err}"
    );

    // After the failure, a fresh decode with valid bytes must succeed.
    let good = vec!["a".to_string(), "b".to_string()];
    let good_bytes = vox_postcard::serialize::to_vec(&good).expect("encode");
    let decoded: Vec<String> =
        from_slice_with_plan(&good_bytes, &plan, &registry).expect("decode after failed decode");
    assert_eq!(decoded, good);
}

// ---------------------------------------------------------------------------
// Drop safety: nested Vec<Vec<u8>> where inner allocation fails mid-way.
// ---------------------------------------------------------------------------

#[test]
fn partial_init_nested_vec_eof_mid_inner() {
    let plan = build_identity_plan(<Vec<Vec<u8>> as facet::Facet>::SHAPE);
    let registry = SchemaRegistry::new();

    // Outer count = 2
    // Inner 0: [0x01, 0x02] — valid
    // Inner 1: length claims 100 bytes but 0 follow — EOF
    let mut bytes = encode_varint(2);
    bytes.extend(encode_varint(2));
    bytes.extend_from_slice(&[0x01, 0x02]);
    bytes.extend(encode_varint(100)); // EOF will follow

    let result = from_slice_with_plan::<Vec<Vec<u8>>>(&bytes, &plan, &registry);
    let err = result.expect_err("expected EOF error");
    assert_eq!(ErrorClass::of(&err), ErrorClass::UnexpectedEof, "{err}");
}

// ---------------------------------------------------------------------------
// Empty Vec<T>: the calibrated empty-bytes path must produce an empty vec,
// not an uninitialized one. The oracle always does this correctly.
// This test serves as a reference for the JIT's calibrated path.
// ---------------------------------------------------------------------------

#[test]
fn partial_init_empty_vec_decodes_correctly() {
    let plan = build_identity_plan(<Vec<u32> as facet::Facet>::SHAPE);
    let registry = SchemaRegistry::new();

    let empty: Vec<u32> = vec![];
    let bytes = vox_postcard::serialize::to_vec(&empty).expect("encode");
    let decoded: Vec<u32> = from_slice_with_plan(&bytes, &plan, &registry).expect("decode");
    assert!(decoded.is_empty(), "expected empty vec");
    assert_eq!(
        decoded.capacity(),
        0,
        "empty vec should have zero capacity from oracle"
    );
}

// ---------------------------------------------------------------------------
// Borrowed decode: borrowed strings must not outlive the input slice.
//
// We test this by structurally: after borrowed decode, we verify the result
// string contains the right content. We can't test lifetime errors at runtime
// (they're compile-time), but Miri will catch any actual UAF in the JIT path.
// ---------------------------------------------------------------------------

#[test]
fn partial_init_borrowed_string_content_correct() {
    use vox_postcard::from_slice_borrowed_with_plan;

    let plan = build_identity_plan(<Vec<String> as facet::Facet>::SHAPE);
    let registry = SchemaRegistry::new();

    let values = vec!["hello".to_string(), "world".to_string()];
    let bytes = vox_postcard::serialize::to_vec(&values).expect("encode");

    let decoded: Vec<String> =
        from_slice_borrowed_with_plan(&bytes, &plan, &registry).expect("borrowed decode");
    assert_eq!(decoded, values);
}

// ---------------------------------------------------------------------------
// Large Vec<u32>: 256 elements decoded cleanly (exercises multi-allocation path).
// ---------------------------------------------------------------------------

#[test]
fn partial_init_large_vec_clean() {
    let plan = build_identity_plan(<Vec<u32> as facet::Facet>::SHAPE);
    let registry = SchemaRegistry::new();

    let large = VecU32::large();
    let bytes = vox_postcard::serialize::to_vec(&large.items).expect("encode");
    let decoded: Vec<u32> = from_slice_with_plan(&bytes, &plan, &registry).expect("decode");
    assert_eq!(decoded, large.items);
}

// ---------------------------------------------------------------------------
// Struct with Option<Vec<u32>>: None branch must not attempt to decode payload.
// ---------------------------------------------------------------------------

#[test]
fn partial_init_option_none_no_payload() {
    let plan = build_identity_plan(<Option<Vec<u32>> as facet::Facet>::SHAPE);
    let registry = SchemaRegistry::new();

    // Tag 0x00 = None — nothing else follows
    let bytes = vec![0x00u8];
    let decoded: Option<Vec<u32>> =
        from_slice_with_plan(&bytes, &plan, &registry).expect("decode None");
    assert!(decoded.is_none());
}
