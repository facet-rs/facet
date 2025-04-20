use facet_json::from_str;

#[test]
fn json_read_hashmap() {
    facet_testhelpers::setup();

    let json = r#"{"key1": "value1", "key2": "value2", "key3": "value3"}"#;

    let m: std::collections::HashMap<String, String> = match from_str(json) {
        Ok(m) => m,
        Err(e) => panic!("Error deserializing JSON: {}", e),
    };
    assert_eq!(m.get("key1").unwrap(), "value1");
    assert_eq!(m.get("key2").unwrap(), "value2");
    assert_eq!(m.get("key3").unwrap(), "value3");
}
