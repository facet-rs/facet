#![forbid(unsafe_code)]

//! Test for metadata containers as flattened map keys.
//!
//! This tests that types like `Documented<String>` or `Spanned<String>` can be
//! used as keys in a `#[facet(flatten)]` map and deserialized correctly.

use std::collections::HashMap;

use facet::Facet;
use facet_reflect::Span;
use facet_testhelpers::test;

/// A metadata container that captures span information.
#[derive(Debug, Clone, Facet)]
#[facet(metadata_container)]
struct Spanned<T> {
    pub value: T,
    #[facet(metadata = "span")]
    pub span: Option<Span>,
}

impl<T: PartialEq> PartialEq for Spanned<T> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}
impl<T: Eq> Eq for Spanned<T> {}
impl<T: std::hash::Hash> std::hash::Hash for Spanned<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.value.hash(state);
    }
}

#[derive(Facet, Debug)]
struct SpannedMap {
    #[facet(flatten)]
    items: HashMap<Spanned<String>, String>,
}

#[test]
fn metadata_container_as_flattened_map_key_deserialize() {
    let json = r#"{"foo": "bar", "baz": "qux"}"#;
    let result: SpannedMap = facet_json::from_str(json).expect("should deserialize");

    assert_eq!(result.items.len(), 2);

    // Check that keys exist (we need to find them by value since we can't look up by &str)
    let keys: Vec<_> = result.items.keys().map(|k| k.value.as_str()).collect();
    assert!(keys.contains(&"foo"), "missing key 'foo'");
    assert!(keys.contains(&"baz"), "missing key 'baz'");
}

#[test]
fn metadata_container_as_flattened_map_key_serialize() {
    let mut items = HashMap::new();
    items.insert(
        Spanned {
            value: "foo".to_string(),
            span: None,
        },
        "bar".to_string(),
    );
    items.insert(
        Spanned {
            value: "baz".to_string(),
            span: None,
        },
        "qux".to_string(),
    );

    let wrapper = SpannedMap { items };
    let json = facet_json::to_string(&wrapper).expect("should serialize");

    assert!(
        json.contains("\"foo\""),
        "expected 'foo' in output, got: {json}"
    );
    assert!(
        json.contains("\"baz\""),
        "expected 'baz' in output, got: {json}"
    );
}
