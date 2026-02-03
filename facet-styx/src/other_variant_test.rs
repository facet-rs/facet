//! Tests for #[facet(other)] variants with #[facet(tag)] and #[facet(content)] fields.
//!
//! These tests verify that when deserializing self-describing formats (like Styx)
//! that emit VariantTag events, the #[facet(other)] catch-all variant can capture
//! both the tag name and its payload using field-level attributes.
//!
//! Note: Styx documents are implicitly objects, so to deserialize a single tagged value
//! we need a wrapper struct with a field. E.g., `@string` must be parsed as `v @string`
//! with a `struct SchemaDoc { v: Schema }`.

use facet::Facet;
use facet_testhelpers::test;

use crate::from_str;

/// Schema enum where unknown type tags should be captured.
/// Example: @object{...} matches Object, but @string should be captured as Type { name: "string", payload: () }
#[derive(Facet, Debug, PartialEq)]
#[facet(rename_all = "lowercase")]
#[repr(u8)]
enum Schema {
    /// Known variant: object schema
    Object { fields: Vec<String> },
    /// Known variant: sequence schema
    Seq { item: Box<Schema> },
    /// Catch-all for unknown type names like @string, @unit, @custom
    #[facet(other)]
    Type {
        /// Captures the variant tag name (e.g., "string", "unit")
        #[facet(tag)]
        name: String,
        // Note: no #[facet(content)] field means payload must be unit
    },
}

/// Wrapper struct to test Schema deserialization within a document context.
/// Styx documents are implicitly objects, so we need a struct field to hold the value.
#[derive(Facet, Debug, PartialEq)]
struct SchemaDoc {
    v: Schema,
}

#[test]
fn test_known_variant_object() {
    // v @object{...} - field v has value of Object variant
    let input = r#"v @object{fields (a b c)}"#;
    let result: SchemaDoc = from_str(input).unwrap();
    assert_eq!(
        result.v,
        Schema::Object {
            fields: vec!["a".into(), "b".into(), "c".into()]
        }
    );
}

#[test]
fn test_known_variant_seq() {
    // v @seq{...} - field v has value of Seq variant
    let input = r#"v @seq{item @string}"#;
    let result: SchemaDoc = from_str(input).unwrap();
    assert_eq!(
        result.v,
        Schema::Seq {
            item: Box::new(Schema::Type {
                name: "string".into()
            })
        }
    );
}

#[test]
fn test_other_variant_captures_tag_name() {
    // v @string - @string should be caught by Type { name: "string" }
    let input = r#"v @string"#;
    let result: SchemaDoc = from_str(input).unwrap();
    assert_eq!(
        result.v,
        Schema::Type {
            name: "string".into()
        }
    );
}

#[test]
fn test_other_variant_unit_tag() {
    // v @unit - @unit should be caught by Type { name: "unit" }
    let input = r#"v @unit"#;
    let result: SchemaDoc = from_str(input).unwrap();
    assert_eq!(
        result.v,
        Schema::Type {
            name: "unit".into()
        }
    );
}

#[test]
fn test_other_variant_custom_type() {
    // v @MyCustomType - should be caught by Type { name: "MyCustomType" }
    let input = r#"v @MyCustomType"#;
    let result: SchemaDoc = from_str(input).unwrap();
    assert_eq!(
        result.v,
        Schema::Type {
            name: "MyCustomType".into()
        }
    );
}

/// Schema with both tag and content capture
#[derive(Facet, Debug, PartialEq)]
#[facet(rename_all = "lowercase")]
#[repr(u8)]
enum Value {
    /// Null value
    Null,
    /// Boolean value
    Bool(bool),
    /// Catch-all for other tagged values
    /// Note: payload is Vec because in Styx, @tag(...) creates a sequence payload
    #[facet(other)]
    Tagged {
        /// The tag name
        #[facet(tag)]
        tag: String,
        /// The payload - a sequence because @tag(...) syntax creates a sequence
        #[facet(content)]
        payload: Vec<Value>,
    },
}

/// Wrapper struct to test Value deserialization within a document context.
/// Styx documents are implicitly objects, so we need a struct field to hold the value.
#[derive(Facet, Debug, PartialEq)]
struct Doc {
    v: Value,
}

#[test]
fn test_known_variant_null() {
    // v @null - should match the Null variant
    let input = r#"v @null"#;
    let result: Doc = from_str(input).unwrap();
    assert_eq!(result.v, Value::Null);
}

#[test]
fn test_known_variant_bool_not_representable() {
    // Bool(bool) is a newtype variant, but Styx can't naturally represent this:
    // - @bool true - can't tag bare scalars
    // - @bool(true) - parens create a sequence, not a scalar
    // - @bool{@ true} - creates a struct, not a newtype
    //
    // This test verifies that the struct syntax doesn't accidentally work.
    let input = r#"v @bool{@ true}"#;
    let result: Result<Doc, _> = from_str(input);
    assert!(
        result.is_err(),
        "newtype variants can't be represented with struct payload syntax"
    );
}

#[test]
fn test_known_variant_bool_unhappy() {
    // v @bool(true) is WRONG - parens = sequence, not grouping
    // Bool(bool) expects a scalar payload, not a sequence
    let input = r#"v @bool(true)"#;
    let result: Result<Doc, _> = from_str(input);
    assert!(
        result.is_err(),
        "parens create a sequence, not a scalar payload"
    );
}

#[test]
fn test_other_variant_with_content() {
    // v @custom(@null) - parens create a sequence, so payload is Vec containing Null
    let input = r#"v @custom(@null)"#;
    let result: Doc = from_str(input).unwrap();
    assert_eq!(
        result.v,
        Value::Tagged {
            tag: "custom".into(),
            payload: vec![Value::Null],
        }
    );
}

#[test]
fn test_other_variant_nested() {
    // v @wrapper(@inner(@null)) - outer parens create sequence containing Tagged
    // inner parens create sequence containing Null
    let input = r#"v @wrapper(@inner(@null))"#;
    let result: Doc = from_str(input).unwrap();
    assert_eq!(
        result.v,
        Value::Tagged {
            tag: "wrapper".into(),
            payload: vec![Value::Tagged {
                tag: "inner".into(),
                payload: vec![Value::Null],
            }],
        }
    );
}

// ============================================================================
// Round-trip tests for #[facet(other)] variants (Issue #2004)
// ============================================================================
//
// These tests verify that serializing and then deserializing values with
// #[facet(other)] variants produces the same value - i.e., round-tripping works.
//
// The fix changes two things:
// 1. #[facet(other)] variants are excluded from VariantLookup
// 2. #[facet(other)] variants serialize as untagged (just payload, no tag wrapper)
//
// This means EqBare("$id") should serialize as "$id", not @eq-bare"$id"

use crate::from_str_expr;

/// Simple enum to test serialization behavior directly
#[derive(Facet, Debug, PartialEq)]
#[facet(rename_all = "kebab-case")]
#[repr(u8)]
enum SimpleFilter {
    Null,
    Gt(Vec<String>),
    #[facet(other)]
    EqBare(Option<String>),
}

#[test]
fn test_other_variant_serializes_untagged() {
    // With the fix, #[facet(other)] variants should serialize untagged
    // i.e., EqBare(Some("$id")) -> "$id", not @eq-bare"$id"
    let value = SimpleFilter::EqBare(Some("$id".to_string()));
    let serialized = crate::to_string_compact(&value).unwrap();
    eprintln!("Serialized EqBare: {serialized}");

    // With the fix, it should be just the string, no tag
    // Currently (without fix), it's @eq-bare"$id"
    // After fix, it should be just "$id"
    assert!(
        !serialized.contains("eq-bare"),
        "Expected untagged serialization, but got: {serialized}"
    );
}

#[test]
fn test_other_variant_roundtrip_via_expr() {
    // Test that roundtrip works for #[facet(other)] variant
    let original = SimpleFilter::EqBare(Some("$id".to_string()));
    let serialized = crate::to_string_compact(&original).unwrap();
    eprintln!("Serialized: {serialized}");
    let deserialized: SimpleFilter = from_str_expr(&serialized).unwrap();
    assert_eq!(original, deserialized);
}

#[test]
fn test_known_variant_still_tagged() {
    // Known variants should still serialize with their tag
    let value = SimpleFilter::Gt(vec!["$value".to_string()]);
    let serialized = crate::to_string_compact(&value).unwrap();
    eprintln!("Serialized Gt: {serialized}");
    assert!(
        serialized.contains("gt"),
        "Expected tagged serialization, but got: {serialized}"
    );
}
