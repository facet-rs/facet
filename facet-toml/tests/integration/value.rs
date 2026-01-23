//! Tests for deserializing into facet_value::Value.
//!
//! Value can capture arbitrary TOML structures when the schema isn't known.

use facet::Facet;
use facet_value::{Value, value};

// ============================================================================
// Basic Value deserialization
// ============================================================================

#[test]
fn deserialize_value_metadata() {
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

    let toml = r#"
[package]
name = "test"
version = "0.1.0"

[package.metadata.custom]
foo = "bar"
numbers = [1, 2, 3]
nested = { key = "value" }
"#;

    let manifest: Manifest = facet_toml::from_str(toml).unwrap();
    assert_eq!(manifest.package.name, "test");
    assert_eq!(manifest.package.version, "0.1.0");

    let metadata = manifest
        .package
        .metadata
        .expect("metadata should be present");
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
fn value_with_various_toml_types() {
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

    let config: Config = facet_toml::from_str(toml).unwrap();

    let obj = config.data.as_object().expect("data should be an object");

    // String
    let string_val = obj.get("string").unwrap();
    assert_eq!(string_val.as_string().map(|s| s.as_str()), Some("hello"));

    // Integer
    let int_val = obj.get("integer").unwrap();
    assert_eq!(int_val.as_number().and_then(|n| n.to_i64()), Some(42));

    // Float
    let float_val = obj.get("float").unwrap();
    let f = float_val.as_number().and_then(|n| n.to_f64()).unwrap();
    #[allow(clippy::approx_constant)]
    {
        assert!((f - 3.14).abs() < 0.001);
    }

    // Boolean
    let bool_val = obj.get("boolean").unwrap();
    assert_eq!(bool_val.as_bool(), Some(true));

    // Array
    let array_val = obj.get("array").unwrap();
    let array = array_val.as_array().unwrap();
    assert_eq!(array.len(), 3);

    // Nested object
    let nested_val = obj.get("nested").unwrap();
    let nested = nested_val.as_object().unwrap();
    let key_val = nested.get("key").unwrap();
    assert_eq!(key_val.as_string().map(|s| s.as_str()), Some("value"));
}

// ============================================================================
// Array-of-tables in Value fields
// ============================================================================

#[test]
fn array_of_tables_in_value() {
    #[derive(Facet, Debug, Clone)]
    pub struct Package {
        pub name: String,
        pub version: String,
        pub metadata: Option<Value>,
    }

    #[derive(Facet, Debug, Clone)]
    pub struct Manifest {
        pub package: Package,
    }

    let toml = r#"
[package]
name = "test"
version = "0.1.0"

[[package.metadata.release.pre-release-replacements]]
file = "CHANGELOG.md"
search = "Unreleased"

[[package.metadata.release.pre-release-replacements]]
file = "CHANGELOG.md"
search = "HEAD"
"#;

    let manifest: Manifest = facet_toml::from_str(toml).unwrap();
    assert_eq!(manifest.package.name, "test");

    let metadata = manifest.package.metadata.as_ref().unwrap();
    let obj = metadata.as_object().unwrap();
    let release = obj.get("release").unwrap().as_object().unwrap();
    let replacements = release.get("pre-release-replacements").unwrap();
    let array = replacements.as_array().unwrap();
    assert_eq!(array.len(), 2, "Should have 2 replacement entries");
}

#[test]
fn simple_array_of_tables_in_value() {
    #[derive(Facet, Debug)]
    struct Config {
        data: Value,
    }

    let toml = r#"
[[data.items]]
name = "first"

[[data.items]]
name = "second"
"#;

    let config: Config = facet_toml::from_str(toml).unwrap();
    let obj = config.data.as_object().unwrap();
    let items = obj.get("items").unwrap().as_array().unwrap();
    assert_eq!(items.len(), 2, "Should have 2 items");
}
