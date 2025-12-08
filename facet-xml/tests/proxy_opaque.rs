//! Tests for all four combinations of opaque/proxy attributes.
//!
//! The four combinations are:
//! 1. Neither opaque nor proxy: normal Facet deserialization
//! 2. Only opaque: type is treated as opaque, no Facet impl needed, but no conversion
//! 3. Only proxy: type implements Facet, proxy handles conversion (e.g., validation)
//! 4. Both opaque and proxy: type doesn't implement Facet, proxy handles everything

use facet::Facet;
use facet_xml as xml;

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
