//! Tests for container-level proxy attribute (#1109).
//!
//! Container-level proxy allows `#[facet(proxy = ProxyType)]` at the struct/enum level,
//! so any field of that type automatically uses the proxy without per-field annotations.
//! This includes nested types like `Vec<Mine>`, `Option<Mine>`, etc.

use facet::Facet;
use facet_json as json;

/// Proxy type that represents an integer as a string for serialization.
/// This is the proxy type that will be used at the container level.
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
impl TryFrom<&MyInt> for IntAsString {
    type Error = std::convert::Infallible;
    fn try_from(v: &MyInt) -> Result<Self, Self::Error> {
        Ok(IntAsString(v.value.to_string()))
    }
}

/// Test basic container-level proxy on a simple field.
#[test]
fn test_basic_container_proxy() {
    #[derive(Facet, Debug, Clone, PartialEq)]
    pub struct Wrapper {
        pub item: MyInt,
    }

    // Deserialization: JSON string "42" should deserialize to MyInt { value: 42 }
    let json = r#"{"item":"42"}"#;
    let wrapper: Wrapper = json::from_str(json).unwrap();
    assert_eq!(wrapper.item, MyInt { value: 42 });

    // Serialization: MyInt { value: 42 } should serialize to JSON string "42"
    let serialized = json::to_string(&wrapper);
    assert_eq!(serialized, r#"{"item":"42"}"#);
}

/// Test container-level proxy with Vec<T>.
/// When serializing Vec<MyInt>, each element should use the proxy.
#[test]
fn test_vec_with_container_proxy() {
    #[derive(Facet, Debug, Clone, PartialEq)]
    pub struct Collection {
        pub items: Vec<MyInt>,
    }

    // Deserialization
    let json = r#"{"items":["1","2","3"]}"#;
    let collection: Collection = json::from_str(json).unwrap();
    assert_eq!(collection.items.len(), 3);
    assert_eq!(collection.items[0], MyInt { value: 1 });
    assert_eq!(collection.items[1], MyInt { value: 2 });
    assert_eq!(collection.items[2], MyInt { value: 3 });

    // Serialization
    let serialized = json::to_string(&collection);
    assert_eq!(serialized, r#"{"items":["1","2","3"]}"#);
}

/// Test container-level proxy with Option<T>.
#[test]
fn test_option_with_container_proxy() {
    #[derive(Facet, Debug, Clone, PartialEq)]
    pub struct MaybeInt {
        pub value: Option<MyInt>,
    }

    // With Some value
    let json = r#"{"value":"42"}"#;
    let maybe: MaybeInt = json::from_str(json).unwrap();
    assert_eq!(maybe.value, Some(MyInt { value: 42 }));

    let serialized = json::to_string(&maybe);
    assert_eq!(serialized, r#"{"value":"42"}"#);

    // With null
    let json2 = r#"{"value":null}"#;
    let maybe2: MaybeInt = json::from_str(json2).unwrap();
    assert!(maybe2.value.is_none());

    let serialized2 = json::to_string(&maybe2);
    assert_eq!(serialized2, r#"{"value":null}"#);
}

/// Test container-level proxy with nested Vec<Vec<T>>.
#[test]
fn test_nested_vec_with_container_proxy() {
    #[derive(Facet, Debug, Clone, PartialEq)]
    pub struct NestedCollection {
        pub matrix: Vec<Vec<MyInt>>,
    }

    let json = r#"{"matrix":[["1","2"],["3","4"]]}"#;
    let nested: NestedCollection = json::from_str(json).unwrap();
    assert_eq!(nested.matrix.len(), 2);
    assert_eq!(
        nested.matrix[0],
        vec![MyInt { value: 1 }, MyInt { value: 2 }]
    );
    assert_eq!(
        nested.matrix[1],
        vec![MyInt { value: 3 }, MyInt { value: 4 }]
    );

    let serialized = json::to_string(&nested);
    assert_eq!(serialized, r#"{"matrix":[["1","2"],["3","4"]]}"#);
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

    impl TryFrom<&MyInt> for HexIntProxy {
        type Error = std::convert::Infallible;
        fn try_from(v: &MyInt) -> Result<Self, Self::Error> {
            Ok(HexIntProxy(format!("{:x}", v.value)))
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

    let json = r#"{"decimal":"255","hex":"ff"}"#;
    let mixed: Mixed = json::from_str(json).unwrap();
    assert_eq!(mixed.decimal, MyInt { value: 255 });
    assert_eq!(mixed.hex, MyInt { value: 255 });

    let serialized = json::to_string(&mixed);
    assert_eq!(serialized, r#"{"decimal":"255","hex":"ff"}"#);
}

/// Test round-trip serialization/deserialization.
#[test]
fn test_round_trip() {
    #[derive(Facet, Debug, Clone, PartialEq)]
    pub struct Complex {
        pub single: MyInt,
        pub optional: Option<MyInt>,
        pub list: Vec<MyInt>,
    }

    let original = Complex {
        single: MyInt { value: 100 },
        optional: Some(MyInt { value: 200 }),
        list: vec![MyInt { value: 1 }, MyInt { value: 2 }, MyInt { value: 3 }],
    };

    let serialized = json::to_string(&original);
    let deserialized: Complex = json::from_str(&serialized).unwrap();
    assert_eq!(original, deserialized);
}

/// Test container-level proxy on an enum.
#[test]
fn test_enum_container_proxy() {
    /// An enum with container-level proxy
    #[derive(Facet, Debug, Clone, PartialEq)]
    #[facet(proxy = IntAsString)]
    #[repr(C)]
    pub enum MyIntEnum {
        Value { value: i32 },
    }

    impl TryFrom<IntAsString> for MyIntEnum {
        type Error = std::num::ParseIntError;
        fn try_from(proxy: IntAsString) -> Result<Self, Self::Error> {
            Ok(MyIntEnum::Value {
                value: proxy.0.parse()?,
            })
        }
    }

    impl TryFrom<&MyIntEnum> for IntAsString {
        type Error = std::convert::Infallible;
        fn try_from(v: &MyIntEnum) -> Result<Self, Self::Error> {
            match v {
                MyIntEnum::Value { value } => Ok(IntAsString(value.to_string())),
            }
        }
    }

    #[derive(Facet, Debug, Clone, PartialEq)]
    pub struct EnumWrapper {
        pub item: MyIntEnum,
    }

    let json = r#"{"item":"42"}"#;
    let wrapper: EnumWrapper = json::from_str(json).unwrap();
    assert_eq!(wrapper.item, MyIntEnum::Value { value: 42 });

    let serialized = json::to_string(&wrapper);
    assert_eq!(serialized, r#"{"item":"42"}"#);
}

/// Test deserialization error propagation from proxy conversion.
#[test]
fn test_proxy_conversion_error() {
    #[derive(Facet, Debug, Clone, PartialEq)]
    pub struct Wrapper {
        pub item: MyInt,
    }

    // Invalid integer string should fail
    let json = r#"{"item":"not_a_number"}"#;
    let result: Result<Wrapper, _> = json::from_str(json);
    assert!(result.is_err());
}
