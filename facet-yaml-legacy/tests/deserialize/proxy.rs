//! Tests for proxy attribute support in facet-yaml.
//!
//! This tests both container-level proxy (`#[facet(proxy = ProxyType)]` at struct level)
//! and field-level proxy (`#[facet(proxy = ProxyType)]` at field level).

use facet::Facet;
use facet_yaml_legacy as yaml;

/// Proxy type that represents an integer as a string for serialization.
#[derive(Facet, Clone, Debug)]
#[facet(transparent)]
pub struct IntAsString(pub String);

/// A type with container-level proxy.
/// Any `MyInt` value will serialize/deserialize through `IntAsString`.
#[derive(Facet, Debug, Clone, PartialEq)]
#[facet(proxy = IntAsString)]
pub struct MyInt {
    pub value: i32,
}

/// Convert from proxy (deserialization)
impl TryFrom<IntAsString> for MyInt {
    type Error = std::num::ParseIntError;
    fn try_from(proxy: IntAsString) -> Result<Self, Self::Error> {
        Ok(MyInt {
            value: proxy.0.parse()?,
        })
    }
}

/// Convert to proxy (serialization)
impl From<&MyInt> for IntAsString {
    fn from(v: &MyInt) -> Self {
        IntAsString(v.value.to_string())
    }
}

/// Test basic container-level proxy on a simple field.
#[test]
fn test_basic_container_proxy() {
    #[derive(Facet, Debug, Clone, PartialEq)]
    pub struct Wrapper {
        pub item: MyInt,
    }

    // Deserialization: YAML string "42" should deserialize to MyInt { value: 42 }
    let yaml = r#"item: "42""#;
    let wrapper: Wrapper = yaml::from_str(yaml).unwrap();
    assert_eq!(wrapper.item, MyInt { value: 42 });
}

/// Test container-level proxy with Vec<T>.
#[test]
fn test_vec_with_container_proxy() {
    #[derive(Facet, Debug, Clone, PartialEq)]
    pub struct Collection {
        pub items: Vec<MyInt>,
    }

    // Deserialization
    let yaml = r#"
items:
  - "1"
  - "2"
  - "3"
"#;
    let collection: Collection = yaml::from_str(yaml).unwrap();
    assert_eq!(collection.items.len(), 3);
    assert_eq!(collection.items[0], MyInt { value: 1 });
    assert_eq!(collection.items[1], MyInt { value: 2 });
    assert_eq!(collection.items[2], MyInt { value: 3 });
}

/// Test container-level proxy with Option<T>.
#[test]
fn test_option_with_container_proxy() {
    #[derive(Facet, Debug, Clone, PartialEq)]
    pub struct MaybeInt {
        pub value: Option<MyInt>,
    }

    // With Some value
    let yaml = r#"value: "42""#;
    let maybe: MaybeInt = yaml::from_str(yaml).unwrap();
    assert_eq!(maybe.value, Some(MyInt { value: 42 }));

    // With null
    let yaml2 = r#"value: null"#;
    let maybe2: MaybeInt = yaml::from_str(yaml2).unwrap();
    assert!(maybe2.value.is_none());
}

/// Test that field-level proxy overrides container-level proxy.
#[test]
fn test_field_proxy_overrides_container_proxy() {
    /// Alternative proxy that uses hex encoding
    #[derive(Facet, Clone, Debug)]
    #[facet(transparent)]
    pub struct HexIntProxy(pub String);

    impl TryFrom<HexIntProxy> for MyInt {
        type Error = std::num::ParseIntError;
        fn try_from(proxy: HexIntProxy) -> Result<Self, Self::Error> {
            // Parse as hex (without 0x prefix for simplicity)
            Ok(MyInt {
                value: i32::from_str_radix(&proxy.0, 16)?,
            })
        }
    }

    impl From<&MyInt> for HexIntProxy {
        fn from(v: &MyInt) -> Self {
            HexIntProxy(format!("{:x}", v.value))
        }
    }

    #[derive(Facet, Debug, Clone, PartialEq)]
    pub struct Mixed {
        // Uses container-level proxy (IntAsString - decimal)
        pub decimal: MyInt,
        // Uses field-level proxy (HexIntProxy - hex)
        #[facet(proxy = HexIntProxy)]
        pub hex: MyInt,
    }

    let yaml = r#"
decimal: "255"
hex: "ff"
"#;
    let mixed: Mixed = yaml::from_str(yaml).unwrap();
    assert_eq!(mixed.decimal, MyInt { value: 255 });
    assert_eq!(mixed.hex, MyInt { value: 255 });
}

/// Test deserialization error propagation from proxy conversion.
#[test]
fn test_proxy_conversion_error() {
    #[derive(Facet, Debug, Clone, PartialEq)]
    pub struct Wrapper {
        pub item: MyInt,
    }

    // Invalid integer string should fail
    let yaml = r#"item: "not_a_number""#;
    let result: Result<Wrapper, _> = yaml::from_str(yaml);
    assert!(result.is_err());
}

/// Test the exact scenario from issue #1177:
/// EnvString with proxy=EnvStringDesr and transparent EnvStringDesr
#[test]
fn test_env_string_proxy_pattern() {
    /// EnvString is the target type - it stores a processed string
    #[derive(Clone, Facet, Debug, PartialEq)]
    #[facet(proxy = EnvStringDesr)]
    pub struct EnvString(String);

    /// EnvStringDesr is the proxy type - transparent wrapper around String
    #[derive(Facet)]
    #[facet(transparent)]
    struct EnvStringDesr(String);

    impl TryFrom<EnvStringDesr> for EnvString {
        type Error = String;

        fn try_from(value: EnvStringDesr) -> Result<Self, Self::Error> {
            // In the real use case, this would do env var substitution
            // For testing, we just wrap the string
            Ok(EnvString(value.0))
        }
    }

    impl From<&EnvString> for EnvStringDesr {
        fn from(v: &EnvString) -> Self {
            EnvStringDesr(v.0.clone())
        }
    }

    #[derive(Facet, Clone, Debug, PartialEq)]
    pub struct Logging {
        pub job_path: EnvString,
    }

    #[derive(Facet, Clone, Debug, PartialEq)]
    pub struct Config {
        pub logging: Logging,
    }

    // This is the exact YAML from the issue
    let yaml = r#"
logging:
  job_path: ${JOB_PATH}
"#;

    let config: Config = yaml::from_str(yaml).unwrap();
    assert_eq!(
        config.logging.job_path,
        EnvString("${JOB_PATH}".to_string())
    );
}

/// Test nested Vec with proxy types
#[test]
fn test_nested_vec_with_container_proxy() {
    #[derive(Facet, Debug, Clone, PartialEq)]
    pub struct NestedCollection {
        pub matrix: Vec<Vec<MyInt>>,
    }

    let yaml = r#"
matrix:
  - ["1", "2"]
  - ["3", "4"]
"#;
    let nested: NestedCollection = yaml::from_str(yaml).unwrap();
    assert_eq!(nested.matrix.len(), 2);
    assert_eq!(
        nested.matrix[0],
        vec![MyInt { value: 1 }, MyInt { value: 2 }]
    );
    assert_eq!(
        nested.matrix[1],
        vec![MyInt { value: 3 }, MyInt { value: 4 }]
    );
}
