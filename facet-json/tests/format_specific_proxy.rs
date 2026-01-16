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
