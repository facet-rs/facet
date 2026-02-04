#![forbid(unsafe_code)]

use facet::Facet;
use facet_testhelpers::test;
use tendril::{Atomic, StrTendril, Tendril, fmt};

/// Atomic variant of StrTendril (Send + Sync)
type AtomicStrTendril = Tendril<fmt::UTF8, Atomic>;

#[derive(Facet, Debug, PartialEq)]
struct Document {
    title: StrTendril,
    body: StrTendril,
}

#[derive(Facet, Debug, PartialEq)]
struct AtomicDocument {
    title: AtomicStrTendril,
    body: AtomicStrTendril,
}

#[test]
fn str_tendril_serialize() {
    let doc = Document {
        title: StrTendril::from("Hello"),
        body: StrTendril::from("World"),
    };

    let json = facet_json::to_string(&doc).expect("should serialize");
    assert_eq!(json, r#"{"title":"Hello","body":"World"}"#);
}

#[test]
fn str_tendril_deserialize() {
    let json = r#"{"title":"Hello","body":"World"}"#;
    let doc: Document = facet_json::from_str(json).expect("should deserialize");

    assert_eq!(&*doc.title, "Hello");
    assert_eq!(&*doc.body, "World");
}

#[test]
fn str_tendril_roundtrip() {
    let original = Document {
        title: StrTendril::from("Test Title"),
        body: StrTendril::from("Some body text with special chars: éàü"),
    };

    let json = facet_json::to_string(&original).expect("should serialize");
    let restored: Document = facet_json::from_str(&json).expect("should deserialize");

    assert_eq!(original, restored);
}

#[test]
fn atomic_str_tendril_serialize() {
    let doc = AtomicDocument {
        title: AtomicStrTendril::from("Hello"),
        body: AtomicStrTendril::from("World"),
    };

    let json = facet_json::to_string(&doc).expect("should serialize");
    assert_eq!(json, r#"{"title":"Hello","body":"World"}"#);
}

#[test]
fn atomic_str_tendril_deserialize() {
    let json = r#"{"title":"Hello","body":"World"}"#;
    let doc: AtomicDocument = facet_json::from_str(json).expect("should deserialize");

    assert_eq!(&*doc.title, "Hello");
    assert_eq!(&*doc.body, "World");
}

#[test]
fn atomic_str_tendril_roundtrip() {
    let original = AtomicDocument {
        title: AtomicStrTendril::from("Test Title"),
        body: AtomicStrTendril::from("Some body text with special chars: éàü"),
    };

    let json = facet_json::to_string(&original).expect("should serialize");
    let restored: AtomicDocument = facet_json::from_str(&json).expect("should deserialize");

    assert_eq!(&*original.title, &*restored.title);
    assert_eq!(&*original.body, &*restored.body);
}

#[test]
fn str_tendril_empty_string() {
    let doc = Document {
        title: StrTendril::from(""),
        body: StrTendril::from(""),
    };

    let json = facet_json::to_string(&doc).expect("should serialize");
    assert_eq!(json, r#"{"title":"","body":""}"#);

    let restored: Document = facet_json::from_str(&json).expect("should deserialize");
    assert_eq!(doc, restored);
}
