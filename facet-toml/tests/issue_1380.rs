//! Test for issue #1380: Box<T> in untagged enum variant fails to deserialize
//!
//! This test verifies that Box<T> works correctly in untagged enum variants for:
//! - Scalar types (Box<i32>, Box<String>)
//! - Struct types with all fields provided
//!
//! Note: Currently, structs with missing Optional fields in Box<Struct> via table headers
//! require all fields to be provided or the struct to have #[facet(default)].

use facet::Facet;
use facet_testhelpers::test;

#[derive(Facet, Debug, PartialEq)]
pub struct DetailedConfig {
    pub version: Option<String>,
    pub path: Option<String>,
    pub features: Option<Vec<String>>,
}

// This works:
#[derive(Facet, Debug, PartialEq)]
#[repr(u8)]
#[facet(untagged)]
pub enum ConfigWorking {
    Simple(String),
    Detailed(DetailedConfig),
}

// This should also work but currently fails:
#[derive(Facet, Debug, PartialEq)]
#[repr(u8)]
#[facet(untagged)]
#[allow(clippy::box_collection)] // Testing Box<struct>, not just collections
pub enum ConfigBroken {
    Simple(String),
    Detailed(Box<DetailedConfig>),
}

#[test]
fn test_untagged_enum_without_box() {
    let toml = r#"
[deps]
foo = "1.0"

[deps.bar]
version = "2.0"
path = "/some/path"
features = ["serde"]
"#;

    #[derive(Facet, Debug, PartialEq)]
    struct Manifest {
        deps: std::collections::HashMap<String, ConfigWorking>,
    }

    let result: Manifest = facet_toml::from_str(toml).unwrap();

    assert_eq!(result.deps.len(), 2);
    assert_eq!(
        result.deps.get("foo").unwrap(),
        &ConfigWorking::Simple("1.0".to_string())
    );

    match result.deps.get("bar").unwrap() {
        ConfigWorking::Detailed(detail) => {
            assert_eq!(detail.version, Some("2.0".to_string()));
            assert_eq!(detail.features, Some(vec!["serde".to_string()]));
        }
        _ => panic!("Expected Detailed variant"),
    }
}

#[test]
fn test_untagged_enum_with_box() {
    let toml = r#"
[deps]
foo = "1.0"

[deps.bar]
version = "2.0"
path = "/some/path"
features = ["serde"]
"#;

    #[derive(Facet, Debug, PartialEq)]
    struct ManifestBroken {
        deps: std::collections::HashMap<String, ConfigBroken>,
    }

    let result: ManifestBroken = facet_toml::from_str(toml).unwrap();

    assert_eq!(result.deps.len(), 2);
    assert_eq!(
        result.deps.get("foo").unwrap(),
        &ConfigBroken::Simple("1.0".to_string())
    );

    match result.deps.get("bar").unwrap() {
        ConfigBroken::Detailed(detail) => {
            assert_eq!(detail.version, Some("2.0".to_string()));
            assert_eq!(detail.features, Some(vec!["serde".to_string()]));
        }
        _ => panic!("Expected Detailed variant"),
    }
}

#[test]
fn test_untagged_enum_box_scalar() {
    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    #[facet(untagged)]
    enum Value {
        Int(Box<i32>),
        #[allow(clippy::box_collection)]
        Str(Box<String>),
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        values: std::collections::HashMap<String, Value>,
    }

    let toml = r#"
[values]
a = 42
b = "hello"
"#;

    let result: Config = facet_toml::from_str(toml).unwrap();

    assert_eq!(result.values.len(), 2);
    assert_eq!(result.values.get("a").unwrap(), &Value::Int(Box::new(42)));
    assert_eq!(
        result.values.get("b").unwrap(),
        &Value::Str(Box::new("hello".to_string()))
    );
}
