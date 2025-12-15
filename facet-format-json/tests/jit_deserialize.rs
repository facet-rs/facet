//! Tests for JIT-compiled deserialization.

use facet::Facet;
use facet_format::jit;
use facet_format_json::JsonParser;

#[derive(Debug, PartialEq, Facet)]
struct SimpleStruct {
    name: String,
    age: i64,
    active: bool,
}

#[test]
fn test_jit_simple_struct() {
    // Check compatibility
    assert!(jit::is_jit_compatible::<SimpleStruct>());

    // Parse with JIT
    let json = br#"{"name": "Alice", "age": 30, "active": true}"#;
    let mut parser = JsonParser::new(json);

    let result = jit::try_deserialize::<SimpleStruct, JsonParser<'_>>(&mut parser);

    assert!(result.is_some(), "JIT deserialization should be attempted");
    let result = result.unwrap();
    assert!(
        result.is_ok(),
        "JIT deserialization should succeed: {:?}",
        result
    );

    let value = result.unwrap();
    assert_eq!(value.name, "Alice");
    assert_eq!(value.age, 30);
    assert!(value.active);
}

#[derive(Debug, PartialEq, Facet)]
struct MixedTypes {
    count: u64,
    ratio: f64,
    flag: bool,
}

#[test]
fn test_jit_mixed_types() {
    assert!(jit::is_jit_compatible::<MixedTypes>());

    let json = br#"{"count": 42, "ratio": 3.14, "flag": false}"#;
    let mut parser = JsonParser::new(json);

    let result = jit::try_deserialize::<MixedTypes, JsonParser<'_>>(&mut parser);

    assert!(result.is_some());
    let result = result.unwrap();
    assert!(
        result.is_ok(),
        "JIT deserialization should succeed: {:?}",
        result
    );

    let value = result.unwrap();
    assert_eq!(value.count, 42);
    assert!((value.ratio - 3.14).abs() < 0.001);
    assert!(!value.flag);
}

#[derive(Debug, PartialEq, Facet)]
struct OutOfOrder {
    a: i64,
    b: i64,
    c: i64,
}

#[test]
fn test_jit_out_of_order_fields() {
    // JSON fields in different order than struct definition
    let json = br#"{"c": 3, "a": 1, "b": 2}"#;
    let mut parser = JsonParser::new(json);

    let result = jit::try_deserialize::<OutOfOrder, JsonParser<'_>>(&mut parser);

    assert!(result.is_some());
    let result = result.unwrap();
    assert!(
        result.is_ok(),
        "JIT deserialization should succeed: {:?}",
        result
    );

    let value = result.unwrap();
    assert_eq!(value.a, 1);
    assert_eq!(value.b, 2);
    assert_eq!(value.c, 3);
}

#[test]
fn test_jit_unknown_fields_skipped() {
    // Extra fields should be skipped
    let json = br#"{"name": "Bob", "extra": "ignored", "age": 25, "active": false}"#;
    let mut parser = JsonParser::new(json);

    let result = jit::try_deserialize::<SimpleStruct, JsonParser<'_>>(&mut parser);

    assert!(result.is_some());
    let result = result.unwrap();
    assert!(
        result.is_ok(),
        "JIT deserialization should succeed: {:?}",
        result
    );

    let value = result.unwrap();
    assert_eq!(value.name, "Bob");
    assert_eq!(value.age, 25);
    assert!(!value.active);
}
