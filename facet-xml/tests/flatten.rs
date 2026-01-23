//! Tests for flatten behavior in facet-xml.

use std::collections::HashMap;

use facet::Facet;
use facet_testhelpers::test;
use facet_xml as xml;

// ============================================================================
// flatten - flattened structs
// ============================================================================

#[test]
fn flatten_struct_fields_appear_as_siblings() {
    #[derive(Facet, Debug, PartialEq)]
    struct Address {
        city: String,
        country: String,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Person {
        name: String,
        #[facet(flatten)]
        address: Address,
    }

    // Address fields appear directly under <person>, not nested in <address>
    let result: Person = facet_xml::from_str(
        "<person><name>Alice</name><city>Paris</city><country>France</country></person>",
    )
    .unwrap();
    assert_eq!(result.name, "Alice");
    assert_eq!(result.address.city, "Paris");
    assert_eq!(result.address.country, "France");
}

#[test]
fn flatten_struct_with_attributes() {
    #[derive(Facet, Debug, PartialEq)]
    struct CommonAttrs {
        #[facet(xml::attribute)]
        id: String,
        #[facet(xml::attribute)]
        class: Option<String>,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Element {
        #[facet(flatten)]
        attrs: CommonAttrs,
        #[facet(xml::text)]
        content: String,
    }

    let result: Element =
        facet_xml::from_str(r#"<element id="123" class="foo">hello</element>"#).unwrap();
    assert_eq!(result.attrs.id, "123");
    assert_eq!(result.attrs.class, Some("foo".to_string()));
    assert_eq!(result.content, "hello");
}

// ============================================================================
// flatten with HashMap - capture unknown attributes
// ============================================================================

#[test]
fn flatten_hashmap_captures_unknown_attributes() {
    #[derive(Facet, Debug, PartialEq)]
    struct Element {
        #[facet(xml::attribute)]
        id: String,
        #[facet(flatten, default)]
        extra: HashMap<String, String>,
    }

    let result: Element =
        facet_xml::from_str(r#"<element id="123" data-foo="bar" aria-label="test"/>"#).unwrap();
    assert_eq!(result.id, "123");
    assert_eq!(result.extra.get("data-foo"), Some(&"bar".to_string()));
    assert_eq!(result.extra.get("aria-label"), Some(&"test".to_string()));
}

#[test]
fn flatten_hashmap_captures_unknown_elements() {
    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        name: String,
        #[facet(flatten, default)]
        settings: HashMap<String, String>,
    }

    let result: Config = facet_xml::from_str(
        "<config><name>app</name><timeout>30</timeout><host>localhost</host></config>",
    )
    .unwrap();
    assert_eq!(result.name, "app");
    assert_eq!(result.settings.get("timeout"), Some(&"30".to_string()));
    assert_eq!(result.settings.get("host"), Some(&"localhost".to_string()));
}

#[test]
fn flatten_hashmap_inside_flattened_struct() {
    #[derive(Facet, Debug, PartialEq)]
    struct CommonAttrs {
        #[facet(xml::attribute)]
        id: Option<String>,
        #[facet(flatten, default)]
        extra: HashMap<String, String>,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Element {
        #[facet(flatten)]
        attrs: CommonAttrs,
        #[facet(xml::text)]
        content: String,
    }

    let result: Element =
        facet_xml::from_str(r#"<element id="123" data-custom="value">hello</element>"#).unwrap();
    assert_eq!(result.attrs.id, Some("123".to_string()));
    assert_eq!(
        result.attrs.extra.get("data-custom"),
        Some(&"value".to_string())
    );
    assert_eq!(result.content, "hello");
}

// ============================================================================
// flatten with Option - optional flattened struct
// ============================================================================

#[test]
fn flatten_option_struct_present() {
    #[derive(Facet, Debug, PartialEq)]
    struct Metadata {
        author: String,
        version: String,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Document {
        title: String,
        #[facet(flatten)]
        metadata: Option<Metadata>,
    }

    let result: Document = facet_xml::from_str(
        "<document><title>Doc</title><author>Alice</author><version>1.0</version></document>",
    )
    .unwrap();
    assert_eq!(result.title, "Doc");
    assert!(result.metadata.is_some());
    let meta = result.metadata.unwrap();
    assert_eq!(meta.author, "Alice");
    assert_eq!(meta.version, "1.0");
}

#[test]
fn flatten_option_struct_absent() {
    #[derive(Facet, Debug, PartialEq, Default)]
    struct Metadata {
        author: String,
        version: String,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Document {
        title: String,
        #[facet(flatten, default)]
        metadata: Option<Metadata>,
    }

    let result: Document = facet_xml::from_str("<document><title>Doc</title></document>").unwrap();
    assert_eq!(result.title, "Doc");
    assert!(result.metadata.is_none());
}

// ============================================================================
// flatten with Vec<Enum> - heterogeneous children
// ============================================================================

#[test]
fn flatten_vec_enum_basic() {
    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    enum Shape {
        Circle { radius: f64 },
        Rect { width: f64, height: f64 },
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Canvas {
        #[facet(flatten)]
        shapes: Vec<Shape>,
    }

    let result: Canvas = facet_xml::from_str(
        "<canvas><circle><radius>5.0</radius></circle><rect><width>10</width><height>20</height></rect></canvas>",
    )
    .unwrap();
    assert_eq!(result.shapes.len(), 2);
    assert_eq!(result.shapes[0], Shape::Circle { radius: 5.0 });
    assert_eq!(
        result.shapes[1],
        Shape::Rect {
            width: 10.0,
            height: 20.0
        }
    );
}

#[test]
fn flatten_vec_enum_with_attributes() {
    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    enum Shape {
        Circle {
            #[facet(xml::attribute)]
            r: f64,
        },
        Rect {
            #[facet(xml::attribute)]
            width: f64,
            #[facet(xml::attribute)]
            height: f64,
        },
        Path {
            #[facet(xml::attribute)]
            d: String,
        },
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Canvas {
        #[facet(flatten)]
        shapes: Vec<Shape>,
    }

    let result: Canvas = facet_xml::from_str(
        r#"<canvas><circle r="5"/><rect width="10" height="20"/><path d="M0 0 L10 10"/></canvas>"#,
    )
    .unwrap();
    assert_eq!(result.shapes.len(), 3);
    assert_eq!(result.shapes[0], Shape::Circle { r: 5.0 });
    assert_eq!(
        result.shapes[1],
        Shape::Rect {
            width: 10.0,
            height: 20.0
        }
    );
    assert_eq!(
        result.shapes[2],
        Shape::Path {
            d: "M0 0 L10 10".to_string()
        }
    );
}

#[test]
fn flatten_vec_enum_interleaved_with_regular_elements() {
    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    enum Shape {
        Circle { radius: f64 },
        Rect { width: f64, height: f64 },
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Canvas {
        name: String,
        #[facet(flatten)]
        shapes: Vec<Shape>,
        description: Option<String>,
    }

    // Regular elements interleaved with enum variants
    let result: Canvas = facet_xml::from_str(
        "<canvas><name>MyCanvas</name><circle><radius>5.0</radius></circle><description>A canvas</description><rect><width>10</width><height>20</height></rect></canvas>",
    )
    .unwrap();
    assert_eq!(result.name, "MyCanvas");
    assert_eq!(result.description, Some("A canvas".to_string()));
    assert_eq!(result.shapes.len(), 2);
    assert_eq!(result.shapes[0], Shape::Circle { radius: 5.0 });
    assert_eq!(
        result.shapes[1],
        Shape::Rect {
            width: 10.0,
            height: 20.0
        }
    );
}

#[test]
fn flatten_vec_enum_empty() {
    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    enum Shape {
        Circle { radius: f64 },
        Rect { width: f64, height: f64 },
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Canvas {
        name: String,
        #[facet(flatten)]
        shapes: Vec<Shape>,
    }

    let result: Canvas = facet_xml::from_str("<canvas><name>Empty</name></canvas>").unwrap();
    assert_eq!(result.name, "Empty");
    assert!(result.shapes.is_empty());
}

#[test]
fn flatten_vec_enum_unit_variants() {
    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    enum Command {
        Start,
        Stop,
        Pause,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Script {
        #[facet(flatten)]
        commands: Vec<Command>,
    }

    let result: Script = facet_xml::from_str("<script><start/><pause/><stop/></script>").unwrap();
    assert_eq!(result.commands.len(), 3);
    assert_eq!(result.commands[0], Command::Start);
    assert_eq!(result.commands[1], Command::Pause);
    assert_eq!(result.commands[2], Command::Stop);
}

#[test]
fn flatten_vec_enum_newtype_variants() {
    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    enum Value {
        Text(String),
        Number(i32),
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Data {
        #[facet(flatten)]
        values: Vec<Value>,
    }

    let result: Data =
        facet_xml::from_str("<data><text>hello</text><number>42</number><text>world</text></data>")
            .unwrap();
    assert_eq!(result.values.len(), 3);
    assert_eq!(result.values[0], Value::Text("hello".to_string()));
    assert_eq!(result.values[1], Value::Number(42));
    assert_eq!(result.values[2], Value::Text("world".to_string()));
}
