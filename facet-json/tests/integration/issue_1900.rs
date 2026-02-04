//! Test for issue #1900: Cow enums should serialize transparently.
//!
//! With `#[facet(cow)]`, enums should serialize as the inner value directly,
//! not as `{"Owned": "value"}` or `{"Borrowed": "value"}`.
//!
//! The Borrowed/Owned distinction is an implementation detail for memory
//! management, not part of the data model.

use compact_str::CompactString;
use facet::Facet;
use facet_json::{from_str, to_string};
use facet_testhelpers::test;

/// Example from the issue - a cow-like enum using CompactString
#[derive(Debug, PartialEq, Facet)]
#[facet(cow)]
#[repr(u8)]
pub enum Stem<'a> {
    Borrowed(&'a str),
    Owned(CompactString),
}

#[test]
fn test_issue_1900_serialize_transparent() {
    // Before fix: {"Owned":"hello"}
    // After fix: "hello"
    let s = Stem::Owned("hello".into());
    let json = to_string(&s).expect("should serialize");
    assert_eq!(json, r#""hello""#);
}

#[test]
fn test_issue_1900_deserialize_transparent() {
    // Deserialize from plain string, not wrapped object
    let json = r#""hello""#;
    let result: Stem<'static> = from_str(json).expect("should deserialize");
    assert_eq!(result, Stem::Owned("hello".into()));
}

#[test]
fn test_issue_1900_roundtrip() {
    let original = Stem::Owned("test value".into());
    let json = to_string(&original).expect("should serialize");
    assert_eq!(json, r#""test value""#);

    let parsed: Stem<'static> = from_str(&json).expect("should deserialize");
    assert_eq!(parsed, original);
}
