//! Regression test for https://github.com/facet-rs/facet/issues/2007
//!
//! Vec deserialization fails for structs with flattened tagged enum tuple variants.
//! Single element works fine, but Vec fails with "missing field `0`".

use facet::Facet;
use facet_testhelpers::test;

#[derive(Facet, Clone, PartialEq, Debug)]
pub struct Inner {
    pub value: f64,
}

#[derive(Facet, Clone, PartialEq, Debug)]
#[facet(tag = "type")]
#[repr(C)]
pub enum Tagged {
    TypeA(Inner),
    TypeB(Inner),
}

#[derive(Facet, Clone, PartialEq, Debug)]
pub struct Outer {
    #[facet(flatten)]
    pub tagged: Tagged,
}

#[test]
fn test_single_flattened_tagged_enum_deserialize() {
    let json_single = r#"{"type":"TypeA","value":42.0}"#;
    let single: Outer = facet_json::from_str(json_single).expect("single should work");
    assert!(matches!(single.tagged, Tagged::TypeA(_)));

    if let Tagged::TypeA(inner) = &single.tagged {
        assert_eq!(inner.value, 42.0);
    }
}

#[test]
fn test_vec_flattened_tagged_enum_deserialize() {
    let json_vec = r#"[{"type":"TypeA","value":42.0},{"type":"TypeB","value":99.0}]"#;
    let vec_result: Result<Vec<Outer>, _> = facet_json::from_str(json_vec);
    assert!(
        vec_result.is_ok(),
        "Vec deserialization should work but fails with: {:?}",
        vec_result.err()
    );

    let vec = vec_result.unwrap();
    assert_eq!(vec.len(), 2);

    assert!(matches!(vec[0].tagged, Tagged::TypeA(_)));
    if let Tagged::TypeA(inner) = &vec[0].tagged {
        assert_eq!(inner.value, 42.0);
    }

    assert!(matches!(vec[1].tagged, Tagged::TypeB(_)));
    if let Tagged::TypeB(inner) = &vec[1].tagged {
        assert_eq!(inner.value, 99.0);
    }
}

// Simpler test - just 2 elements to isolate the bug
#[test]
fn test_two_elements_same_variant() {
    let json = r#"[{"type":"TypeA","value":1.0},{"type":"TypeA","value":2.0}]"#;
    let result: Result<Vec<Outer>, _> = facet_json::from_str(json);
    assert!(
        result.is_ok(),
        "Two elements with same variant should work but fails with: {:?}",
        result.err()
    );
}

// Test single element in Vec - should work
#[test]
fn test_single_element_in_vec() {
    let json = r#"[{"type":"TypeA","value":1.0}]"#;
    let result: Result<Vec<Outer>, _> = facet_json::from_str(json);
    assert!(
        result.is_ok(),
        "Single element in Vec should work but fails with: {:?}",
        result.err()
    );
}

// Test: what about if the tuple variant wraps a scalar instead of a struct?
#[derive(Facet, Clone, PartialEq, Debug)]
#[facet(tag = "type")]
#[repr(C)]
pub enum TaggedScalar {
    TypeA(f64),
    TypeB(f64),
}

#[derive(Facet, Clone, PartialEq, Debug)]
pub struct OuterScalar {
    #[facet(flatten)]
    pub tagged: TaggedScalar,
}

#[test]
fn test_scalar_tuple_variant_in_vec() {
    // This tests if the issue is specific to tuple variants wrapping structs
    // For scalar tuple variants, the "value" key doesn't exist - it's just the tuple element
    // Actually for internally-tagged enums, the variant fields need to be flattened
    // So a scalar wouldn't have a field name...
    // Let me test with a struct variant that has named fields instead
}

// Test with struct variant (named fields) instead of tuple variant
#[derive(Facet, Clone, PartialEq, Debug)]
#[facet(tag = "type")]
#[repr(C)]
pub enum TaggedStruct {
    TypeA { value: f64 },
    TypeB { value: f64 },
}

#[derive(Facet, Clone, PartialEq, Debug)]
pub struct OuterStruct {
    #[facet(flatten)]
    pub tagged: TaggedStruct,
}

#[test]
fn test_struct_variant_in_vec() {
    let json = r#"[{"type":"TypeA","value":1.0}]"#;
    let result: Result<Vec<OuterStruct>, _> = facet_json::from_str(json);
    assert!(
        result.is_ok(),
        "Struct variant in Vec should work but fails with: {:?}",
        result.err()
    );
}

#[test]
fn test_roundtrip_vec_flattened_tagged_enum() {
    let original = vec![
        Outer {
            tagged: Tagged::TypeA(Inner { value: 42.0 }),
        },
        Outer {
            tagged: Tagged::TypeB(Inner { value: 99.0 }),
        },
    ];

    let json = facet_json::to_string(&original).expect("serialization should work");
    let deserialized: Vec<Outer> =
        facet_json::from_str(&json).expect("deserialization should work");

    assert_eq!(original, deserialized);
}
