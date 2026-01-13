//! Tests for DOM-based XML deserialization.

use facet::Facet;
use facet_xml::{self as xml, from_str};

#[test]
fn test_simple_struct() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(rename = "person")]
    struct Person {
        #[facet(xml::attribute)]
        id: String,
        #[facet(xml::attribute)]
        name: String,
    }

    let xml = r#"<person id="1" name="Alice"/>"#;
    let person: Person = from_str(xml).unwrap();
    assert_eq!(person.id, "1");
    assert_eq!(person.name, "Alice");
}

#[test]
fn test_struct_with_text() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(rename = "message")]
    struct Message {
        #[facet(xml::attribute)]
        from: String,
        #[facet(xml::text)]
        content: String,
    }

    let xml = r#"<message from="Bob">Hello, world!</message>"#;
    let msg: Message = from_str(xml).unwrap();
    assert_eq!(msg.from, "Bob");
    assert_eq!(msg.content, "Hello, world!");
}

#[test]
fn test_struct_with_child_elements() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(rename = "item")]
    struct Item {
        #[facet(xml::attribute)]
        id: String,
    }

    #[derive(Facet, Debug, PartialEq)]
    #[facet(rename = "container")]
    struct Container {
        #[facet(xml::elements)]
        items: Vec<Item>,
    }

    let xml = r#"<container><item id="1"/><item id="2"/><item id="3"/></container>"#;
    let container: Container = from_str(xml).unwrap();
    assert_eq!(container.items.len(), 3);
    assert_eq!(container.items[0].id, "1");
    assert_eq!(container.items[1].id, "2");
    assert_eq!(container.items[2].id, "3");
}

#[test]
fn test_empty_element() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(rename = "empty")]
    struct Empty {
        #[facet(xml::attribute)]
        flag: Option<String>,
    }

    let xml = r#"<empty/>"#;
    let empty: Empty = from_str(xml).unwrap();
    assert_eq!(empty.flag, None);

    let xml_with_attr = r#"<empty flag="yes"/>"#;
    let empty: Empty = from_str(xml_with_attr).unwrap();
    assert_eq!(empty.flag, Some("yes".to_string()));
}
