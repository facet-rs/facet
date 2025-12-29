use facet::Facet;
use facet_json_legacy::RawJson;

// =============================================================================
// Basic deserialization tests
// =============================================================================

#[test]
fn deserialize_raw_json_object() {
    #[derive(Facet, Debug)]
    struct Response<'a> {
        status: u32,
        data: RawJson<'a>,
    }

    let json = r#"{"status": 200, "data": {"nested": [1, 2, 3], "complex": true}}"#;
    let response: Response = facet_json_legacy::from_str(json).unwrap();

    assert_eq!(response.status, 200);
    assert_eq!(
        response.data.as_str(),
        r#"{"nested": [1, 2, 3], "complex": true}"#
    );
}

#[test]
fn deserialize_raw_json_array() {
    #[derive(Facet, Debug)]
    struct Response<'a> {
        items: RawJson<'a>,
    }

    let json = r#"{"items": [1, 2, 3, "four", null, true]}"#;
    let response: Response = facet_json_legacy::from_str(json).unwrap();

    assert_eq!(response.items.as_str(), r#"[1, 2, 3, "four", null, true]"#);
}

#[test]
fn deserialize_raw_json_string() {
    #[derive(Facet, Debug)]
    struct Response<'a> {
        value: RawJson<'a>,
    }

    let json = r#"{"value": "hello world"}"#;
    let response: Response = facet_json_legacy::from_str(json).unwrap();

    assert_eq!(response.value.as_str(), r#""hello world""#);
}

#[test]
fn deserialize_raw_json_number() {
    #[derive(Facet, Debug)]
    struct Response<'a> {
        value: RawJson<'a>,
    }

    let json = r#"{"value": 42}"#;
    let response: Response = facet_json_legacy::from_str(json).unwrap();

    assert_eq!(response.value.as_str(), "42");
}

#[test]
fn deserialize_raw_json_float() {
    #[derive(Facet, Debug)]
    struct Response<'a> {
        value: RawJson<'a>,
    }

    let json = r#"{"value": 3.14159}"#;
    let response: Response = facet_json_legacy::from_str(json).unwrap();

    assert_eq!(response.value.as_str(), "3.14159");
}

#[test]
fn deserialize_raw_json_boolean_true() {
    #[derive(Facet, Debug)]
    struct Response<'a> {
        value: RawJson<'a>,
    }

    let json = r#"{"value": true}"#;
    let response: Response = facet_json_legacy::from_str(json).unwrap();

    assert_eq!(response.value.as_str(), "true");
}

#[test]
fn deserialize_raw_json_boolean_false() {
    #[derive(Facet, Debug)]
    struct Response<'a> {
        value: RawJson<'a>,
    }

    let json = r#"{"value": false}"#;
    let response: Response = facet_json_legacy::from_str(json).unwrap();

    assert_eq!(response.value.as_str(), "false");
}

#[test]
fn deserialize_raw_json_null() {
    #[derive(Facet, Debug)]
    struct Response<'a> {
        value: RawJson<'a>,
    }

    let json = r#"{"value": null}"#;
    let response: Response = facet_json_legacy::from_str(json).unwrap();

    assert_eq!(response.value.as_str(), "null");
}

// =============================================================================
// Nested structures
// =============================================================================

#[test]
fn deserialize_raw_json_deeply_nested() {
    #[derive(Facet, Debug)]
    struct Response<'a> {
        data: RawJson<'a>,
    }

    let json = r#"{"data": {"a": {"b": {"c": {"d": [1, 2, 3]}}}}}"#;
    let response: Response = facet_json_legacy::from_str(json).unwrap();

    assert_eq!(
        response.data.as_str(),
        r#"{"a": {"b": {"c": {"d": [1, 2, 3]}}}}"#
    );
}

#[test]
fn deserialize_raw_json_array_of_objects() {
    #[derive(Facet, Debug)]
    struct Response<'a> {
        items: RawJson<'a>,
    }

    let json = r#"{"items": [{"id": 1}, {"id": 2}, {"id": 3}]}"#;
    let response: Response = facet_json_legacy::from_str(json).unwrap();

    assert_eq!(
        response.items.as_str(),
        r#"[{"id": 1}, {"id": 2}, {"id": 3}]"#
    );
}

// =============================================================================
// Multiple RawJson fields
// =============================================================================

#[test]
fn deserialize_multiple_raw_json_fields() {
    #[derive(Facet, Debug)]
    struct Response<'a> {
        first: RawJson<'a>,
        second: RawJson<'a>,
        third: RawJson<'a>,
    }

    let json = r#"{"first": [1, 2], "second": {"key": "value"}, "third": null}"#;
    let response: Response = facet_json_legacy::from_str(json).unwrap();

    assert_eq!(response.first.as_str(), "[1, 2]");
    assert_eq!(response.second.as_str(), r#"{"key": "value"}"#);
    assert_eq!(response.third.as_str(), "null");
}

// =============================================================================
// Serialization tests
// =============================================================================

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

    let json = facet_json_legacy::to_string(&response);
    assert_eq!(json, r#"{"status":200,"data":{"nested": true}}"#);
}

#[test]
fn serialize_raw_json_array() {
    #[derive(Facet, Debug)]
    struct Response<'a> {
        items: RawJson<'a>,
    }

    let response = Response {
        items: RawJson::new(r#"[1, 2, 3]"#),
    };

    let json = facet_json_legacy::to_string(&response);
    assert_eq!(json, r#"{"items":[1, 2, 3]}"#);
}

#[test]
fn serialize_raw_json_primitive() {
    #[derive(Facet, Debug)]
    struct Response<'a> {
        value: RawJson<'a>,
    }

    let response = Response {
        value: RawJson::new("42"),
    };

    let json = facet_json_legacy::to_string(&response);
    assert_eq!(json, r#"{"value":42}"#);
}

// =============================================================================
// Roundtrip tests
// =============================================================================

#[test]
fn roundtrip_raw_json() {
    #[derive(Facet, Debug, PartialEq)]
    struct Response<'a> {
        status: u32,
        data: RawJson<'a>,
    }

    let original_json = r#"{"status":200,"data":{"complex":true}}"#;
    let response: Response = facet_json_legacy::from_str(original_json).unwrap();

    assert_eq!(response.status, 200);
    assert_eq!(response.data.as_str(), r#"{"complex":true}"#);

    let serialized = facet_json_legacy::to_string(&response);
    assert_eq!(serialized, original_json);
}

// =============================================================================
// Edge cases
// =============================================================================

#[test]
fn deserialize_raw_json_empty_object() {
    #[derive(Facet, Debug)]
    struct Response<'a> {
        data: RawJson<'a>,
    }

    let json = r#"{"data": {}}"#;
    let response: Response = facet_json_legacy::from_str(json).unwrap();

    assert_eq!(response.data.as_str(), "{}");
}

#[test]
fn deserialize_raw_json_empty_array() {
    #[derive(Facet, Debug)]
    struct Response<'a> {
        data: RawJson<'a>,
    }

    let json = r#"{"data": []}"#;
    let response: Response = facet_json_legacy::from_str(json).unwrap();

    assert_eq!(response.data.as_str(), "[]");
}

#[test]
fn deserialize_raw_json_with_escaped_strings() {
    #[derive(Facet, Debug)]
    struct Response<'a> {
        data: RawJson<'a>,
    }

    let json = r#"{"data": "hello \"world\""}"#;
    let response: Response = facet_json_legacy::from_str(json).unwrap();

    assert_eq!(response.data.as_str(), r#""hello \"world\"""#);
}

#[test]
fn deserialize_raw_json_with_unicode() {
    #[derive(Facet, Debug)]
    struct Response<'a> {
        data: RawJson<'a>,
    }

    let json = r#"{"data": "hello 世界 🌍"}"#;
    let response: Response = facet_json_legacy::from_str(json).unwrap();

    assert_eq!(response.data.as_str(), r#""hello 世界 🌍""#);
}

#[test]
fn deserialize_raw_json_negative_number() {
    #[derive(Facet, Debug)]
    struct Response<'a> {
        value: RawJson<'a>,
    }

    let json = r#"{"value": -42}"#;
    let response: Response = facet_json_legacy::from_str(json).unwrap();

    assert_eq!(response.value.as_str(), "-42");
}

#[test]
fn deserialize_raw_json_scientific_notation() {
    #[derive(Facet, Debug)]
    struct Response<'a> {
        value: RawJson<'a>,
    }

    let json = r#"{"value": 1.23e10}"#;
    let response: Response = facet_json_legacy::from_str(json).unwrap();

    assert_eq!(response.value.as_str(), "1.23e10");
}

// =============================================================================
// Into owned
// =============================================================================

#[test]
fn raw_json_into_owned() {
    let json = r#"{"data": {"key": "value"}}"#;

    #[derive(Facet, Debug)]
    struct Response<'a> {
        data: RawJson<'a>,
    }

    let response: Response = facet_json_legacy::from_str(json).unwrap();
    let owned: RawJson<'static> = response.data.into_owned();

    assert_eq!(owned.as_str(), r#"{"key": "value"}"#);
}

// =============================================================================
// Top-level RawJson
// =============================================================================

#[test]
fn deserialize_top_level_raw_json_object() {
    let json = r#"{"key": "value", "number": 42}"#;
    let raw: RawJson = facet_json_legacy::from_str(json).unwrap();

    assert_eq!(raw.as_str(), json);
}

#[test]
fn deserialize_top_level_raw_json_array() {
    let json = r#"[1, 2, 3, "four"]"#;
    let raw: RawJson = facet_json_legacy::from_str(json).unwrap();

    assert_eq!(raw.as_str(), json);
}

#[test]
fn serialize_top_level_raw_json() {
    let raw = RawJson::new(r#"{"key": "value"}"#);
    let json = facet_json_legacy::to_string(&raw);

    assert_eq!(json, r#"{"key": "value"}"#);
}
