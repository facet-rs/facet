//! Tests for deferred processing inside lists.
//!
//! These tests exercise scenarios where frames inside list elements need to be
//! stored for deferred processing. This requires that list element memory remains
//! stable during building (i.e., doesn't move when the list grows).
//!
//! The key insight is that with direct-fill into Vec's buffer, reallocation can
//! invalidate stored frame pointers. A rope-based approach (stable chunks) fixes this.

use std::collections::HashMap;

use facet::Facet;
use facet_testhelpers::test;

// =============================================================================
// Basic flattened internally-tagged enum in Vec
// =============================================================================

#[derive(Clone, Debug, Facet, PartialEq)]
#[facet(tag = "type")]
#[repr(C)]
pub enum TaggedEnum {
    VariantA { value: f64 },
    VariantB { x: f64, y: f64 },
}

#[derive(Clone, Debug, Facet, PartialEq)]
pub struct FlattenedItem {
    #[facet(flatten)]
    pub inner: TaggedEnum,
    pub name: String,
}

/// Basic case: Vec of structs with flattened tagged enum, fields in order
#[test]
fn vec_flattened_tagged_enum_fields_in_order() {
    let json = r#"[
        {"type": "VariantA", "value": 1.0, "name": "first"},
        {"type": "VariantB", "x": 2.0, "y": 3.0, "name": "second"}
    ]"#;

    let items: Vec<FlattenedItem> =
        facet_json::from_str(json).expect("fields in order should work");
    assert_eq!(items.len(), 2);
    assert_eq!(
        items[0],
        FlattenedItem {
            inner: TaggedEnum::VariantA { value: 1.0 },
            name: "first".into()
        }
    );
    assert_eq!(
        items[1],
        FlattenedItem {
            inner: TaggedEnum::VariantB { x: 2.0, y: 3.0 },
            name: "second".into()
        }
    );
}

/// Failing case: Vec of structs with flattened tagged enum, fields out of order
/// Tag appears after some variant fields
#[test]
fn vec_flattened_tagged_enum_fields_out_of_order() {
    let json = r#"[
        {"value": 1.0, "type": "VariantA", "name": "first"},
        {"x": 2.0, "name": "second", "type": "VariantB", "y": 3.0}
    ]"#;

    let items: Vec<FlattenedItem> =
        facet_json::from_str(json).expect("fields out of order should work");
    assert_eq!(items.len(), 2);
    assert_eq!(
        items[0],
        FlattenedItem {
            inner: TaggedEnum::VariantA { value: 1.0 },
            name: "first".into()
        }
    );
    assert_eq!(
        items[1],
        FlattenedItem {
            inner: TaggedEnum::VariantB { x: 2.0, y: 3.0 },
            name: "second".into()
        }
    );
}

/// Tag at the very end, all variant fields before it
#[test]
fn vec_flattened_tagged_enum_tag_last() {
    let json = r#"[
        {"value": 1.0, "name": "first", "type": "VariantA"},
        {"x": 2.0, "y": 3.0, "name": "second", "type": "VariantB"}
    ]"#;

    let items: Vec<FlattenedItem> = facet_json::from_str(json).expect("tag last should work");
    assert_eq!(items.len(), 2);
}

// =============================================================================
// Nested structures: Vec inside HashMap inside struct
// =============================================================================

#[derive(Clone, Debug, Facet, PartialEq)]
pub struct NestedContainer {
    pub groups: HashMap<String, Vec<FlattenedItem>>,
}

/// HashMap containing Vec of flattened tagged enums
#[test]
fn hashmap_of_vec_flattened_tagged_enum() {
    let json = r#"{
        "groups": {
            "group1": [
                {"type": "VariantA", "value": 1.0, "name": "a1"},
                {"x": 2.0, "type": "VariantB", "y": 3.0, "name": "b1"}
            ],
            "group2": [
                {"name": "a2", "value": 4.0, "type": "VariantA"}
            ]
        }
    }"#;

    let container: NestedContainer =
        facet_json::from_str(json).expect("nested HashMap<String, Vec<...>> should work");
    assert_eq!(container.groups.len(), 2);
    assert_eq!(container.groups["group1"].len(), 2);
    assert_eq!(container.groups["group2"].len(), 1);
}

// =============================================================================
// Vec<Vec<T>> - nested lists
// =============================================================================

#[derive(Clone, Debug, Facet, PartialEq)]
pub struct Matrix {
    pub rows: Vec<Vec<FlattenedItem>>,
}

/// Nested Vec<Vec<...>> with flattened tagged enums
#[test]
fn nested_vec_of_vec_flattened_tagged_enum() {
    let json = r#"{
        "rows": [
            [
                {"type": "VariantA", "value": 1.0, "name": "r0c0"},
                {"x": 2.0, "type": "VariantB", "y": 3.0, "name": "r0c1"}
            ],
            [
                {"name": "r1c0", "value": 4.0, "type": "VariantA"}
            ]
        ]
    }"#;

    let matrix: Matrix = facet_json::from_str(json).expect("Vec<Vec<...>> should work");
    assert_eq!(matrix.rows.len(), 2);
    assert_eq!(matrix.rows[0].len(), 2);
    assert_eq!(matrix.rows[1].len(), 1);
}

// =============================================================================
// Large lists - stress test for rope chunking
// =============================================================================

/// Many elements to ensure chunking works correctly
#[test]
fn large_vec_flattened_tagged_enum() {
    // Generate JSON with 100 elements, alternating variants, fields in various orders
    let mut elements = Vec::new();
    for i in 0..100 {
        if i % 3 == 0 {
            // Tag first
            elements.push(format!(
                r#"{{"type": "VariantA", "value": {}.0, "name": "item{}"}}"#,
                i, i
            ));
        } else if i % 3 == 1 {
            // Tag middle
            elements.push(format!(
                r#"{{"x": {}.0, "type": "VariantB", "y": {}.0, "name": "item{}"}}"#,
                i,
                i + 1,
                i
            ));
        } else {
            // Tag last
            elements.push(format!(
                r#"{{"value": {}.0, "name": "item{}", "type": "VariantA"}}"#,
                i, i
            ));
        }
    }
    let json = format!("[{}]", elements.join(","));

    let items: Vec<FlattenedItem> = facet_json::from_str(&json).expect("large Vec should work");
    assert_eq!(items.len(), 100);

    // Verify a few elements
    assert_eq!(
        items[0],
        FlattenedItem {
            inner: TaggedEnum::VariantA { value: 0.0 },
            name: "item0".into()
        }
    );
    assert_eq!(
        items[1],
        FlattenedItem {
            inner: TaggedEnum::VariantB { x: 1.0, y: 2.0 },
            name: "item1".into()
        }
    );
}

// =============================================================================
// Option inside list elements
// =============================================================================

#[derive(Clone, Debug, Facet, PartialEq)]
pub struct ItemWithOptionalFields {
    #[facet(flatten)]
    pub inner: TaggedEnum,
    pub optional_name: Option<String>,
    pub optional_count: Option<i32>,
}

/// Vec with Option fields that may or may not be present
#[test]
fn vec_with_optional_fields() {
    let json = r#"[
        {"type": "VariantA", "value": 1.0, "optional_name": "has_name"},
        {"x": 2.0, "type": "VariantB", "y": 3.0, "optional_count": 42},
        {"value": 4.0, "type": "VariantA"}
    ]"#;

    let items: Vec<ItemWithOptionalFields> =
        facet_json::from_str(json).expect("Vec with optional fields should work");
    assert_eq!(items.len(), 3);
    assert_eq!(items[0].optional_name, Some("has_name".into()));
    assert_eq!(items[0].optional_count, None);
    assert_eq!(items[1].optional_name, None);
    assert_eq!(items[1].optional_count, Some(42));
    assert_eq!(items[2].optional_name, None);
    assert_eq!(items[2].optional_count, None);
}

// =============================================================================
// Multiple flattened fields
// =============================================================================

#[derive(Clone, Debug, Facet, PartialEq)]
#[facet(tag = "meta_type")]
#[repr(C)]
pub enum MetaEnum {
    MetaA { meta_value: String },
    MetaB { meta_x: i32, meta_y: i32 },
}

#[derive(Clone, Debug, Facet, PartialEq)]
pub struct ItemWithTwoFlattened {
    #[facet(flatten)]
    pub data: TaggedEnum,
    #[facet(flatten)]
    pub meta: MetaEnum,
    pub id: String,
}

/// Item with two separate flattened tagged enums
#[test]
fn vec_with_two_flattened_enums() {
    let json = r#"[
        {
            "type": "VariantA", "value": 1.0,
            "meta_type": "MetaA", "meta_value": "info1",
            "id": "first"
        },
        {
            "meta_x": 10, "type": "VariantB", "x": 2.0,
            "id": "second", "meta_type": "MetaB", "y": 3.0, "meta_y": 20
        }
    ]"#;

    let items: Vec<ItemWithTwoFlattened> =
        facet_json::from_str(json).expect("Vec with two flattened enums should work");
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].id, "first");
    assert_eq!(items[1].id, "second");
    assert!(matches!(items[0].data, TaggedEnum::VariantA { .. }));
    assert!(matches!(items[0].meta, MetaEnum::MetaA { .. }));
    assert!(matches!(items[1].data, TaggedEnum::VariantB { .. }));
    assert!(matches!(items[1].meta, MetaEnum::MetaB { .. }));
}

// =============================================================================
// Deeply nested: Vec inside Option inside struct inside Vec
// =============================================================================

#[derive(Clone, Debug, Facet, PartialEq)]
pub struct Wrapper {
    pub items: Option<Vec<FlattenedItem>>,
}

#[derive(Clone, Debug, Facet, PartialEq)]
pub struct DeepNested {
    pub wrappers: Vec<Wrapper>,
}

/// Vec<Wrapper { items: Option<Vec<FlattenedItem>> }>
#[test]
fn deeply_nested_vec_option_vec() {
    let json = r#"{
        "wrappers": [
            {"items": [{"type": "VariantA", "value": 1.0, "name": "w0i0"}]},
            {"items": null},
            {"items": [
                {"x": 2.0, "type": "VariantB", "y": 3.0, "name": "w2i0"},
                {"value": 4.0, "name": "w2i1", "type": "VariantA"}
            ]}
        ]
    }"#;

    let nested: DeepNested =
        facet_json::from_str(json).expect("deeply nested structure should work");
    assert_eq!(nested.wrappers.len(), 3);
    assert_eq!(nested.wrappers[0].items.as_ref().unwrap().len(), 1);
    assert!(nested.wrappers[1].items.is_none());
    assert_eq!(nested.wrappers[2].items.as_ref().unwrap().len(), 2);
}

// =============================================================================
// Externally tagged enum in Vec (should already work, but verify)
// =============================================================================

#[derive(Clone, Debug, Facet, PartialEq)]
#[repr(C)]
pub enum ExternallyTagged {
    Alpha { value: f64 },
    Beta { x: f64, y: f64 },
}

#[derive(Clone, Debug, Facet, PartialEq)]
pub struct ItemWithExternallyTagged {
    #[facet(flatten)]
    pub data: ExternallyTagged,
    pub name: String,
}

/// Externally tagged enum in Vec (baseline - should work)
#[test]
fn vec_externally_tagged_enum() {
    let json = r#"[
        {"Alpha": {"value": 1.0}, "name": "first"},
        {"Beta": {"x": 2.0, "y": 3.0}, "name": "second"}
    ]"#;

    let items: Vec<ItemWithExternallyTagged> =
        facet_json::from_str(json).expect("externally tagged enum in Vec should work");
    assert_eq!(items.len(), 2);
}

// =============================================================================
// Empty and single-element lists
// =============================================================================

#[test]
fn empty_vec() {
    let json = r#"[]"#;
    let items: Vec<FlattenedItem> = facet_json::from_str(json).expect("empty Vec should work");
    assert!(items.is_empty());
}

#[test]
fn single_element_vec_tag_last() {
    let json = r#"[{"value": 1.0, "name": "only", "type": "VariantA"}]"#;
    let items: Vec<FlattenedItem> =
        facet_json::from_str(json).expect("single element Vec should work");
    assert_eq!(items.len(), 1);
}

// =============================================================================
// Roundtrip tests
// =============================================================================

#[test]
fn roundtrip_vec_flattened_tagged_enum() {
    let original = vec![
        FlattenedItem {
            inner: TaggedEnum::VariantA { value: 1.0 },
            name: "first".into(),
        },
        FlattenedItem {
            inner: TaggedEnum::VariantB { x: 2.0, y: 3.0 },
            name: "second".into(),
        },
    ];

    let json = facet_json::to_string(&original).expect("serialization should work");
    let deserialized: Vec<FlattenedItem> =
        facet_json::from_str(&json).expect("deserialization should work");
    assert_eq!(original, deserialized);
}
