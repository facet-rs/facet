//! Test for #[facet(cow)] attribute on enums.
//!
//! This tests the cow-like deserialization semantics where an enum like:
//!   enum Stem<'a> { Borrowed(&'a str), Owned(String) }
//! can be deserialized from JSON (where borrowing is not possible) by
//! automatically selecting the Owned variant.

use facet::Facet;
use facet_json::from_str;

/// A cow-like enum that can hold either a borrowed or owned string.
#[derive(Debug, PartialEq, Facet)]
#[facet(cow)]
#[repr(u8)]
pub enum Stem<'a> {
    Borrowed(&'a str),
    Owned(String),
}

/// A struct containing a cow-like field.
#[derive(Debug, PartialEq, Facet)]
pub struct Document<'a> {
    pub title: Stem<'a>,
    pub content: Stem<'a>,
}

#[test]
fn test_cow_enum_deserialize_owned_variant() {
    // When deserializing from JSON (BORROW=false), selecting "Borrowed" should
    // automatically redirect to "Owned" variant.
    let json = r#"{"Borrowed": "hello"}"#;
    let result: Stem<'static> = from_str(json).expect("should deserialize");

    // Should be Owned, not Borrowed, because JSON deserialization cannot borrow
    assert_eq!(result, Stem::Owned("hello".to_string()));
}

#[test]
fn test_cow_enum_deserialize_owned_variant_explicit() {
    // Explicitly selecting "Owned" variant should work normally.
    let json = r#"{"Owned": "world"}"#;
    let result: Stem<'static> = from_str(json).expect("should deserialize");

    assert_eq!(result, Stem::Owned("world".to_string()));
}

#[test]
fn test_cow_enum_in_struct() {
    // Test cow-like enums inside a struct.
    let json = r#"{"title": {"Borrowed": "My Title"}, "content": {"Owned": "Some content"}}"#;
    let result: Document<'static> = from_str(json).expect("should deserialize");

    assert_eq!(result.title, Stem::Owned("My Title".to_string()));
    assert_eq!(result.content, Stem::Owned("Some content".to_string()));
}

#[test]
fn test_cow_enum_roundtrip() {
    use facet_json::to_string;

    let doc = Document {
        title: Stem::Owned("Test".to_string()),
        content: Stem::Owned("Content".to_string()),
    };

    let json = to_string(&doc).expect("should serialize");
    let parsed: Document<'static> = from_str(&json).expect("should deserialize");

    assert_eq!(parsed, doc);
}
