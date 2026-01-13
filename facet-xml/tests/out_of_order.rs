//! Tests for out-of-order field handling in structs.
//!
//! In XML, element order doesn't matter - fields can appear in any order.
//! Note: In XML, internally-tagged and adjacently-tagged enums use the same
//! representation as externally-tagged: the element name IS the variant.

use facet::Facet;
use test_log::test;

#[derive(Debug, PartialEq, Facet)]
#[repr(u8)]
pub enum Shape {
    Circle { radius: f64 },
}

#[derive(Debug, PartialEq, Facet)]
#[repr(u8)]
pub enum Message {
    Text(String),
}

#[test]
fn test_struct_variant_out_of_order() {
    // In XML, the element name is the variant discriminant
    // Fields within the variant can appear in any order
    let xml = r#"<Circle><radius>5.0</radius></Circle>"#;
    let result: Shape = facet_xml::from_str(xml).expect("should deserialize");
    assert_eq!(result, Shape::Circle { radius: 5.0 });
}

#[test]
fn test_newtype_variant() {
    // Newtype variants: element name is variant, content is the inner value
    let xml = r#"<Text>hello</Text>"#;
    let result: Message = facet_xml::from_str(xml).expect("should deserialize");
    assert_eq!(result, Message::Text("hello".into()));
}
