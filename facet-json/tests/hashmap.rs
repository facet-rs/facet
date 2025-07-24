use facet_json::{from_str, to_string};
use facet_testhelpers::test;
use std::collections::HashMap;

#[test]
fn json_read_hashmap() {
    let json = r#"{"key1": "value1", "key2": "value2", "key3": "value3"}"#;

    let m: std::collections::HashMap<String, String> = from_str(json).unwrap();
    assert_eq!(m.get("key1").unwrap(), "value1");
    assert_eq!(m.get("key2").unwrap(), "value2");
    assert_eq!(m.get("key3").unwrap(), "value3");
}

#[test]
fn serialize_hashmap_i32_number_keys() {
    let mut map = std::collections::HashMap::new();
    map.insert(1, 2);
    map.insert(3, 4);

    let output = to_string(&map);

    assert!(output.contains("\"1\":2"));
    assert!(output.contains("\"3\":4"));
}

#[test]
fn serialize_hashmap_u8_number_keys() {
    let mut map: HashMap<u8, u8> = std::collections::HashMap::new();
    map.insert(1, 2);
    map.insert(3, 4);

    let output = to_string(&map);

    assert!(output.contains("\"1\":2"));
    assert!(output.contains("\"3\":4"));
}
