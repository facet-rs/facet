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

    let json = br#"{"count": 42, "ratio": 2.5, "flag": false}"#;
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
    assert!((value.ratio - 2.5).abs() < 0.001);
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

#[derive(Debug, PartialEq, Facet)]
struct Inner {
    x: i64,
    y: i64,
}

#[derive(Debug, PartialEq, Facet)]
struct Outer {
    id: u64,
    inner: Inner,
    name: String,
}

#[test]
fn test_jit_nested_struct() {
    // Check compatibility
    assert!(jit::is_jit_compatible::<Outer>());
    assert!(jit::is_jit_compatible::<Inner>());

    // Parse with JIT
    let json = br#"{"id": 42, "inner": {"x": 10, "y": 20}, "name": "test"}"#;
    let mut parser = JsonParser::new(json);

    let result = jit::try_deserialize::<Outer, JsonParser<'_>>(&mut parser);

    assert!(result.is_some(), "JIT deserialization should be attempted");
    let result = result.unwrap();
    assert!(
        result.is_ok(),
        "JIT deserialization should succeed: {:?}",
        result
    );

    let value = result.unwrap();
    assert_eq!(value.id, 42);
    assert_eq!(value.inner.x, 10);
    assert_eq!(value.inner.y, 20);
    assert_eq!(value.name, "test");
}

#[derive(Debug, PartialEq, Facet)]
struct WithOption {
    id: u64,
    maybe_count: Option<i64>,
    maybe_flag: Option<bool>,
}

#[test]
fn test_jit_option_none() {
    // Test with null values
    let json = br#"{"id": 42, "maybe_count": null, "maybe_flag": null}"#;
    let mut parser = JsonParser::new(json);

    let result = jit::try_deserialize::<WithOption, JsonParser<'_>>(&mut parser);

    assert!(result.is_some(), "JIT should attempt with Option fields");
    let result = result.unwrap();
    assert!(
        result.is_ok(),
        "JIT deserialization should succeed: {:?}",
        result
    );

    let value = result.unwrap();
    assert_eq!(value.id, 42);
    assert_eq!(value.maybe_count, None);
    assert_eq!(value.maybe_flag, None);
}

#[test]
fn test_jit_option_some() {
    // Test with Some values
    let json = br#"{"id": 42, "maybe_count": 123, "maybe_flag": true}"#;
    let mut parser = JsonParser::new(json);

    let result = jit::try_deserialize::<WithOption, JsonParser<'_>>(&mut parser);

    assert!(result.is_some());
    let result = result.unwrap();
    assert!(
        result.is_ok(),
        "JIT deserialization should succeed: {:?}",
        result
    );

    let value = result.unwrap();
    assert_eq!(value.id, 42);
    assert_eq!(value.maybe_count, Some(123));
    assert_eq!(value.maybe_flag, Some(true));
}

#[test]
fn test_jit_vec_bool() {
    // Check compatibility - Vec<bool> should be JIT compatible
    assert!(
        jit::is_jit_compatible::<Vec<bool>>(),
        "Vec<bool> should be JIT compatible"
    );

    // Parse with JIT
    let json = br#"[true, false, true, true, false]"#;
    let mut parser = JsonParser::new(json);

    let result = jit::try_deserialize::<Vec<bool>, JsonParser<'_>>(&mut parser);

    assert!(result.is_some(), "JIT deserialization should be attempted");
    let result = result.unwrap();
    assert!(
        result.is_ok(),
        "JIT deserialization should succeed: {:?}",
        result
    );

    let value = result.unwrap();
    assert_eq!(value, vec![true, false, true, true, false]);
}

#[test]
fn test_jit_vec_i64() {
    assert!(jit::is_jit_compatible::<Vec<i64>>());

    let json = br#"[1, 2, 3, -4, 5]"#;
    let mut parser = JsonParser::new(json);

    let result = jit::try_deserialize::<Vec<i64>, JsonParser<'_>>(&mut parser);

    assert!(result.is_some());
    let result = result.unwrap();
    assert!(
        result.is_ok(),
        "JIT deserialization should succeed: {:?}",
        result
    );

    let value = result.unwrap();
    assert_eq!(value, vec![1, 2, 3, -4, 5]);
}

#[test]
fn test_jit_vec_f64() {
    assert!(jit::is_jit_compatible::<Vec<f64>>());

    let json = br#"[1.5, 2.0, 3.14]"#;
    let mut parser = JsonParser::new(json);

    let result = jit::try_deserialize::<Vec<f64>, JsonParser<'_>>(&mut parser);

    assert!(result.is_some());
    let result = result.unwrap();
    assert!(
        result.is_ok(),
        "JIT deserialization should succeed: {:?}",
        result
    );

    let value = result.unwrap();
    assert_eq!(value.len(), 3);
    assert!((value[0] - 1.5).abs() < 0.001);
    assert!((value[1] - 2.0).abs() < 0.001);
    assert!((value[2] - 3.14).abs() < 0.001);
}

#[test]
fn test_jit_vec_string() {
    assert!(jit::is_jit_compatible::<Vec<String>>());

    let json = br#"["hello", "world", "test"]"#;
    let mut parser = JsonParser::new(json);

    let result = jit::try_deserialize::<Vec<String>, JsonParser<'_>>(&mut parser);

    assert!(result.is_some());
    let result = result.unwrap();
    assert!(
        result.is_ok(),
        "JIT deserialization should succeed: {:?}",
        result
    );

    let value = result.unwrap();
    assert_eq!(value, vec!["hello", "world", "test"]);
}
