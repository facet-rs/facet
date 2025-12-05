use facet::Facet;
use facet_xml as xml;

#[derive(Facet, Debug, PartialEq)]
struct Test1 {
    #[facet(xml::attribute)]
    required: String,
}

#[derive(Facet, Debug, PartialEq)]
struct Test2 {
    #[facet(xml::attribute)]
    required: String,
    #[facet(default, xml::attribute)]
    optional: Option<String>,
}

#[test]
fn test_basic_required() {
    let xml = r#"<Test1 required="hello"/>"#;
    let result: Test1 = xml::from_str(xml).unwrap();
    assert_eq!(result.required, "hello");
}

#[test]
fn test_optional_present() {
    let xml = r#"<Test2 required="hello" optional="world"/>"#;
    let result: Test2 = xml::from_str(xml).unwrap();
    assert_eq!(result.required, "hello");
    assert_eq!(result.optional, Some("world".to_string()));
}

#[test]
fn test_optional_absent() {
    let xml = r#"<Test2 required="hello"/>"#;
    let result: Test2 = xml::from_str(xml).unwrap();
    assert_eq!(result.required, "hello");
    assert_eq!(result.optional, None);
}

/// Test for issue #1075: UB/SIGABRT when using opaque + proxy on Option<T> fields
/// This simpler case triggers SIGSEGV when default is used
#[test]
fn test_opaque_proxy_option_attribute_simple() {
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
    impl TryFrom<&Option<PathData>> for PathDataProxy {
        type Error = std::convert::Infallible;
        fn try_from(v: &Option<PathData>) -> Result<Self, Self::Error> {
            Ok(PathDataProxy(
                v.as_ref().map(|d| d.commands.join(",")).unwrap_or_default(),
            ))
        }
    }

    #[derive(Facet, Debug, Clone, Default)]
    pub struct Path {
        #[facet(default, xml::attribute, opaque, proxy = PathDataProxy)]
        pub d: Option<PathData>,
    }

    // Test with absent attribute (uses default) - this triggers SIGSEGV
    let xml = r#"<Path/>"#;
    let path: Path = xml::from_str(xml).unwrap();
    // Should be None, using default value
    // Note: printing path.d causes SIGABRT due to corrupted memory
    assert!(path.d.is_none());
}

/// Test for issue #1075: Full reproduction with enum wrapper and namespaces
/// This matches the original reporter's case more closely
#[test]
fn test_opaque_proxy_option_attribute_nested_enum() {
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
    impl TryFrom<&Option<PathData>> for PathDataProxy {
        type Error = std::convert::Infallible;
        fn try_from(v: &Option<PathData>) -> Result<Self, Self::Error> {
            Ok(PathDataProxy(
                v.as_ref().map(|d| d.commands.join(",")).unwrap_or_default(),
            ))
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
    let xml = r#"<Svg xmlns="http://www.w3.org/2000/svg"><path d="M0,0 L10,10"/></Svg>"#;
    let svg: Svg = xml::from_str(xml).unwrap();
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
    // This should be None - before the fix, it triggered UB
    assert!(path.d.is_none());
}

// ============================================================================
// Pretty-printing tests
// ============================================================================

#[derive(Facet, Debug, PartialEq)]
struct Person {
    #[facet(xml::attribute)]
    id: u32,
    #[facet(xml::element)]
    name: String,
    #[facet(xml::element)]
    age: u32,
}

#[test]
fn test_to_string_compact() {
    let person = Person {
        id: 42,
        name: "Alice".to_string(),
        age: 30,
    };
    let xml_output = xml::to_string(&person).unwrap();
    // Compact output: no newlines or indentation
    assert_eq!(
        xml_output,
        r#"<Person id="42"><name>Alice</name><age>30</age></Person>"#
    );
}

#[test]
fn test_to_string_pretty() {
    let person = Person {
        id: 42,
        name: "Alice".to_string(),
        age: 30,
    };
    let xml_output = xml::to_string_pretty(&person).unwrap();
    // Pretty output: newlines and default indentation (2 spaces)
    let expected = r#"<Person id="42">
  <name>Alice</name>
  <age>30</age>
</Person>"#;
    assert_eq!(xml_output, expected);
}

#[test]
fn test_to_string_with_options_custom_indent() {
    let person = Person {
        id: 42,
        name: "Alice".to_string(),
        age: 30,
    };
    let xml_output =
        xml::to_string_with_options(&person, &xml::SerializeOptions::default().indent("\t"))
            .unwrap();
    // Pretty output with tabs
    let expected = "<Person id=\"42\">\n\t<name>Alice</name>\n\t<age>30</age>\n</Person>";
    assert_eq!(xml_output, expected);
}

#[derive(Facet, Debug, PartialEq)]
struct Nested {
    #[facet(xml::element)]
    person: Person,
}

#[test]
fn test_pretty_nested_elements() {
    let nested = Nested {
        person: Person {
            id: 1,
            name: "Bob".to_string(),
            age: 25,
        },
    };
    let xml_output = xml::to_string_pretty(&nested).unwrap();
    let expected = r#"<Nested>
  <person id="1">
    <name>Bob</name>
    <age>25</age>
  </person>
</Nested>"#;
    assert_eq!(xml_output, expected);
}

#[test]
fn test_pretty_roundtrip() {
    let person = Person {
        id: 42,
        name: "Alice".to_string(),
        age: 30,
    };
    // Pretty-print, then parse back
    let xml_output = xml::to_string_pretty(&person).unwrap();
    let parsed: Person = xml::from_str(&xml_output).unwrap();
    assert_eq!(parsed, person);
}
