use facet_json::from_str;
use facet_testhelpers::test;

#[test]
fn json_read_hashmap() {
    let json = r#"{"key1": "value1", "key2": "value2", "key3": "value3"}"#;

    let m: std::collections::HashMap<String, String> = from_str(json)?;
    assert_eq!(m.get("key1").unwrap(), "value1");
    assert_eq!(m.get("key2").unwrap(), "value2");
    assert_eq!(m.get("key3").unwrap(), "value3");
}
