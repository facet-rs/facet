#![forbid(unsafe_code)]

use facet::Facet;
use facet_format::FormatDeserializer;
use facet_json::JsonParser;
use std::collections::HashMap;

// Test for issue #1621: nested flattened maps not capturing unknown fields

#[derive(Facet, Debug, Default, PartialEq)]
struct Inner {
    #[facet(default)]
    known_field: Option<String>,
    #[facet(flatten, default)]
    extra: HashMap<String, String>,
}

#[derive(Facet, Debug, Default, PartialEq)]
struct Outer {
    #[facet(flatten, default)]
    inner: Inner,
}

#[test]
fn nested_flatten_map_captures_unknown_fields() {
    let input = br#"{"known_field":"hello","unknown1":"value1","unknown2":"value2"}"#;

    let parser = JsonParser::<false>::new(input);
    let mut de = FormatDeserializer::new_owned(parser);
    let value: Outer = de
        .deserialize_root()
        .expect("should deserialize with nested flatten map");

    assert_eq!(value.inner.known_field, Some("hello".to_string()));
    assert!(
        value.inner.extra.contains_key("unknown1"),
        "unknown1 should be in extra, got: {:?}",
        value.inner.extra
    );
    assert_eq!(
        value.inner.extra.get("unknown1"),
        Some(&"value1".to_string())
    );
    assert!(
        value.inner.extra.contains_key("unknown2"),
        "unknown2 should be in extra"
    );
    assert_eq!(
        value.inner.extra.get("unknown2"),
        Some(&"value2".to_string())
    );
}

#[test]
fn nested_flatten_map_empty_if_no_unknown() {
    let input = br#"{"known_field":"hello"}"#;

    let parser = JsonParser::<false>::new(input);
    let mut de = FormatDeserializer::new_owned(parser);
    let value: Outer = de
        .deserialize_root()
        .expect("should deserialize with no unknown fields");

    assert_eq!(value.inner.known_field, Some("hello".to_string()));
    assert!(
        value.inner.extra.is_empty(),
        "extra should be empty when no unknown fields"
    );
}
