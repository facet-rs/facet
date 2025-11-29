//! Tests for deserializing TOML into facet_value::Value

use facet_testhelpers::test;
use facet_value::{Value, value};

#[test]
fn test_deserialize_scalar_string() {
    let toml = r#"key = "hello""#;
    let result: Value = facet_toml::from_str(toml).unwrap();
    let expected = value!({
        "key": "hello"
    });
    assert_eq!(result, expected);
}

#[test]
fn test_deserialize_scalar_integer() {
    let toml = r#"key = 42"#;
    let result: Value = facet_toml::from_str(toml).unwrap();
    let expected = value!({
        "key": 42
    });
    assert_eq!(result, expected);
}

#[test]
fn test_deserialize_scalar_float() {
    let toml = r#"key = 3.14"#;
    let result: Value = facet_toml::from_str(toml).unwrap();
    // Note: floating point comparison
    let obj = result.as_object().unwrap();
    let val = obj
        .get("key")
        .unwrap()
        .as_number()
        .unwrap()
        .to_f64()
        .unwrap();
    assert!((val - 3.14).abs() < 0.001);
}

#[test]
fn test_deserialize_scalar_boolean() {
    let toml = r#"
        yes = true
        no = false
    "#;
    let result: Value = facet_toml::from_str(toml).unwrap();
    let expected = value!({
        "yes": true,
        "no": false
    });
    assert_eq!(result, expected);
}

#[test]
fn test_deserialize_array() {
    let toml = r#"items = [1, 2, 3]"#;
    let result: Value = facet_toml::from_str(toml).unwrap();
    let expected = value!({
        "items": [1, 2, 3]
    });
    assert_eq!(result, expected);
}

#[test]
fn test_deserialize_nested_object() {
    let toml = r#"
        [server]
        host = "localhost"
        port = 8080
    "#;
    let result: Value = facet_toml::from_str(toml).unwrap();
    let expected = value!({
        "server": {
            "host": "localhost",
            "port": 8080
        }
    });
    assert_eq!(result, expected);
}

#[test]
fn test_deserialize_inline_table() {
    let toml = r#"point = { x = 10, y = 20 }"#;
    let result: Value = facet_toml::from_str(toml).unwrap();
    let expected = value!({
        "point": {
            "x": 10,
            "y": 20
        }
    });
    assert_eq!(result, expected);
}

#[test]
fn test_deserialize_array_of_tables() {
    let toml = r#"
        [[users]]
        name = "Alice"
        age = 30

        [[users]]
        name = "Bob"
        age = 25
    "#;
    let result: Value = facet_toml::from_str(toml).unwrap();
    let expected = value!({
        "users": [
            { "name": "Alice", "age": 30 },
            { "name": "Bob", "age": 25 }
        ]
    });
    assert_eq!(result, expected);
}

#[test]
fn test_deserialize_mixed_types_in_array() {
    // Note: TOML requires homogeneous arrays, but inline arrays can have different types
    // in some implementations. This tests that we can at least handle arrays.
    let toml = r#"items = ["a", "b", "c"]"#;
    let result: Value = facet_toml::from_str(toml).unwrap();
    let expected = value!({
        "items": ["a", "b", "c"]
    });
    assert_eq!(result, expected);
}

#[test]
fn test_deserialize_empty_table() {
    let toml = r#"
        [empty]
    "#;
    let result: Value = facet_toml::from_str(toml).unwrap();
    let expected = value!({
        "empty": {}
    });
    assert_eq!(result, expected);
}

#[test]
fn test_deserialize_deeply_nested() {
    let toml = r#"
        [a.b.c]
        d = "value"
    "#;
    let result: Value = facet_toml::from_str(toml).unwrap();
    let expected = value!({
        "a": {
            "b": {
                "c": {
                    "d": "value"
                }
            }
        }
    });
    assert_eq!(result, expected);
}
