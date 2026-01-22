//! Tests for enum handling in facet-xml.
//!
//! In XML, the element name determines which variant is selected.

use facet::Facet;
use facet_testhelpers::test;
use facet_xml as xml;

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

// ============================================================================
// Enum as attribute value (issue #1830)
// ============================================================================

#[test]
fn enum_as_attribute_value() {
    // Reproduces issue #1830: parsing enums as XML attribute values
    // was allocating wrong shape (String instead of the enum type)

    #[derive(Debug, Clone, Copy, PartialEq, Facet)]
    #[repr(C)]
    enum Name {
        #[facet(rename = "voltage")]
        Voltage,
        #[facet(rename = "value")]
        Value,
        #[facet(rename = "adValue")]
        AdValue,
    }

    #[derive(Debug, Clone, PartialEq, Facet)]
    #[facet(rename = "Property")]
    struct XmlScaleRangeProperty {
        #[facet(xml::attribute)]
        value: f32,
        #[facet(xml::attribute)]
        name: Name,
    }

    let property: XmlScaleRangeProperty =
        facet_xml::from_str(r#"<Property value="5" name="voltage" />"#).unwrap();
    assert_eq!(property.value, 5.0);
    assert!(matches!(property.name, Name::Voltage));

    let property2: XmlScaleRangeProperty =
        facet_xml::from_str(r#"<Property value="10" name="adValue" />"#).unwrap();
    assert_eq!(property2.value, 10.0);
    assert!(matches!(property2.name, Name::AdValue));
}

#[test]
fn enum_as_attribute_value_with_option() {
    // Test that Option<Enum> works as attribute value too

    #[derive(Debug, Clone, Copy, PartialEq, Facet)]
    #[repr(C)]
    enum Priority {
        #[facet(rename = "low")]
        Low,
        #[facet(rename = "medium")]
        Medium,
        #[facet(rename = "high")]
        High,
    }

    #[derive(Debug, Clone, PartialEq, Facet)]
    #[facet(rename = "Task")]
    struct Task {
        #[facet(xml::attribute)]
        name: String,
        #[facet(xml::attribute)]
        priority: Option<Priority>,
    }

    let task: Task = facet_xml::from_str(r#"<Task name="test" priority="high" />"#).unwrap();
    assert_eq!(task.name, "test");
    assert_eq!(task.priority, Some(Priority::High));

    // Without the optional attribute
    let task2: Task = facet_xml::from_str(r#"<Task name="test2" />"#).unwrap();
    assert_eq!(task2.name, "test2");
    assert_eq!(task2.priority, None);
}

// ============================================================================
// Enum variant fields with xml::attribute (issue #1855)
// ============================================================================

#[test]
fn enum_variant_fields_with_attributes() {
    // Reproduces issue #1855: xml::attribute on enum variant fields
    // was being ignored during serialization

    #[derive(Debug, PartialEq, Facet)]
    #[repr(C)]
    enum Foo {
        #[facet(rename = "Foo")]
        Variant {
            #[facet(xml::attribute)]
            name: String,
            #[facet(xml::attribute)]
            value: String,
        },
    }

    // Test deserialization
    let x: Foo = facet_xml::from_str(r#"<Foo name="bar" value="baz" />"#).unwrap();
    assert_eq!(
        x,
        Foo::Variant {
            name: "bar".to_string(),
            value: "baz".to_string()
        }
    );

    // Test serialization - should produce attributes, not child elements
    let y = facet_xml::to_string_pretty(&x).unwrap();

    // Should serialize with attributes, not child elements
    assert!(y.contains(r#"name="bar""#), "name should be an attribute");
    assert!(y.contains(r#"value="baz""#), "value should be an attribute");
    assert!(!y.contains("<name>"), "name should not be a child element");
    assert!(
        !y.contains("<value>"),
        "value should not be a child element"
    );
}

#[test]
fn enum_variant_mixed_attributes_and_elements() {
    // Test enum variants with both attributes and child elements

    #[derive(Debug, PartialEq, Facet)]
    #[non_exhaustive]
    #[repr(C)]
    enum XmlParameter {
        #[facet(rename = "Property")]
        Property {
            #[facet(xml::attribute)]
            name: String,
            #[facet(xml::attribute)]
            value: String,
        },
        #[facet(rename = "Array")]
        Array {
            #[facet(xml::attribute)]
            name: String,
            #[facet(flatten)]
            value: Vec<XmlParameter>,
        },
        #[facet(rename = "Group")]
        Group {
            #[facet(xml::attribute)]
            name: String,
            #[facet(flatten)]
            value: Vec<XmlParameter>,
        },
    }

    // Test deserialization
    let p: XmlParameter = facet_xml::from_str(
        r#"
        <Array name="State_Text">
            <Property name="State_Text" value="A" />
            <Property name="State_Text" value="B Text" />
            <Property name="State_Text" value="C text" />
            <Group name="Voltage_Range">
                <Property name="Voltage_Range" value="foo" />
            </Group>
        </Array>
        "#,
    )
    .unwrap();

    // Test serialization roundtrip
    let serialized = facet_xml::to_string_pretty(&p).unwrap();

    // Should have attributes on all variants
    assert!(
        serialized.contains(r#"<Array name="State_Text">"#),
        "Array should have name attribute"
    );
    assert!(
        serialized.contains(r#"<Property name="State_Text" value="A""#),
        "Property should have attributes"
    );
    assert!(
        serialized.contains(r#"<Group name="Voltage_Range">"#),
        "Group should have name attribute"
    );

    // Verify roundtrip works
    let roundtrip: XmlParameter = facet_xml::from_str(&serialized).unwrap();

    // Compare by serializing both and checking they produce the same result
    let original_serialized = facet_xml::to_string_pretty(&p).unwrap();
    let roundtrip_serialized = facet_xml::to_string_pretty(&roundtrip).unwrap();
    assert_eq!(
        original_serialized, roundtrip_serialized,
        "Roundtrip should produce identical XML"
    );
}
