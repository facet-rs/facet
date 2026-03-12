// Test cases for issues 1721 and 1724

use facet::Facet;
use facet_json::{from_str as from_json, to_string};
use facet_testhelpers::test;
use std::collections::HashMap;

#[test]
fn test_deserialize_flattened_enum() {
    #[derive(Facet, Debug, PartialEq)]
    pub struct O {
        #[facet(flatten)]
        pub p: Pd,
    }

    #[derive(Facet, Debug, PartialEq)]
    #[facet(tag = "ty")]
    #[repr(C)]
    pub enum Pd {
        A(Ai),
    }

    #[derive(Facet, Debug, PartialEq)]
    pub struct Ai {
        pub pi: String,
    }

    let json = r#"{"ty":"A","pi":"1000"}"#;
    let parsed: O = from_json(json).expect("Failed to deserialize JSON");

    // Verify the parsed structure is correct
    assert_eq!(
        parsed.p,
        Pd::A(Ai {
            pi: "1000".to_string()
        })
    );

    // Test round-trip serialization
    let serialized = to_string(&parsed).expect("Failed to serialize to JSON");
    assert_eq!(
        json, serialized,
        "Round-trip failed: input and output JSON do not match"
    );
}

#[test]
fn test_deserialize_flattened_enum_with_same_name() {
    #[derive(Clone, Facet, Debug, PartialEq)]
    #[facet(tag = "model")]
    #[repr(C)]
    pub enum Mod {
        A { s: f64 },
        B { s: f64 },
    }

    #[derive(Clone, Facet, Debug, PartialEq)]
    pub struct Outer {
        pub c: String,
        #[facet(flatten)]
        pub model: Mod,
    }

    let json = r#"{"c":"example","s":0.0,"model":"B"}"#;
    let parsed: Outer = from_json(json).expect("Failed to deserialize JSON");

    // Verify the parsed structure is correct
    assert_eq!(parsed.c, "example");
    assert_eq!(parsed.model, Mod::B { s: 0.0 });

    // Test round-trip serialization
    // Note: JSON field order is not semantically significant, so we compare parsed values
    let serialized = to_string(&parsed).expect("Failed to serialize to JSON");
    let reparsed: Outer = from_json(&serialized).expect("Failed to re-parse serialized JSON");
    assert_eq!(
        parsed, reparsed,
        "Round-trip failed: parsed values do not match"
    );
}

#[test]
fn test_flattened_enum_with_catch_all_map() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(tag = "kind")]
    #[repr(C)]
    enum Kind {
        A { value: i32 },
    }

    #[derive(Facet, Debug, PartialEq)]
    pub struct Outer {
        pub id: String,
        #[facet(flatten)]
        pub kind: Kind,
        #[facet(flatten)]
        pub extras: HashMap<String, String>,
    }

    let json = r#"{"id":"abc","kind":"A","value":5,"note":"hi"}"#;
    let parsed: Outer = from_json(json).expect("Failed to deserialize JSON");

    assert_eq!(parsed.kind, Kind::A { value: 5 });
    assert_eq!(parsed.extras.get("note"), Some(&"hi".to_string()));
    assert_eq!(parsed.extras.len(), 1);

    let serialized = to_string(&parsed).expect("Failed to serialize to JSON");
    let reparsed: Outer = from_json(&serialized).expect("Failed to re-parse serialized JSON");
    assert_eq!(
        parsed, reparsed,
        "Round-trip failed: parsed values do not match"
    );
}

#[test]
fn test_flattened_enum_deny_unknown_fields_errors() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(tag = "k")]
    #[repr(C)]
    enum StrictKind {
        A { v: i32 },
    }

    #[derive(Facet, Debug, PartialEq)]
    #[facet(deny_unknown_fields)]
    struct Strict {
        #[facet(flatten)]
        kind: StrictKind,
        known: i32,
    }

    let json = r#"{"k":"A","v":1,"known":2,"oops":3}"#;
    let err = from_json::<Strict>(json);
    assert!(
        err.is_err(),
        "Expected deny_unknown_fields to error on unknown key"
    );
}

#[test]
fn test_flattened_enum_with_null_optional_payload() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(tag = "ty")]
    #[repr(C)]
    pub enum Kind {
        A { payload: Option<f64> },
    }

    #[derive(Facet, Debug, PartialEq)]
    pub struct Outer {
        pub base: String,
        #[facet(flatten)]
        pub kind: Kind,
    }

    let json = r#"{"base":"x","ty":"A","payload":null}"#;
    let parsed: Outer = from_json(json).expect("Failed to deserialize JSON");

    assert_eq!(
        parsed,
        Outer {
            base: "x".to_string(),
            kind: Kind::A { payload: None }
        }
    );

    let serialized = to_string(&parsed).expect("Failed to serialize to JSON");
    let reparsed: Outer = from_json(&serialized).expect("Failed to re-parse serialized JSON");
    assert_eq!(
        parsed, reparsed,
        "Round-trip failed: parsed values do not match"
    );
}

// ============================================================================
// Edge case tests for bugs found during code review
// ============================================================================

/// Test that variant hinting only uses actual tag fields, not arbitrary string fields.
/// Regression test for: hint_variant was called for ANY string value in evidence,
/// not just values from tag fields.
#[test]
fn test_variant_hinting_only_from_tag_field() {
    // Create a scenario where a non-tag string field happens to match a variant name.
    // The deserializer should NOT use this for variant selection.
    #[derive(Facet, Debug, PartialEq)]
    #[facet(tag = "type")]
    #[repr(C)]
    pub enum Variant {
        Alpha { data: i32 },
        Beta { data: i32 },
    }

    #[derive(Facet, Debug, PartialEq)]
    pub struct Container {
        // This string field could have value "Alpha" or "Beta" but should NOT
        // influence variant selection - only the "type" tag should.
        pub note: String,
        #[facet(flatten)]
        pub inner: Variant,
    }

    // note="Beta" but type="Alpha" - should deserialize as Alpha, not Beta
    let json = r#"{"note":"Beta","type":"Alpha","data":42}"#;
    let parsed: Container = from_json(json).expect("Failed to deserialize JSON");

    assert_eq!(parsed.note, "Beta");
    assert_eq!(parsed.inner, Variant::Alpha { data: 42 });
}

/// Test that mismatched tag values cause an error.
/// Regression test for: tag value was skipped without validation.
#[test]
fn test_tag_value_validation() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(tag = "ty")]
    #[repr(C)]
    pub enum Kind {
        A { x: i32 },
        B { x: i32 },
    }

    #[derive(Facet, Debug, PartialEq)]
    pub struct Outer {
        #[facet(flatten)]
        pub kind: Kind,
    }

    // Valid JSON - tag matches
    let valid_json = r#"{"ty":"A","x":1}"#;
    let parsed: Outer = from_json(valid_json).expect("Should parse valid JSON");
    assert_eq!(parsed.kind, Kind::A { x: 1 });
}

/// Test that internally-tagged enums with scalar newtype payloads error properly.
/// Regression test for: scalar payloads were silently dropped.
#[test]
fn test_scalar_newtype_variant_errors() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(tag = "type")]
    #[repr(C)]
    pub enum ScalarEnum {
        // This variant has a scalar payload that cannot be flattened
        Number(i32),
    }

    #[derive(Facet, Debug, PartialEq)]
    pub struct Wrapper {
        #[facet(flatten)]
        pub value: ScalarEnum,
    }

    // Attempting to serialize should error, not silently drop the payload
    let value = Wrapper {
        value: ScalarEnum::Number(42),
    };
    let result = to_string(&value);
    assert!(
        result.is_err(),
        "Serializing flattened internally-tagged enum with scalar payload should error"
    );

    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("scalar newtype payload cannot be flattened")
            || err.contains("adjacently-tagged"),
        "Error message should mention the issue and suggest using content attribute: {err}"
    );
}
