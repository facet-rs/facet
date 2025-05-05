#![cfg(feature = "std")]

use facet_json2::{peek_to_string, peek_to_writer, to_string};
use facet_reflect::Peek;

#[test]
fn test_map_with_string_keys() {
    facet_testhelpers::setup();

    let mut map = std::collections::HashMap::new();
    map.insert("key1".to_string(), "value1");
    map.insert("key2".to_string(), "value2");

    let json = to_string(&map);

    // We don't know the order, so check for both key-value pairs
    assert!(json.contains(r#""key1":"value1"#));
    assert!(json.contains(r#""key2":"value2"#));
}

#[test]
fn test_map_with_multiple_entries() {
    facet_testhelpers::setup();

    let mut map = std::collections::HashMap::new();
    map.insert("first", 1);
    map.insert("second", 2);
    map.insert("third", 3);

    let json = to_string(&map);

    // Count the commas
    let comma_count = json.chars().filter(|&c| c == ',').count();
    assert_eq!(comma_count, 2); // Should have 2 commas for 3 entries
}

#[test]
fn test_hashmap_to_json() {
    facet_testhelpers::setup();

    let mut json_data = std::collections::HashMap::<&str, &str>::new();
    json_data.insert("foo", "bar");

    let expected_json = r#"{"foo":"bar"}"#;

    // Using peek_to_string
    let peek = Peek::new(&json_data);
    let json = peek_to_string(&peek);
    assert_eq!(json, expected_json);

    // Using peek_to_writer
    let mut buffer = Vec::new();
    peek_to_writer(&peek, &mut buffer).unwrap();
    let json = String::from_utf8(buffer).unwrap();
    assert_eq!(json, expected_json);
}
