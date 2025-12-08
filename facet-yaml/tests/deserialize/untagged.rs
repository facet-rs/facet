//! Tests for untagged enum deserialization.

use facet::Facet;
use facet_testhelpers::test;

// Test case from the original issue: tuple variants with sequences
// Note: Tuple variants for untagged enums require sequence format in YAML
// This differs from struct variants which use mapping format
#[test]
fn test_untagged_tuple_variants_issue_1183() {
    // The original issue used tuple variants, but untagged enum deserialization
    // with tuple variants that are sequences is not yet implemented.
    // This test documents the current limitation.
    // For now, users should use struct variants for untagged enums.

    // Struct-based equivalent that works:
    #[derive(Debug, Facet, PartialEq, Clone)]
    #[facet(untagged)]
    #[repr(C)]
    #[allow(dead_code)]
    pub enum CntStruct {
        Unit { value: String },
        Weight { value: String, weight: f64 },
    }

    // Unit variant
    let unit: CntStruct = facet_yaml::from_str("value: sugar").unwrap();
    assert_eq!(
        unit,
        CntStruct::Unit {
            value: "sugar".to_string()
        }
    );

    // Weight variant
    let weight: CntStruct = facet_yaml::from_str("value: flour\nweight: 2.5").unwrap();
    assert_eq!(
        weight,
        CntStruct::Weight {
            value: "flour".to_string(),
            weight: 2.5
        }
    );
}

#[test]
fn test_untagged_struct_variants() {
    #[derive(Debug, Facet, PartialEq)]
    #[repr(C)]
    #[facet(untagged)]
    #[allow(dead_code)]
    enum Shape {
        Circle { radius: f64 },
        Rectangle { width: f64, height: f64 },
    }

    // Test Circle variant
    let circle: Shape = facet_yaml::from_str("radius: 5.0").unwrap();
    assert_eq!(circle, Shape::Circle { radius: 5.0 });

    // Test Rectangle variant
    let rect: Shape = facet_yaml::from_str("width: 10.0\nheight: 20.0").unwrap();
    assert_eq!(
        rect,
        Shape::Rectangle {
            width: 10.0,
            height: 20.0
        }
    );
}

#[test]
fn test_untagged_discriminating_field() {
    // Test case from the issue: distinguishing variants by field names
    #[derive(Debug, Facet, PartialEq, Clone)]
    #[facet(untagged)]
    #[repr(C)]
    #[allow(dead_code)]
    pub enum Cnt {
        Unit { name: String },
        Weight { name: String, weight: f64 },
    }

    // Unit variant (has only "name")
    let unit: Cnt = facet_yaml::from_str("name: sugar").unwrap();
    assert_eq!(
        unit,
        Cnt::Unit {
            name: "sugar".to_string()
        }
    );

    // Weight variant (has "name" and "weight")
    let weight: Cnt = facet_yaml::from_str("name: flour\nweight: 2.5").unwrap();
    assert_eq!(
        weight,
        Cnt::Weight {
            name: "flour".to_string(),
            weight: 2.5
        }
    );
}

#[test]
fn test_untagged_in_struct() {
    #[derive(Debug, Facet, PartialEq)]
    #[repr(C)]
    #[facet(untagged)]
    #[allow(dead_code)]
    enum Value {
        Text { text: String },
        Number { num: i32 },
    }

    #[derive(Debug, Facet, PartialEq)]
    struct Container {
        name: String,
        value: Value,
    }

    let yaml = r#"
name: test
value:
  text: hello
"#;
    let container: Container = facet_yaml::from_str(yaml).unwrap();
    assert_eq!(container.name, "test");
    assert_eq!(
        container.value,
        Value::Text {
            text: "hello".to_string()
        }
    );

    let yaml2 = r#"
name: test2
value:
  num: 42
"#;
    let container2: Container = facet_yaml::from_str(yaml2).unwrap();
    assert_eq!(container2.name, "test2");
    assert_eq!(container2.value, Value::Number { num: 42 });
}

#[test]
fn test_untagged_list_of_variants() {
    #[derive(Debug, Facet, PartialEq)]
    #[repr(C)]
    #[facet(untagged)]
    #[allow(dead_code)]
    enum Item {
        Simple { id: u32 },
        Complex { id: u32, data: String },
    }

    let yaml = r#"
- id: 1
- id: 2
  data: extra
- id: 3
"#;
    let items: Vec<Item> = facet_yaml::from_str(yaml).unwrap();
    assert_eq!(items.len(), 3);
    assert_eq!(items[0], Item::Simple { id: 1 });
    assert_eq!(
        items[1],
        Item::Complex {
            id: 2,
            data: "extra".to_string()
        }
    );
    assert_eq!(items[2], Item::Simple { id: 3 });
}
