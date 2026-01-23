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

/// A proxy that represents a Vec<u32> as a comma-separated string.
#[derive(Facet, Clone, Debug)]
#[facet(transparent)]
pub struct CommaSeparatedU32s(pub String);

impl TryFrom<CommaSeparatedU32s> for Vec<u32> {
    type Error = std::num::ParseIntError;
    fn try_from(proxy: CommaSeparatedU32s) -> Result<Self, Self::Error> {
        if proxy.0.is_empty() {
            return Ok(vec![]);
        }
        proxy.0.split(',').map(|s| s.trim().parse()).collect()
    }
}

impl From<&Vec<u32>> for CommaSeparatedU32s {
    fn from(v: &Vec<u32>) -> Self {
        CommaSeparatedU32s(
            v.iter()
                .map(|n| n.to_string())
                .collect::<Vec<_>>()
                .join(","),
        )
    }
}

/// Test field-level proxy that converts entire Vec to a single string.
#[derive(Facet, Debug, PartialEq)]
struct ContainerWithCommaSeparatedField {
    name: String,
    #[facet(xml::proxy = CommaSeparatedU32s)]
    numbers: Vec<u32>,
}

#[test]
fn test_field_level_proxy_vec_as_comma_separated_string_roundtrip() {
    let original = ContainerWithCommaSeparatedField {
        name: "test".to_string(),
        numbers: vec![1, 2, 3, 4, 5],
    };
    let xml = to_string(&original).unwrap();
    eprintln!("XML: {xml}");
    assert!(
        xml.contains("1,2,3,4,5"),
        "Should serialize as comma-separated string, got: {xml}"
    );

    let roundtripped: ContainerWithCommaSeparatedField = from_str(&xml).unwrap();
    assert_eq!(original, roundtripped);
}

#[test]
fn test_field_level_proxy_vec_as_comma_separated_string_empty_roundtrip() {
    let original = ContainerWithCommaSeparatedField {
        name: "empty".to_string(),
        numbers: vec![],
    };
    let xml = to_string(&original).unwrap();
    eprintln!("XML: {xml}");

    let roundtripped: ContainerWithCommaSeparatedField = from_str(&xml).unwrap();
    assert_eq!(original, roundtripped);
}

// ============================================================================
// Edge case tests for proxy handling
// ============================================================================

/// Edge case 1: Field-level proxy on an attribute field.
/// Tests that `#[facet(xml::attribute, xml::proxy = P)]` works correctly.
#[derive(Facet, Debug, PartialEq)]
struct StructWithProxiedAttribute {
    name: String,
    #[facet(facet_xml::attribute, xml::proxy = BinaryString)]
    flags: u32,
}

#[test]
fn test_field_level_proxy_on_attribute_roundtrip() {
    let original = StructWithProxiedAttribute {
        name: "test".to_string(),
        flags: 0b10101010,
    };
    let xml = to_string(&original).unwrap();
    eprintln!("XML: {xml}");
    assert!(
        xml.contains(r#"flags="0b10101010""#),
        "Attribute should use binary proxy, got: {xml}"
    );

    let roundtripped: StructWithProxiedAttribute = from_str(&xml).unwrap();
    assert_eq!(original, roundtripped);
}

/// Edge case 2: Field-level proxy combined with rename.
/// Tests that `#[facet(rename = "foo", xml::proxy = P)]` works correctly.
#[derive(Facet, Debug, PartialEq)]
struct StructWithRenamedProxiedField {
    name: String,
    #[facet(rename = "binaryValue", xml::proxy = BinaryString)]
    value: u32,
}

#[test]
fn test_field_level_proxy_with_rename_roundtrip() {
    let original = StructWithRenamedProxiedField {
        name: "test".to_string(),
        value: 0b11110000,
    };
    let xml = to_string(&original).unwrap();
    eprintln!("XML: {xml}");
    assert!(
        xml.contains("<binaryValue>0b11110000</binaryValue>"),
        "Should use renamed element with binary proxy, got: {xml}"
    );

    let roundtripped: StructWithRenamedProxiedField = from_str(&xml).unwrap();
    assert_eq!(original, roundtripped);
}

/// A proxy for Option<String> that uses "N/A" for None.
#[derive(Facet, Clone, Debug)]
#[facet(transparent)]
pub struct OptionalStringProxy(pub String);

impl TryFrom<OptionalStringProxy> for Option<String> {
    type Error = std::convert::Infallible;
    fn try_from(proxy: OptionalStringProxy) -> Result<Self, Self::Error> {
        if proxy.0 == "N/A" {
            Ok(None)
        } else {
            Ok(Some(proxy.0))
        }
    }
}

impl From<&Option<String>> for OptionalStringProxy {
    fn from(opt: &Option<String>) -> Self {
        match opt {
            Some(s) => OptionalStringProxy(s.clone()),
            None => OptionalStringProxy("N/A".to_string()),
        }
    }
}

/// Edge case 3: Field-level proxy on Option<T> where the proxy handles the whole Option.
/// This is different from Option<T> where T has a proxy - here we proxy the Option itself.
#[derive(Facet, Debug, PartialEq)]
struct StructWithProxiedOption {
    name: String,
    #[facet(xml::proxy = OptionalStringProxy)]
    description: Option<String>,
}

#[test]
fn test_field_level_proxy_on_option_some_roundtrip() {
    let original = StructWithProxiedOption {
        name: "test".to_string(),
        description: Some("hello world".to_string()),
    };
    let xml = to_string(&original).unwrap();
    eprintln!("XML: {xml}");
    assert!(
        xml.contains("<description>hello world</description>"),
        "Should serialize Some value, got: {xml}"
    );

    let roundtripped: StructWithProxiedOption = from_str(&xml).unwrap();
    assert_eq!(original, roundtripped);
}

#[test]
fn test_field_level_proxy_on_option_none_roundtrip() {
    let original = StructWithProxiedOption {
        name: "test".to_string(),
        description: None,
    };
    let xml = to_string(&original).unwrap();
    eprintln!("XML: {xml}");
    assert!(
        xml.contains("<description>N/A</description>"),
        "Should serialize None as 'N/A', got: {xml}"
    );

    let roundtripped: StructWithProxiedOption = from_str(&xml).unwrap();
    assert_eq!(original, roundtripped);
}

/// Edge case 4: Multiple fields with different proxies in the same struct.
#[derive(Facet, Debug, PartialEq)]
struct StructWithMultipleProxies {
    name: String,
    #[facet(xml::proxy = BinaryString)]
    binary_value: u32,
    #[facet(xml::proxy = HexString)]
    hex_value: u32,
    #[facet(xml::proxy = CommaSeparatedU32s)]
    list_value: Vec<u32>,
}

#[test]
fn test_multiple_fields_with_different_proxies_roundtrip() {
    let original = StructWithMultipleProxies {
        name: "test".to_string(),
        binary_value: 0b1111,
        hex_value: 0xFF,
        list_value: vec![1, 2, 3],
    };
    let xml = to_string(&original).unwrap();
    eprintln!("XML: {xml}");
    assert!(
        xml.contains("0b1111"),
        "binary_value should use binary proxy, got: {xml}"
    );
    assert!(
        xml.contains("0xff"),
        "hex_value should use hex proxy, got: {xml}"
    );
    assert!(
        xml.contains("1,2,3"),
        "list_value should use comma-separated proxy, got: {xml}"
    );

    let roundtripped: StructWithMultipleProxies = from_str(&xml).unwrap();
    assert_eq!(original, roundtripped);
}

/// Edge case 5: Enum with variant containing a field that has container-level proxy.
#[derive(Facet, Debug, PartialEq)]
#[repr(C)]
enum ShapeEnum {
    Circle { radius: u32 },
    Rectangle { width: u32, height: u32 },
    Point(Point), // Point has container-level proxy
}

#[test]
fn test_enum_variant_with_container_proxy_roundtrip() {
    let original = ShapeEnum::Point(Point { x: 10, y: 20 });
    let xml = to_string(&original).unwrap();
    eprintln!("XML: {xml}");

    let roundtripped: ShapeEnum = from_str(&xml).unwrap();
    assert_eq!(original, roundtripped);
}

#[test]
fn test_enum_variant_without_proxy_still_works() {
    let original = ShapeEnum::Rectangle {
        width: 100,
        height: 50,
    };
    let xml = to_string(&original).unwrap();
    eprintln!("XML: {xml}");

    let roundtripped: ShapeEnum = from_str(&xml).unwrap();
    assert_eq!(original, roundtripped);
}

// ============================================================================
// Proxies inside enum variants - comprehensive tests
// ============================================================================

/// Enum with struct variant that has a field with field-level proxy.
#[derive(Facet, Debug, PartialEq)]
#[repr(C)]
enum EnumWithFieldProxyInStructVariant {
    Named {
        name: String,
        #[facet(xml::proxy = BinaryString)]
        flags: u32,
    },
    Other {
        value: i32,
    },
}

#[test]
fn test_enum_struct_variant_with_field_level_proxy_roundtrip() {
    let original = EnumWithFieldProxyInStructVariant::Named {
        name: "test".to_string(),
        flags: 0b10101010,
    };
    let xml = to_string(&original).unwrap();
    eprintln!("XML: {xml}");
    assert!(
        xml.contains("0b10101010"),
        "Should use binary proxy in struct variant field, got: {xml}"
    );

    let roundtripped: EnumWithFieldProxyInStructVariant = from_str(&xml).unwrap();
    assert_eq!(original, roundtripped);
}

/// Enum with struct variant where a field's type has container-level proxy.
#[derive(Facet, Debug, PartialEq)]
#[repr(C)]
enum EnumWithContainerProxyInStructVariant {
    WithPoint {
        name: String,
        location: Point, // Point has container-level proxy
    },
    WithBinary {
        label: String,
        value: BinaryU32, // BinaryU32 has container-level proxy
    },
}

#[test]
fn test_enum_struct_variant_with_container_proxy_point_roundtrip() {
    let original = EnumWithContainerProxyInStructVariant::WithPoint {
        name: "origin".to_string(),
        location: Point { x: 0, y: 0 },
    };
    let xml = to_string(&original).unwrap();
    eprintln!("XML: {xml}");

    let roundtripped: EnumWithContainerProxyInStructVariant = from_str(&xml).unwrap();
    assert_eq!(original, roundtripped);
}

#[test]
fn test_enum_struct_variant_with_container_proxy_binary_roundtrip() {
    let original = EnumWithContainerProxyInStructVariant::WithBinary {
        label: "flags".to_string(),
        value: BinaryU32(0b11110000),
    };
    let xml = to_string(&original).unwrap();
    eprintln!("XML: {xml}");
    assert!(
        xml.contains("0b11110000"),
        "Should use binary proxy, got: {xml}"
    );

    let roundtripped: EnumWithContainerProxyInStructVariant = from_str(&xml).unwrap();
    assert_eq!(original, roundtripped);
}

/// Enum with newtype variant where the inner type has container-level proxy.
#[derive(Facet, Debug, PartialEq)]
#[repr(C)]
enum EnumWithNewtypeProxyVariant {
    PointVariant(Point),       // Point has container-level proxy
    BinaryVariant(BinaryU32),  // BinaryU32 has container-level proxy
    PlainVariant(String),
}

#[test]
fn test_enum_newtype_variant_with_container_proxy_point_roundtrip() {
    let original = EnumWithNewtypeProxyVariant::PointVariant(Point { x: 42, y: 84 });
    let xml = to_string(&original).unwrap();
    eprintln!("XML: {xml}");

    let roundtripped: EnumWithNewtypeProxyVariant = from_str(&xml).unwrap();
    assert_eq!(original, roundtripped);
}

#[test]
fn test_enum_newtype_variant_with_container_proxy_binary_roundtrip() {
    let original = EnumWithNewtypeProxyVariant::BinaryVariant(BinaryU32(0b1111));
    let xml = to_string(&original).unwrap();
    eprintln!("XML: {xml}");
    assert!(
        xml.contains("0b1111"),
        "Should use binary proxy in newtype variant, got: {xml}"
    );

    let roundtripped: EnumWithNewtypeProxyVariant = from_str(&xml).unwrap();
    assert_eq!(original, roundtripped);
}

#[test]
fn test_enum_newtype_variant_plain_still_works() {
    let original = EnumWithNewtypeProxyVariant::PlainVariant("hello".to_string());
    let xml = to_string(&original).unwrap();
    eprintln!("XML: {xml}");

    let roundtripped: EnumWithNewtypeProxyVariant = from_str(&xml).unwrap();
    assert_eq!(original, roundtripped);
}

/// Enum with tuple variant where one element has container-level proxy.
#[derive(Facet, Debug, PartialEq)]
#[repr(C)]
enum EnumWithTupleProxyVariant {
    NamedPoint(String, Point),        // Point has proxy
    NamedBinary(String, BinaryU32),   // BinaryU32 has proxy
    TwoPoints(Point, Point),          // Both have proxy
}

#[test]
fn test_enum_tuple_variant_with_container_proxy_roundtrip() {
    let original = EnumWithTupleProxyVariant::NamedPoint(
        "origin".to_string(),
        Point { x: 0, y: 0 },
    );
    let xml = to_string(&original).unwrap();
    eprintln!("XML: {xml}");

    let roundtripped: EnumWithTupleProxyVariant = from_str(&xml).unwrap();
    assert_eq!(original, roundtripped);
}

#[test]
fn test_enum_tuple_variant_with_binary_proxy_roundtrip() {
    let original = EnumWithTupleProxyVariant::NamedBinary(
        "flags".to_string(),
        BinaryU32(0b10101010),
    );
    let xml = to_string(&original).unwrap();
    eprintln!("XML: {xml}");
    assert!(
        xml.contains("0b10101010"),
        "Should use binary proxy in tuple variant, got: {xml}"
    );

    let roundtripped: EnumWithTupleProxyVariant = from_str(&xml).unwrap();
    assert_eq!(original, roundtripped);
}

#[test]
fn test_enum_tuple_variant_with_two_proxied_types_roundtrip() {
    let original = EnumWithTupleProxyVariant::TwoPoints(
        Point { x: 1, y: 2 },
        Point { x: 3, y: 4 },
    );
    let xml = to_string(&original).unwrap();
    eprintln!("XML: {xml}");

    let roundtripped: EnumWithTupleProxyVariant = from_str(&xml).unwrap();
    assert_eq!(original, roundtripped);
}

/// Enum with struct variant containing Vec field with field-level proxy.
#[derive(Facet, Debug, PartialEq)]
#[repr(C)]
enum EnumWithVecProxyInVariant {
    WithNumbers {
        name: String,
        #[facet(xml::proxy = CommaSeparatedU32s)]
        values: Vec<u32>,
    },
    Simple {
        name: String,
    },
}

#[test]
fn test_enum_struct_variant_with_vec_field_proxy_roundtrip() {
    let original = EnumWithVecProxyInVariant::WithNumbers {
        name: "test".to_string(),
        values: vec![1, 2, 3, 4, 5],
    };
    let xml = to_string(&original).unwrap();
    eprintln!("XML: {xml}");
    assert!(
        xml.contains("1,2,3,4,5"),
        "Should use comma-separated proxy in enum variant, got: {xml}"
    );

    let roundtripped: EnumWithVecProxyInVariant = from_str(&xml).unwrap();
    assert_eq!(original, roundtripped);
}

/// Enum with struct variant containing Option field with field-level proxy.
#[derive(Facet, Debug, PartialEq)]
#[repr(C)]
enum EnumWithOptionProxyInVariant {
    WithDescription {
        name: String,
        #[facet(xml::proxy = OptionalStringProxy)]
        description: Option<String>,
    },
}

#[test]
fn test_enum_struct_variant_with_option_proxy_some_roundtrip() {
    let original = EnumWithOptionProxyInVariant::WithDescription {
        name: "test".to_string(),
        description: Some("hello".to_string()),
    };
    let xml = to_string(&original).unwrap();
    eprintln!("XML: {xml}");

    let roundtripped: EnumWithOptionProxyInVariant = from_str(&xml).unwrap();
    assert_eq!(original, roundtripped);
}

#[test]
fn test_enum_struct_variant_with_option_proxy_none_roundtrip() {
    let original = EnumWithOptionProxyInVariant::WithDescription {
        name: "test".to_string(),
        description: None,
    };
    let xml = to_string(&original).unwrap();
    eprintln!("XML: {xml}");
    assert!(
        xml.contains("N/A"),
        "Should use 'N/A' for None in enum variant, got: {xml}"
    );

    let roundtripped: EnumWithOptionProxyInVariant = from_str(&xml).unwrap();
    assert_eq!(original, roundtripped);
}

// ============================================================================
// Additional edge cases to round out to 40 tests
// ============================================================================

/// Edge case: Tuple struct (not enum variant) with a proxied inner type.
#[derive(Facet, Debug, PartialEq)]
struct TupleStructWithProxy(Point, String);

#[test]
fn test_tuple_struct_with_proxied_field_roundtrip() {
    let original = TupleStructWithProxy(Point { x: 10, y: 20 }, "label".to_string());
    let xml = to_string(&original).unwrap();
    eprintln!("XML: {xml}");
    // Should have wrapper element with _0 and _1 children
    assert!(
        xml.contains("<_0>") && xml.contains("<_1>"),
        "Should have _0 and _1 elements for tuple fields, got: {xml}"
    );

    let roundtripped: TupleStructWithProxy = from_str(&xml).unwrap();
    assert_eq!(original, roundtripped);
}

/// Edge case: Box<T> where T has a container-level proxy.
#[derive(Facet, Debug, PartialEq)]
struct ContainerWithBoxedProxy {
    name: String,
    point: Box<Point>,
}

#[test]
fn test_boxed_type_with_container_proxy_roundtrip() {
    let original = ContainerWithBoxedProxy {
        name: "boxed".to_string(),
        point: Box::new(Point { x: 100, y: 200 }),
    };
    let xml = to_string(&original).unwrap();
    eprintln!("XML: {xml}");

    let roundtripped: ContainerWithBoxedProxy = from_str(&xml).unwrap();
    assert_eq!(original, roundtripped);
}

/// Edge case: Deeply nested proxy - struct containing struct containing proxied type.
#[derive(Facet, Debug, PartialEq)]
struct Level1 {
    name: String,
    level2: Level2,
}

#[derive(Facet, Debug, PartialEq)]
struct Level2 {
    id: u32,
    level3: Level3,
}

#[derive(Facet, Debug, PartialEq)]
struct Level3 {
    #[facet(xml::proxy = BinaryString)]
    flags: u32,
    point: Point, // container-level proxy
}

#[test]
fn test_deeply_nested_proxies_roundtrip() {
    let original = Level1 {
        name: "root".to_string(),
        level2: Level2 {
            id: 42,
            level3: Level3 {
                flags: 0b11001100,
                point: Point { x: 5, y: 10 },
            },
        },
    };
    let xml = to_string(&original).unwrap();
    eprintln!("XML: {xml}");
    assert!(
        xml.contains("0b11001100"),
        "Should use binary proxy at level 3, got: {xml}"
    );

    let roundtripped: Level1 = from_str(&xml).unwrap();
    assert_eq!(original, roundtripped);
}

// ============================================================================
// Devious proxy edge cases (10 more tests)
// ============================================================================

/// Devious case 1: Field-level proxy OVERRIDING container-level proxy.
/// Point has container-level proxy (PointProxy), but field uses HexPoint instead.
#[derive(Facet, Clone, Debug)]
#[facet(transparent)]
pub struct HexPointProxy(pub String);

impl TryFrom<HexPointProxy> for Point {
    type Error = &'static str;
    fn try_from(proxy: HexPointProxy) -> Result<Self, Self::Error> {
        // Format: "x:hex,y:hex" e.g., "a:14" for x=10, y=20
        let parts: Vec<&str> = proxy.0.split(',').collect();
        if parts.len() != 2 {
            return Err("invalid hex point format");
        }
        let x = i32::from_str_radix(parts[0], 16).map_err(|_| "invalid hex x")?;
        let y = i32::from_str_radix(parts[1], 16).map_err(|_| "invalid hex y")?;
        Ok(Point { x, y })
    }
}

impl From<&Point> for HexPointProxy {
    fn from(p: &Point) -> Self {
        HexPointProxy(format!("{:x},{:x}", p.x, p.y))
    }
}

#[derive(Facet, Debug, PartialEq)]
struct FieldProxyOverridesContainer {
    /// Uses Point's container-level proxy (PointProxy - struct with x, y elements)
    normal_point: Point,
    /// Field-level proxy overrides to use hex string format
    #[facet(xml::proxy = HexPointProxy)]
    hex_point: Point,
}

#[test]
fn test_field_proxy_overrides_container_proxy() {
    let original = FieldProxyOverridesContainer {
        normal_point: Point { x: 10, y: 20 },
        hex_point: Point { x: 255, y: 256 },
    };
    let xml = to_string(&original).unwrap();
    eprintln!("XML: {xml}");
    // normal_point should use PointProxy (struct elements)
    assert!(
        xml.contains("<x>10</x>"),
        "normal_point should use container proxy, got: {xml}"
    );
    // hex_point should use HexPointProxy (single hex string)
    assert!(
        xml.contains("ff,100"),
        "hex_point should use field proxy override, got: {xml}"
    );

    let roundtripped: FieldProxyOverridesContainer = from_str(&xml).unwrap();
    assert_eq!(original, roundtripped);
}

/// Devious case 2: Same underlying type with DIFFERENT field-level proxies.
#[derive(Facet, Debug, PartialEq)]
struct SameTypeDifferentProxies {
    #[facet(xml::proxy = BinaryString)]
    as_binary: u32,
    #[facet(xml::proxy = HexString)]
    as_hex: u32,
    /// No proxy - uses default decimal representation
    as_decimal: u32,
}

#[test]
fn test_same_type_different_field_proxies() {
    let original = SameTypeDifferentProxies {
        as_binary: 255,
        as_hex: 255,
        as_decimal: 255,
    };
    let xml = to_string(&original).unwrap();
    eprintln!("XML: {xml}");
    assert!(xml.contains("0b11111111"), "Should have binary, got: {xml}");
    assert!(xml.contains("0xff"), "Should have hex, got: {xml}");
    assert!(xml.contains(">255<"), "Should have decimal, got: {xml}");

    let roundtripped: SameTypeDifferentProxies = from_str(&xml).unwrap();
    assert_eq!(original, roundtripped);
}

/// Devious case 3: Boolean serialized as "yes"/"no" string.
#[derive(Facet, Clone, Debug)]
#[facet(transparent)]
pub struct YesNoProxy(pub String);

impl TryFrom<YesNoProxy> for bool {
    type Error = &'static str;
    fn try_from(proxy: YesNoProxy) -> Result<Self, Self::Error> {
        match proxy.0.to_lowercase().as_str() {
            "yes" | "true" | "1" => Ok(true),
            "no" | "false" | "0" => Ok(false),
            _ => Err("expected yes/no"),
        }
    }
}

impl From<&bool> for YesNoProxy {
    fn from(b: &bool) -> Self {
        YesNoProxy(if *b { "yes" } else { "no" }.to_string())
    }
}

#[derive(Facet, Debug, PartialEq)]
struct BoolAsYesNo {
    name: String,
    #[facet(xml::proxy = YesNoProxy)]
    enabled: bool,
    #[facet(xml::proxy = YesNoProxy)]
    visible: bool,
}

#[test]
fn test_bool_as_yes_no_proxy() {
    let original = BoolAsYesNo {
        name: "feature".to_string(),
        enabled: true,
        visible: false,
    };
    let xml = to_string(&original).unwrap();
    eprintln!("XML: {xml}");
    assert!(
        xml.contains("<enabled>yes</enabled>"),
        "true should be 'yes', got: {xml}"
    );
    assert!(
        xml.contains("<visible>no</visible>"),
        "false should be 'no', got: {xml}"
    );

    let roundtripped: BoolAsYesNo = from_str(&xml).unwrap();
    assert_eq!(original, roundtripped);
}

/// Devious case 4: Nested Vec - Vec<Vec<T>> where T has proxy.
#[derive(Facet, Debug, PartialEq)]
struct GridOfColors {
    name: String,
    #[facet(rename = "row")]
    rows: Vec<ColorRow>,
}

#[derive(Facet, Debug, Clone, PartialEq)]
struct ColorRow {
    #[facet(rename = "cell")]
    cells: Vec<Color>,
}

#[test]
fn test_nested_vec_with_proxied_items() {
    let original = GridOfColors {
        name: "checkerboard".to_string(),
        rows: vec![
            ColorRow {
                cells: vec![
                    Color { r: 0, g: 0, b: 0 },
                    Color {
                        r: 255,
                        g: 255,
                        b: 255,
                    },
                ],
            },
            ColorRow {
                cells: vec![
                    Color {
                        r: 255,
                        g: 255,
                        b: 255,
                    },
                    Color { r: 0, g: 0, b: 0 },
                ],
            },
        ],
    };
    let xml = to_string(&original).unwrap();
    eprintln!("XML: {xml}");
    // Each color should use the hex string proxy
    assert!(
        xml.contains("#000000"),
        "Should have black cells, got: {xml}"
    );
    assert!(
        xml.contains("#ffffff"),
        "Should have white cells, got: {xml}"
    );

    let roundtripped: GridOfColors = from_str(&xml).unwrap();
    assert_eq!(original, roundtripped);
}

/// Devious case 5: Option<Vec<T>> where T has container-level proxy.
#[derive(Facet, Debug, PartialEq)]
struct OptionalVecOfProxied {
    name: String,
    #[facet(rename = "point")]
    points: Option<Vec<Point>>,
}

#[test]
fn test_option_vec_of_proxied_some() {
    let original = OptionalVecOfProxied {
        name: "path".to_string(),
        points: Some(vec![Point { x: 1, y: 2 }, Point { x: 3, y: 4 }]),
    };
    let xml = to_string(&original).unwrap();
    eprintln!("XML: {xml}");

    let roundtripped: OptionalVecOfProxied = from_str(&xml).unwrap();
    assert_eq!(original, roundtripped);
}

#[test]
fn test_option_vec_of_proxied_none() {
    let original = OptionalVecOfProxied {
        name: "empty".to_string(),
        points: None,
    };
    let xml = to_string(&original).unwrap();
    eprintln!("XML: {xml}");

    let roundtripped: OptionalVecOfProxied = from_str(&xml).unwrap();
    assert_eq!(original, roundtripped);
}

/// Devious case 6: Proxy that collapses a struct into a single delimited string.
#[derive(Facet, Clone, Debug)]
#[facet(transparent)]
pub struct RgbString(pub String);

#[derive(Facet, Debug, Clone, PartialEq)]
#[facet(xml::proxy = RgbString)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl TryFrom<RgbString> for Color {
    type Error = &'static str;
    fn try_from(proxy: RgbString) -> Result<Self, Self::Error> {
        // Format: "r,g,b" or "#RRGGBB"
        let s = proxy.0.trim();
        if s.starts_with('#') && s.len() == 7 {
            let r = u8::from_str_radix(&s[1..3], 16).map_err(|_| "invalid r")?;
            let g = u8::from_str_radix(&s[3..5], 16).map_err(|_| "invalid g")?;
            let b = u8::from_str_radix(&s[5..7], 16).map_err(|_| "invalid b")?;
            Ok(Color { r, g, b })
        } else {
            let parts: Vec<&str> = s.split(',').collect();
            if parts.len() != 3 {
                return Err("expected r,g,b or #RRGGBB");
            }
            let r: u8 = parts[0].trim().parse().map_err(|_| "invalid r")?;
            let g: u8 = parts[1].trim().parse().map_err(|_| "invalid g")?;
            let b: u8 = parts[2].trim().parse().map_err(|_| "invalid b")?;
            Ok(Color { r, g, b })
        }
    }
}

impl From<&Color> for RgbString {
    fn from(c: &Color) -> Self {
        RgbString(format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b))
    }
}

#[derive(Facet, Debug, PartialEq)]
struct PaletteEntry {
    name: String,
    color: Color,
}

#[test]
fn test_struct_collapsed_to_string_proxy() {
    let original = PaletteEntry {
        name: "red".to_string(),
        color: Color {
            r: 255,
            g: 0,
            b: 128,
        },
    };
    let xml = to_string(&original).unwrap();
    eprintln!("XML: {xml}");
    // Color should be serialized as hex string, not struct
    assert!(
        xml.contains("#ff0080"),
        "Color should be hex string, got: {xml}"
    );
    assert!(
        !xml.contains("<r>"),
        "Should NOT have <r> element, got: {xml}"
    );

    let roundtripped: PaletteEntry = from_str(&xml).unwrap();
    assert_eq!(original, roundtripped);
}

/// Devious case 7: Vec of colors (each color uses its container proxy).
#[derive(Facet, Debug, PartialEq)]
struct Palette {
    name: String,
    #[facet(rename = "color")]
    colors: Vec<Color>,
}

#[test]
fn test_vec_of_struct_with_string_proxy() {
    let original = Palette {
        name: "primary".to_string(),
        colors: vec![
            Color { r: 255, g: 0, b: 0 },
            Color { r: 0, g: 255, b: 0 },
            Color { r: 0, g: 0, b: 255 },
        ],
    };
    let xml = to_string(&original).unwrap();
    eprintln!("XML: {xml}");
    assert!(xml.contains("#ff0000"), "Should have red");
    assert!(xml.contains("#00ff00"), "Should have green");
    assert!(xml.contains("#0000ff"), "Should have blue");

    let roundtripped: Palette = from_str(&xml).unwrap();
    assert_eq!(original, roundtripped);
}

/// Devious case 8: Enum variants with different proxy behaviors.
#[derive(Facet, Debug, PartialEq)]
#[repr(C)]
enum ShapeWithMixedProxies {
    /// Point uses its container-level proxy
    Point(Point),
    /// Color uses its container-level proxy (string)
    ColoredDot { location: Point, color: Color },
    /// Raw coordinates (no proxy involvement)
    RawCoords { x: i32, y: i32 },
}

#[test]
fn test_enum_variants_with_mixed_proxy_behaviors() {
    let point = ShapeWithMixedProxies::Point(Point { x: 5, y: 10 });
    let xml = to_string(&point).unwrap();
    eprintln!("Point XML: {xml}");
    let rt: ShapeWithMixedProxies = from_str(&xml).unwrap();
    assert_eq!(point, rt);

    let colored = ShapeWithMixedProxies::ColoredDot {
        location: Point { x: 100, y: 200 },
        color: Color {
            r: 128,
            g: 64,
            b: 32,
        },
    };
    let xml = to_string(&colored).unwrap();
    eprintln!("ColoredDot XML: {xml}");
    assert!(xml.contains("#804020"), "Color should be hex string");
    let rt: ShapeWithMixedProxies = from_str(&xml).unwrap();
    assert_eq!(colored, rt);

    let raw = ShapeWithMixedProxies::RawCoords { x: 42, y: 84 };
    let xml = to_string(&raw).unwrap();
    eprintln!("RawCoords XML: {xml}");
    let rt: ShapeWithMixedProxies = from_str(&xml).unwrap();
    assert_eq!(raw, rt);
}

/// Devious case 9: Proxy combined with xml::attribute.
#[derive(Facet, Debug, PartialEq)]
#[facet(rename = "rect")]
struct RectWithProxiedAttributes {
    #[facet(facet_xml::attribute, xml::proxy = HexString)]
    width: u32,
    #[facet(facet_xml::attribute, xml::proxy = HexString)]
    height: u32,
    #[facet(facet_xml::attribute)]
    fill: Color,
}

#[test]
fn test_multiple_proxied_attributes() {
    let original = RectWithProxiedAttributes {
        width: 256,
        height: 128,
        fill: Color {
            r: 255,
            g: 128,
            b: 0,
        },
    };
    let xml = to_string(&original).unwrap();
    eprintln!("XML: {xml}");
    // Attributes should use proxies
    assert!(
        xml.contains(r#"width="0x100""#),
        "width should be hex, got: {xml}"
    );
    assert!(
        xml.contains(r#"height="0x80""#),
        "height should be hex, got: {xml}"
    );
    assert!(
        xml.contains(r##"fill="#ff8000""##),
        "fill should be color hex, got: {xml}"
    );

    let roundtripped: RectWithProxiedAttributes = from_str(&xml).unwrap();
    assert_eq!(original, roundtripped);
}

/// Devious case 10: Recursive structure where proxy is used at each level.
#[derive(Facet, Debug, Clone, PartialEq)]
struct TreeNode {
    value: Color,
    #[facet(rename = "child")]
    children: Vec<TreeNode>,
}

#[test]
fn test_recursive_structure_with_proxy() {
    let original = TreeNode {
        value: Color { r: 255, g: 0, b: 0 },
        children: vec![
            TreeNode {
                value: Color { r: 0, g: 255, b: 0 },
                children: vec![TreeNode {
                    value: Color { r: 0, g: 0, b: 255 },
                    children: vec![],
                }],
            },
            TreeNode {
                value: Color {
                    r: 255,
                    g: 255,
                    b: 0,
                },
                children: vec![],
            },
        ],
    };
    let xml = to_string(&original).unwrap();
    eprintln!("XML: {xml}");
    // All colors at all levels should use the proxy
    assert!(xml.contains("#ff0000"), "Root should be red");
    assert!(xml.contains("#00ff00"), "Child should be green");
    assert!(xml.contains("#0000ff"), "Grandchild should be blue");
    assert!(xml.contains("#ffff00"), "Second child should be yellow");

    let roundtripped: TreeNode = from_str(&xml).unwrap();
    assert_eq!(original, roundtripped);
}
