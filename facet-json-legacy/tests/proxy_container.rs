//! Tests for container-level proxy attribute (#1109).
//!
//! Container-level proxy allows `#[facet(proxy = ProxyType)]` at the struct/enum level,
//! so any field of that type automatically uses the proxy without per-field annotations.
//! This includes nested types like `Vec<Mine>`, `Option<Mine>`, etc.

use facet::Facet;
use facet_json_legacy as json;

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

    impl From<&MyIntEnum> for IntAsString {
        fn from(v: &MyIntEnum) -> Self {
            match v {
                MyIntEnum::Value { value } => IntAsString(value.to_string()),
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

/// Test container-level proxy on a generic struct.
/// This is a regression test for https://github.com/facet-rs/facet/issues/1109
#[test]
fn test_generic_struct_with_container_proxy() {
    use core::fmt::Debug;

    /// A simple proxy that wraps a value
    #[derive(Facet, Clone, Debug)]
    pub struct ValueProxy<T: Clone + Debug + 'static> {
        pub wrapped: T,
    }

    /// A generic struct that uses a generic proxy
    /// Note: The TryFrom impls must be blanket (for all T) or match the bounds here
    #[derive(Facet, Clone, Debug, PartialEq)]
    #[facet(proxy = ValueProxy<T>)]
    pub struct GenericValue<T: Clone + Debug + 'static> {
        pub inner: T,
    }

    // Blanket From impls that work for all T
    impl<T: Clone + Debug + 'static> From<ValueProxy<T>> for GenericValue<T> {
        fn from(proxy: ValueProxy<T>) -> Self {
            GenericValue {
                inner: proxy.wrapped,
            }
        }
    }

    impl<T: Clone + Debug + 'static> From<&GenericValue<T>> for ValueProxy<T> {
        fn from(v: &GenericValue<T>) -> Self {
            ValueProxy {
                wrapped: v.inner.clone(),
            }
        }
    }

    // Test with i32
    #[derive(Facet, Debug, Clone, PartialEq)]
    pub struct WrapperI32 {
        pub item: GenericValue<i32>,
    }

    let json = r#"{"item":{"wrapped":42}}"#;
    let wrapper: WrapperI32 = json::from_str(json).unwrap();
    assert_eq!(wrapper.item, GenericValue { inner: 42 });

    let serialized = json::to_string(&wrapper);
    assert_eq!(serialized, r#"{"item":{"wrapped":42}}"#);

    // Test with String
    #[derive(Facet, Debug, Clone, PartialEq)]
    pub struct WrapperString {
        pub item: GenericValue<String>,
    }

    let json = r#"{"item":{"wrapped":"hello"}}"#;
    let wrapper: WrapperString = json::from_str(json).unwrap();
    assert_eq!(
        wrapper.item,
        GenericValue {
            inner: "hello".to_string()
        }
    );

    let serialized = json::to_string(&wrapper);
    assert_eq!(serialized, r#"{"item":{"wrapped":"hello"}}"#);
}

/// Test multiple structs using the same generic proxy type.
/// This ensures the inherent impl pattern works correctly when the same proxy
/// is reused across different concrete types.
#[test]
fn test_multiple_structs_same_generic_proxy() {
    use core::fmt::Debug;

    /// A shared proxy type used by multiple structs
    #[derive(Facet, Clone, Debug)]
    pub struct SharedProxy<T: Clone + Debug + 'static> {
        pub data: T,
    }

    /// First struct using SharedProxy
    #[derive(Facet, Clone, Debug, PartialEq)]
    #[facet(proxy = SharedProxy<i32>)]
    pub struct IntHolder {
        pub value: i32,
    }

    impl From<SharedProxy<i32>> for IntHolder {
        fn from(proxy: SharedProxy<i32>) -> Self {
            IntHolder { value: proxy.data }
        }
    }

    impl From<&IntHolder> for SharedProxy<i32> {
        fn from(v: &IntHolder) -> Self {
            SharedProxy { data: v.value }
        }
    }

    /// Second struct using SharedProxy with a different type parameter
    #[derive(Facet, Clone, Debug, PartialEq)]
    #[facet(proxy = SharedProxy<String>)]
    pub struct StringHolder {
        pub text: String,
    }

    impl From<SharedProxy<String>> for StringHolder {
        fn from(proxy: SharedProxy<String>) -> Self {
            StringHolder { text: proxy.data }
        }
    }

    impl From<&StringHolder> for SharedProxy<String> {
        fn from(v: &StringHolder) -> Self {
            SharedProxy {
                data: v.text.clone(),
            }
        }
    }

    /// Third struct also using SharedProxy<i32> - same monomorphization as IntHolder
    #[derive(Facet, Clone, Debug, PartialEq)]
    #[facet(proxy = SharedProxy<i32>)]
    pub struct AnotherIntHolder {
        pub num: i32,
    }

    impl From<SharedProxy<i32>> for AnotherIntHolder {
        fn from(proxy: SharedProxy<i32>) -> Self {
            AnotherIntHolder { num: proxy.data }
        }
    }

    impl From<&AnotherIntHolder> for SharedProxy<i32> {
        fn from(v: &AnotherIntHolder) -> Self {
            SharedProxy { data: v.num }
        }
    }

    // Test IntHolder
    let json = r#"{"data":42}"#;
    let holder: IntHolder = json::from_str(json).unwrap();
    assert_eq!(holder, IntHolder { value: 42 });
    assert_eq!(json::to_string(&holder), r#"{"data":42}"#);

    // Test StringHolder
    let json = r#"{"data":"hello"}"#;
    let holder: StringHolder = json::from_str(json).unwrap();
    assert_eq!(
        holder,
        StringHolder {
            text: "hello".to_string()
        }
    );
    assert_eq!(json::to_string(&holder), r#"{"data":"hello"}"#);

    // Test AnotherIntHolder (same proxy type as IntHolder)
    let json = r#"{"data":99}"#;
    let holder: AnotherIntHolder = json::from_str(json).unwrap();
    assert_eq!(holder, AnotherIntHolder { num: 99 });
    assert_eq!(json::to_string(&holder), r#"{"data":99}"#);

    // Test all three in a combined struct
    #[derive(Facet, Debug, Clone, PartialEq)]
    pub struct Combined {
        pub int_holder: IntHolder,
        pub string_holder: StringHolder,
        pub another_int: AnotherIntHolder,
    }

    let json =
        r#"{"int_holder":{"data":1},"string_holder":{"data":"test"},"another_int":{"data":2}}"#;
    let combined: Combined = json::from_str(json).unwrap();
    assert_eq!(combined.int_holder, IntHolder { value: 1 });
    assert_eq!(
        combined.string_holder,
        StringHolder {
            text: "test".to_string()
        }
    );
    assert_eq!(combined.another_int, AnotherIntHolder { num: 2 });
}

/// Test deserialization of untagged enums with struct variants.
/// This is a regression test for https://github.com/facet-rs/facet/issues/1175
#[test]
fn test_untagged_struct_variants_deserialization() {
    #[derive(Debug, Facet, PartialEq)]
    #[repr(C)]
    #[facet(untagged)]
    #[allow(dead_code)]
    enum Shape {
        Circle { radius: f64 },
        Rectangle { width: f64, height: f64 },
    }

    // Deserialize into Circle variant
    let json_circle = r#"{"radius":5.0}"#;
    let circle: Shape = json::from_str(json_circle).expect("should deserialize Circle");
    assert_eq!(circle, Shape::Circle { radius: 5.0 });

    // Deserialize into Rectangle variant
    let json_rect = r#"{"width":10.0,"height":20.0}"#;
    let rect: Shape = json::from_str(json_rect).expect("should deserialize Rectangle");
    assert_eq!(
        rect,
        Shape::Rectangle {
            width: 10.0,
            height: 20.0
        }
    );
}

/// Test container-level proxy with an untagged enum.
/// This is a regression test for https://github.com/facet-rs/facet/issues/1175
#[test]
fn test_proxy_with_untagged_enum() {
    // Simplified stand-in for Curve64
    #[derive(Facet, Debug, Clone, PartialEq)]
    pub struct Curve64 {
        value: f64,
    }

    // The proxy enum - untagged means it should match based on structure
    #[derive(Facet, Debug, Clone, PartialEq)]
    #[facet(untagged)]
    #[repr(C)]
    pub enum XCurveRepr {
        Linear(Curve64),
        Constant { constant: f64 },
        Special { spe: Curve64 },
    }

    // The main type that uses the proxy
    #[derive(Facet, Debug, Clone)]
    #[facet(proxy = XCurveRepr)]
    pub struct XCurve {
        pub repr: XCurveRepr,
    }

    impl From<XCurveRepr> for XCurve {
        fn from(repr: XCurveRepr) -> Self {
            XCurve { repr }
        }
    }

    impl From<&XCurve> for XCurveRepr {
        fn from(curve: &XCurve) -> Self {
            curve.repr.clone()
        }
    }

    // First, verify the untagged enum works directly
    let json = r#"{"constant":0.0}"#;
    let repr: XCurveRepr = json::from_str(json).expect("untagged enum should parse directly");
    assert_eq!(repr, XCurveRepr::Constant { constant: 0.0 });

    // Now test through the proxy
    let curve: XCurve = json::from_str(json).expect("proxy with untagged enum should work");
    assert_eq!(curve.repr, XCurveRepr::Constant { constant: 0.0 });
}
