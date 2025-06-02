//! Tests for Vec serialization and deserialization using TOML array of tables syntax.
//!
//! TOML has two ways to represent arrays:
//! - Inline arrays: `[1, 2, 3]` or `[{a = 1}, {a = 2}]`
//! - Array of tables: `[[array]]` followed by table contents
//!
//! This module tests that Vec<Struct> uses the array of tables syntax.

use facet::Facet;

#[derive(Debug, PartialEq, Facet)]
struct NestedUnit;

#[derive(Debug, PartialEq, Facet)]
struct Root {
    value: i32,
    unit: Vec<NestedUnit>,
}

#[derive(Debug, PartialEq, Facet)]
struct Nested {
    field1: String,
    field2: i32,
}

#[derive(Debug, PartialEq, Facet)]
struct RootWithNested {
    nested: Vec<Nested>,
}

#[test]
#[should_panic(expected = "Expected field with name 'unit'")]
fn test_nested_unit_struct_vec() {
    // Unit structs have no data to serialize, so a Vec<UnitStruct>
    // cannot be represented in TOML. This is expected behavior.
    // The 'unit' field will be missing from the serialized output.
    let root = Root {
        value: 42,
        unit: vec![NestedUnit, NestedUnit],
    };

    let serialized = facet_toml::to_string(&root).unwrap();
    println!("Serialized nested unit struct vec:\n{}", serialized);

    // This will panic because the 'unit' field is not present in the serialized TOML
    let _deserialized: Root = facet_toml::from_str(&serialized).unwrap();
}

#[test]
fn test_nested_struct_multiple_fields_vec() {
    let root = RootWithNested {
        nested: vec![
            Nested {
                field1: "first".to_string(),
                field2: 1,
            },
            Nested {
                field1: "second".to_string(),
                field2: 2,
            },
        ],
    };

    let serialized = facet_toml::to_string(&root).unwrap();
    println!("Serialized nested struct vec:\n{}", serialized);

    // Test if it properly uses [[nested]] syntax
    assert!(
        serialized.contains("[[nested]]"),
        "Expected [[nested]] syntax for Vec serialization"
    );

    let deserialized: RootWithNested = facet_toml::from_str(&serialized).unwrap();
    assert_eq!(root, deserialized);
}

#[test]
fn test_deserialize_array_of_tables() {
    // Test deserializing TOML with array of tables syntax
    let toml = r#"
[[nested]]
field1 = "first"
field2 = 1

[[nested]]
field1 = "second"
field2 = 2
"#;

    let deserialized: RootWithNested = facet_toml::from_str(toml).unwrap();
    assert_eq!(deserialized.nested.len(), 2);
    assert_eq!(deserialized.nested[0].field1, "first");
    assert_eq!(deserialized.nested[0].field2, 1);
    assert_eq!(deserialized.nested[1].field1, "second");
    assert_eq!(deserialized.nested[1].field2, 2);
}
