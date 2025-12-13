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
#[ignore] // Will fail until Phase 4 is complete
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
#[ignore] // Will fail until Phase 4 is complete
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
#[ignore] // Will fail until Phase 4 is complete
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
