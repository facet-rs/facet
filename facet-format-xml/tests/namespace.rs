//! Tests for XML namespace support in facet-format-xml.
//!
//! These tests verify that namespace-aware field matching works correctly
//! during deserialization, and that serialization properly emits xmlns
//! declarations and namespace prefixes.

use facet::Facet;
use facet_format::FormatDeserializer;
use facet_format_xml::{XmlParser, to_vec};
use facet_xml as xml; // Import to enable xml:: syntax in facet attributes

/// Helper to deserialize XML using facet-format-xml
fn from_str<T: Facet<'static>>(xml_str: &str) -> Result<T, Box<dyn std::error::Error>> {
    let parser = XmlParser::new(xml_str.as_bytes());
    let mut deserializer = FormatDeserializer::new_owned(parser);
    Ok(deserializer.deserialize()?)
}

/// Helper to serialize to XML using facet-format-xml
fn to_string<T: Facet<'static>>(value: &T) -> Result<String, Box<dyn std::error::Error>> {
    let bytes = to_vec(value)?;
    Ok(String::from_utf8(bytes)?)
}

// ============================================================================
// Basic namespace matching
// ============================================================================

/// Test that elements with declared namespaces are matched correctly.
#[derive(Facet, Debug, PartialEq, Default)]
#[facet(rename = "root", default)]
struct NamespacedRoot {
    /// This field requires the element to be in the "http://example.com/ns" namespace.
    #[facet(xml::element, xml::ns = "http://example.com/ns")]
    item: String,
}

#[test]
fn test_element_with_declared_namespace() {
    // Element with namespace declaration and prefix
    let xml = r#"<root xmlns:ex="http://example.com/ns"><ex:item>value</ex:item></root>"#;
    let parsed: NamespacedRoot = from_str(xml).unwrap();
    assert_eq!(parsed.item, "value");
}

#[test]
fn test_element_with_default_namespace() {
    // Element inherits default namespace
    let xml = r#"<root><item xmlns="http://example.com/ns">value</item></root>"#;
    let parsed: NamespacedRoot = from_str(xml).unwrap();
    assert_eq!(parsed.item, "value");
}

#[test]
fn test_element_namespace_mismatch() {
    // Element in wrong namespace - does not match the field with xml::ns constraint.
    // Since String has a Default impl, the field gets an empty string.
    let xml = r#"<root xmlns:other="http://other.com/ns"><other:item>value</other:item></root>"#;
    let parsed: NamespacedRoot = from_str(xml).unwrap();
    // Field is empty because no element matched the namespace constraint
    assert_eq!(parsed.item, "");
}

// ============================================================================
// Attribute namespace matching
// ============================================================================

#[derive(Facet, Debug, PartialEq, Default)]
#[facet(rename = "root", default)]
struct NamespacedAttr {
    /// This field requires the attribute to be in the "http://example.com/ns" namespace.
    #[facet(xml::attribute, xml::ns = "http://example.com/ns")]
    value: String,
}

#[test]
fn test_attribute_with_declared_namespace() {
    // Attribute with namespace prefix
    let xml = r#"<root xmlns:ex="http://example.com/ns" ex:value="hello"/>"#;
    let parsed: NamespacedAttr = from_str(xml).unwrap();
    assert_eq!(parsed.value, "hello");
}

#[test]
fn test_attribute_namespace_mismatch() {
    // Unprefixed attributes are in "no namespace" (not the default xmlns!), so won't match.
    // Since String has a Default impl, the field gets an empty string.
    let xml = r#"<root xmlns="http://example.com/ns" value="hello"/>"#;
    let parsed: NamespacedAttr = from_str(xml).unwrap();
    // Field is empty because unprefixed attribute is not in the required namespace
    assert_eq!(parsed.value, "");
}

// ============================================================================
// Mixed namespaced and non-namespaced fields
// ============================================================================

#[derive(Facet, Debug, PartialEq)]
#[facet(rename = "root")]
struct MixedNamespaces {
    /// No namespace constraint - matches any namespace or no namespace.
    #[facet(xml::element)]
    plain: String,

    /// Requires specific namespace.
    #[facet(xml::element, xml::ns = "http://example.com/special")]
    special: String,
}

#[test]
fn test_mixed_namespaces() {
    let xml = r#"<root xmlns:sp="http://example.com/special">
        <plain>plain value</plain>
        <sp:special>special value</sp:special>
    </root>"#;
    let parsed: MixedNamespaces = from_str(xml).unwrap();
    assert_eq!(parsed.plain, "plain value");
    assert_eq!(parsed.special, "special value");
}

#[test]
fn test_mixed_namespaces_with_default() {
    // plain element in default namespace, special in prefixed namespace
    let xml = r#"<root xmlns="http://default.com" xmlns:sp="http://example.com/special">
        <plain>plain value</plain>
        <sp:special>special value</sp:special>
    </root>"#;
    let parsed: MixedNamespaces = from_str(xml).unwrap();
    // plain matches because it has no xml::ns constraint
    assert_eq!(parsed.plain, "plain value");
    assert_eq!(parsed.special, "special value");
}

// ============================================================================
// Same local name, different namespaces
// ============================================================================

#[derive(Facet, Debug, PartialEq)]
#[facet(rename = "root")]
struct SameLocalNameDifferentNs {
    #[facet(xml::element, rename = "item", xml::ns = "http://ns1.com")]
    item_ns1: String,

    #[facet(xml::element, rename = "item", xml::ns = "http://ns2.com")]
    item_ns2: String,
}

#[test]
fn test_same_local_name_different_namespaces() {
    let xml = r#"<root xmlns:a="http://ns1.com" xmlns:b="http://ns2.com">
        <a:item>from ns1</a:item>
        <b:item>from ns2</b:item>
    </root>"#;
    let parsed: SameLocalNameDifferentNs = from_str(xml).unwrap();
    assert_eq!(parsed.item_ns1, "from ns1");
    assert_eq!(parsed.item_ns2, "from ns2");
}

// ============================================================================
// Prefix independence (semantic equivalence)
// ============================================================================

#[derive(Facet, Debug, PartialEq)]
#[facet(rename = "data")]
struct PrefixIndependent {
    #[facet(xml::element, xml::ns = "http://example.com/ns")]
    value: String,
}

#[test]
fn test_prefix_independence() {
    // Same namespace, different prefixes - should be semantically equivalent
    let xml1 = r#"<data xmlns:a="http://example.com/ns"><a:value>test</a:value></data>"#;
    let xml2 = r#"<data xmlns:xyz="http://example.com/ns"><xyz:value>test</xyz:value></data>"#;
    let xml3 = r#"<data xmlns:foo="http://example.com/ns"><foo:value>test</foo:value></data>"#;

    let parsed1: PrefixIndependent = from_str(xml1).unwrap();
    let parsed2: PrefixIndependent = from_str(xml2).unwrap();
    let parsed3: PrefixIndependent = from_str(xml3).unwrap();

    assert_eq!(parsed1, parsed2);
    assert_eq!(parsed2, parsed3);
}

// ============================================================================
// Backward compatibility: no xml::ns means match any
// ============================================================================

#[derive(Facet, Debug, PartialEq)]
#[facet(rename = "root")]
struct BackwardCompatible {
    /// No xml::ns - matches any namespace including none
    #[facet(xml::element)]
    item: String,
}

#[test]
fn test_backward_compatible_no_namespace() {
    let xml = r#"<root><item>value</item></root>"#;
    let parsed: BackwardCompatible = from_str(xml).unwrap();
    assert_eq!(parsed.item, "value");
}

#[test]
fn test_backward_compatible_with_namespace() {
    // Field without xml::ns should match element with any namespace
    let xml = r#"<root xmlns:ex="http://example.com"><ex:item>value</ex:item></root>"#;
    let parsed: BackwardCompatible = from_str(xml).unwrap();
    assert_eq!(parsed.item, "value");
}

#[test]
fn test_backward_compatible_with_default_namespace() {
    // Field without xml::ns should match element in default namespace
    let xml = r#"<root xmlns="http://example.com"><item>value</item></root>"#;
    let parsed: BackwardCompatible = from_str(xml).unwrap();
    assert_eq!(parsed.item, "value");
}

// ============================================================================
// Container-level xml::ns_all
// ============================================================================

/// Container with ns_all - all fields default to this namespace
#[derive(Facet, Debug, PartialEq, Default)]
#[facet(rename = "root", xml::ns_all = "http://example.com/ns", default)]
struct NsAllContainer {
    #[facet(xml::element)]
    first: String,

    #[facet(xml::element)]
    second: String,

    /// This field overrides ns_all with its own namespace
    #[facet(xml::element, xml::ns = "http://other.com/ns")]
    other: String,
}

#[test]
fn test_ns_all_basic() {
    // All elements must be in the ns_all namespace
    let xml = r#"<root xmlns:ex="http://example.com/ns" xmlns:other="http://other.com/ns">
        <ex:first>one</ex:first>
        <ex:second>two</ex:second>
        <other:other>three</other:other>
    </root>"#;
    let parsed: NsAllContainer = from_str(xml).unwrap();
    assert_eq!(parsed.first, "one");
    assert_eq!(parsed.second, "two");
    assert_eq!(parsed.other, "three");
}

#[test]
fn test_ns_all_with_default_xmlns() {
    // Using default xmlns for ns_all namespace
    let xml = r#"<root xmlns="http://example.com/ns" xmlns:other="http://other.com/ns">
        <first>one</first>
        <second>two</second>
        <other:other>three</other:other>
    </root>"#;
    let parsed: NsAllContainer = from_str(xml).unwrap();
    assert_eq!(parsed.first, "one");
    assert_eq!(parsed.second, "two");
    assert_eq!(parsed.other, "three");
}

#[test]
fn test_ns_all_different_prefix() {
    // Same namespace, different prefix - should still work
    let xml = r#"<root xmlns:foo="http://example.com/ns" xmlns:bar="http://other.com/ns">
        <foo:first>one</foo:first>
        <foo:second>two</foo:second>
        <bar:other>three</bar:other>
    </root>"#;
    let parsed: NsAllContainer = from_str(xml).unwrap();
    assert_eq!(parsed.first, "one");
    assert_eq!(parsed.second, "two");
    assert_eq!(parsed.other, "three");
}

#[test]
fn test_ns_all_mismatch() {
    // Elements in wrong namespace - fields get defaults
    let xml = r#"<root xmlns:wrong="http://wrong.com/ns">
        <wrong:first>one</wrong:first>
        <wrong:second>two</wrong:second>
        <wrong:other>three</wrong:other>
    </root>"#;
    let parsed: NsAllContainer = from_str(xml).unwrap();
    // All fields get default values because no elements matched
    assert_eq!(parsed.first, "");
    assert_eq!(parsed.second, "");
    assert_eq!(parsed.other, "");
}

// ============================================================================
// Attribute namespace rules with ns_all
// ============================================================================

/// When a container has `xml::ns_all`, this should only affect elements,
/// not attributes. Attributes without an explicit `xml::ns` should match
/// unprefixed attributes (which are in "no namespace").
#[derive(Facet, Debug, PartialEq)]
#[facet(rename = "svg", xml::ns_all = "http://www.w3.org/2000/svg")]
struct SvgWithAttributes {
    /// Unprefixed attributes should match fields without xml::ns
    #[facet(xml::attribute, rename = "viewBox")]
    view_box: Option<String>,

    #[facet(xml::attribute)]
    width: Option<String>,

    #[facet(xml::attribute)]
    height: Option<String>,

    /// Elements DO inherit the default namespace, so this works with ns_all
    #[facet(xml::element)]
    title: Option<String>,
}

#[test]
fn test_unprefixed_attributes_with_ns_all() {
    // This XML has:
    // - xmlns declaration making http://www.w3.org/2000/svg the default namespace
    // - Unprefixed attributes (viewBox, width, height) which are in "no namespace"
    // - An element (title) which inherits the default namespace
    let xml = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100" width="100" height="100">
        <title>My SVG</title>
    </svg>"#;

    let parsed: SvgWithAttributes = from_str(xml).unwrap();

    // All unprefixed attributes should be parsed correctly
    assert_eq!(
        parsed.view_box,
        Some("0 0 100 100".to_string()),
        "viewBox attribute should be parsed"
    );
    assert_eq!(
        parsed.width,
        Some("100".to_string()),
        "width attribute should be parsed"
    );
    assert_eq!(
        parsed.height,
        Some("100".to_string()),
        "height attribute should be parsed"
    );
    // Element inherits namespace and should work
    assert_eq!(
        parsed.title,
        Some("My SVG".to_string()),
        "title element should be parsed"
    );
}

#[test]
fn test_unprefixed_attributes_without_default_xmlns() {
    // Without any xmlns, elements and attributes are both in "no namespace"
    let xml = r#"<svg viewBox="0 0 100 100" width="100" height="100">
        <title>My SVG</title>
    </svg>"#;

    let parsed: SvgWithAttributes = from_str(xml).unwrap();

    // Attributes should still work
    assert_eq!(parsed.view_box, Some("0 0 100 100".to_string()));
    assert_eq!(parsed.width, Some("100".to_string()));
    assert_eq!(parsed.height, Some("100".to_string()));
    // Element in "no namespace" won't match ns_all requirement
    // (it expects http://www.w3.org/2000/svg)
    assert_eq!(parsed.title, None);
}

// ============================================================================
// Serialization with namespaces (Round-trip tests)
// ============================================================================
// NOTE: These will fail until Phase 4 (serialization) is implemented

#[test]
fn test_serialize_namespaced_element() {
    // Serialize a struct with xml::ns on a field
    let value = NamespacedRoot {
        item: "value".to_string(),
    };
    let xml_output = to_string(&value).unwrap();

    // Should contain xmlns declaration and prefixed element name
    assert!(
        xml_output.contains("xmlns:"),
        "Should contain xmlns declaration: {xml_output}"
    );
    assert!(
        xml_output.contains(":item"),
        "Should contain prefixed element: {xml_output}"
    );
    assert!(
        xml_output.contains("http://example.com/ns"),
        "Should contain namespace URI: {xml_output}"
    );

    // Round-trip: the serialized XML should deserialize back to the same value
    let parsed: NamespacedRoot = from_str(&xml_output).unwrap();
    assert_eq!(parsed, value);
}

#[test]
fn test_serialize_namespaced_attribute() {
    // Serialize a struct with xml::ns on an attribute
    let value = NamespacedAttr {
        value: "hello".to_string(),
    };
    let xml_output = to_string(&value).unwrap();

    // Should contain xmlns declaration and prefixed attribute
    assert!(
        xml_output.contains("xmlns:"),
        "Should contain xmlns declaration: {xml_output}"
    );
    assert!(
        xml_output.contains(":value="),
        "Should contain prefixed attribute: {xml_output}"
    );

    // Round-trip
    let parsed: NamespacedAttr = from_str(&xml_output).unwrap();
    assert_eq!(parsed, value);
}

#[test]
fn test_serialize_ns_all() {
    let value = NsAllContainer {
        first: "one".to_string(),
        second: "two".to_string(),
        other: "three".to_string(),
    };
    let xml_output = to_string(&value).unwrap();

    // Should contain xmlns declarations
    assert!(xml_output.contains("http://example.com/ns"));
    assert!(xml_output.contains("http://other.com/ns"));

    // Round-trip
    let parsed: NsAllContainer = from_str(&xml_output).unwrap();
    assert_eq!(parsed, value);
}

// ============================================================================
// PR #1481: Namespace definition handling (backported from facet-xml)
// ============================================================================

/// Test that namespace prefix declarations (xmlns:prefix="...") are properly
/// ignored during deserialization, even when deny_unknown_fields is enabled.
///
/// This is the key test from PR #1481 that ensures the namespace definition
/// check happens BEFORE namespace resolution, preventing errors like
/// "Unknown prefix: xmlns".
#[test]
fn test_namespace_with_prefix_is_ignored() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(deny_unknown_fields, rename = "root")]
    struct Root {
        #[facet(xml::element)]
        item: String,
    }

    let xml = r#"<root xmlns:gml="http://www.opengis.net/gml"><gml:item>test</gml:item></root>"#;
    let deserialized: Root = from_str(xml).unwrap();
    assert_eq!(deserialized.item, "test");
}

/// Test that namespace definitions are ignored even when combined with ns_all.
#[test]
fn test_namespace_with_deny_unknown_fields() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(
        deny_unknown_fields,
        rename = "root",
        xml::ns_all = "http://example.com/ns"
    )]
    struct NamespacedRoot {
        #[facet(xml::element)]
        item: String,
    }

    let doc = NamespacedRoot {
        item: "value".to_string(),
    };

    let serialized = to_string(&doc).unwrap();
    let deserialized: NamespacedRoot = from_str(&serialized).unwrap();

    assert_eq!(doc, deserialized);
}

// ============================================================================
// Mixed namespace serialization tests
// ============================================================================

#[test]
fn test_serialize_mixed_namespaces() {
    let value = MixedNamespaces {
        plain: "plain value".to_string(),
        special: "special value".to_string(),
    };
    let xml_output = to_string(&value).unwrap();

    // plain should not have a prefix, special should
    assert!(
        xml_output.contains("<plain>"),
        "Plain element should not be prefixed: {xml_output}"
    );
    assert!(
        xml_output.contains(":special>"),
        "Special element should be prefixed: {xml_output}"
    );

    // Round-trip
    let parsed: MixedNamespaces = from_str(&xml_output).unwrap();
    assert_eq!(parsed, value);
}

#[test]
fn test_serialize_same_local_name_different_namespaces() {
    let value = SameLocalNameDifferentNs {
        item_ns1: "from ns1".to_string(),
        item_ns2: "from ns2".to_string(),
    };
    let xml_output = to_string(&value).unwrap();

    // Both should be "item" but with different prefixes
    assert!(
        xml_output.contains("http://ns1.com"),
        "Should contain ns1: {xml_output}"
    );
    assert!(
        xml_output.contains("http://ns2.com"),
        "Should contain ns2: {xml_output}"
    );

    // Round-trip
    let parsed: SameLocalNameDifferentNs = from_str(&xml_output).unwrap();
    assert_eq!(parsed, value);
}

// ============================================================================
// Comprehensive SVG namespace tests (DESERIALIZATION)
// ============================================================================
//
// NOTE: The serialization roundtrip tests are intentionally omitted here because
// facet-format-xml has known parity gaps in its serialization behavior compared
// to facet-xml:
// 1. Uses prefixed namespaces (xmlns:svg="...") instead of default namespace (xmlns="...")
// 2. Root element name doesn't respect the `rename` attribute
// 3. Nested element attributes aren't serialized correctly with ns_all
//
// See: https://github.com/facet-rs/facet/issues/XXX for tracking

/// Simple SVG struct with attributes and a child element.
#[derive(Facet, Debug, PartialEq, Clone)]
#[facet(rename = "svg", xml::ns_all = "http://www.w3.org/2000/svg")]
struct SimpleSvg {
    #[facet(xml::attribute, rename = "viewBox")]
    view_box: Option<String>,
    #[facet(xml::attribute)]
    width: Option<String>,
    #[facet(xml::attribute)]
    height: Option<String>,
    #[facet(xml::element, rename = "circle")]
    circle: Option<SvgCircleData>,
}

/// The data for a circle element
#[derive(Facet, Debug, PartialEq, Clone)]
#[facet(xml::ns_all = "http://www.w3.org/2000/svg")]
struct SvgCircleData {
    #[facet(xml::attribute)]
    cx: Option<String>,
    #[facet(xml::attribute)]
    cy: Option<String>,
    #[facet(xml::attribute)]
    r: Option<String>,
    #[facet(xml::attribute)]
    fill: Option<String>,
}

#[test]
fn test_svg_deserialization_from_browser_style_xml() {
    // This is the format that browsers/real SVG tools produce
    let xml = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100" width="100" height="100">
        <circle cx="50" cy="50" r="25" fill="red"/>
    </svg>"#;

    let parsed: SimpleSvg = from_str(xml).unwrap();

    assert_eq!(parsed.view_box, Some("0 0 100 100".to_string()));
    assert_eq!(parsed.width, Some("100".to_string()));
    assert_eq!(parsed.height, Some("100".to_string()));
    assert!(parsed.circle.is_some());
    let circle = parsed.circle.unwrap();
    assert_eq!(circle.cx, Some("50".to_string()));
    assert_eq!(circle.cy, Some("50".to_string()));
    assert_eq!(circle.r, Some("25".to_string()));
    assert_eq!(circle.fill, Some("red".to_string()));
}

/// SVG with mixed namespace (e.g., xlink:href)
#[derive(Facet, Debug, PartialEq, Clone)]
#[facet(rename = "svg", xml::ns_all = "http://www.w3.org/2000/svg")]
struct SvgWithXlink {
    #[facet(xml::attribute, rename = "viewBox")]
    view_box: Option<String>,
    #[facet(xml::element, rename = "use")]
    use_elem: Option<SvgUseData>,
}

#[derive(Facet, Debug, PartialEq, Clone)]
#[facet(xml::ns_all = "http://www.w3.org/2000/svg")]
struct SvgUseData {
    #[facet(xml::attribute)]
    x: Option<String>,
    #[facet(xml::attribute)]
    y: Option<String>,
    /// xlink:href has an explicit namespace
    #[facet(
        xml::attribute,
        rename = "href",
        xml::ns = "http://www.w3.org/1999/xlink"
    )]
    xlink_href: Option<String>,
}

#[test]
fn test_svg_with_xlink_deserialization() {
    // Test deserialization of SVG with xlink namespace
    let xml = r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 100 100">
        <use x="10" y="10" xlink:href="#mySymbol"/>
    </svg>"##;

    let parsed: SvgWithXlink = from_str(xml).unwrap();

    assert_eq!(parsed.view_box, Some("0 0 100 100".to_string()));
    assert!(parsed.use_elem.is_some());
    let use_elem = parsed.use_elem.unwrap();
    assert_eq!(use_elem.x, Some("10".to_string()));
    assert_eq!(use_elem.y, Some("10".to_string()));
    assert_eq!(use_elem.xlink_href, Some("#mySymbol".to_string()));
}

/// Test deeply nested SVG elements
#[derive(Facet, Debug, PartialEq, Clone)]
#[facet(xml::ns_all = "http://www.w3.org/2000/svg")]
struct SvgGroupData {
    #[facet(xml::attribute)]
    id: Option<String>,
    #[facet(xml::attribute)]
    transform: Option<String>,
    #[facet(xml::element, rename = "circle")]
    circle: Option<SvgCircleData>,
}

#[derive(Facet, Debug, PartialEq, Clone)]
#[facet(rename = "svg", xml::ns_all = "http://www.w3.org/2000/svg")]
struct SvgWithGroup {
    #[facet(xml::attribute, rename = "viewBox")]
    view_box: Option<String>,
    #[facet(xml::element, rename = "g")]
    group: Option<SvgGroupData>,
}

#[test]
fn test_deeply_nested_svg_deserialization() {
    let xml = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 200">
        <g id="group1" transform="translate(0,0)">
            <circle cx="25" cy="25" r="20" fill="blue"/>
        </g>
    </svg>"#;

    let parsed: SvgWithGroup = from_str(xml).unwrap();

    assert_eq!(parsed.view_box, Some("0 0 200 200".to_string()));
    assert!(parsed.group.is_some());
    let group = parsed.group.unwrap();
    assert_eq!(group.id, Some("group1".to_string()));
    assert_eq!(group.transform, Some("translate(0,0)".to_string()));
    assert!(group.circle.is_some());
    let circle = group.circle.unwrap();
    assert_eq!(circle.cx, Some("25".to_string()));
    assert_eq!(circle.cy, Some("25".to_string()));
    assert_eq!(circle.r, Some("20".to_string()));
    assert_eq!(circle.fill, Some("blue".to_string()));
}
