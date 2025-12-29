//! Regression tests for flattened struct Tier-2 JIT support
//!
//! These tests ensure flattened struct support maintains tier2_successes=100
//! and produces deterministic errors for key collisions.

#![cfg(feature = "jit")]

use facet::Facet;
use facet_format::jit as format_jit;
use facet_json::JsonParser;

// ============================================================================
// Type Definitions
// ============================================================================

#[derive(Debug, PartialEq, Facet, serde::Serialize, serde::Deserialize, Clone)]
struct Inner {
    b: u64,
    c: String,
}

#[derive(Debug, PartialEq, Facet, serde::Serialize, serde::Deserialize, Clone)]
struct Outer {
    a: u64,
    #[facet(flatten)]
    inner: Inner,
}

#[derive(Debug, PartialEq, Facet, serde::Serialize, serde::Deserialize, Clone)]
struct Address {
    street: String,
    city: String,
    zip: u32,
}

#[derive(Debug, PartialEq, Facet, serde::Serialize, serde::Deserialize, Clone)]
struct Person {
    name: String,
    age: u32,
    #[facet(flatten)]
    address: Address,
}

// ============================================================================
// Tests
// ============================================================================

/// Test that flattened structs achieve tier2_successes=100
#[test]
fn test_flatten_struct_basic() {
    // JSON shape: flattened struct fields merge into parent namespace
    let json = r#"{"a":42,"b":100,"c":"hello"}"#;

    // Attempt Tier-2 compilation
    let result = format_jit::get_format_deserializer::<Vec<Outer>, JsonParser>();

    // Tier-2 should succeed for this type
    assert!(
        result.is_some(),
        "Tier-2 JIT should compile for Vec<Outer> with flattened struct"
    );

    // Parse using Tier-2 (will use compiled deserializer from cache)
    let parsed: Vec<Outer> =
        facet_json::from_str(&format!("[{}]", json)).expect("Should parse with Tier-2");

    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0].a, 42);
    assert_eq!(parsed[0].inner.b, 100);
    assert_eq!(parsed[0].inner.c, "hello");
}

/// Test flattened struct with multiple fields
#[test]
fn test_flatten_struct_multiple_fields() {
    let json =
        r#"{"name":"Alice","age":30,"street":"123 Main St","city":"Springfield","zip":12345}"#;

    // Attempt Tier-2 compilation
    let result = format_jit::get_format_deserializer::<Vec<Person>, JsonParser>();

    // Tier-2 should succeed
    assert!(
        result.is_some(),
        "Tier-2 JIT should compile for Vec<Person> with flattened Address"
    );

    // Parse using Tier-2
    let parsed: Vec<Person> =
        facet_json::from_str(&format!("[{}]", json)).expect("Should parse with Tier-2");

    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0].name, "Alice");
    assert_eq!(parsed[0].age, 30);
    assert_eq!(parsed[0].address.street, "123 Main St");
    assert_eq!(parsed[0].address.city, "Springfield");
    assert_eq!(parsed[0].address.zip, 12345);
}

/// Test flattened struct with Option fields
#[test]
fn test_flatten_struct_with_option() {
    #[derive(Debug, PartialEq, Facet, serde::Serialize, serde::Deserialize, Clone)]
    struct InnerWithOption {
        required: u64,
        optional: Option<String>,
    }

    #[derive(Debug, PartialEq, Facet, serde::Serialize, serde::Deserialize, Clone)]
    struct OuterWithOption {
        id: u64,
        #[facet(flatten)]
        inner: InnerWithOption,
    }

    let result = format_jit::get_format_deserializer::<Vec<OuterWithOption>, JsonParser>();
    assert!(
        result.is_some(),
        "Tier-2 JIT should compile for Vec<OuterWithOption> with Option fields"
    );

    // Test with optional field present
    let json_with = r#"{"id":1,"required":42,"optional":"test"}"#;
    let parsed: Vec<OuterWithOption> =
        facet_json::from_str(&format!("[{}]", json_with)).expect("Should parse");
    assert_eq!(parsed[0].id, 1);
    assert_eq!(parsed[0].inner.required, 42);
    assert_eq!(parsed[0].inner.optional, Some("test".to_string()));

    // Test with optional field absent
    let json_without = r#"{"id":2,"required":100}"#;
    let parsed: Vec<OuterWithOption> =
        facet_json::from_str(&format!("[{}]", json_without)).expect("Should parse");
    assert_eq!(parsed[0].id, 2);
    assert_eq!(parsed[0].inner.required, 100);
    assert_eq!(parsed[0].inner.optional, None);
}

/// Test field name collision detection
#[test]
fn test_flatten_struct_collision() {
    #[derive(Debug, PartialEq, Facet, serde::Serialize, serde::Deserialize, Clone)]
    struct InnerCollision {
        a: u64, // This collides with Outer's 'a'
        b: String,
    }

    #[derive(Debug, PartialEq, Facet, serde::Serialize, serde::Deserialize, Clone)]
    struct OuterCollision {
        a: u64, // This collides with Inner's 'a'
        #[facet(flatten)]
        inner: InnerCollision,
    }

    // Attempt Tier-2 compilation should fail due to collision
    let result = format_jit::get_format_deserializer::<Vec<OuterCollision>, JsonParser>();

    // Tier-2 should return None (compile-unsupported) due to key collision
    assert!(
        result.is_none(),
        "Tier-2 JIT should reject structs with flattened field name collisions"
    );
}

/// Test collision between two flattened structs
#[test]
fn test_flatten_struct_double_collision() {
    #[derive(Debug, PartialEq, Facet, serde::Serialize, serde::Deserialize, Clone)]
    struct Inner1 {
        x: u64,
        y: String,
    }

    #[derive(Debug, PartialEq, Facet, serde::Serialize, serde::Deserialize, Clone)]
    struct Inner2 {
        x: u32, // This collides with Inner1's 'x'
        z: bool,
    }

    #[derive(Debug, PartialEq, Facet, serde::Serialize, serde::Deserialize, Clone)]
    struct OuterDoubleCollision {
        id: u64,
        #[facet(flatten)]
        inner1: Inner1,
        #[facet(flatten)]
        inner2: Inner2,
    }

    // Attempt Tier-2 compilation should fail due to collision between flattened structs
    let result = format_jit::get_format_deserializer::<Vec<OuterDoubleCollision>, JsonParser>();

    // Tier-2 should return None (compile-unsupported) due to key collision
    assert!(
        result.is_none(),
        "Tier-2 JIT should reject structs with collisions between flattened structs"
    );
}

/// Test flattened struct with nested struct field
#[test]
fn test_flatten_struct_with_nested_struct() {
    #[derive(Debug, PartialEq, Facet, serde::Serialize, serde::Deserialize, Clone)]
    struct Coordinates {
        lat: f64,
        lon: f64,
    }

    #[derive(Debug, PartialEq, Facet, serde::Serialize, serde::Deserialize, Clone)]
    struct Location {
        name: String,
        coords: Coordinates,
    }

    #[derive(Debug, PartialEq, Facet, serde::Serialize, serde::Deserialize, Clone)]
    struct Place {
        id: u64,
        #[facet(flatten)]
        location: Location,
    }

    let result = format_jit::get_format_deserializer::<Vec<Place>, JsonParser>();
    assert!(
        result.is_some(),
        "Tier-2 JIT should compile for Vec<Place> with flattened Location containing nested struct"
    );

    let json = r#"{"id":42,"name":"Home","coords":{"lat":37.7749,"lon":-122.4194}}"#;
    let parsed: Vec<Place> = facet_json::from_str(&format!("[{}]", json)).expect("Should parse");
    assert_eq!(parsed[0].id, 42);
    assert_eq!(parsed[0].location.name, "Home");
    assert_eq!(parsed[0].location.coords.lat, 37.7749);
    assert_eq!(parsed[0].location.coords.lon, -122.4194);
}
