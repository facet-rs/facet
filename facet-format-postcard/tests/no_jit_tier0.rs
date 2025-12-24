//! Tests for non-JIT Tier-0 reflection-based deserialization.
//!
//! These tests verify that when the `jit` feature is disabled, facet-format-postcard
//! correctly falls back to Tier-0 reflection-based deserialization instead of erroring.
//!
//! This is critical for WASM targets where Cranelift JIT is not available.

#![cfg(not(feature = "jit"))]

use facet::Facet;
use facet_format_postcard::{from_slice, to_vec};

/// Test basic primitive deserialization without JIT.
#[test]
fn test_tier0_u32() {
    facet_testhelpers::setup();

    let original: u32 = 42;
    let bytes = to_vec(&original).expect("serialization should succeed");
    let deserialized: u32 = from_slice(&bytes).expect("Tier-0 deserialization should succeed");
    assert_eq!(deserialized, original);
}

/// Test bool deserialization without JIT.
#[test]
fn test_tier0_bool() {
    facet_testhelpers::setup();

    let original = true;
    let bytes = to_vec(&original).expect("serialization should succeed");
    let deserialized: bool = from_slice(&bytes).expect("Tier-0 deserialization should succeed");
    assert_eq!(deserialized, original);
}

/// Test Vec<bool> deserialization without JIT.
#[test]
fn test_tier0_vec_bool() {
    facet_testhelpers::setup();

    let original = vec![true, false, true];
    let bytes = to_vec(&original).expect("serialization should succeed");
    let deserialized: Vec<bool> =
        from_slice(&bytes).expect("Tier-0 deserialization should succeed");
    assert_eq!(deserialized, original);
}

/// Test String deserialization without JIT.
#[test]
fn test_tier0_string() {
    facet_testhelpers::setup();

    let original = "Hello, WASM!".to_string();
    let bytes = to_vec(&original).expect("serialization should succeed");
    let deserialized: String = from_slice(&bytes).expect("Tier-0 deserialization should succeed");
    assert_eq!(deserialized, original);
}

/// Test struct deserialization without JIT.
#[test]
fn test_tier0_struct() {
    facet_testhelpers::setup();

    #[derive(Debug, PartialEq, Facet)]
    struct Point {
        x: i32,
        y: i32,
    }

    let original = Point { x: 10, y: -20 };
    let bytes = to_vec(&original).expect("serialization should succeed");
    let deserialized: Point = from_slice(&bytes).expect("Tier-0 deserialization should succeed");
    assert_eq!(deserialized, original);
}

/// Test enum deserialization without JIT.
#[test]
fn test_tier0_enum() {
    facet_testhelpers::setup();

    #[derive(Debug, PartialEq, Facet)]
    #[repr(u8)]
    enum Message {
        Quit,
        Text(String),
        Number(u32),
    }

    // Test unit variant
    let original = Message::Quit;
    let bytes = to_vec(&original).expect("serialization should succeed");
    let deserialized: Message = from_slice(&bytes).expect("Tier-0 deserialization should succeed");
    assert_eq!(deserialized, original);

    // Test newtype variant with String
    let original = Message::Text("hello".to_string());
    let bytes = to_vec(&original).expect("serialization should succeed");
    let deserialized: Message = from_slice(&bytes).expect("Tier-0 deserialization should succeed");
    assert_eq!(deserialized, original);

    // Test newtype variant with Number
    let original = Message::Number(42);
    let bytes = to_vec(&original).expect("serialization should succeed");
    let deserialized: Message = from_slice(&bytes).expect("Tier-0 deserialization should succeed");
    assert_eq!(deserialized, original);
}

/// Test Option deserialization without JIT.
#[test]
fn test_tier0_option() {
    facet_testhelpers::setup();

    // None case
    let original: Option<u32> = None;
    let bytes = to_vec(&original).expect("serialization should succeed");
    let deserialized: Option<u32> =
        from_slice(&bytes).expect("Tier-0 deserialization should succeed");
    assert_eq!(deserialized, original);

    // Some case
    let original: Option<u32> = Some(42);
    let bytes = to_vec(&original).expect("serialization should succeed");
    let deserialized: Option<u32> =
        from_slice(&bytes).expect("Tier-0 deserialization should succeed");
    assert_eq!(deserialized, original);
}

/// Test Result deserialization without JIT.
#[test]
fn test_tier0_result() {
    facet_testhelpers::setup();

    // Ok case
    let original: Result<u32, String> = Ok(42);
    let bytes = to_vec(&original).expect("serialization should succeed");
    let deserialized: Result<u32, String> =
        from_slice(&bytes).expect("Tier-0 deserialization should succeed");
    assert_eq!(deserialized, original);

    // Err case
    let original: Result<u32, String> = Err("error".to_string());
    let bytes = to_vec(&original).expect("serialization should succeed");
    let deserialized: Result<u32, String> =
        from_slice(&bytes).expect("Tier-0 deserialization should succeed");
    assert_eq!(deserialized, original);
}

/// Test nested enum deserialization without JIT (Tier-0 specialty).
///
/// Nested enums are not Tier-2 compatible, so this specifically tests that
/// Tier-0 reflection handles them correctly.
#[test]
fn test_tier0_nested_enum() {
    facet_testhelpers::setup();

    #[derive(Debug, PartialEq, Facet)]
    #[repr(u8)]
    enum Inner {
        A,
        B(u32),
    }

    #[derive(Debug, PartialEq, Facet)]
    #[repr(u8)]
    enum Outer {
        None,
        Inner(Inner),
    }

    // Test nested variant A
    let original = Outer::Inner(Inner::A);
    let bytes = to_vec(&original).expect("serialization should succeed");
    let deserialized: Outer = from_slice(&bytes).expect("Tier-0 deserialization should succeed");
    assert_eq!(deserialized, original);

    // Test nested variant B
    let original = Outer::Inner(Inner::B(42));
    let bytes = to_vec(&original).expect("serialization should succeed");
    let deserialized: Outer = from_slice(&bytes).expect("Tier-0 deserialization should succeed");
    assert_eq!(deserialized, original);
}

/// Test complex struct with multiple field types without JIT.
#[test]
fn test_tier0_complex_struct() {
    facet_testhelpers::setup();

    #[derive(Debug, PartialEq, Facet)]
    struct Complex {
        id: u32,
        name: String,
        values: Vec<i32>,
        optional: Option<bool>,
    }

    let original = Complex {
        id: 123,
        name: "test".to_string(),
        values: vec![1, 2, 3],
        optional: Some(true),
    };

    let bytes = to_vec(&original).expect("serialization should succeed");
    let deserialized: Complex = from_slice(&bytes).expect("Tier-0 deserialization should succeed");
    assert_eq!(deserialized, original);
}

/// Test that the example from the issue description works.
#[test]
fn test_issue_1461_example() {
    facet_testhelpers::setup();

    // The example from issue #1461
    let bytes = &[0x03, 0x01, 0x00, 0x01];
    let result: Vec<bool> = from_slice(bytes).expect("should deserialize without jit feature");
    assert_eq!(result, vec![true, false, true]);
}
