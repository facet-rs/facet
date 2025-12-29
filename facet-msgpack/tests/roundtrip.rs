//! Integration tests for MsgPack serialization and deserialization.
//!
//! These tests verify that the full end-to-end serialization and deserialization works correctly.
//! We use both facet-msgpack's serializer and rmp-serde to verify compatibility.

use facet::Facet;
use facet_msgpack::{from_slice, to_vec};
use serde::{Deserialize, Serialize};

// =============================================================================
// Simple Types
// =============================================================================

#[derive(Debug, Facet, PartialEq, Serialize, Deserialize)]
struct SimpleStruct {
    a: u32,
    b: String,
    c: bool,
}

#[test]
fn test_simple_struct_roundtrip() {
    let value = SimpleStruct {
        a: 123,
        b: "hello".to_string(),
        c: true,
    };

    // Test our serialization
    let bytes = to_vec(&value).unwrap();
    let result: SimpleStruct = from_slice(&bytes).unwrap();
    assert_eq!(result, value);
}

#[test]
fn test_simple_struct_compat_with_rmp() {
    let value = SimpleStruct {
        a: 456,
        b: "world".to_string(),
        c: false,
    };

    // Serialize with our serializer, deserialize with rmp-serde
    let bytes = to_vec(&value).unwrap();
    let result: SimpleStruct = rmp_serde::from_slice(&bytes).unwrap();
    assert_eq!(result, value);

    // Serialize with rmp-serde, deserialize with our parser
    let mut rmp_buf = Vec::new();
    value
        .serialize(&mut rmp_serde::Serializer::new(&mut rmp_buf).with_struct_map())
        .unwrap();
    let result2: SimpleStruct = from_slice(&rmp_buf).unwrap();
    assert_eq!(result2, value);
}

// =============================================================================
// Nested Types
// =============================================================================

#[derive(Debug, Facet, PartialEq, Serialize, Deserialize)]
struct NestedStruct {
    inner: SimpleStruct,
    value: i64,
}

#[test]
fn test_nested_struct_roundtrip() {
    let value = NestedStruct {
        inner: SimpleStruct {
            a: 789,
            b: "nested".to_string(),
            c: true,
        },
        value: -42,
    };

    let bytes = to_vec(&value).unwrap();
    let result: NestedStruct = from_slice(&bytes).unwrap();
    assert_eq!(result, value);
}

// =============================================================================
// Optional Types
// =============================================================================

#[derive(Debug, Facet, PartialEq, Serialize, Deserialize)]
struct OptionalStruct {
    required: String,
    optional: Option<i32>,
}

#[test]
fn test_optional_some() {
    let value = OptionalStruct {
        required: "test".to_string(),
        optional: Some(42),
    };

    let bytes = to_vec(&value).unwrap();
    let result: OptionalStruct = from_slice(&bytes).unwrap();
    assert_eq!(result, value);
}

#[test]
fn test_optional_none() {
    let value = OptionalStruct {
        required: "test".to_string(),
        optional: None,
    };

    let bytes = to_vec(&value).unwrap();
    let result: OptionalStruct = from_slice(&bytes).unwrap();
    assert_eq!(result, value);
}

// =============================================================================
// Arrays/Vecs
// =============================================================================

#[derive(Debug, Facet, PartialEq, Serialize, Deserialize)]
struct VecStruct {
    items: Vec<i32>,
}

#[test]
fn test_vec_roundtrip() {
    let value = VecStruct {
        items: vec![1, 2, 3, 4, 5],
    };

    let bytes = to_vec(&value).unwrap();
    let result: VecStruct = from_slice(&bytes).unwrap();
    assert_eq!(result, value);
}

#[test]
fn test_vec_empty() {
    let value = VecStruct { items: vec![] };

    let bytes = to_vec(&value).unwrap();
    let result: VecStruct = from_slice(&bytes).unwrap();
    assert_eq!(result, value);
}

// =============================================================================
// Scalar Types
// =============================================================================

#[test]
fn test_scalar_u8() {
    let bytes = to_vec(&42u8).unwrap();
    let result: u8 = from_slice(&bytes).unwrap();
    assert_eq!(result, 42);
}

#[test]
fn test_scalar_u16() {
    let bytes = to_vec(&1000u16).unwrap();
    let result: u16 = from_slice(&bytes).unwrap();
    assert_eq!(result, 1000);
}

#[test]
fn test_scalar_u32() {
    let bytes = to_vec(&100000u32).unwrap();
    let result: u32 = from_slice(&bytes).unwrap();
    assert_eq!(result, 100000);
}

#[test]
fn test_scalar_u64() {
    let bytes = to_vec(&10000000000u64).unwrap();
    let result: u64 = from_slice(&bytes).unwrap();
    assert_eq!(result, 10000000000);
}

#[test]
fn test_scalar_i8() {
    let bytes = to_vec(&(-42i8)).unwrap();
    let result: i8 = from_slice(&bytes).unwrap();
    assert_eq!(result, -42);
}

#[test]
fn test_scalar_i16() {
    let bytes = to_vec(&(-1000i16)).unwrap();
    let result: i16 = from_slice(&bytes).unwrap();
    assert_eq!(result, -1000);
}

#[test]
fn test_scalar_i32() {
    let bytes = to_vec(&(-100000i32)).unwrap();
    let result: i32 = from_slice(&bytes).unwrap();
    assert_eq!(result, -100000);
}

#[test]
fn test_scalar_i64() {
    let bytes = to_vec(&(-10000000000i64)).unwrap();
    let result: i64 = from_slice(&bytes).unwrap();
    assert_eq!(result, -10000000000);
}

#[test]
fn test_scalar_f32() {
    let bytes = to_vec(&std::f32::consts::PI).unwrap();
    let result: f32 = from_slice(&bytes).unwrap();
    assert!((result - std::f32::consts::PI).abs() < 0.001);
}

#[test]
fn test_scalar_f64() {
    let bytes = to_vec(&std::f64::consts::PI).unwrap();
    let result: f64 = from_slice(&bytes).unwrap();
    assert!((result - std::f64::consts::PI).abs() < 0.0000001);
}

#[test]
fn test_scalar_bool() {
    let bytes = to_vec(&true).unwrap();
    let result: bool = from_slice(&bytes).unwrap();
    assert!(result);

    let bytes = to_vec(&false).unwrap();
    let result: bool = from_slice(&bytes).unwrap();
    assert!(!result);
}

#[test]
fn test_scalar_string() {
    let bytes = to_vec(&"hello world".to_string()).unwrap();
    let result: String = from_slice(&bytes).unwrap();
    assert_eq!(result, "hello world");
}

// =============================================================================
// Enums
// =============================================================================

#[derive(Debug, Facet, PartialEq, Serialize, Deserialize)]
#[repr(u8)]
enum SimpleEnum {
    Unit,
    WithData(i32),
    WithStruct { x: i32, y: i32 },
}

#[test]
fn test_enum_unit() {
    let value = SimpleEnum::Unit;
    let bytes = to_vec(&value).unwrap();
    let result: SimpleEnum = from_slice(&bytes).unwrap();
    assert_eq!(result, value);
}

#[test]
fn test_enum_with_data() {
    let value = SimpleEnum::WithData(42);
    let bytes = to_vec(&value).unwrap();
    let result: SimpleEnum = from_slice(&bytes).unwrap();
    assert_eq!(result, value);
}

#[test]
fn test_enum_with_struct() {
    let value = SimpleEnum::WithStruct { x: 10, y: 20 };
    let bytes = to_vec(&value).unwrap();
    let result: SimpleEnum = from_slice(&bytes).unwrap();
    assert_eq!(result, value);
}
