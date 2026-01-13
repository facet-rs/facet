//! Tests for out-of-order field handling in tagged enums.
//!
//! These tests verify that the DOM-based deserializer correctly handles
//! cases where the tag field appears after other fields in the XML.

use facet::Facet;

#[derive(Debug, PartialEq, Facet)]
#[facet(tag = "type")]
#[repr(u8)]
pub enum InternallyTagged {
    Circle { radius: f64 },
}

#[derive(Debug, PartialEq, Facet)]
#[facet(tag = "t", content = "c")]
#[repr(u8)]
pub enum AdjacentlyTagged {
    Message(String),
}

#[test]
fn test_internally_tagged_out_of_order() {
    // Tag comes AFTER the other field - this should work now!
    let xml = r#"<shape><radius>5.0</radius><type>Circle</type></shape>"#;
    let result: InternallyTagged = facet_xml::from_str(xml).expect("should deserialize");
    assert_eq!(result, InternallyTagged::Circle { radius: 5.0 });
}

#[test]
fn test_adjacently_tagged_out_of_order() {
    // Content comes BEFORE the tag - this should work now!
    let xml = r#"<value><c>hello</c><t>Message</t></value>"#;
    let result: AdjacentlyTagged = facet_xml::from_str(xml).expect("should deserialize");
    assert_eq!(result, AdjacentlyTagged::Message("hello".into()));
}
