#![forbid(unsafe_code)]

//! Test for issue #1627: flatten with SmolStr map keys produces empty output

use std::collections::HashMap;

use facet::Facet;
use facet_testhelpers::test;
use smol_str::SmolStr;

#[derive(Facet, Debug)]
struct Inner {
    value: bool,
}

#[derive(Facet, Debug)]
struct Wrapper {
    #[facet(flatten)]
    map: HashMap<SmolStr, Vec<Inner>>,
}

#[test]
fn smolstr_flatten_map_serializes_correctly() {
    let wrapper = Wrapper {
        map: HashMap::from([(SmolStr::from("key"), vec![Inner { value: true }])]),
    };

    let json = facet_json::to_string(&wrapper).expect("should serialize");

    // The output should include the flattened map entry
    assert!(
        json.contains("\"key\""),
        "expected 'key' in output, got: {json}"
    );
    assert!(
        json.contains("\"value\":true"),
        "expected '\"value\":true' in output, got: {json}"
    );
}

#[test]
fn smolstr_flatten_map_with_multiple_keys() {
    let wrapper = Wrapper {
        map: HashMap::from([
            (SmolStr::from("first"), vec![Inner { value: true }]),
            (SmolStr::from("second"), vec![Inner { value: false }]),
        ]),
    };

    let json = facet_json::to_string(&wrapper).expect("should serialize");

    assert!(
        json.contains("\"first\""),
        "expected 'first' in output, got: {json}"
    );
    assert!(
        json.contains("\"second\""),
        "expected 'second' in output, got: {json}"
    );
}

#[test]
fn smolstr_flatten_map_empty() {
    let wrapper = Wrapper {
        map: HashMap::new(),
    };

    let json = facet_json::to_string(&wrapper).expect("should serialize");

    // Empty map should produce empty object
    assert_eq!(json, "{}", "expected empty object for empty map");
}

// Also test with String keys to make sure we didn't break anything
#[derive(Facet, Debug)]
struct WrapperWithString {
    #[facet(flatten)]
    map: HashMap<String, Vec<Inner>>,
}

#[test]
fn string_flatten_map_still_works() {
    let wrapper = WrapperWithString {
        map: HashMap::from([("key".to_string(), vec![Inner { value: true }])]),
    };

    let json = facet_json::to_string(&wrapper).expect("should serialize");

    assert!(
        json.contains("\"key\""),
        "expected 'key' in output, got: {json}"
    );
    assert!(
        json.contains("\"value\":true"),
        "expected '\"value\":true' in output, got: {json}"
    );
}
