use facet_testhelpers::test;
use std::collections::HashMap;

#[test]
fn test_deserialize_string_to_string_map() {
    let yaml = r#"
        key1: value1
        key2: value2
        key3: value3
    "#;

    let map: HashMap<String, String> = facet_yaml::from_str(yaml).unwrap();
    assert_eq!(map.len(), 3);
    assert_eq!(map.get("key1"), Some(&"value1".to_string()));
    assert_eq!(map.get("key2"), Some(&"value2".to_string()));
    assert_eq!(map.get("key3"), Some(&"value3".to_string()));
}

#[test]
fn test_deserialize_string_to_u64_map() {
    let yaml = r#"
        one: 1
        two: 2
        three: 3
    "#;

    let map: HashMap<String, u64> = facet_yaml::from_str(yaml).unwrap();
    assert_eq!(map.len(), 3);
    assert_eq!(map.get("one"), Some(&1));
    assert_eq!(map.get("two"), Some(&2));
    assert_eq!(map.get("three"), Some(&3));
}

#[test]
fn test_deserialize_empty_map() {
    let yaml = r#"{}"#;

    let map: HashMap<String, String> = facet_yaml::from_str(yaml).unwrap();
    assert_eq!(map.len(), 0);
}

#[test]
fn test_deserialize_nested_maps() {
    let yaml = r#"
        outer1:
            inner1: value1
            inner2: value2
        outer2:
            inner3: value3
            inner4: value4
    "#;

    let map: HashMap<String, HashMap<String, String>> = facet_yaml::from_str(yaml).unwrap();
    assert_eq!(map.len(), 2);

    let inner1 = map.get("outer1").expect("outer1 key not found");
    assert_eq!(inner1.len(), 2);
    assert_eq!(inner1.get("inner1"), Some(&"value1".to_string()));
    assert_eq!(inner1.get("inner2"), Some(&"value2".to_string()));

    let inner2 = map.get("outer2").expect("outer2 key not found");
    assert_eq!(inner2.len(), 2);
    assert_eq!(inner2.get("inner3"), Some(&"value3".to_string()));
    assert_eq!(inner2.get("inner4"), Some(&"value4".to_string()));
}
