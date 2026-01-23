//! Test for #[facet(cow)] attribute on enums.
//!
//! This tests the cow-like serialization/deserialization semantics where an enum like:
//!   enum Stem<'a> { Borrowed(&'a str), Owned(String) }
//! serializes/deserializes transparently as its inner value.
//!
//! The Borrowed/Owned distinction is purely an implementation detail for memory
//! management and should not appear in the serialized output.

use facet::Facet;
use facet_json::{from_str, to_string};

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
fn test_cow_enum_serialize_transparent() {
    // Cow-like enums should serialize transparently as the inner value
    let stem = Stem::Owned("hello".to_string());
    let json = to_string(&stem).expect("should serialize");

    // Should serialize as just the string, not {"Owned": "hello"}
    assert_eq!(json, r#""hello""#);
}

#[test]
fn test_cow_enum_serialize_borrowed_transparent() {
    // Even Borrowed variant should serialize transparently
    let stem = Stem::Borrowed("world");
    let json = to_string(&stem).expect("should serialize");

    // Should serialize as just the string
    assert_eq!(json, r#""world""#);
}

#[test]
fn test_cow_enum_deserialize_transparent() {
    // Cow-like enums should deserialize transparently from the inner value
    let json = r#""hello""#;
    let result: Stem<'static> = from_str(json).expect("should deserialize");

    // Should deserialize to Owned variant (since JSON can't borrow)
    assert_eq!(result, Stem::Owned("hello".to_string()));
}

#[test]
fn test_cow_enum_in_struct() {
    // Test cow-like enums inside a struct.
    let json = r#"{"title": "My Title", "content": "Some content"}"#;
    let result: Document<'static> = from_str(json).expect("should deserialize");

    assert_eq!(result.title, Stem::Owned("My Title".to_string()));
    assert_eq!(result.content, Stem::Owned("Some content".to_string()));
}

#[test]
fn test_cow_enum_roundtrip() {
    let doc = Document {
        title: Stem::Owned("Test".to_string()),
        content: Stem::Owned("Content".to_string()),
    };

    let json = to_string(&doc).expect("should serialize");

    // Should serialize transparently without variant wrappers
    assert_eq!(json, r#"{"title":"Test","content":"Content"}"#);

    let parsed: Document<'static> = from_str(&json).expect("should deserialize");
    assert_eq!(parsed, doc);
}

#[test]
fn test_cow_enum_roundtrip_borrowed() {
    // Test that Borrowed variant also roundtrips correctly
    let stem = Stem::Borrowed("borrowed");
    let json = to_string(&stem).expect("should serialize");
    assert_eq!(json, r#""borrowed""#);

    // When deserializing, we get Owned since JSON can't borrow
    let parsed: Stem<'static> = from_str(&json).expect("should deserialize");
    assert_eq!(parsed, Stem::Owned("borrowed".to_string()));
}
