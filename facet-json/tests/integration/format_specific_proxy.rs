//! Tests for format-specific proxy attributes.
//!
//! This tests the `#[facet(json::proxy = ...)]` syntax for format-specific proxy types.

use facet::Facet;
use facet_json::{from_str, to_string};
use facet_testhelpers::test;

/// A proxy type that formats values as hex strings.
#[derive(Facet, Clone, Debug)]
#[facet(transparent)]
pub struct HexString(pub String);

/// A proxy type that formats values as binary strings.
#[derive(Facet, Clone, Debug)]
#[facet(transparent)]
pub struct BinaryString(pub String);

/// A type that uses different proxies for different formats.
/// - For JSON, the value is serialized as a hex string
/// - For other formats (without format_namespace), use the default proxy
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct FormatAwareValue {
    pub name: String,
    #[facet(json::proxy = HexString)]
    #[facet(proxy = BinaryString)]
    pub value: u32,
}

// JSON proxy conversion: u32 <-> hex string
impl TryFrom<HexString> for u32 {
    type Error = std::num::ParseIntError;
    fn try_from(proxy: HexString) -> Result<Self, Self::Error> {
        let s = proxy.0.trim_start_matches("0x").trim_start_matches("0X");
        u32::from_str_radix(s, 16)
    }
}

impl From<&u32> for HexString {
    fn from(v: &u32) -> Self {
        HexString(format!("0x{:x}", v))
    }
}

// Default proxy conversion: u32 <-> binary string
impl TryFrom<BinaryString> for u32 {
    type Error = std::num::ParseIntError;
    fn try_from(proxy: BinaryString) -> Result<Self, Self::Error> {
        u32::from_str_radix(proxy.0.trim_start_matches("0b"), 2)
    }
}

impl From<&u32> for BinaryString {
    fn from(v: &u32) -> Self {
        BinaryString(format!("0b{:b}", v))
    }
}

#[test]
fn test_format_specific_proxy_serialization() {
    let data = FormatAwareValue {
        name: "test".to_string(),
        value: 255,
    };

    // JSON should use the hex proxy (json::proxy takes precedence)
    let json = to_string(&data).unwrap();
    assert!(
        json.contains("0xff"),
        "JSON should use hex format, got: {json}"
    );
}

#[test]
fn test_hex_string_conversion() {
    // Test that our TryFrom works correctly
    let hex = HexString("0x1a".to_string());
    let value: u32 = hex.try_into().unwrap();
    assert_eq!(value, 0x1a);
}

#[test]
fn test_format_specific_proxy_deserialization() {
    let json = r#"{"name":"test","value":"0x1a"}"#;
    let data: FormatAwareValue = from_str(json).unwrap();

    assert_eq!(data.name, "test");
    assert_eq!(data.value, 0x1a);
}

/// A struct that only has a format-specific proxy (no fallback).
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct JsonOnlyProxy {
    pub label: String,
    #[facet(json::proxy = HexString)]
    pub id: u32,
}

#[test]
fn test_json_only_proxy_roundtrip() {
    let original = JsonOnlyProxy {
        label: "item".to_string(),
        id: 0xbeef,
    };

    let json = to_string(&original).unwrap();
    assert!(
        json.contains("0xbeef"),
        "JSON should use hex format, got: {json}"
    );

    let roundtripped: JsonOnlyProxy = from_str(&json).unwrap();
    assert_eq!(original, roundtripped);
}

/// Test that format-specific proxy shapes are correctly stored in the Field.
#[test]
fn test_format_proxy_field_metadata() {
    use facet::Facet;
    use facet_core::{Type, UserType};

    let shape = <FormatAwareValue as Facet>::SHAPE;

    // Get the struct type
    let struct_type = match shape.ty {
        Type::User(UserType::Struct(s)) => s,
        _ => panic!("Expected struct type, got {:?}", shape.ty),
    };

    // Find the "value" field
    let value_field = struct_type
        .fields
        .iter()
        .find(|f| f.name == "value")
        .expect("Should have value field");

    // Should have format_proxies
    assert!(
        !value_field.format_proxies.is_empty(),
        "Should have format-specific proxies"
    );

    // Should have one for "json"
    let json_proxy = value_field.format_proxy("json");
    assert!(json_proxy.is_some(), "Should have json proxy");

    // Should also have the default proxy
    assert!(value_field.proxy.is_some(), "Should have default proxy");

    // effective_proxy with "json" should return the json-specific one
    let effective_json = value_field.effective_proxy(Some("json"));
    assert!(effective_json.is_some());

    // effective_proxy with "xml" (no specific proxy) should fall back to default
    let effective_xml = value_field.effective_proxy(Some("xml"));
    assert!(effective_xml.is_some(), "Should fall back to default proxy");

    // They should be different (json-specific vs default)
    assert_ne!(
        effective_json.map(|p| p.shape.id),
        effective_xml.map(|p| p.shape.id),
        "JSON and XML should use different proxies"
    );
}

// =============================================================================
// Container-level format-specific proxy tests
// =============================================================================

/// A wrapper type that serializes to JSON as a hex string representation.
/// The container itself uses a proxy for the entire type.
#[derive(Facet, Debug, Clone, PartialEq)]
#[facet(transparent)]
pub struct JsonHexNumber(pub u32);

/// Proxy type for container-level JSON serialization.
#[derive(Facet, Clone, Debug)]
#[facet(transparent)]
pub struct JsonNumberProxy(pub String);

/// Proxy type for container-level fallback serialization.
#[derive(Facet, Clone, Debug)]
#[facet(transparent)]
pub struct DefaultNumberProxy(pub String);

/// A container type that uses different proxies at the container level.
/// - For JSON format: uses JsonNumberProxy (hex representation)
/// - For other formats: uses DefaultNumberProxy (decimal representation)
#[derive(Facet, Debug, Clone, PartialEq)]
#[facet(json::proxy = JsonNumberProxy)]
#[facet(proxy = DefaultNumberProxy)]
pub struct ContainerFormatProxy {
    pub inner: u32,
}

// Container-level JSON proxy conversions: ContainerFormatProxy <-> JsonNumberProxy
impl TryFrom<JsonNumberProxy> for ContainerFormatProxy {
    type Error = std::num::ParseIntError;
    fn try_from(proxy: JsonNumberProxy) -> Result<Self, Self::Error> {
        let s = proxy.0.trim_start_matches("0x").trim_start_matches("0X");
        let inner = u32::from_str_radix(s, 16)?;
        Ok(ContainerFormatProxy { inner })
    }
}

impl From<&ContainerFormatProxy> for JsonNumberProxy {
    fn from(v: &ContainerFormatProxy) -> Self {
        JsonNumberProxy(format!("0x{:x}", v.inner))
    }
}

// Container-level default proxy conversions: ContainerFormatProxy <-> DefaultNumberProxy
impl TryFrom<DefaultNumberProxy> for ContainerFormatProxy {
    type Error = std::num::ParseIntError;
    fn try_from(proxy: DefaultNumberProxy) -> Result<Self, Self::Error> {
        let inner = proxy.0.parse::<u32>()?;
        Ok(ContainerFormatProxy { inner })
    }
}

impl From<&ContainerFormatProxy> for DefaultNumberProxy {
    fn from(v: &ContainerFormatProxy) -> Self {
        DefaultNumberProxy(format!("{}", v.inner))
    }
}

#[test]
fn test_container_format_specific_proxy_serialization() {
    let data = ContainerFormatProxy { inner: 255 };

    // JSON should use the container-level json::proxy (hex format)
    let json = to_string(&data).unwrap();
    assert!(
        json.contains("0xff"),
        "JSON should use hex format via container proxy, got: {json}"
    );
}

#[test]
fn test_container_format_specific_proxy_deserialization() {
    let json = r#""0x1a""#;
    let data: ContainerFormatProxy = from_str(json).unwrap();

    assert_eq!(data.inner, 0x1a);
}

#[test]
fn test_container_format_specific_proxy_roundtrip() {
    let original = ContainerFormatProxy { inner: 0xbeef };

    let json = to_string(&original).unwrap();
    assert!(
        json.contains("0xbeef"),
        "JSON should use hex format, got: {json}"
    );

    let roundtripped: ContainerFormatProxy = from_str(&json).unwrap();
    assert_eq!(original, roundtripped);
}

#[test]
fn test_container_format_proxy_shape_metadata() {
    use facet::Facet;

    let shape = <ContainerFormatProxy as Facet>::SHAPE;

    // Should have format_proxies at the Shape level
    assert!(
        !shape.format_proxies.is_empty(),
        "Should have container-level format-specific proxies"
    );

    // Should have one for "json"
    let json_proxy = shape.format_proxy("json");
    assert!(
        json_proxy.is_some(),
        "Should have json proxy at container level"
    );

    // Should also have the default proxy
    assert!(
        shape.proxy.is_some(),
        "Should have default container-level proxy"
    );

    // effective_proxy with "json" should return the json-specific one
    let effective_json = shape.effective_proxy(Some("json"));
    assert!(effective_json.is_some());

    // effective_proxy with "xml" (no specific proxy) should fall back to default
    let effective_xml = shape.effective_proxy(Some("xml"));
    assert!(effective_xml.is_some(), "Should fall back to default proxy");

    // They should be different (json-specific vs default)
    assert_ne!(
        effective_json.map(|p| p.shape.id),
        effective_xml.map(|p| p.shape.id),
        "JSON and XML should use different container proxies"
    );
}

/// A container with only a format-specific proxy (no default).
#[derive(Facet, Debug, Clone, PartialEq)]
#[facet(json::proxy = JsonNumberProxy)]
pub struct JsonOnlyContainerProxy {
    pub inner: u32,
}

// Conversions for JsonOnlyContainerProxy
impl TryFrom<JsonNumberProxy> for JsonOnlyContainerProxy {
    type Error = std::num::ParseIntError;
    fn try_from(proxy: JsonNumberProxy) -> Result<Self, Self::Error> {
        let s = proxy.0.trim_start_matches("0x").trim_start_matches("0X");
        let inner = u32::from_str_radix(s, 16)?;
        Ok(JsonOnlyContainerProxy { inner })
    }
}

impl From<&JsonOnlyContainerProxy> for JsonNumberProxy {
    fn from(v: &JsonOnlyContainerProxy) -> Self {
        JsonNumberProxy(format!("0x{:x}", v.inner))
    }
}

#[test]
fn test_json_only_container_proxy_roundtrip() {
    let original = JsonOnlyContainerProxy { inner: 0xcafe };

    let json = to_string(&original).unwrap();
    assert!(
        json.contains("0xcafe"),
        "JSON should use hex format, got: {json}"
    );

    let roundtripped: JsonOnlyContainerProxy = from_str(&json).unwrap();
    assert_eq!(original, roundtripped);
}

#[test]
fn test_json_only_container_proxy_metadata() {
    use facet::Facet;

    let shape = <JsonOnlyContainerProxy as Facet>::SHAPE;

    // Should have format_proxies at the Shape level
    assert!(
        !shape.format_proxies.is_empty(),
        "Should have container-level format-specific proxies"
    );

    // Should have one for "json"
    let json_proxy = shape.format_proxy("json");
    assert!(
        json_proxy.is_some(),
        "Should have json proxy at container level"
    );

    // Should NOT have a default proxy
    assert!(
        shape.proxy.is_none(),
        "Should NOT have default container-level proxy"
    );

    // effective_proxy with "json" should return the json-specific one
    let effective_json = shape.effective_proxy(Some("json"));
    assert!(effective_json.is_some());

    // effective_proxy with "xml" (no specific proxy) should return None (no fallback)
    let effective_xml = shape.effective_proxy(Some("xml"));
    assert!(
        effective_xml.is_none(),
        "Should NOT have fallback proxy for xml"
    );
}

// =============================================================================
// Container-level proxy used as a field in another struct
// Regression test for https://github.com/facet-rs/facet/issues/1825
// =============================================================================

/// A proxy type that wraps strings (uses FromStr/Display).
#[derive(Facet, Clone, Debug)]
#[facet(transparent)]
pub struct StringRepr(pub String);

impl TryFrom<StringRepr> for JsonConstValue {
    type Error = &'static str;
    fn try_from(value: StringRepr) -> Result<Self, Self::Error> {
        value.0.parse()
    }
}

impl From<&JsonConstValue> for StringRepr {
    fn from(_value: &JsonConstValue) -> Self {
        StringRepr("CONST_VALUE".to_string())
    }
}

/// A zero-sized type that always serializes to a specific constant string.
/// The proxy is defined at the container level.
#[derive(Debug, Default, Clone, Copy, Facet, PartialEq)]
#[facet(json::proxy = StringRepr)]
pub struct JsonConstValue;

impl core::fmt::Display for JsonConstValue {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "CONST_VALUE")
    }
}

impl core::str::FromStr for JsonConstValue {
    type Err = &'static str;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s == "CONST_VALUE" {
            Ok(Self)
        } else {
            Err("expected `CONST_VALUE`")
        }
    }
}

/// A struct that uses JsonConstValue as a field.
/// The proxy is defined on JsonConstValue (container level), not on this field.
#[derive(Facet, Debug, PartialEq)]
struct StructWithContainerProxyField {
    name: String,
    const_val: JsonConstValue,
}

/// Test that container-level proxies work when the type is used as a field.
/// This is a regression test for <https://github.com/facet-rs/facet/issues/1825>.
#[test]
fn test_container_level_proxy_in_field_deserialization() {
    let json = r#"{"name":"test","const_val":"CONST_VALUE"}"#;
    let data: StructWithContainerProxyField = from_str(json).unwrap();
    assert_eq!(data.name, "test");
    assert_eq!(data.const_val, JsonConstValue);
}

/// Test serialization also works with container-level proxies.
#[test]
fn test_container_level_proxy_in_field_serialization() {
    let data = StructWithContainerProxyField {
        name: "test".to_string(),
        const_val: JsonConstValue,
    };
    let json = to_string(&data).unwrap();
    assert!(
        json.contains("CONST_VALUE"),
        "JSON should contain 'CONST_VALUE', got: {json}"
    );
}

/// Test round-trip with container-level proxy in a field.
#[test]
fn test_container_level_proxy_in_field_roundtrip() {
    let original = StructWithContainerProxyField {
        name: "example".to_string(),
        const_val: JsonConstValue,
    };
    let json = to_string(&original).unwrap();
    let roundtripped: StructWithContainerProxyField = from_str(&json).unwrap();
    assert_eq!(original, roundtripped);
}
