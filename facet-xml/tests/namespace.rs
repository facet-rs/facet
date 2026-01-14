//! Tests for XML namespace support in facet-xml.
//!
//! These tests verify that namespace-aware field matching works correctly
//! during deserialization, and that serialization properly emits xmlns
//! declarations and namespace prefixes.

use facet::Facet;
use facet_testhelpers::test;
use facet_xml::{self as xml, to_vec};

/// Helper to deserialize XML using facet-xml
fn from_str<T: Facet<'static>>(xml_str: &str) -> Result<T, Box<dyn std::error::Error>> {
    Ok(facet_xml::from_str(xml_str)?)
}

/// Helper to serialize to XML using facet-xml
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

    // Should contain default namespace declaration (xmlns="...")
    assert!(
        xml_output.contains("xmlns=\"http://example.com/ns\""),
        "Should have default xmlns: {xml_output}"
    );
    // 'other' element should use prefixed namespace since it differs from default
    assert!(
        xml_output.contains("http://other.com/ns"),
        "Should contain other namespace: {xml_output}"
    );

    // Elements in ns_all namespace should be unprefixed (inherit default xmlns)
    assert!(
        xml_output.contains("<first>"),
        "first should be unprefixed: {xml_output}"
    );
    assert!(
        xml_output.contains("<second>"),
        "second should be unprefixed: {xml_output}"
    );

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
// Comprehensive SVG namespace tests
// ============================================================================
//
// These tests verify that SVG-style namespacing works correctly with ns_all:
// - Default namespace declaration (xmlns="...")
// - Unprefixed elements inherit the default namespace
// - Unprefixed attributes are in "no namespace" (per XML spec)
// - Explicit xml::ns on fields produces prefixed elements/attributes

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

// ============================================================================
// SVG Serialization Roundtrip Tests
// ============================================================================

#[test]
fn test_svg_serialization_produces_valid_svg() {
    let svg = SimpleSvg {
        view_box: Some("0 0 100 100".to_string()),
        width: Some("100".to_string()),
        height: Some("100".to_string()),
        circle: Some(SvgCircleData {
            cx: Some("50".to_string()),
            cy: Some("50".to_string()),
            r: Some("25".to_string()),
            fill: Some("red".to_string()),
        }),
    };

    let xml_output = to_string(&svg).unwrap();

    // Should have default namespace declaration
    assert!(
        xml_output.contains("xmlns=\"http://www.w3.org/2000/svg\""),
        "Should have default xmlns: {xml_output}"
    );

    // Attributes should NOT be prefixed (per XML spec, unprefixed attrs are in "no namespace")
    assert!(
        !xml_output.contains("svg:viewBox"),
        "viewBox should not be prefixed: {xml_output}"
    );
    assert!(
        xml_output.contains("viewBox="),
        "viewBox should be present: {xml_output}"
    );

    // Child element should be named 'circle' (from field rename)
    assert!(
        xml_output.contains("<circle"),
        "Should have <circle element: {xml_output}"
    );

    // Circle attributes should not be prefixed
    assert!(
        xml_output.contains("cx=") && !xml_output.contains(":cx="),
        "cx should be unprefixed: {xml_output}"
    );
}

#[test]
fn test_svg_roundtrip() {
    let svg = SimpleSvg {
        view_box: Some("0 0 100 100".to_string()),
        width: Some("100".to_string()),
        height: Some("100".to_string()),
        circle: Some(SvgCircleData {
            cx: Some("50".to_string()),
            cy: Some("50".to_string()),
            r: Some("25".to_string()),
            fill: Some("red".to_string()),
        }),
    };

    let xml_output = to_string(&svg).unwrap();
    let parsed: SimpleSvg = from_str(&xml_output).unwrap();

    assert_eq!(parsed, svg, "Roundtrip should preserve all values");
}

#[test]
fn test_svg_with_xlink_roundtrip() {
    let svg = SvgWithXlink {
        view_box: Some("0 0 100 100".to_string()),
        use_elem: Some(SvgUseData {
            x: Some("10".to_string()),
            y: Some("10".to_string()),
            xlink_href: Some("#mySymbol".to_string()),
        }),
    };

    let xml_output = to_string(&svg).unwrap();

    // Should have default SVG namespace
    assert!(
        xml_output.contains("xmlns=\"http://www.w3.org/2000/svg\""),
        "Should have SVG default xmlns: {xml_output}"
    );

    // xlink:href should be prefixed (it has explicit xml::ns)
    assert!(
        xml_output.contains("xlink:href=") || xml_output.contains(":href="),
        "xlink:href should be prefixed: {xml_output}"
    );

    // Other attributes should not be prefixed
    assert!(
        xml_output.contains("x=") && !xml_output.contains(":x="),
        "x should be unprefixed: {xml_output}"
    );

    // Roundtrip
    let parsed: SvgWithXlink = from_str(&xml_output).unwrap();
    assert_eq!(parsed, svg);
}

#[test]
fn test_deeply_nested_svg_roundtrip() {
    let svg = SvgWithGroup {
        view_box: Some("0 0 200 200".to_string()),
        group: Some(SvgGroupData {
            id: Some("group1".to_string()),
            transform: Some("translate(0,0)".to_string()),
            circle: Some(SvgCircleData {
                cx: Some("25".to_string()),
                cy: Some("25".to_string()),
                r: Some("20".to_string()),
                fill: Some("blue".to_string()),
            }),
        }),
    };

    let xml_output = to_string(&svg).unwrap();

    // Verify structure - child elements use field rename
    assert!(
        xml_output.contains("<g"),
        "Should have g elements: {xml_output}"
    );
    assert!(
        xml_output.contains("<circle"),
        "Should have circle elements: {xml_output}"
    );

    // Should have default namespace on root
    assert!(
        xml_output.contains("xmlns=\"http://www.w3.org/2000/svg\""),
        "Should have default xmlns: {xml_output}"
    );

    // Nothing should be prefixed with svg:
    assert!(
        !xml_output.contains("svg:"),
        "Nothing should be prefixed with svg:: {xml_output}"
    );

    // Roundtrip
    let parsed: SvgWithGroup = from_str(&xml_output).unwrap();
    assert_eq!(parsed, svg);
}

#[test]
fn test_empty_svg_element() {
    let svg = SimpleSvg {
        view_box: Some("0 0 100 100".to_string()),
        width: None,
        height: None,
        circle: None,
    };

    let xml_output = to_string(&svg).unwrap();

    // Should have default namespace
    assert!(
        xml_output.contains("xmlns=\"http://www.w3.org/2000/svg\""),
        "Should have xmlns: {xml_output}"
    );

    let parsed: SimpleSvg = from_str(&xml_output).unwrap();
    assert_eq!(parsed, svg);
}

#[test]
fn test_svg_attributes_only() {
    // SVG element with only attributes, no children
    let svg = SimpleSvg {
        view_box: Some("0 0 100 100".to_string()),
        width: Some("100".to_string()),
        height: Some("100".to_string()),
        circle: None,
    };

    let xml_output = to_string(&svg).unwrap();

    // All attributes should be unprefixed
    assert!(
        xml_output.contains("viewBox=\"0 0 100 100\""),
        "viewBox should be correct: {xml_output}"
    );
    assert!(
        xml_output.contains("width=\"100\""),
        "width should be correct: {xml_output}"
    );
    assert!(
        xml_output.contains("height=\"100\""),
        "height should be correct: {xml_output}"
    );

    let parsed: SimpleSvg = from_str(&xml_output).unwrap();
    assert_eq!(parsed, svg);
}

// ============================================================================
// xml::text serialization tests (GitHub issue #1495)
// ============================================================================

/// Test that xml::text fields are serialized as element text content, not attributes.
/// This is the core test for GitHub issue #1495.
#[derive(Facet, Debug, PartialEq, Clone)]
#[facet(rename = "text")]
struct SvgText {
    #[facet(xml::attribute)]
    x: f64,
    #[facet(xml::attribute)]
    y: f64,
    #[facet(xml::attribute)]
    fill: Option<String>,
    #[facet(xml::text, default)]
    content: String,
}

#[test]
fn test_xml_text_serialization() {
    let text = SvgText {
        x: 165.931,
        y: 1144.47,
        fill: Some("#d3d3d3".to_string()),
        content: "raise-function".to_string(),
    };

    let xml_output = to_string(&text).unwrap();

    // The content should appear as element text, NOT as an attribute or child element
    // Expected: <root x="165.931" y="1144.47" fill="#d3d3d3">raise-function</root>
    // Bug (attribute): <text ... content="raise-function" />
    // Bug (child element): <root ...><content>raise-function</content></root>

    assert!(
        !xml_output.contains("content="),
        "content should NOT be serialized as an attribute: {xml_output}"
    );
    assert!(
        !xml_output.contains("<content>"),
        "content should NOT be serialized as a child element: {xml_output}"
    );
    assert!(
        xml_output.contains(">raise-function</"),
        "content should be serialized as element text: {xml_output}"
    );
}

#[test]
fn test_xml_text_roundtrip() {
    let text = SvgText {
        x: 165.931,
        y: 1144.47,
        fill: Some("#d3d3d3".to_string()),
        content: "raise-function".to_string(),
    };

    let xml_output = to_string(&text).unwrap();
    let parsed: SvgText = from_str(&xml_output).unwrap();

    assert_eq!(parsed, text, "Roundtrip should preserve all values");
}

/// Test xml::text with empty content
#[test]
fn test_xml_text_empty_content() {
    let text = SvgText {
        x: 10.0,
        y: 20.0,
        fill: None,
        content: "".to_string(),
    };

    let xml_output = to_string(&text).unwrap();
    // Even with empty content, it should not appear as an attribute
    assert!(
        !xml_output.contains("content="),
        "Empty content should NOT be serialized as an attribute: {xml_output}"
    );

    let parsed: SvgText = from_str(&xml_output).unwrap();
    assert_eq!(parsed, text);
}

/// Test xml::text with special characters that need escaping
#[test]
fn test_xml_text_escaping() {
    let text = SvgText {
        x: 0.0,
        y: 0.0,
        fill: None,
        // Note: We don't use spaces around special chars because XML parsers normalize whitespace
        content: "<hello>&\"world\"".to_string(),
    };

    let xml_output = to_string(&text).unwrap();
    // Content should be escaped as element text
    assert!(
        xml_output.contains("&lt;hello&gt;&amp;"),
        "Content should be properly escaped: {xml_output}"
    );

    let parsed: SvgText = from_str(&xml_output).unwrap();
    assert_eq!(
        parsed.content, "<hello>&\"world\"",
        "Roundtrip should unescape"
    );
}

/// Test xml::text with Option<String>
#[derive(Facet, Debug, PartialEq, Clone)]
#[facet(rename = "label")]
struct OptionalText {
    #[facet(xml::attribute)]
    id: String,
    #[facet(xml::text)]
    content: Option<String>,
}

#[test]
fn test_xml_text_optional_some() {
    let label = OptionalText {
        id: "lbl1".to_string(),
        content: Some("Hello World".to_string()),
    };

    let xml_output = to_string(&label).unwrap();
    assert!(
        !xml_output.contains("content="),
        "content should NOT be an attribute: {xml_output}"
    );
    assert!(
        xml_output.contains(">Hello World</"),
        "content should be element text: {xml_output}"
    );

    let parsed: OptionalText = from_str(&xml_output).unwrap();
    assert_eq!(parsed, label);
}

#[test]
fn test_xml_text_optional_none() {
    let label = OptionalText {
        id: "lbl2".to_string(),
        content: None,
    };

    let xml_output = to_string(&label).unwrap();
    // With None content, no text content should be emitted
    assert!(
        !xml_output.contains("content="),
        "None content should NOT be an attribute: {xml_output}"
    );

    let parsed: OptionalText = from_str(&xml_output).unwrap();
    assert_eq!(parsed, label);
}

/// Test mixed attributes and xml::text (like the facet-svg Text struct)
#[derive(Facet, Debug, PartialEq, Clone)]
#[facet(
    rename = "text",
    xml::ns_all = "http://www.w3.org/2000/svg",
    rename_all = "kebab-case"
)]
struct SvgTextFull {
    #[facet(xml::attribute)]
    x: Option<f64>,
    #[facet(xml::attribute)]
    y: Option<f64>,
    #[facet(xml::attribute)]
    fill: Option<String>,
    #[facet(xml::attribute)]
    text_anchor: Option<String>,
    #[facet(xml::attribute)]
    dominant_baseline: Option<String>,
    #[facet(xml::text)]
    content: String,
}

#[test]
fn test_svg_text_element_full() {
    // This is the exact case from issue #1495
    let text = SvgTextFull {
        x: Some(165.931),
        y: Some(1144.47),
        fill: Some("#d3d3d3".to_string()),
        text_anchor: Some("middle".to_string()),
        dominant_baseline: Some("central".to_string()),
        content: "raise-function".to_string(),
    };

    let xml_output = to_string(&text).unwrap();

    // Verify the expected output format
    // Expected: <text x="165.931" y="1144.47" fill="#d3d3d3" text-anchor="middle" dominant-baseline="central">raise-function</text>
    assert!(
        xml_output.contains("x=\"165.931\""),
        "x attribute should be present: {xml_output}"
    );
    assert!(
        xml_output.contains("y=\"1144.47\""),
        "y attribute should be present: {xml_output}"
    );
    assert!(
        xml_output.contains("fill=\"#d3d3d3\""),
        "fill attribute should be present: {xml_output}"
    );
    assert!(
        xml_output.contains("text-anchor=\"middle\""),
        "text-anchor attribute should be present: {xml_output}"
    );
    assert!(
        xml_output.contains("dominant-baseline=\"central\""),
        "dominant-baseline attribute should be present: {xml_output}"
    );
    assert!(
        !xml_output.contains("content="),
        "content should NOT be an attribute: {xml_output}"
    );
    assert!(
        xml_output.contains(">raise-function</"),
        "content should be element text: {xml_output}"
    );

    // Roundtrip
    let parsed: SvgTextFull = from_str(&xml_output).unwrap();
    assert_eq!(parsed, text);
}

// ============================================================================
// xml::elements tests - collecting multiple child elements into a Vec
// ============================================================================

/// Test that xml::elements collects multiple same-named elements into a Vec
#[derive(Facet, Debug, PartialEq, Clone)]
#[facet(rename = "container")]
struct ContainerWithItems {
    #[facet(xml::attribute)]
    id: Option<String>,
    #[facet(xml::elements)]
    items: Vec<Item>,
}

#[derive(Facet, Debug, PartialEq, Clone)]
#[facet(rename = "item")]
struct Item {
    #[facet(xml::attribute)]
    name: String,
}

#[test]
fn test_elements_same_name() {
    let xml = r#"<container id="c1">
        <item name="first"/>
        <item name="second"/>
        <item name="third"/>
    </container>"#;

    let parsed: ContainerWithItems = from_str(xml).unwrap();
    assert_eq!(parsed.id, Some("c1".to_string()));
    assert_eq!(parsed.items.len(), 3);
    assert_eq!(parsed.items[0].name, "first");
    assert_eq!(parsed.items[1].name, "second");
    assert_eq!(parsed.items[2].name, "third");
}

#[test]
fn test_elements_roundtrip() {
    let container = ContainerWithItems {
        id: Some("c1".to_string()),
        items: vec![
            Item {
                name: "first".to_string(),
            },
            Item {
                name: "second".to_string(),
            },
        ],
    };

    let xml_output = to_string(&container).unwrap();
    let parsed: ContainerWithItems = from_str(&xml_output).unwrap();
    assert_eq!(parsed, container);
}

/// Test xml::elements with an enum - different element names map to different variants
#[derive(Facet, Debug, PartialEq, Clone)]
#[facet(rename_all = "lowercase")]
#[repr(u8)]
enum Shape {
    Circle(CircleData),
    Rect(RectData),
    Line(LineData),
}

#[derive(Facet, Debug, PartialEq, Clone, Default)]
struct CircleData {
    #[facet(xml::attribute)]
    cx: Option<f64>,
    #[facet(xml::attribute)]
    cy: Option<f64>,
    #[facet(xml::attribute)]
    r: Option<f64>,
}

#[derive(Facet, Debug, PartialEq, Clone, Default)]
struct RectData {
    #[facet(xml::attribute)]
    x: Option<f64>,
    #[facet(xml::attribute)]
    y: Option<f64>,
    #[facet(xml::attribute)]
    width: Option<f64>,
    #[facet(xml::attribute)]
    height: Option<f64>,
}

#[derive(Facet, Debug, PartialEq, Clone, Default)]
struct LineData {
    #[facet(xml::attribute)]
    x1: Option<f64>,
    #[facet(xml::attribute)]
    y1: Option<f64>,
    #[facet(xml::attribute)]
    x2: Option<f64>,
    #[facet(xml::attribute)]
    y2: Option<f64>,
}

#[derive(Facet, Debug, PartialEq, Clone)]
#[facet(rename = "drawing")]
struct Drawing {
    #[facet(xml::attribute)]
    name: Option<String>,
    #[facet(xml::elements)]
    shapes: Vec<Shape>,
}

#[test]
fn test_elements_with_enum_variants() {
    let xml = r#"<drawing name="my-drawing">
        <circle cx="50" cy="50" r="25"/>
        <rect x="10" y="10" width="100" height="50"/>
        <line x1="0" y1="0" x2="100" y2="100"/>
        <circle cx="75" cy="75" r="10"/>
    </drawing>"#;

    let parsed: Drawing = from_str(xml).unwrap();
    assert_eq!(parsed.name, Some("my-drawing".to_string()));
    assert_eq!(parsed.shapes.len(), 4);

    // Check each shape
    match &parsed.shapes[0] {
        Shape::Circle(c) => {
            assert_eq!(c.cx, Some(50.0));
            assert_eq!(c.cy, Some(50.0));
            assert_eq!(c.r, Some(25.0));
        }
        _ => panic!("Expected Circle"),
    }

    match &parsed.shapes[1] {
        Shape::Rect(r) => {
            assert_eq!(r.x, Some(10.0));
            assert_eq!(r.y, Some(10.0));
            assert_eq!(r.width, Some(100.0));
            assert_eq!(r.height, Some(50.0));
        }
        _ => panic!("Expected Rect"),
    }

    match &parsed.shapes[2] {
        Shape::Line(l) => {
            assert_eq!(l.x1, Some(0.0));
            assert_eq!(l.y1, Some(0.0));
            assert_eq!(l.x2, Some(100.0));
            assert_eq!(l.y2, Some(100.0));
        }
        _ => panic!("Expected Line"),
    }

    match &parsed.shapes[3] {
        Shape::Circle(c) => {
            assert_eq!(c.cx, Some(75.0));
            assert_eq!(c.cy, Some(75.0));
            assert_eq!(c.r, Some(10.0));
        }
        _ => panic!("Expected Circle"),
    }
}

#[test]
fn test_elements_enum_roundtrip() {
    let drawing = Drawing {
        name: Some("test".to_string()),
        shapes: vec![
            Shape::Circle(CircleData {
                cx: Some(10.0),
                cy: Some(20.0),
                r: Some(5.0),
            }),
            Shape::Rect(RectData {
                x: Some(0.0),
                y: Some(0.0),
                width: Some(50.0),
                height: Some(30.0),
            }),
        ],
    };

    let xml_output = to_string(&drawing).unwrap();
    eprintln!("Serialized: {}", xml_output);

    let parsed: Drawing = from_str(&xml_output).unwrap();
    assert_eq!(parsed, drawing);
}

/// Test xml::elements with namespace support
#[derive(Facet, Debug, PartialEq, Clone)]
#[facet(rename = "svg", xml::ns_all = "http://www.w3.org/2000/svg")]
struct SimpleSvgWithElements {
    #[facet(xml::attribute, rename = "viewBox")]
    view_box: Option<String>,
    #[facet(xml::elements)]
    shapes: Vec<SvgShape>,
}

#[derive(Facet, Debug, PartialEq, Clone)]
#[facet(rename_all = "lowercase", xml::ns_all = "http://www.w3.org/2000/svg")]
#[repr(u8)]
enum SvgShape {
    Circle(SvgCircle),
    Rect(SvgRect),
}

#[derive(Facet, Debug, PartialEq, Clone, Default)]
#[facet(xml::ns_all = "http://www.w3.org/2000/svg")]
struct SvgCircle {
    #[facet(xml::attribute)]
    cx: Option<f64>,
    #[facet(xml::attribute)]
    cy: Option<f64>,
    #[facet(xml::attribute)]
    r: Option<f64>,
}

#[derive(Facet, Debug, PartialEq, Clone, Default)]
#[facet(xml::ns_all = "http://www.w3.org/2000/svg")]
struct SvgRect {
    #[facet(xml::attribute)]
    x: Option<f64>,
    #[facet(xml::attribute)]
    y: Option<f64>,
    #[facet(xml::attribute)]
    width: Option<f64>,
    #[facet(xml::attribute)]
    height: Option<f64>,
}

#[test]
fn test_elements_with_namespace() {
    let xml = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
        <circle cx="50" cy="50" r="25"/>
        <rect x="10" y="10" width="30" height="20"/>
    </svg>"#;

    let parsed: SimpleSvgWithElements = from_str(xml).unwrap();
    assert_eq!(parsed.view_box, Some("0 0 100 100".to_string()));
    assert_eq!(parsed.shapes.len(), 2);

    match &parsed.shapes[0] {
        SvgShape::Circle(c) => assert_eq!(c.r, Some(25.0)),
        _ => panic!("Expected Circle"),
    }
    match &parsed.shapes[1] {
        SvgShape::Rect(r) => assert_eq!(r.width, Some(30.0)),
        _ => panic!("Expected Rect"),
    }
}

#[test]
fn test_elements_namespace_roundtrip() {
    let svg = SimpleSvgWithElements {
        view_box: Some("0 0 200 200".to_string()),
        shapes: vec![
            SvgShape::Rect(SvgRect {
                x: Some(0.0),
                y: Some(0.0),
                width: Some(100.0),
                height: Some(100.0),
            }),
            SvgShape::Circle(SvgCircle {
                cx: Some(50.0),
                cy: Some(50.0),
                r: Some(20.0),
            }),
        ],
    };

    let xml_output = to_string(&svg).unwrap();
    let parsed: SimpleSvgWithElements = from_str(&xml_output).unwrap();
    assert_eq!(parsed, svg);
}

/// Test empty elements list
#[test]
fn test_elements_empty_list() {
    let xml = r#"<container id="empty"></container>"#;

    let parsed: ContainerWithItems = from_str(xml).unwrap();
    assert_eq!(parsed.id, Some("empty".to_string()));
    assert!(parsed.items.is_empty());
}

/// Test elements interleaved with other fields
#[derive(Facet, Debug, PartialEq, Clone)]
#[facet(rename = "doc")]
struct DocumentWithMixedChildren {
    #[facet(xml::element)]
    title: Option<String>,
    #[facet(xml::elements)]
    paragraphs: Vec<Paragraph>,
    #[facet(xml::element)]
    footer: Option<String>,
}

#[derive(Facet, Debug, PartialEq, Clone)]
#[facet(rename = "p")]
struct Paragraph {
    #[facet(xml::text)]
    content: String,
}

#[test]
fn test_elements_mixed_with_single_elements() {
    let xml = r#"<doc>
        <title>My Document</title>
        <p>First paragraph</p>
        <p>Second paragraph</p>
        <footer>The End</footer>
    </doc>"#;

    let parsed: DocumentWithMixedChildren = from_str(xml).unwrap();
    assert_eq!(parsed.title, Some("My Document".to_string()));
    assert_eq!(parsed.paragraphs.len(), 2);
    assert_eq!(parsed.paragraphs[0].content, "First paragraph");
    assert_eq!(parsed.paragraphs[1].content, "Second paragraph");
    assert_eq!(parsed.footer, Some("The End".to_string()));
}

#[test]
fn debug_shape_variants() {
    use facet::Facet;
    use facet_core::{Type, UserType};

    let shape = <Shape as Facet>::SHAPE;
    eprintln!("Shape: {:?}", shape.type_identifier);
    if let Type::User(UserType::Enum(e)) = &shape.ty {
        for v in e.variants {
            eprintln!("Variant name: {}", v.name);
            for attr in v.attributes {
                eprintln!("  Attr ns={:?} key={}", attr.ns, attr.key);
            }
        }
    }
}

#[test]
fn debug_elements_roundtrip() {
    let container = ContainerWithItems {
        id: Some("c1".to_string()),
        items: vec![
            Item {
                name: "first".to_string(),
            },
            Item {
                name: "second".to_string(),
            },
        ],
    };

    let xml_output = to_string(&container).unwrap();
    eprintln!("Serialized XML: {}", xml_output);
    // Try to parse it back
    let parsed: Result<ContainerWithItems, _> = from_str(&xml_output);
    eprintln!("Parse result: {:?}", parsed);
}

// ============================================================================
// SerializeOptions tests (GitHub issue #1501)
// ============================================================================

#[test]
fn test_to_string_pretty() {
    use facet_xml::to_string_pretty;

    #[derive(Facet, Debug, PartialEq)]
    #[facet(rename = "root")]
    struct SimpleStruct {
        #[facet(xml::element)]
        name: String,
        #[facet(xml::element)]
        value: i32,
    }

    let data = SimpleStruct {
        name: "test".to_string(),
        value: 42,
    };

    let xml_output = to_string_pretty(&data).unwrap();

    // Pretty output should contain newlines
    assert!(
        xml_output.contains('\n'),
        "Pretty output should contain newlines: {xml_output}"
    );

    // Should be properly indented
    assert!(
        xml_output.contains("  <name>"),
        "Elements should be indented: {xml_output}"
    );

    // Should still roundtrip
    let parsed: SimpleStruct = from_str(&xml_output).unwrap();
    assert_eq!(parsed, data);
}

#[test]
fn test_serialize_options_custom_indent() {
    use facet_xml::{SerializeOptions, to_string_with_options};

    #[derive(Facet, Debug, PartialEq)]
    #[facet(rename = "root")]
    struct Item {
        #[facet(xml::element)]
        name: String,
    }

    let data = Item {
        name: "test".to_string(),
    };

    let options = SerializeOptions::new().indent("\t");
    let xml_output = to_string_with_options(&data, &options).unwrap();

    // Should use tab indentation
    assert!(
        xml_output.contains("\t<name>"),
        "Should use tab indentation: {xml_output:?}"
    );

    // Should still roundtrip
    let parsed: Item = from_str(&xml_output).unwrap();
    assert_eq!(parsed, data);
}

#[test]
fn test_serialize_options_float_formatter() {
    use facet_xml::{SerializeOptions, to_string_with_options};
    use std::io::Write;

    #[derive(Facet, Debug, PartialEq)]
    #[facet(rename = "point")]
    struct Point {
        #[facet(xml::attribute)]
        x: f64,
        #[facet(xml::attribute)]
        y: f64,
    }

    let point = Point {
        x: 1.123456789,
        y: 2.0,
    };

    // Custom formatter that uses 3 decimal places
    fn fmt_3dec(value: f64, w: &mut dyn Write) -> std::io::Result<()> {
        write!(w, "{:.3}", value)
    }

    let options = SerializeOptions::new().float_formatter(fmt_3dec);
    let xml_output = to_string_with_options(&point, &options).unwrap();

    // Should have formatted floats
    assert!(
        xml_output.contains("x=\"1.123\""),
        "x should be formatted to 3 decimals: {xml_output}"
    );
    assert!(
        xml_output.contains("y=\"2.000\""),
        "y should be formatted to 3 decimals: {xml_output}"
    );
}

#[test]
fn test_serialize_options_preserve_entities() {
    use facet_xml::{SerializeOptions, to_string_with_options};

    #[derive(Facet, Debug)]
    #[facet(rename = "root")]
    struct Content {
        #[facet(xml::element)]
        text: String,
    }

    let data = Content {
        text: "Hello &amp; World &lt;3".to_string(),
    };

    // Without preserve_entities (default) - & gets escaped to &amp;
    let xml_default = to_string(&data).unwrap();
    assert!(
        xml_default.contains("&amp;amp;"),
        "Without preserve_entities, & should be escaped: {xml_default}"
    );

    // With preserve_entities - entities are preserved
    let options = SerializeOptions::new().preserve_entities(true);
    let xml_preserved = to_string_with_options(&data, &options).unwrap();
    assert!(
        xml_preserved.contains("&amp;") && !xml_preserved.contains("&amp;amp;"),
        "With preserve_entities, &amp; should be preserved: {xml_preserved}"
    );
}
