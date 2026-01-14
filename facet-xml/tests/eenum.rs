//! Tests for enum handling in facet-xml.
//!
//! In XML, the element name determines which variant is selected.

use facet::Facet;
use facet_testhelpers::test;

// ============================================================================
// Basic enum variants
// ============================================================================

#[test]
fn struct_variant() {
    #[derive(Debug, PartialEq, Facet)]
    #[repr(u8)]
    enum Shape {
        Circle { radius: f64 },
    }

    let result: Shape = facet_xml::from_str("<circle><radius>5.0</radius></circle>").unwrap();
    assert_eq!(result, Shape::Circle { radius: 5.0 });
}

#[test]
fn newtype_variant() {
    #[derive(Debug, PartialEq, Facet)]
    #[repr(u8)]
    enum Message {
        Text(String),
    }

    let result: Message = facet_xml::from_str("<text>hello</text>").unwrap();
    assert_eq!(result, Message::Text("hello".into()));
}

#[test]
fn unit_variant() {
    #[derive(Debug, PartialEq, Facet)]
    #[repr(u8)]
    enum Status {
        Active,
        Inactive,
    }

    let result: Status = facet_xml::from_str("<active/>").unwrap();
    assert_eq!(result, Status::Active);

    let result: Status = facet_xml::from_str("<inactive/>").unwrap();
    assert_eq!(result, Status::Inactive);
}

// ============================================================================
// Enum with multiple variants
// ============================================================================

#[test]
fn multiple_struct_variants() {
    #[derive(Debug, PartialEq, Facet)]
    #[repr(u8)]
    enum Shape {
        Circle { radius: f64 },
        Rect { width: f64, height: f64 },
    }

    let result: Shape = facet_xml::from_str("<circle><radius>5.0</radius></circle>").unwrap();
    assert_eq!(result, Shape::Circle { radius: 5.0 });

    let result: Shape =
        facet_xml::from_str("<rect><width>10.0</width><height>20.0</height></rect>").unwrap();
    assert_eq!(
        result,
        Shape::Rect {
            width: 10.0,
            height: 20.0
        }
    );
}

// ============================================================================
// Enum with rename
// ============================================================================

#[test]
fn variant_with_rename() {
    #[derive(Debug, PartialEq, Facet)]
    #[repr(u8)]
    enum Event {
        #[facet(rename = "mouse-click")]
        MouseClick { x: i32, y: i32 },
    }

    let result: Event =
        facet_xml::from_str("<mouse-click><x>100</x><y>200</y></mouse-click>").unwrap();
    assert_eq!(result, Event::MouseClick { x: 100, y: 200 });
}

// ============================================================================
// Vec of enums
// ============================================================================

#[test]
fn vec_of_enum_variants() {
    #[derive(Debug, PartialEq, Facet)]
    #[repr(u8)]
    enum Shape {
        Circle { radius: f64 },
        Rect { width: f64, height: f64 },
    }

    #[derive(Debug, PartialEq, Facet)]
    struct Drawing {
        #[facet(flatten, default)]
        shapes: Vec<Shape>,
    }

    let result: Drawing = facet_xml::from_str(
        "<drawing><circle><radius>5.0</radius></circle><rect><width>10.0</width><height>20.0</height></rect></drawing>",
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
