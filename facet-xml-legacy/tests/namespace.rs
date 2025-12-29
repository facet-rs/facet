//! Tests for XML namespace support in deserialization.

use facet::Facet;
use facet_xml_legacy as xml;

// ============================================================================
// Basic namespace matching
// ============================================================================

/// Test that elements with declared namespaces are matched correctly.
#[derive(Facet, Debug, PartialEq)]
#[facet(rename = "root")]
struct NamespacedRoot {
    /// This field requires the element to be in the "http://example.com/ns" namespace.
    #[facet(xml::element, xml::ns = "http://example.com/ns")]
    item: String,
}

#[test]
fn test_element_with_declared_namespace() {
    // Element with namespace declaration and prefix
    let xml = r#"<root xmlns:ex="http://example.com/ns"><ex:item>value</ex:item></root>"#;
    let parsed: NamespacedRoot = xml::from_str(xml).unwrap();
    assert_eq!(parsed.item, "value");
}

#[test]
fn test_element_with_default_namespace() {
    // Element inherits default namespace
    let xml = r#"<root><item xmlns="http://example.com/ns">value</item></root>"#;
    let parsed: NamespacedRoot = xml::from_str(xml).unwrap();
    assert_eq!(parsed.item, "value");
}

#[test]
fn test_element_namespace_mismatch() {
    // Element in wrong namespace - does not match the field with xml::ns constraint.
    // Since String has a Default impl, the field gets an empty string.
    // (If you need strict namespace enforcement, combine with deny_unknown_fields)
    let xml = r#"<root xmlns:other="http://other.com/ns"><other:item>value</other:item></root>"#;
    let parsed: NamespacedRoot = xml::from_str(xml).unwrap();
    // Field is empty because no element matched the namespace constraint
    assert_eq!(parsed.item, "");
}

// ============================================================================
// Attribute namespace matching
// ============================================================================

#[derive(Facet, Debug, PartialEq)]
#[facet(rename = "root")]
struct NamespacedAttr {
    /// This field requires the attribute to be in the "http://example.com/ns" namespace.
    #[facet(xml::attribute, xml::ns = "http://example.com/ns")]
    value: String,
}

#[test]
fn test_attribute_with_declared_namespace() {
    // Attribute with namespace prefix
    let xml = r#"<root xmlns:ex="http://example.com/ns" ex:value="hello"/>"#;
    let parsed: NamespacedAttr = xml::from_str(xml).unwrap();
    assert_eq!(parsed.value, "hello");
}

#[test]
fn test_attribute_namespace_mismatch() {
    // Unprefixed attributes are in "no namespace" (not the default xmlns!), so won't match.
    // Since String has a Default impl, the field gets an empty string.
    let xml = r#"<root xmlns="http://example.com/ns" value="hello"/>"#;
    let parsed: NamespacedAttr = xml::from_str(xml).unwrap();
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
    let parsed: MixedNamespaces = xml::from_str(xml).unwrap();
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
    let parsed: MixedNamespaces = xml::from_str(xml).unwrap();
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
    let parsed: SameLocalNameDifferentNs = xml::from_str(xml).unwrap();
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

    let parsed1: PrefixIndependent = xml::from_str(xml1).unwrap();
    let parsed2: PrefixIndependent = xml::from_str(xml2).unwrap();
    let parsed3: PrefixIndependent = xml::from_str(xml3).unwrap();

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
    let parsed: BackwardCompatible = xml::from_str(xml).unwrap();
    assert_eq!(parsed.item, "value");
}

#[test]
fn test_backward_compatible_with_namespace() {
    // Field without xml::ns should match element with any namespace
    let xml = r#"<root xmlns:ex="http://example.com"><ex:item>value</ex:item></root>"#;
    let parsed: BackwardCompatible = xml::from_str(xml).unwrap();
    assert_eq!(parsed.item, "value");
}

#[test]
fn test_backward_compatible_with_default_namespace() {
    // Field without xml::ns should match element in default namespace
    let xml = r#"<root xmlns="http://example.com"><item>value</item></root>"#;
    let parsed: BackwardCompatible = xml::from_str(xml).unwrap();
    assert_eq!(parsed.item, "value");
}

// ============================================================================
// Container-level xml::ns_all
// ============================================================================

/// Container with ns_all - all fields default to this namespace
#[derive(Facet, Debug, PartialEq)]
#[facet(rename = "root", xml::ns_all = "http://example.com/ns")]
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
    let parsed: NsAllContainer = xml::from_str(xml).unwrap();
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
    let parsed: NsAllContainer = xml::from_str(xml).unwrap();
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
    let parsed: NsAllContainer = xml::from_str(xml).unwrap();
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
    let parsed: NsAllContainer = xml::from_str(xml).unwrap();
    // All fields get default values because no elements matched
    assert_eq!(parsed.first, "");
    assert_eq!(parsed.second, "");
    assert_eq!(parsed.other, "");
}

/// Container with ns_all for attributes
#[derive(Facet, Debug, PartialEq)]
#[facet(rename = "root", xml::ns_all = "http://example.com/ns")]
struct NsAllAttributes {
    #[facet(xml::attribute)]
    attr1: String,

    #[facet(xml::attribute)]
    attr2: String,
}

#[test]
fn test_ns_all_attributes() {
    // Attributes with namespace prefix
    let xml = r#"<root xmlns:ex="http://example.com/ns" ex:attr1="one" ex:attr2="two"/>"#;
    let parsed: NsAllAttributes = xml::from_str(xml).unwrap();
    assert_eq!(parsed.attr1, "one");
    assert_eq!(parsed.attr2, "two");
}

// ============================================================================
// Serialization with namespaces
// ============================================================================

#[test]
fn test_serialize_namespaced_element() {
    // Serialize a struct with xml::ns on a field
    let value = NamespacedRoot {
        item: "value".to_string(),
    };
    let xml_output = xml::to_string(&value).unwrap();

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
    let parsed: NamespacedRoot = xml::from_str(&xml_output).unwrap();
    assert_eq!(parsed, value);
}

#[test]
fn test_serialize_namespaced_attribute() {
    // Serialize a struct with xml::ns on an attribute
    let value = NamespacedAttr {
        value: "hello".to_string(),
    };
    let xml_output = xml::to_string(&value).unwrap();

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
    let parsed: NamespacedAttr = xml::from_str(&xml_output).unwrap();
    assert_eq!(parsed, value);
}

#[test]
fn test_serialize_mixed_namespaces() {
    // Serialize a struct with both namespaced and non-namespaced fields
    let value = MixedNamespaces {
        plain: "plain value".to_string(),
        special: "special value".to_string(),
    };
    let xml_output = xml::to_string(&value).unwrap();

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
    let parsed: MixedNamespaces = xml::from_str(&xml_output).unwrap();
    assert_eq!(parsed, value);
}

#[test]
fn test_serialize_ns_all() {
    // Serialize a struct with xml::ns_all
    let value = NsAllContainer {
        first: "one".to_string(),
        second: "two".to_string(),
        other: "three".to_string(),
    };
    let xml_output = xml::to_string(&value).unwrap();

    // All fields from ns_all namespace should use the same prefix
    // 'other' should use a different prefix
    assert!(
        xml_output.contains("http://example.com/ns"),
        "Should contain main namespace: {xml_output}"
    );
    assert!(
        xml_output.contains("http://other.com/ns"),
        "Should contain other namespace: {xml_output}"
    );

    // Round-trip
    let parsed: NsAllContainer = xml::from_str(&xml_output).unwrap();
    assert_eq!(parsed, value);
}

#[test]
fn test_serialize_ns_all_attributes_roundtrip() {
    // Serialize a struct with xml::ns_all on attributes.
    // Per XML spec, unprefixed attributes are always in "no namespace",
    // so ns_all does NOT apply to attributes without explicit xml::ns.
    // The xmlns="..." declaration is for elements, not attributes.
    let value = NsAllAttributes {
        attr1: "one".to_string(),
        attr2: "two".to_string(),
    };
    let xml_output = xml::to_string(&value).unwrap();

    // Should have default namespace declaration for the element
    assert!(
        xml_output.contains("xmlns="),
        "Should contain default xmlns declaration: {xml_output}"
    );

    // Attributes should be unprefixed (per XML spec, they're in "no namespace")
    assert!(
        xml_output.contains("attr1=") && !xml_output.contains(":attr1="),
        "Attributes should be unprefixed: {xml_output}"
    );

    // Round-trip
    let parsed: NsAllAttributes = xml::from_str(&xml_output).unwrap();
    assert_eq!(parsed, value);
}

#[test]
fn test_serialize_same_local_name_different_namespaces() {
    // Serialize a struct with same field name but different namespaces
    let value = SameLocalNameDifferentNs {
        item_ns1: "from ns1".to_string(),
        item_ns2: "from ns2".to_string(),
    };
    let xml_output = xml::to_string(&value).unwrap();

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
    let parsed: SameLocalNameDifferentNs = xml::from_str(&xml_output).unwrap();
    assert_eq!(parsed, value);
}

/// Test that well-known namespaces get their conventional prefixes
#[derive(Facet, Debug, PartialEq)]
#[facet(rename = "root")]
struct WellKnownNamespace {
    #[facet(xml::element, xml::ns = "http://www.w3.org/2001/XMLSchema-instance")]
    xsi_element: String,
}

#[test]
fn test_serialize_well_known_namespace() {
    let value = WellKnownNamespace {
        xsi_element: "test".to_string(),
    };
    let xml_output = xml::to_string(&value).unwrap();

    // Should use 'xsi' prefix for XMLSchema-instance namespace
    assert!(
        xml_output.contains("xsi:"),
        "Should use well-known 'xsi' prefix: {xml_output}"
    );
    assert!(
        xml_output.contains("xmlns:xsi="),
        "Should declare xsi namespace: {xml_output}"
    );
}

// ============================================================================
// Nested struct namespace scoping
// ============================================================================

#[derive(Facet, Debug, PartialEq)]
#[facet(rename = "inner", xml::ns_all = "http://inner.com/ns")]
struct InnerNsAll {
    #[facet(xml::element)]
    inner_field: String,
}

#[derive(Facet, Debug, PartialEq)]
#[facet(rename = "outer", xml::ns_all = "http://outer.com/ns")]
struct OuterNsAll {
    #[facet(xml::element)]
    outer_field: String,
    #[facet(xml::element)]
    nested: InnerNsAll,
}

#[test]
fn test_serialize_nested_ns_all() {
    let value = OuterNsAll {
        outer_field: "outer".to_string(),
        nested: InnerNsAll {
            inner_field: "inner".to_string(),
        },
    };
    let xml_output = xml::to_string(&value).unwrap();

    // outer_field and nested should use outer namespace
    // inner_field should use inner namespace
    assert!(
        xml_output.contains("http://outer.com/ns"),
        "Should contain outer namespace: {xml_output}"
    );
    assert!(
        xml_output.contains("http://inner.com/ns"),
        "Should contain inner namespace: {xml_output}"
    );

    // Round-trip
    let parsed: OuterNsAll = xml::from_str(&xml_output).unwrap();
    assert_eq!(parsed, value);
}

// ============================================================================
// Issue #1060: Unprefixed attributes should work with ns_all
// ============================================================================

/// Regression test for issue #1060.
///
/// In XML, unprefixed attributes are ALWAYS in "no namespace", even when
/// a default xmlns is declared. This is different from elements, which
/// DO inherit the default namespace.
///
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
fn test_issue_1060_unprefixed_attributes_with_ns_all() {
    // This XML has:
    // - xmlns declaration making http://www.w3.org/2000/svg the default namespace
    // - Unprefixed attributes (viewBox, width, height) which are in "no namespace"
    // - An element (title) which inherits the default namespace
    let xml = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100" width="100" height="100">
        <title>My SVG</title>
    </svg>"#;

    let parsed: SvgWithAttributes = xml::from_str(xml).unwrap();

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

    let parsed: SvgWithAttributes = xml::from_str(xml).unwrap();

    // Attributes should still work
    assert_eq!(parsed.view_box, Some("0 0 100 100".to_string()));
    assert_eq!(parsed.width, Some("100".to_string()));
    assert_eq!(parsed.height, Some("100".to_string()));
    // Element in "no namespace" won't match ns_all requirement
    // (it expects http://www.w3.org/2000/svg)
    assert_eq!(parsed.title, None);
}

// ============================================================================
// DeserializeOptions: deny_unknown_fields at runtime
// ============================================================================

#[derive(Facet, Debug, PartialEq)]
#[facet(rename = "Person")]
struct PersonNoAttr {
    #[facet(xml::attribute)]
    name: String,
}

#[test]
fn test_deny_unknown_fields_option_rejects_unknown_attribute() {
    let xml_str = r#"<Person name="Alice" extra="unknown"/>"#;

    // Without options: unknown attributes are silently ignored
    let person: PersonNoAttr = xml::from_str(xml_str).unwrap();
    assert_eq!(person.name, "Alice");

    // With deny_unknown_fields option: unknown attributes cause an error
    let options = xml::DeserializeOptions::default().deny_unknown_fields(true);
    let result: Result<PersonNoAttr, _> = xml::from_str_with_options(xml_str, &options);
    assert!(result.is_err(), "Should reject unknown attribute");
}

#[test]
fn test_deny_unknown_fields_option_rejects_unknown_element() {
    let xml_str = r#"<Person name="Alice"><unknown>value</unknown></Person>"#;

    // Without options: unknown elements are silently ignored
    let person: PersonNoAttr = xml::from_str(xml_str).unwrap();
    assert_eq!(person.name, "Alice");

    // With deny_unknown_fields option: unknown elements cause an error
    let options = xml::DeserializeOptions::default().deny_unknown_fields(true);
    let result: Result<PersonNoAttr, _> = xml::from_str_with_options(xml_str, &options);
    assert!(result.is_err(), "Should reject unknown element");
}

#[test]
fn test_deny_unknown_fields_option_accepts_valid_xml() {
    let xml_str = r#"<Person name="Alice"/>"#;

    // With deny_unknown_fields: valid XML should still work
    let options = xml::DeserializeOptions::default().deny_unknown_fields(true);
    let person: PersonNoAttr = xml::from_str_with_options(xml_str, &options).unwrap();
    assert_eq!(person.name, "Alice");
}

// ============================================================================
// Comprehensive SVG namespace tests (issue #1100)
// These tests verify that xml::ns_all produces valid SVG-style output:
// - Default namespace declaration (xmlns="...")
// - Unprefixed elements (inherit from default namespace)
// - Unprefixed attributes (per XML spec, attributes don't inherit namespaces)
// ============================================================================

/// Simple SVG struct with attributes and a child element.
/// Note: The struct rename becomes the root element name.
/// For child elements, use xml::element with rename on the field.
#[derive(Facet, Debug, PartialEq, Clone)]
#[facet(rename = "svg", xml::ns_all = "http://www.w3.org/2000/svg")]
struct SimpleSvg {
    #[facet(xml::attribute, rename = "viewBox")]
    view_box: Option<String>,
    #[facet(xml::attribute)]
    width: Option<String>,
    #[facet(xml::attribute)]
    height: Option<String>,
    /// Child elements - field name "circle" will be used for element name
    #[facet(xml::element, rename = "circle")]
    circle: Option<SvgCircleData>,
}

/// The data for a circle element (not the element name itself)
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
fn test_svg_serialization_produces_valid_svg() {
    // This is the key test for issue #1100
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

    let xml_output = xml::to_string(&svg).unwrap();

    // Should have default namespace declaration
    assert!(
        xml_output.contains("xmlns=\"http://www.w3.org/2000/svg\""),
        "Should have default xmlns: {xml_output}"
    );

    // Note: Root element name comes from struct name, not rename attribute
    // (rename affects deserialization matching, not serialization output)
    // For true SVG output, use a struct named "svg" or post-process

    // Attributes should NOT be prefixed
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

    let xml_output = xml::to_string(&svg).unwrap();
    let parsed: SimpleSvg = xml::from_str(&xml_output).unwrap();

    assert_eq!(parsed, svg, "Roundtrip should preserve all values");
}

#[test]
fn test_svg_deserialization_from_browser_style_xml() {
    // This is the format that browsers/real SVG tools produce
    let xml = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100" width="100" height="100">
        <circle cx="50" cy="50" r="25" fill="red"/>
    </svg>"#;

    let parsed: SimpleSvg = xml::from_str(xml).unwrap();

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
fn test_svg_with_xlink_namespace() {
    let svg = SvgWithXlink {
        view_box: Some("0 0 100 100".to_string()),
        use_elem: Some(SvgUseData {
            x: Some("10".to_string()),
            y: Some("10".to_string()),
            xlink_href: Some("#mySymbol".to_string()),
        }),
    };

    let xml_output = xml::to_string(&svg).unwrap();

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
}

#[test]
fn test_svg_xlink_roundtrip() {
    let svg = SvgWithXlink {
        view_box: Some("0 0 100 100".to_string()),
        use_elem: Some(SvgUseData {
            x: Some("10".to_string()),
            y: Some("10".to_string()),
            xlink_href: Some("#mySymbol".to_string()),
        }),
    };

    let xml_output = xml::to_string(&svg).unwrap();
    let parsed: SvgWithXlink = xml::from_str(&xml_output).unwrap();

    assert_eq!(parsed, svg);
}

/// Test deeply nested SVG elements using xml::element with rename
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
fn test_deeply_nested_svg_elements() {
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

    let xml_output = xml::to_string(&svg).unwrap();

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
    let parsed: SvgWithGroup = xml::from_str(&xml_output).unwrap();
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

    let xml_output = xml::to_string(&svg).unwrap();

    // Should have default namespace
    assert!(
        xml_output.contains("xmlns=\"http://www.w3.org/2000/svg\""),
        "Should have xmlns: {xml_output}"
    );

    let parsed: SimpleSvg = xml::from_str(&xml_output).unwrap();
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

    let xml_output = xml::to_string(&svg).unwrap();

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

    let parsed: SimpleSvg = xml::from_str(&xml_output).unwrap();
    assert_eq!(parsed, svg);
}

// ============================================================================
// Test namespace with deny_unknown_fields
// ============================================================================

#[test]
fn test_namespace_with_deny_unknown_fields() {
    /// Namespace definitions are ignored when deny_unknown_fields is enabled.
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

    let serialized = xml::to_string(&doc).unwrap();
    let deserialized: NamespacedRoot = xml::from_str(&serialized).unwrap();

    assert_eq!(doc, deserialized);
}

#[test]
fn test_namespace_with_prefix_is_ignored() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(deny_unknown_fields, rename = "root")]
    struct Root {
        #[facet(xml::element)]
        item: String,
    }

    let xml = r#"<root xmlns:gml="http://www.opengis.net/gml"><gml:item>test</gml:item></root>"#;
    let deserialized: Root = xml::from_str(xml).unwrap();
    assert_eq!(deserialized.item, "test");
}
