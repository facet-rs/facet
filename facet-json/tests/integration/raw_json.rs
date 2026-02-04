//! Tests for RawJson support in facet-json.

use facet::Facet;
use facet_json::{RawJson, from_str, from_str_borrowed, to_string};
use facet_testhelpers::test;

// ── Deserialization tests ──

#[test]
fn deserialize_raw_json_object() {
    #[derive(Facet, Debug, PartialEq)]
    struct Response<'a> {
        status: u32,
        data: RawJson<'a>,
    }

    let json = r#"{"status": 200, "data": {"nested": [1, 2, 3], "complex": true}}"#;
    let response: Response = from_str_borrowed(json).unwrap();

    assert_eq!(response.status, 200);
    assert_eq!(
        response.data.as_str(),
        r#"{"nested": [1, 2, 3], "complex": true}"#
    );
}

#[test]
fn deserialize_raw_json_array() {
    #[derive(Facet, Debug, PartialEq)]
    struct Container<'a> {
        items: RawJson<'a>,
    }

    let json = r#"{"items": [1, "two", null, true]}"#;
    let container: Container = from_str_borrowed(json).unwrap();

    assert_eq!(container.items.as_str(), r#"[1, "two", null, true]"#);
}

#[test]
fn deserialize_raw_json_string() {
    #[derive(Facet, Debug, PartialEq)]
    struct Container<'a> {
        value: RawJson<'a>,
    }

    let json = r#"{"value": "hello world"}"#;
    let container: Container = from_str_borrowed(json).unwrap();

    assert_eq!(container.value.as_str(), r#""hello world""#);
}

#[test]
fn deserialize_raw_json_number() {
    #[derive(Facet, Debug, PartialEq)]
    struct Container<'a> {
        value: RawJson<'a>,
    }

    let json = r#"{"value": 42}"#;
    let container: Container = from_str_borrowed(json).unwrap();

    assert_eq!(container.value.as_str(), "42");
}

#[test]
fn deserialize_raw_json_boolean() {
    #[derive(Facet, Debug, PartialEq)]
    struct Container<'a> {
        value: RawJson<'a>,
    }

    let json = r#"{"value": true}"#;
    let container: Container = from_str_borrowed(json).unwrap();

    assert_eq!(container.value.as_str(), "true");
}

#[test]
fn deserialize_raw_json_null() {
    #[derive(Facet, Debug, PartialEq)]
    struct Container<'a> {
        value: RawJson<'a>,
    }

    let json = r#"{"value": null}"#;
    let container: Container = from_str_borrowed(json).unwrap();

    assert_eq!(container.value.as_str(), "null");
}

#[test]
fn deserialize_raw_json_owned() {
    #[derive(Facet, Debug, PartialEq)]
    struct Response {
        status: u32,
        data: RawJson<'static>,
    }

    let json = r#"{"status": 200, "data": {"nested": true}}"#;
    let response: Response = from_str(json).unwrap();

    assert_eq!(response.status, 200);
    assert_eq!(response.data.as_str(), r#"{"nested": true}"#);
}

#[test]
fn deserialize_multiple_raw_json_fields() {
    #[derive(Facet, Debug, PartialEq)]
    struct Multi<'a> {
        first: RawJson<'a>,
        second: RawJson<'a>,
        third: RawJson<'a>,
    }

    let json = r#"{"first": [1, 2], "second": {"key": "value"}, "third": null}"#;
    let multi: Multi = from_str_borrowed(json).unwrap();

    assert_eq!(multi.first.as_str(), "[1, 2]");
    assert_eq!(multi.second.as_str(), r#"{"key": "value"}"#);
    assert_eq!(multi.third.as_str(), "null");
}

// ── Serialization tests ──

#[test]
fn serialize_raw_json_object() {
    #[derive(Facet, Debug)]
    struct Response<'a> {
        status: u32,
        data: RawJson<'a>,
    }

    let response = Response {
        status: 200,
        data: RawJson::new(r#"{"nested": true}"#),
    };

    let json = to_string(&response).unwrap();
    assert_eq!(json, r#"{"status":200,"data":{"nested": true}}"#);
}

#[test]
fn serialize_raw_json_array() {
    #[derive(Facet, Debug)]
    struct Container<'a> {
        items: RawJson<'a>,
    }

    let container = Container {
        items: RawJson::new(r#"[1, 2, 3]"#),
    };

    let json = to_string(&container).unwrap();
    assert_eq!(json, r#"{"items":[1, 2, 3]}"#);
}

#[test]
fn serialize_raw_json_number() {
    #[derive(Facet, Debug)]
    struct Container<'a> {
        value: RawJson<'a>,
    }

    let container = Container {
        value: RawJson::new("42"),
    };

    let json = to_string(&container).unwrap();
    assert_eq!(json, r#"{"value":42}"#);
}

// ── Round-trip tests ──

#[test]
fn round_trip_raw_json_complex() {
    #[derive(Facet, Debug, PartialEq)]
    struct Wrapper<'a> {
        raw: RawJson<'a>,
    }

    let original_json = r#"{"raw": {"a": [1, 2, 3], "b": {"nested": true}}}"#;
    let parsed: Wrapper = from_str_borrowed(original_json).unwrap();

    // Re-serialize
    let re_serialized = to_string(&parsed).unwrap();

    // Parse again
    let reparsed: Wrapper = from_str_borrowed(&re_serialized).unwrap();

    assert_eq!(parsed.raw.as_str(), reparsed.raw.as_str());
}

#[test]
fn raw_json_into_owned() {
    #[derive(Facet, Debug)]
    struct Response<'a> {
        data: RawJson<'a>,
    }

    let json = r#"{"data": {"key": "value"}}"#;
    let response: Response = from_str_borrowed(json).unwrap();

    // Convert to owned
    let owned: RawJson<'static> = response.data.into_owned();
    assert_eq!(owned.as_str(), r#"{"key": "value"}"#);
}

// ── Top-level RawJson tests ──

#[test]
fn deserialize_top_level_raw_json_object() {
    let json = r#"{"key": "value", "nested": [1, 2, 3]}"#;
    let raw: RawJson = from_str_borrowed(json).unwrap();
    assert_eq!(raw.as_str(), json);
}

#[test]
fn deserialize_top_level_raw_json_array() {
    let json = r#"[1, 2, 3, "four"]"#;
    let raw: RawJson = from_str_borrowed(json).unwrap();
    assert_eq!(raw.as_str(), json);
}

#[test]
fn serialize_top_level_raw_json() {
    let raw = RawJson::new(r#"{"key": "value"}"#);
    let json = to_string(&raw).unwrap();
    assert_eq!(json, r#"{"key": "value"}"#);
}

// ── Option<RawJson> tests ──

#[test]
fn deserialize_option_raw_json_some() {
    #[derive(Facet, Debug, PartialEq)]
    struct Container<'a> {
        data: Option<RawJson<'a>>,
    }

    let json = r#"{"data": {"nested": true}}"#;
    let container: Container = from_str_borrowed(json).unwrap();

    assert!(container.data.is_some());
    assert_eq!(container.data.unwrap().as_str(), r#"{"nested": true}"#);
}

#[test]
fn deserialize_option_raw_json_none_missing_field() {
    #[derive(Facet, Debug, PartialEq)]
    struct Container<'a> {
        data: Option<RawJson<'a>>,
    }

    let json = r#"{}"#;
    let container: Container = from_str_borrowed(json).unwrap();

    assert!(container.data.is_none());
}

#[test]
fn deserialize_option_raw_json_none_null() {
    #[derive(Facet, Debug, PartialEq)]
    struct Container<'a> {
        data: Option<RawJson<'a>>,
    }

    let json = r#"{"data": null}"#;
    let container: Container = from_str_borrowed(json).unwrap();

    // When deserializing Option<T>, the deserializer peeks for null first.
    // If null is found, it returns None without calling the inner deserializer.
    // So Option<RawJson> with null should be None, not Some("null").
    assert!(container.data.is_none());
}

#[test]
fn deserialize_nested_struct_with_option_raw_json() {
    // This reproduces a bug where Option<RawJson> inside a nested struct
    // causes "capture_raw called while an event is buffered" panic.
    #[derive(Facet, Debug, PartialEq)]
    struct Inner<'a> {
        value: Option<RawJson<'a>>,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Outer<'a> {
        inner: Option<Inner<'a>>,
    }

    let json = r#"{"inner": {"value": [1, 2, 3]}}"#;
    let outer: Outer = from_str_borrowed(json).unwrap();

    assert!(outer.inner.is_some());
    let inner = outer.inner.unwrap();
    assert!(inner.value.is_some());
    assert_eq!(inner.value.unwrap().as_str(), "[1, 2, 3]");
}

#[test]
fn deserialize_struct_with_multiple_option_raw_json_fields() {
    #[derive(Facet, Debug, PartialEq)]
    struct Multi<'a> {
        first: Option<RawJson<'a>>,
        second: Option<RawJson<'a>>,
        third: Option<RawJson<'a>>,
    }

    let json = r#"{"first": {"a": 1}, "second": null, "third": [1, 2]}"#;
    let multi: Multi = from_str_borrowed(json).unwrap();

    assert!(multi.first.is_some());
    assert_eq!(multi.first.unwrap().as_str(), r#"{"a": 1}"#);
    // null is handled by Option deserialization before RawJson, so it becomes None
    assert!(multi.second.is_none());
    assert!(multi.third.is_some());
    assert_eq!(multi.third.unwrap().as_str(), "[1, 2]");
}

// ── Option<RawJson> with arrays (coverage for peeked array path) ──

#[test]
fn deserialize_option_raw_json_array() {
    #[derive(Facet, Debug, PartialEq)]
    struct Container<'a> {
        data: Option<RawJson<'a>>,
    }

    let json = r#"{"data": [1, 2, 3]}"#;
    let container: Container = from_str_borrowed(json).unwrap();

    assert!(container.data.is_some());
    assert_eq!(container.data.unwrap().as_str(), "[1, 2, 3]");
}

#[test]
fn deserialize_option_raw_json_scalar_number() {
    // Test peeked scalar path in capture_raw
    #[derive(Facet, Debug, PartialEq)]
    struct Container<'a> {
        value: Option<RawJson<'a>>,
    }

    let json = r#"{"value": 42}"#;
    let container: Container = from_str_borrowed(json).unwrap();

    assert!(container.value.is_some());
    assert_eq!(container.value.unwrap().as_str(), "42");
}

#[test]
fn deserialize_option_raw_json_scalar_string() {
    // Test peeked scalar path in capture_raw with string
    #[derive(Facet, Debug, PartialEq)]
    struct Container<'a> {
        value: Option<RawJson<'a>>,
    }

    let json = r#"{"value": "hello"}"#;
    let container: Container = from_str_borrowed(json).unwrap();

    assert!(container.value.is_some());
    assert_eq!(container.value.unwrap().as_str(), r#""hello""#);
}

#[test]
fn deserialize_option_raw_json_scalar_bool() {
    // Test peeked scalar path in capture_raw with boolean
    #[derive(Facet, Debug, PartialEq)]
    struct Container<'a> {
        value: Option<RawJson<'a>>,
    }

    let json = r#"{"value": true}"#;
    let container: Container = from_str_borrowed(json).unwrap();

    assert!(container.value.is_some());
    assert_eq!(container.value.unwrap().as_str(), "true");
}

// ── Skip value tests (for skip_value coverage) ──

#[test]
fn skip_peeked_object_value() {
    // This tests the skip_value path when a peeked object needs to be skipped
    // (e.g., when an unknown field contains an object)
    #[derive(Facet, Debug, PartialEq)]
    struct Simple {
        name: String,
    }

    // "extra" field contains an object that will be peeked then skipped
    let json = r#"{"extra": {"nested": true}, "name": "test"}"#;
    let simple: Simple = from_str_borrowed(json).unwrap();

    assert_eq!(simple.name, "test");
}

#[test]
fn skip_peeked_array_value() {
    // This tests the skip_value path when a peeked array needs to be skipped
    #[derive(Facet, Debug, PartialEq)]
    struct Simple {
        name: String,
    }

    // "extra" field contains an array that will be peeked then skipped
    let json = r#"{"extra": [1, 2, 3], "name": "test"}"#;
    let simple: Simple = from_str_borrowed(json).unwrap();

    assert_eq!(simple.name, "test");
}

#[test]
fn skip_peeked_scalar_value() {
    // This tests the skip_value path when a peeked scalar needs to be skipped
    #[derive(Facet, Debug, PartialEq)]
    struct Simple {
        name: String,
    }

    // "extra" field contains a scalar that will be peeked then skipped
    let json = r#"{"extra": 42, "name": "test"}"#;
    let simple: Simple = from_str_borrowed(json).unwrap();

    assert_eq!(simple.name, "test");
}
