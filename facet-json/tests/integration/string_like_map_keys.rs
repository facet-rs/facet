//! Tests for string-like types as map keys.
//!
//! Issue #1614: Box<str>, Arc<str>, CompactString, etc. should serialize correctly
//! as map keys, not using debug format.

use std::collections::HashMap;
use std::sync::Arc;

use compact_str::CompactString;
use facet_json::to_string;
use facet_testhelpers::test;

/// Test Box<str> as map key - should serialize as `{"key":true}` not `{"⟨Box<str>⟩":true}`
#[test]
fn box_str_map_key() {
    let map = HashMap::from([(Box::<str>::from("key"), true)]);
    let json = to_string(&map).unwrap();
    assert_eq!(json, r#"{"key":true}"#);
}

/// Test Arc<str> as map key - should serialize as `{"key":true}` not `{"⟨Arc<str>⟩":true}`
#[test]
fn arc_str_map_key() {
    let map = HashMap::from([(Arc::<str>::from("key"), true)]);
    let json = to_string(&map).unwrap();
    assert_eq!(json, r#"{"key":true}"#);
}

/// Test CompactString as map key - should serialize as `{"key":true}` not `{"\"key\"":true}`
#[test]
fn compact_string_map_key() {
    let map = HashMap::from([(CompactString::from("key"), true)]);
    let json = to_string(&map).unwrap();
    assert_eq!(json, r#"{"key":true}"#);
}

/// Test multiple string-like keys in the same map
#[test]
fn box_str_multiple_keys() {
    let map = HashMap::from([
        (Box::<str>::from("alpha"), 1),
        (Box::<str>::from("beta"), 2),
    ]);
    let json = to_string(&map).unwrap();
    // HashMap ordering is not guaranteed, so check both possibilities
    assert!(
        json == r#"{"alpha":1,"beta":2}"# || json == r#"{"beta":2,"alpha":1}"#,
        "unexpected json: {json}"
    );
}
