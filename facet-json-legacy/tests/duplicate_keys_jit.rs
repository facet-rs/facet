//! Tests for duplicate key handling in Tier-2 JIT
//!
//! JSON allows duplicate keys, and the standard behavior is "last wins".
//! These tests verify that Tier-2 properly implements last-wins semantics.

#![cfg(feature = "jit")]

use facet::Facet;
use facet_format::jit as format_jit;
use facet_json::JsonParser;

/// Test that duplicate scalar fields use "last wins"
#[test]
fn test_duplicate_scalar_last_wins() {
    #[derive(Debug, PartialEq, Facet, serde::Serialize, serde::Deserialize, Clone)]
    struct Data {
        a: u64,
        b: i64,
    }

    // JSON with duplicate "a" field - last value should win
    let json = r#"{"a":1,"b":10,"a":2}"#;

    // Tier-2 should compile
    let result = format_jit::get_format_deserializer::<Vec<Data>, JsonParser>();
    assert!(result.is_some(), "Tier-2 should compile");

    // Parse and verify last value wins
    let parsed: Vec<Data> = facet_json::from_str(&format!("[{}]", json)).expect("Should parse");

    assert_eq!(parsed[0].a, 2, "Last value for 'a' should win");
    assert_eq!(parsed[0].b, 10);
}

/// Test that duplicate String fields use "last wins"
#[test]
fn test_duplicate_string_last_wins() {
    #[derive(Debug, PartialEq, Facet, serde::Serialize, serde::Deserialize, Clone)]
    struct Data {
        name: String,
        value: u64,
    }

    // JSON with duplicate "name" field
    let json = r#"{"name":"first","value":1,"name":"second"}"#;

    let result = format_jit::get_format_deserializer::<Vec<Data>, JsonParser>();
    assert!(result.is_some(), "Tier-2 should compile");

    let parsed: Vec<Data> = facet_json::from_str(&format!("[{}]", json)).expect("Should parse");

    assert_eq!(parsed[0].name, "second", "Last value for 'name' should win");
    assert_eq!(parsed[0].value, 1);
}

/// Test that duplicate Option fields use "last wins"
#[test]
fn test_duplicate_option_last_wins() {
    #[derive(Debug, PartialEq, Facet, serde::Serialize, serde::Deserialize, Clone)]
    struct Data {
        opt: Option<String>,
        value: u64,
    }

    // First sets opt to Some("first"), second sets to Some("second")
    let json = r#"{"opt":"first","value":1,"opt":"second"}"#;

    let result = format_jit::get_format_deserializer::<Vec<Data>, JsonParser>();
    assert!(result.is_some(), "Tier-2 should compile");

    let parsed: Vec<Data> = facet_json::from_str(&format!("[{}]", json)).expect("Should parse");

    assert_eq!(
        parsed[0].opt,
        Some("second".to_string()),
        "Last value should win"
    );
    assert_eq!(parsed[0].value, 1);
}

/// Test duplicate keys across flattened struct
#[test]
fn test_duplicate_flattened_struct_last_wins() {
    #[derive(Debug, PartialEq, Facet, serde::Serialize, serde::Deserialize, Clone)]
    struct Inner {
        b: u64,
    }

    #[derive(Debug, PartialEq, Facet, serde::Serialize, serde::Deserialize, Clone)]
    struct Outer {
        a: u64,
        #[facet(flatten)]
        inner: Inner,
    }

    // Duplicate "b" field (from flattened inner)
    let json = r#"{"a":1,"b":10,"b":20}"#;

    let result = format_jit::get_format_deserializer::<Vec<Outer>, JsonParser>();
    assert!(result.is_some(), "Tier-2 should compile");

    let parsed: Vec<Outer> = facet_json::from_str(&format!("[{}]", json)).expect("Should parse");

    assert_eq!(parsed[0].a, 1);
    assert_eq!(
        parsed[0].inner.b, 20,
        "Last value for flattened field should win"
    );
}
