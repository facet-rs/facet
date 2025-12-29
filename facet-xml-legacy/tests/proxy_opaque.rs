//! Tests for all four combinations of opaque/proxy attributes.
//!
//! The four combinations are:
//! 1. Neither opaque nor proxy: normal Facet deserialization
//! 2. Only opaque: type is treated as opaque, no Facet impl needed, but no conversion
//! 3. Only proxy: type implements Facet, proxy handles conversion (e.g., validation)
//! 4. Both opaque and proxy: type doesn't implement Facet, proxy handles everything

use facet::Facet;
use facet_testhelpers::test;
use facet_xml_legacy as xml;

/// Test for issue #1112: proxy without opaque should work.
/// Previously this failed with "not currently processing a field" because
/// begin_custom_deserialization was called after begin_some(), losing the field context.
#[test]
fn test_proxy_without_opaque() {
    // Target type that DOES implement Facet
    #[derive(Facet, Debug, Clone, Default, PartialEq)]
    pub struct StyleData {
        pub value: String,
    }

    // Proxy type that implements Facet
    #[derive(Facet, Clone, Debug)]
    #[facet(transparent)]
    pub struct StyleProxy(pub String);

    // Convert proxy -> Option<StyleData> (deserialization)
    impl From<StyleProxy> for Option<StyleData> {
        fn from(proxy: StyleProxy) -> Self {
            if proxy.0.is_empty() {
                None
            } else {
                Some(StyleData { value: proxy.0 })
            }
        }
    }

    // Convert &Option<StyleData> -> proxy (serialization)
    impl From<&Option<StyleData>> for StyleProxy {
        fn from(v: &Option<StyleData>) -> Self {
            StyleProxy(v.as_ref().map(|d| d.value.clone()).unwrap_or_default())
        }
    }

    #[derive(Facet, Debug, Clone, Default, PartialEq)]
    pub struct Element {
        // Note: NO opaque attribute! Just proxy. This used to fail.
        #[facet(default, xml::attribute, proxy = StyleProxy)]
        pub style: Option<StyleData>,
    }

    // Test with attribute present
    let xml_input = r#"<Element style="color:red"/>"#;
    let elem: Element = xml::from_str(xml_input).unwrap();
    assert_eq!(
        elem.style,
        Some(StyleData {
            value: "color:red".to_string()
        })
    );

    // Test with absent attribute (uses default)
    let xml2 = r#"<Element/>"#;
    let elem2: Element = xml::from_str(xml2).unwrap();
    assert!(elem2.style.is_none());

    // Test with empty attribute (proxy converts to None)
    let xml3 = r#"<Element style=""/>"#;
    let elem3: Element = xml::from_str(xml3).unwrap();
    assert!(elem3.style.is_none());

    // Test serialization roundtrip
    let elem4 = Element {
        style: Some(StyleData {
            value: "font-size:12px".to_string(),
        }),
    };
    let serialized = xml::to_string(&elem4).unwrap();
    assert_eq!(serialized, r#"<Element style="font-size:12px"/>"#);

    let deserialized: Element = xml::from_str(&serialized).unwrap();
    assert_eq!(elem4, deserialized);
}

/// Test for issue #1075: UB/SIGABRT when using opaque + proxy on Option<T> fields
#[test]
fn test_opaque_with_proxy_option_simple() {
    // Target type that doesn't implement Facet
    #[derive(Debug, Clone, Default)]
    pub struct PathData {
        pub commands: Vec<String>,
    }

    // Proxy type that implements Facet
    #[derive(Facet, Clone, Debug)]
    #[facet(transparent)]
    pub struct PathDataProxy(pub String);

    // Convert proxy -> Option<PathData> (deserialization)
    impl From<PathDataProxy> for Option<PathData> {
        fn from(proxy: PathDataProxy) -> Self {
            Some(PathData {
                commands: vec![proxy.0],
            })
        }
    }

    // Convert &Option<PathData> -> proxy (serialization)
    impl From<&Option<PathData>> for PathDataProxy {
        fn from(v: &Option<PathData>) -> Self {
            PathDataProxy(v.as_ref().map(|d| d.commands.join(",")).unwrap_or_default())
        }
    }

    #[derive(Facet, Debug, Clone, Default)]
    pub struct Path {
        #[facet(default, xml::attribute, opaque, proxy = PathDataProxy)]
        pub d: Option<PathData>,
    }

    // Test with absent attribute (uses default) - this triggers SIGSEGV before fix
    let xml_input = r#"<Path/>"#;
    let path: Path = xml::from_str(xml_input).unwrap();
    assert!(path.d.is_none());

    // Test with attribute present
    let xml2 = r#"<Path d="M0,0 L10,10"/>"#;
    let path2: Path = xml::from_str(xml2).unwrap();
    assert!(path2.d.is_some());
    assert_eq!(
        path2.d.as_ref().unwrap().commands,
        vec!["M0,0 L10,10".to_string()]
    );
}

/// Test opaque + proxy with nested enum wrapper and namespaces
#[test]
fn test_opaque_with_proxy_nested_enum() {
    // Target type that doesn't implement Facet
    #[derive(Debug, Clone, Default)]
    pub struct PathData {
        pub commands: Vec<String>,
    }

    // Proxy type that implements Facet
    #[derive(Facet, Clone, Debug)]
    #[facet(transparent)]
    pub struct PathDataProxy(pub String);

    // Convert proxy -> Option<PathData> (deserialization)
    impl From<PathDataProxy> for Option<PathData> {
        fn from(proxy: PathDataProxy) -> Self {
            Some(PathData {
                commands: vec![proxy.0],
            })
        }
    }

    // Convert &Option<PathData> -> proxy (serialization)
    impl From<&Option<PathData>> for PathDataProxy {
        fn from(v: &Option<PathData>) -> Self {
            PathDataProxy(v.as_ref().map(|d| d.commands.join(",")).unwrap_or_default())
        }
    }

    #[derive(Facet, Debug, Clone, Default)]
    #[facet(xml::ns_all = "http://www.w3.org/2000/svg")]
    pub struct Path {
        #[facet(default, xml::attribute, opaque, proxy = PathDataProxy)]
        pub d: Option<PathData>,
    }

    #[derive(Facet, Debug, Clone)]
    #[facet(xml::ns_all = "http://www.w3.org/2000/svg")]
    #[repr(u8)]
    pub enum SvgNode {
        #[facet(rename = "path")]
        Path(Path),
    }

    #[derive(Facet, Debug, Clone)]
    #[facet(xml::ns_all = "http://www.w3.org/2000/svg")]
    pub struct Svg {
        #[facet(xml::elements)]
        pub children: Vec<SvgNode>,
    }

    // Test with path element that has the d attribute
    let xml_input = r#"<Svg xmlns="http://www.w3.org/2000/svg"><path d="M0,0 L10,10"/></Svg>"#;
    let svg: Svg = xml::from_str(xml_input).unwrap();
    assert_eq!(svg.children.len(), 1);
    let SvgNode::Path(path) = &svg.children[0];
    assert!(path.d.is_some());
    assert_eq!(
        path.d.as_ref().unwrap().commands,
        vec!["M0,0 L10,10".to_string()]
    );

    // Test with path element WITHOUT the d attribute (uses default) - this triggers the bug
    let xml2 = r#"<Svg xmlns="http://www.w3.org/2000/svg"><path/></Svg>"#;
    let svg2: Svg = xml::from_str(xml2).unwrap();
    assert_eq!(svg2.children.len(), 1);
    let SvgNode::Path(path) = &svg2.children[0];
    assert!(path.d.is_none());
}

// =============================================================================
// Tests for proxy fields with missing XML attributes and various default sources
// =============================================================================
//
// These tests verify that when an XML attribute is missing, facet-xml correctly
// uses the default value. There are several ways defaults can be specified:
//
// 1. Field-level #[facet(default)] attribute
// 2. Type declares Default via #[facet(traits(Default))]
// 3. Container-level #[facet(default)] (all fields get defaults)
//
// IMPORTANT: #[derive(Default)] alone is NOT sufficient! Rust strips #[derive(...)]
// attributes before derive macros run, so Facet cannot detect them. You MUST use
// #[facet(traits(Default))] to tell Facet that a type implements Default.
//
// The bug we're testing: when a proxy field's XML attribute is missing,
// the field should use its default value if one is available.

/// A style type that implements Default and declares it to facet
#[derive(Default, Facet, Debug, Clone, PartialEq)]
#[facet(traits(Default))]
pub struct StyleWithDeriveDefault {
    pub properties: String,
}

/// Proxy for StyleWithDeriveDefault
#[derive(Facet, Clone, Debug)]
#[facet(transparent)]
pub struct StyleWithDeriveDefaultProxy(pub String);

impl From<StyleWithDeriveDefaultProxy> for StyleWithDeriveDefault {
    fn from(proxy: StyleWithDeriveDefaultProxy) -> Self {
        StyleWithDeriveDefault {
            properties: proxy.0,
        }
    }
}

impl From<&StyleWithDeriveDefault> for StyleWithDeriveDefaultProxy {
    fn from(v: &StyleWithDeriveDefault) -> Self {
        StyleWithDeriveDefaultProxy(v.properties.clone())
    }
}

/// A style type that declares Default via facet traits attribute
#[derive(Facet, Debug, Clone, PartialEq)]
#[facet(traits(Default))]
pub struct StyleWithTraitsDefault {
    pub properties: String,
}

impl Default for StyleWithTraitsDefault {
    fn default() -> Self {
        StyleWithTraitsDefault {
            properties: "default-from-traits".to_string(),
        }
    }
}

/// Proxy for StyleWithTraitsDefault
#[derive(Facet, Clone, Debug)]
#[facet(transparent)]
pub struct StyleWithTraitsDefaultProxy(pub String);

impl From<StyleWithTraitsDefaultProxy> for StyleWithTraitsDefault {
    fn from(proxy: StyleWithTraitsDefaultProxy) -> Self {
        StyleWithTraitsDefault {
            properties: proxy.0,
        }
    }
}

impl From<&StyleWithTraitsDefault> for StyleWithTraitsDefaultProxy {
    fn from(v: &StyleWithTraitsDefault) -> Self {
        StyleWithTraitsDefaultProxy(v.properties.clone())
    }
}

/// Test 1: Proxy field with #[facet(default)] on field - attribute missing
#[test]
fn test_proxy_missing_attr_field_default() {
    #[derive(Facet, Debug, Clone, PartialEq)]
    pub struct Element {
        #[facet(default, xml::attribute, proxy = StyleWithDeriveDefaultProxy)]
        pub style: StyleWithDeriveDefault,
    }

    // Attribute present - should use provided value
    let xml_with = r#"<Element style="color:red"/>"#;
    let elem: Element = xml::from_str(xml_with).unwrap();
    assert_eq!(elem.style.properties, "color:red");

    // Attribute missing - should use default (empty string from Default derive)
    let xml_without = r#"<Element/>"#;
    let elem2: Element = xml::from_str(xml_without).unwrap();
    assert_eq!(elem2.style.properties, "");
}

/// Test 2: Proxy field where type has #[facet(traits(Default))] - attribute missing
/// This tests that the type's Default characteristic is detected via traits attribute
#[test]
fn test_proxy_missing_attr_type_traits_default_with_derive() {
    use facet_core::Characteristic;

    // Verify that StyleWithDeriveDefault has the Default characteristic
    // (because it has #[facet(traits(Default))])
    let shape = <StyleWithDeriveDefault as facet::Facet>::SHAPE;
    log::info!(
        "StyleWithDeriveDefault shape has Default characteristic: {}",
        shape.is(Characteristic::Default)
    );
    assert!(
        shape.is(Characteristic::Default),
        "StyleWithDeriveDefault should have Default characteristic via #[facet(traits(Default))]"
    );

    #[derive(Facet, Debug, Clone, PartialEq)]
    pub struct Element {
        // NO #[facet(default)] on field, but type has Default via traits
        #[facet(xml::attribute, proxy = StyleWithDeriveDefaultProxy)]
        pub style: StyleWithDeriveDefault,
    }

    // Attribute present
    let xml_with = r#"<Element style="font-size:12px"/>"#;
    let elem: Element = xml::from_str(xml_with).unwrap();
    assert_eq!(elem.style.properties, "font-size:12px");

    // Attribute missing - should detect type's Default and use it
    let xml_without = r#"<Element/>"#;
    let elem2: Element = xml::from_str(xml_without).unwrap();
    assert_eq!(elem2.style.properties, "");
}

/// Test 3: Proxy field where type has #[facet(traits(Default))] - attribute missing
#[test]
fn test_proxy_missing_attr_type_traits_default() {
    #[derive(Facet, Debug, Clone, PartialEq)]
    pub struct Element {
        // NO #[facet(default)] on field, but type has traits(Default)
        #[facet(xml::attribute, proxy = StyleWithTraitsDefaultProxy)]
        pub style: StyleWithTraitsDefault,
    }

    // Attribute present
    let xml_with = r#"<Element style="custom-value"/>"#;
    let elem: Element = xml::from_str(xml_with).unwrap();
    assert_eq!(elem.style.properties, "custom-value");

    // Attribute missing - should use type's Default impl
    let xml_without = r#"<Element/>"#;
    let elem2: Element = xml::from_str(xml_without).unwrap();
    assert_eq!(elem2.style.properties, "default-from-traits");
}

/// Test 4: Container-level #[facet(default)] with proxy field - attribute missing
///
/// Note: When a field's type has #[facet(traits(Default))], the field will use
/// the type's Default::default(), NOT the container's Default impl.
/// Container-level #[facet(default)] only affects fields whose types don't have Default.
#[test]
fn test_proxy_missing_attr_container_default() {
    #[derive(Facet, Debug, Clone, PartialEq)]
    #[facet(default)]
    pub struct Element {
        #[facet(xml::attribute, proxy = StyleWithDeriveDefaultProxy)]
        pub style: StyleWithDeriveDefault,
        #[facet(xml::attribute)]
        pub other: String,
    }

    impl Default for Element {
        fn default() -> Self {
            Element {
                style: StyleWithDeriveDefault {
                    properties: "container-default-style".to_string(),
                },
                other: "container-default-other".to_string(),
            }
        }
    }

    // All attributes present
    let xml_with = r#"<Element style="explicit" other="also-explicit"/>"#;
    let elem: Element = xml::from_str(xml_with).unwrap();
    assert_eq!(elem.style.properties, "explicit");
    assert_eq!(elem.other, "also-explicit");

    // style missing - uses StyleWithDeriveDefault::default() (empty string),
    // NOT the container's Default (which had "container-default-style")
    let xml_partial = r#"<Element other="only-other"/>"#;
    let elem2: Element = xml::from_str(xml_partial).unwrap();
    assert_eq!(elem2.style.properties, ""); // Type's Default, not container's
    assert_eq!(elem2.other, "only-other");

    // All missing - all fields use their type's Default::default()
    // Container's Default impl is NOT used for individual fields
    let xml_empty = r#"<Element/>"#;
    let elem3: Element = xml::from_str(xml_empty).unwrap();
    assert_eq!(elem3.style.properties, ""); // StyleWithDeriveDefault::default()
    assert_eq!(elem3.other, ""); // String::default()
}

/// Test 5: Container with #[derive(Default)] AND #[facet(traits(Default))]
/// Note: #[derive(Default)] alone is not enough - must also have traits(Default)
#[test]
fn test_proxy_missing_attr_container_derive_default() {
    #[derive(Facet, Debug, Clone, PartialEq, Default)]
    #[facet(traits(Default))]
    pub struct Element {
        #[facet(xml::attribute, proxy = StyleWithDeriveDefaultProxy)]
        pub style: StyleWithDeriveDefault,
        #[facet(xml::attribute)]
        pub name: String,
    }

    // All present
    let xml_with = r#"<Element style="present" name="test"/>"#;
    let elem: Element = xml::from_str(xml_with).unwrap();
    assert_eq!(elem.style.properties, "present");
    assert_eq!(elem.name, "test");

    // style missing
    let xml_partial = r#"<Element name="only-name"/>"#;
    let elem2: Element = xml::from_str(xml_partial).unwrap();
    assert_eq!(elem2.style.properties, "");
    assert_eq!(elem2.name, "only-name");

    // All missing
    let xml_empty = r#"<Element/>"#;
    let elem3: Element = xml::from_str(xml_empty).unwrap();
    assert_eq!(elem3.style.properties, "");
    assert_eq!(elem3.name, "");
}

/// Test 6: Multiple proxy fields with different default sources
#[test]
fn test_proxy_multiple_fields_different_defaults() {
    #[derive(Facet, Debug, Clone, PartialEq)]
    pub struct MultiElement {
        // Field with explicit #[facet(default)] attribute
        #[facet(default, xml::attribute, proxy = StyleWithDeriveDefaultProxy)]
        pub style1: StyleWithDeriveDefault,

        // Field relying on type's #[facet(traits(Default))]
        #[facet(xml::attribute, proxy = StyleWithDeriveDefaultProxy)]
        pub style2: StyleWithDeriveDefault,

        // Field with type that has #[facet(traits(Default))]
        #[facet(xml::attribute, proxy = StyleWithTraitsDefaultProxy)]
        pub style3: StyleWithTraitsDefault,
    }

    // All present
    let xml_all = r#"<MultiElement style1="a" style2="b" style3="c"/>"#;
    let elem: MultiElement = xml::from_str(xml_all).unwrap();
    assert_eq!(elem.style1.properties, "a");
    assert_eq!(elem.style2.properties, "b");
    assert_eq!(elem.style3.properties, "c");

    // All missing - each should use its respective default
    let xml_none = r#"<MultiElement/>"#;
    let elem2: MultiElement = xml::from_str(xml_none).unwrap();
    assert_eq!(elem2.style1.properties, ""); // from derive Default
    assert_eq!(elem2.style2.properties, ""); // from type's derive Default
    assert_eq!(elem2.style3.properties, "default-from-traits"); // from traits Default

    // Partial - only style2 present
    let xml_partial = r#"<MultiElement style2="only-two"/>"#;
    let elem3: MultiElement = xml::from_str(xml_partial).unwrap();
    assert_eq!(elem3.style1.properties, "");
    assert_eq!(elem3.style2.properties, "only-two");
    assert_eq!(elem3.style3.properties, "default-from-traits");
}

// Convert proxy -> Option<StyleWithDeriveDefault> (for test 7)
impl From<StyleWithDeriveDefaultProxy> for Option<StyleWithDeriveDefault> {
    fn from(proxy: StyleWithDeriveDefaultProxy) -> Self {
        if proxy.0.is_empty() {
            None
        } else {
            Some(StyleWithDeriveDefault {
                properties: proxy.0,
            })
        }
    }
}

impl From<&Option<StyleWithDeriveDefault>> for StyleWithDeriveDefaultProxy {
    fn from(v: &Option<StyleWithDeriveDefault>) -> Self {
        StyleWithDeriveDefaultProxy(v.as_ref().map(|s| s.properties.clone()).unwrap_or_default())
    }
}

/// Test 7: Proxy field with Option wrapper and missing attribute
#[test]
fn test_proxy_option_missing_attr() {
    #[derive(Facet, Debug, Clone, PartialEq)]
    pub struct Element {
        #[facet(default, xml::attribute, proxy = StyleWithDeriveDefaultProxy)]
        pub style: Option<StyleWithDeriveDefault>,
    }

    // Present with value
    let xml_with = r#"<Element style="has-value"/>"#;
    let elem: Element = xml::from_str(xml_with).unwrap();
    assert_eq!(
        elem.style,
        Some(StyleWithDeriveDefault {
            properties: "has-value".to_string()
        })
    );

    // Present but empty
    let xml_empty_val = r#"<Element style=""/>"#;
    let elem2: Element = xml::from_str(xml_empty_val).unwrap();
    assert_eq!(elem2.style, None);

    // Attribute missing entirely
    let xml_missing = r#"<Element/>"#;
    let elem3: Element = xml::from_str(xml_missing).unwrap();
    assert_eq!(elem3.style, None);
}

/// Test 8: Nested struct with proxy field missing attribute
#[test]
fn test_proxy_nested_struct_missing_attr() {
    #[derive(Facet, Debug, Clone, PartialEq)]
    pub struct Inner {
        #[facet(xml::attribute, proxy = StyleWithDeriveDefaultProxy)]
        pub style: StyleWithDeriveDefault,
    }

    #[derive(Facet, Debug, Clone, PartialEq)]
    pub struct Outer {
        #[facet(xml::element)]
        pub inner: Inner,
    }

    // Inner has style
    let xml_with = r#"<Outer><inner style="nested-style"/></Outer>"#;
    let elem: Outer = xml::from_str(xml_with).unwrap();
    assert_eq!(elem.inner.style.properties, "nested-style");

    // Inner missing style attribute
    let xml_without = r#"<Outer><inner/></Outer>"#;
    let elem2: Outer = xml::from_str(xml_without).unwrap();
    assert_eq!(elem2.inner.style.properties, "");
}

/// Test 9: Enum variant with proxy field missing attribute
#[test]
fn test_proxy_enum_variant_missing_attr() {
    #[derive(Facet, Debug, Clone, PartialEq)]
    pub struct StyledVariant {
        #[facet(xml::attribute, proxy = StyleWithDeriveDefaultProxy)]
        pub style: StyleWithDeriveDefault,
    }

    #[derive(Facet, Debug, Clone, PartialEq)]
    #[repr(u8)]
    pub enum Node {
        #[facet(rename = "styled")]
        Styled(StyledVariant),
        #[facet(rename = "plain")]
        Plain,
    }

    #[derive(Facet, Debug, Clone, PartialEq)]
    pub struct Container {
        #[facet(xml::elements)]
        pub nodes: Vec<Node>,
    }

    // Styled with attribute
    let xml_with = r#"<Container><styled style="enum-style"/></Container>"#;
    let elem: Container = xml::from_str(xml_with).unwrap();
    assert_eq!(elem.nodes.len(), 1);
    if let Node::Styled(s) = &elem.nodes[0] {
        assert_eq!(s.style.properties, "enum-style");
    } else {
        panic!("Expected Styled variant");
    }

    // Styled without attribute
    let xml_without = r#"<Container><styled/></Container>"#;
    let elem2: Container = xml::from_str(xml_without).unwrap();
    assert_eq!(elem2.nodes.len(), 1);
    if let Node::Styled(s) = &elem2.nodes[0] {
        assert_eq!(s.style.properties, "");
    } else {
        panic!("Expected Styled variant");
    }
}

/// Test proxy on non-Option field (validation use case)
#[test]
fn test_proxy_for_validation() {
    #[derive(Facet, Debug, Clone, PartialEq)]
    pub struct ValidatedEmail {
        pub address: String,
    }

    #[derive(Facet, Clone, Debug)]
    #[facet(transparent)]
    pub struct EmailProxy(pub String);

    impl TryFrom<EmailProxy> for ValidatedEmail {
        type Error = String;
        fn try_from(proxy: EmailProxy) -> Result<Self, Self::Error> {
            if proxy.0.contains('@') {
                Ok(ValidatedEmail { address: proxy.0 })
            } else {
                Err("Invalid email: must contain @".to_string())
            }
        }
    }

    impl From<&ValidatedEmail> for EmailProxy {
        fn from(v: &ValidatedEmail) -> Self {
            EmailProxy(v.address.clone())
        }
    }

    #[derive(Facet, Debug, Clone, PartialEq)]
    pub struct User {
        #[facet(xml::attribute, proxy = EmailProxy)]
        pub email: ValidatedEmail,
    }

    // Valid email
    let xml_input = r#"<User email="test@example.com"/>"#;
    let user: User = xml::from_str(xml_input).unwrap();
    assert_eq!(user.email.address, "test@example.com");

    // Invalid email should fail
    let xml2 = r#"<User email="invalid"/>"#;
    let result: Result<User, _> = xml::from_str(xml2);
    assert!(result.is_err());
}
