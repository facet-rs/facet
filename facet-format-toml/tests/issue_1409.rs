//! Test for issue #1409: facet-format-toml doesn't support deserializing into facet_value::Value

use facet::Facet;
use facet_value::{Value, value};

#[derive(Facet, Debug)]
struct Package {
    name: String,
    version: String,
    metadata: Option<Value>,
}

#[derive(Facet, Debug)]
struct Manifest {
    package: Package,
}

#[test]
fn test_deserialize_value_metadata() {
    let toml = r#"
[package]
name = "test"
version = "0.1.0"

[package.metadata.custom]
foo = "bar"
numbers = [1, 2, 3]
nested = { key = "value" }
"#;

    // This should work - deserialize arbitrary TOML into facet_value::Value
    let manifest: Manifest = facet_format_toml::from_str(toml).unwrap();

    assert_eq!(manifest.package.name, "test");
    assert_eq!(manifest.package.version, "0.1.0");

    let metadata = manifest
        .package
        .metadata
        .expect("metadata should be present");

    // Use the value! macro to construct the expected structure
    let expected = value!({
        "custom": {
            "foo": "bar",
            "numbers": [1, 2, 3],
            "nested": {
                "key": "value"
            }
        }
    });

    assert_eq!(metadata, expected);
}

#[test]
fn test_value_with_various_toml_types() {
    #[derive(Facet, Debug)]
    struct Config {
        data: Value,
    }

    let toml = r#"
[data]
string = "hello"
integer = 42
float = 3.14
boolean = true
array = [1, 2, 3]

[data.nested]
key = "value"
"#;

    let config: Config = facet_format_toml::from_str(toml).unwrap();

    // Verify it's an object with the right fields
    let obj = config.data.as_object().expect("data should be an object");

    // Check string field
    let string_val = obj.get("string").expect("should have string field");
    assert_eq!(string_val.as_string().map(|s| s.as_str()), Some("hello"));

    // Check integer field
    let int_val = obj.get("integer").expect("should have integer field");
    assert_eq!(int_val.as_number().and_then(|n| n.to_i64()), Some(42));

    // Check float field
    let float_val = obj.get("float").expect("should have float field");
    let f = float_val
        .as_number()
        .and_then(|n| n.to_f64())
        .expect("should be float");
    #[allow(clippy::approx_constant)]
    {
        assert!((f - 3.14).abs() < 0.001);
    }

    // Check boolean field
    let bool_val = obj.get("boolean").expect("should have boolean field");
    assert_eq!(bool_val.as_bool(), Some(true));

    // Check array field
    let array_val = obj.get("array").expect("should have array field");
    let array = array_val.as_array().expect("should be array");
    assert_eq!(array.len(), 3);

    // Check nested object
    let nested_val = obj.get("nested").expect("should have nested field");
    let nested = nested_val.as_object().expect("should be object");
    let key_val = nested.get("key").expect("should have key field");
    assert_eq!(key_val.as_string().map(|s| s.as_str()), Some("value"));
}
