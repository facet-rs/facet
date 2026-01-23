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

/// A proxy type that wraps strings (uses FromStr/Display).
#[derive(Facet, Clone, Debug)]
#[facet(transparent)]
pub struct StringRepr(pub String);

impl TryFrom<StringRepr> for XmlScaleRangeName {
    type Error = &'static str;
    fn try_from(value: StringRepr) -> Result<Self, Self::Error> {
        value.0.parse()
    }
}

impl From<&XmlScaleRangeName> for StringRepr {
    fn from(_value: &XmlScaleRangeName) -> Self {
        StringRepr("Scale_Range".to_string())
    }
}

/// A zero-sized type that always serializes as a specific constant string.
/// The proxy is defined at the container level, not on the field.
#[derive(Debug, Default, Clone, Copy, Facet, PartialEq)]
#[facet(xml::proxy = StringRepr)]
pub struct XmlScaleRangeName;

impl core::fmt::Display for XmlScaleRangeName {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Scale_Range")
    }
}

impl core::str::FromStr for XmlScaleRangeName {
    type Err = &'static str;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s == "Scale_Range" {
            Ok(Self)
        } else {
            Err("expected `Scale_Range`")
        }
    }
}

/// A struct that uses the XmlScaleRangeName type as a field.
/// The proxy is defined on XmlScaleRangeName (container level), not on this field.
#[derive(Facet, Debug, PartialEq)]
#[facet(rename = "Array")]
struct ArrayWithContainerProxy {
    #[facet(facet_xml::attribute)]
    name: XmlScaleRangeName,
}

/// Test that container-level proxies work when the type is used as a field.
/// This is a regression test for <https://github.com/facet-rs/facet/issues/1825>.
#[test]
fn test_container_level_proxy_in_field_deserialization() {
    let xml = r#"<Array name="Scale_Range" />"#;
    let data: ArrayWithContainerProxy = from_str(xml).unwrap();
    assert_eq!(data.name, XmlScaleRangeName);
}

/// Test serialization also works with container-level proxies.
#[test]
fn test_container_level_proxy_in_field_serialization() {
    let data = ArrayWithContainerProxy {
        name: XmlScaleRangeName,
    };
    let xml = to_string(&data).unwrap();
    assert!(
        xml.contains("Scale_Range"),
        "XML should contain 'Scale_Range', got: {xml}"
    );
}

/// Test round-trip with container-level proxy.
#[test]
fn test_container_level_proxy_roundtrip() {
    let original = ArrayWithContainerProxy {
        name: XmlScaleRangeName,
    };
    let xml = to_string(&original).unwrap();
    let roundtripped: ArrayWithContainerProxy = from_str(&xml).unwrap();
    assert_eq!(original, roundtripped);
}

// ============================================================================
// Additional proxy coverage tests
// ============================================================================

/// A struct proxy for testing container-level proxies on struct types.
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct PointProxy {
    pub x: i32,
    pub y: i32,
}

/// A point type that uses a proxy for XML serialization.
/// The proxy has different field names to verify the proxy shape is used.
#[derive(Facet, Debug, Clone, PartialEq)]
#[facet(xml::proxy = PointProxy)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

impl TryFrom<PointProxy> for Point {
    type Error = std::convert::Infallible;
    fn try_from(proxy: PointProxy) -> Result<Self, Self::Error> {
        Ok(Point {
            x: proxy.x,
            y: proxy.y,
        })
    }
}

impl From<&Point> for PointProxy {
    fn from(p: &Point) -> Self {
        PointProxy { x: p.x, y: p.y }
    }
}

/// Test container-level proxy as an element field (not attribute).
#[derive(Facet, Debug, PartialEq)]
struct ContainerWithPointElement {
    #[facet(rename = "location")]
    point: Point,
}

#[test]
fn test_container_level_proxy_as_element_field_roundtrip() {
    let original = ContainerWithPointElement {
        point: Point { x: 10, y: 20 },
    };
    let xml = to_string(&original).unwrap();
    eprintln!("XML: {xml}");
    assert!(xml.contains("<location>"), "Should have location element");
    assert!(xml.contains("<x>10</x>"), "Should have x element");
    assert!(xml.contains("<y>20</y>"), "Should have y element");

    let roundtripped: ContainerWithPointElement = from_str(&xml).unwrap();
    assert_eq!(original, roundtripped);
}

/// Test container-level proxy in Option<T>.
#[derive(Facet, Debug, PartialEq)]
struct ContainerWithOptionalPoint {
    #[facet(rename = "location")]
    point: Option<Point>,
}

#[test]
fn test_container_level_proxy_in_option_some_roundtrip() {
    let original = ContainerWithOptionalPoint {
        point: Some(Point { x: 5, y: 15 }),
    };
    let xml = to_string(&original).unwrap();
    eprintln!("XML: {xml}");

    let roundtripped: ContainerWithOptionalPoint = from_str(&xml).unwrap();
    assert_eq!(original, roundtripped);
}

#[test]
fn test_container_level_proxy_in_option_none_roundtrip() {
    let original = ContainerWithOptionalPoint { point: None };
    let xml = to_string(&original).unwrap();
    eprintln!("XML: {xml}");

    let roundtripped: ContainerWithOptionalPoint = from_str(&xml).unwrap();
    assert_eq!(original, roundtripped);
}

/// Test container-level proxy in Vec<T>.
#[derive(Facet, Debug, PartialEq)]
struct ContainerWithPointList {
    #[facet(rename = "point")]
    points: Vec<Point>,
}

#[test]
fn test_container_level_proxy_in_vec_roundtrip() {
    let original = ContainerWithPointList {
        points: vec![
            Point { x: 1, y: 2 },
            Point { x: 3, y: 4 },
            Point { x: 5, y: 6 },
        ],
    };
    let xml = to_string(&original).unwrap();
    eprintln!("XML: {xml}");

    let roundtripped: ContainerWithPointList = from_str(&xml).unwrap();
    assert_eq!(original, roundtripped);
}

#[test]
fn test_container_level_proxy_in_vec_empty_roundtrip() {
    let original = ContainerWithPointList { points: vec![] };
    let xml = to_string(&original).unwrap();
    eprintln!("XML: {xml}");

    let roundtripped: ContainerWithPointList = from_str(&xml).unwrap();
    assert_eq!(original, roundtripped);
}

/// Test container-level proxy as the root type.
#[test]
fn test_container_level_proxy_as_root_type_roundtrip() {
    let original = Point { x: 100, y: 200 };
    let xml = to_string(&original).unwrap();
    eprintln!("XML: {xml}");
    // The element name should come from PointProxy's shape
    assert!(
        xml.contains("<pointProxy>") || xml.contains("<point>"),
        "Should serialize as pointProxy or point element"
    );

    let roundtripped: Point = from_str(&xml).unwrap();
    assert_eq!(original, roundtripped);
}

/// Test proxy in nested struct.
#[derive(Facet, Debug, PartialEq)]
struct OuterContainer {
    name: String,
    inner: InnerWithProxy,
}

#[derive(Facet, Debug, PartialEq)]
struct InnerWithProxy {
    #[facet(rename = "pos")]
    position: Point,
}

#[test]
fn test_proxy_in_nested_struct_roundtrip() {
    let original = OuterContainer {
        name: "test".to_string(),
        inner: InnerWithProxy {
            position: Point { x: 42, y: 84 },
        },
    };
    let xml = to_string(&original).unwrap();
    eprintln!("XML: {xml}");

    let roundtripped: OuterContainer = from_str(&xml).unwrap();
    assert_eq!(original, roundtripped);
}

/// A u32 wrapper that uses binary string proxy at container level.
#[derive(Facet, Debug, Clone, PartialEq)]
#[facet(xml::proxy = BinaryString)]
pub struct BinaryU32(pub u32);

impl TryFrom<BinaryString> for BinaryU32 {
    type Error = std::num::ParseIntError;
    fn try_from(proxy: BinaryString) -> Result<Self, Self::Error> {
        let s = proxy.0.trim_start_matches("0b");
        Ok(BinaryU32(u32::from_str_radix(s, 2)?))
    }
}

impl From<&BinaryU32> for BinaryString {
    fn from(v: &BinaryU32) -> Self {
        BinaryString(format!("0b{:b}", v.0))
    }
}

/// Test container-level proxy on item type in Vec<T>.
#[derive(Facet, Debug, PartialEq)]
struct ContainerWithProxiedItemList {
    #[facet(rename = "value")]
    values: Vec<BinaryU32>,
}

#[test]
fn test_container_level_proxy_on_vec_items_roundtrip() {
    let original = ContainerWithProxiedItemList {
        values: vec![BinaryU32(0b1010), BinaryU32(0b1100), BinaryU32(0b1111)],
    };
    let xml = to_string(&original).unwrap();
    eprintln!("XML: {xml}");
    assert!(xml.contains("0b1010"), "Should use binary format");
    assert!(xml.contains("0b1100"), "Should use binary format");
    assert!(xml.contains("0b1111"), "Should use binary format");

    let roundtripped: ContainerWithProxiedItemList = from_str(&xml).unwrap();
    assert_eq!(original, roundtripped);
}
