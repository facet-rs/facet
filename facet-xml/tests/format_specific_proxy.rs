//! Tests for format-specific proxy attributes in XML.
//!
//! This tests the `#[facet(xml::proxy = ...)]` syntax for format-specific proxy types.

use facet::Facet;
use facet_testhelpers::test;
use facet_xml::{from_str, to_string};

/// A proxy type that formats values as hex strings (for JSON).
#[derive(Facet, Clone, Debug)]
#[facet(transparent)]
pub struct HexString(pub String);

/// A proxy type that formats values as binary strings (for XML).
#[derive(Facet, Clone, Debug)]
#[facet(transparent)]
pub struct BinaryString(pub String);

/// A type that uses different proxies for different formats.
/// - For XML, the value is serialized as a binary string
/// - For JSON (or other formats), use the default hex proxy
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct FormatAwareValue {
    pub name: String,
    #[facet(xml::proxy = BinaryString)]
    #[facet(proxy = HexString)]
    pub value: u32,
}

// JSON/default proxy conversion: u32 <-> hex string
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

// XML proxy conversion: u32 <-> binary string
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
fn test_xml_format_specific_proxy_serialization() {
    let data = FormatAwareValue {
        name: "test".to_string(),
        value: 255,
    };

    // XML should use the binary proxy (xml::proxy takes precedence)
    let xml = to_string(&data).unwrap();
    assert!(
        xml.contains("0b11111111"),
        "XML should use binary format, got: {xml}"
    );
}

#[test]
fn test_binary_string_conversion() {
    // Test that our TryFrom works correctly
    let bin = BinaryString("0b1010".to_string());
    let value: u32 = bin.try_into().unwrap();
    assert_eq!(value, 0b1010);
}

#[test]
fn test_xml_format_specific_proxy_deserialization() {
    let xml = r#"<formatAwareValue><name>test</name><value>0b11010</value></formatAwareValue>"#;
    let data: FormatAwareValue = from_str(xml).unwrap();

    assert_eq!(data.name, "test");
    assert_eq!(data.value, 0b11010);
}

/// A struct that only has an XML-specific proxy (no fallback).
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct XmlOnlyProxy {
    pub label: String,
    #[facet(xml::proxy = BinaryString)]
    pub id: u32,
}

#[test]
fn test_xml_only_proxy_roundtrip() {
    let original = XmlOnlyProxy {
        label: "item".to_string(),
        id: 0b10101010,
    };

    let xml = to_string(&original).unwrap();
    assert!(
        xml.contains("0b10101010"),
        "XML should use binary format, got: {xml}"
    );

    let roundtripped: XmlOnlyProxy = from_str(&xml).unwrap();
    assert_eq!(original, roundtripped);
}

/// Test that format-specific proxy shapes are correctly stored in the Field.
#[test]
fn test_xml_format_proxy_field_metadata() {
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

    // Should have one for "xml"
    let xml_proxy = value_field.format_proxy("xml");
    assert!(xml_proxy.is_some(), "Should have xml proxy");

    // Should also have the default proxy
    assert!(value_field.proxy.is_some(), "Should have default proxy");

    // effective_proxy with "xml" should return the xml-specific one
    let effective_xml = value_field.effective_proxy(Some("xml"));
    assert!(effective_xml.is_some());

    // effective_proxy with "json" (no specific proxy) should fall back to default
    let effective_json = value_field.effective_proxy(Some("json"));
    assert!(
        effective_json.is_some(),
        "Should fall back to default proxy"
    );

    // They should be different (xml-specific vs default)
    assert_ne!(
        effective_xml.map(|p| p.shape.id),
        effective_json.map(|p| p.shape.id),
        "XML and JSON should use different proxies"
    );
}
