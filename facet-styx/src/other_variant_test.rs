//! Tests for #[facet(other)] variants with #[facet(tag)] and #[facet(content)] fields.
//!
//! These tests verify that when deserializing self-describing formats (like Styx)
//! that emit VariantTag events, the #[facet(other)] catch-all variant can capture
//! both the tag name and its payload using field-level attributes.

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
        #[facet(is_tag)]
        name: String,
        // Note: no #[facet(is_content)] field means payload must be unit
    },
}

#[test]
fn test_known_variant_object() {
    // @object should match the Object variant
    let input = r#"@object{fields (a b c)}"#;
    let result: Schema = from_str(input).unwrap();
    assert_eq!(
        result,
        Schema::Object {
            fields: vec!["a".into(), "b".into(), "c".into()]
        }
    );
}

#[test]
fn test_known_variant_seq() {
    // @seq should match the Seq variant
    let input = r#"@seq{item @string}"#;
    let result: Schema = from_str(input).unwrap();
    assert_eq!(
        result,
        Schema::Seq {
            item: Box::new(Schema::Type {
                name: "string".into()
            })
        }
    );
}

#[test]
fn test_other_variant_captures_tag_name() {
    // @string should be caught by Type { name: "string" }
    let input = r#"@string"#;
    let result: Schema = from_str(input).unwrap();
    assert_eq!(
        result,
        Schema::Type {
            name: "string".into()
        }
    );
}

#[test]
fn test_other_variant_unit_tag() {
    // @unit should be caught by Type { name: "unit" }
    let input = r#"@unit"#;
    let result: Schema = from_str(input).unwrap();
    assert_eq!(
        result,
        Schema::Type {
            name: "unit".into()
        }
    );
}

#[test]
fn test_other_variant_custom_type() {
    // @MyCustomType should be caught by Type { name: "MyCustomType" }
    let input = r#"@MyCustomType"#;
    let result: Schema = from_str(input).unwrap();
    assert_eq!(
        result,
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
    #[facet(other)]
    Tagged {
        /// The tag name
        #[facet(is_tag)]
        tag: String,
        /// The payload (could be any value)
        #[facet(is_content)]
        payload: Box<Value>,
    },
}

#[test]
fn test_known_variant_null() {
    let input = r#"@null"#;
    let result: Value = from_str(input).unwrap();
    assert_eq!(result, Value::Null);
}

#[test]
fn test_known_variant_bool() {
    let input = r#"@bool(true)"#;
    let result: Value = from_str(input).unwrap();
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_other_variant_with_content() {
    // @custom(@null) should be Tagged { tag: "custom", payload: Null }
    let input = r#"@custom(@null)"#;
    let result: Value = from_str(input).unwrap();
    assert_eq!(
        result,
        Value::Tagged {
            tag: "custom".into(),
            payload: Box::new(Value::Null),
        }
    );
}

#[test]
fn test_other_variant_nested() {
    // @wrapper(@inner(@null)) should nest correctly
    let input = r#"@wrapper(@inner(@null))"#;
    let result: Value = from_str(input).unwrap();
    assert_eq!(
        result,
        Value::Tagged {
            tag: "wrapper".into(),
            payload: Box::new(Value::Tagged {
                tag: "inner".into(),
                payload: Box::new(Value::Null),
            }),
        }
    );
}
