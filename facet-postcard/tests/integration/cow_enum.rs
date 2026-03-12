//! Test case for cow-like enums with #[repr(u8)]
//!
//! This tests that enums marked with both #[facet(cow)] and #[repr(u8)]
//! serialize/deserialize correctly with postcard.

#![cfg(feature = "jit")]

use facet::Facet;
use facet_postcard::{from_slice, to_vec};

/// A cow-like enum similar to hotmeal's Stem type
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
#[facet(cow)]
#[repr(u8)]
pub enum Stem<'a> {
    Borrowed(&'a str),
    Owned(String),
}

#[test]
fn test_stem_owned_roundtrip() {
    facet_testhelpers::setup();

    let original: Stem<'static> = Stem::Owned("hello".to_string());
    let bytes = to_vec(&original).expect("serialization should succeed");
    eprintln!("Serialized bytes: {:?}", bytes);

    let decoded: Stem<'static> = from_slice(&bytes).expect("deserialization should succeed");
    match &decoded {
        Stem::Owned(s) => assert_eq!(s, "hello"),
        Stem::Borrowed(_) => panic!("expected Owned variant"),
    }
}

#[test]
fn test_stem_borrowed_roundtrip() {
    facet_testhelpers::setup();

    // When serializing Borrowed, it should serialize the inner value transparently
    let original: Stem<'static> = Stem::Borrowed("world");
    let bytes = to_vec(&original).expect("serialization should succeed");
    eprintln!("Serialized bytes: {:?}", bytes);

    // Deserializing always goes to Owned (can't create borrowed reference into owned data)
    let decoded: Stem<'static> = from_slice(&bytes).expect("deserialization should succeed");
    match &decoded {
        Stem::Owned(s) => assert_eq!(s, "world"),
        Stem::Borrowed(_) => panic!("expected Owned variant after deserialization"),
    }
}

#[test]
fn test_vec_stem_roundtrip() {
    facet_testhelpers::setup();

    let original: Vec<Stem<'static>> = vec![
        Stem::Owned("first".to_string()),
        Stem::Owned("second".to_string()),
    ];
    let bytes = to_vec(&original).expect("serialization should succeed");
    eprintln!("Serialized bytes: {:?}", bytes);

    let decoded: Vec<Stem<'static>> = from_slice(&bytes).expect("deserialization should succeed");
    assert_eq!(decoded.len(), 2);
}
