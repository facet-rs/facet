use std::hash::{DefaultHasher, Hash, Hasher};

use facet_reflect::Peek;
use facet_testhelpers::test;

#[test]
fn test_peek_value_twoints() {
    let a = 42_i32;
    let b = 42_i32;

    let av = Peek::new(&a);
    let bv = Peek::new(&b);

    assert_eq!(av, bv);
    assert_eq!(av.to_string(), "42");

    let mut h = DefaultHasher::new();
    a.hash(&mut h);
    let h1 = h.finish();

    let mut h = DefaultHasher::new();
    av.hash(&mut h).unwrap();
    let h2 = h.finish();

    assert_eq!(h1, h2);
}

#[test]
fn test_peek_value_twostrings() {
    let a = Some(42_i32);
    let av = Peek::new(&a);

    assert_eq!(av.to_string(), "⟨Option<i32>⟩");
    assert_eq!(format!("{a:?}"), format!("{av:?}"));
}

/// Regression test for issue #1082: UB in `Peek("").as_str()`
/// Previously, `as_str()` used `get::<&str>()` which tried to read a fat pointer
/// from the str data, causing UB for empty strings (reading 16 bytes from 0-byte allocation).
#[test]
fn test_peek_as_str_empty_string() {
    // This used to trigger UB - reading 16 bytes from a 0-byte allocation
    let peek = Peek::new("");
    assert_eq!(peek.as_str(), Some(""));
}

#[test]
fn test_peek_as_str_non_empty_string() {
    let peek = Peek::new("hello");
    assert_eq!(peek.as_str(), Some("hello"));
}

#[test]
fn test_peek_as_str_owned_string() {
    let s = String::from("owned string");
    let peek = Peek::new(&s);
    assert_eq!(peek.as_str(), Some("owned string"));
}
