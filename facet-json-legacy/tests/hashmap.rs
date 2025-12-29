use facet_json_legacy::{from_str, to_string};
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

// Test for issue #1235: enum as HashMap key (simple case)
#[test]
fn issue_1235_enum_hashmap_key() {
    use facet::Facet;

    #[derive(Facet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
    #[repr(u8)]
    pub enum TTs {
        AA,
        BB,
        CC,
    }

    let json = r#"{"AA": 8, "BB": 9}"#;
    let map: HashMap<TTs, u8> = from_str(json).unwrap();
    assert_eq!(map.get(&TTs::AA), Some(&8));
    assert_eq!(map.get(&TTs::BB), Some(&9));
    assert_eq!(map.get(&TTs::CC), None);
}

// Test for issue #1235: full example from issue (with Arc and struct)
#[test]
fn issue_1235_enum_hashmap_key_full_example() {
    use facet::Facet;
    use std::sync::Arc;

    #[derive(Facet, Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
    #[repr(u8)]
    pub enum TTs {
        AA,
        BB,
        CC,
    }

    #[derive(Facet, Debug)]
    pub struct Data {
        #[facet(default)]
        pub ds: Arc<HashMap<TTs, u8>>,
        pub t: String,
    }

    let json = r#"
    {
        "t": "asdf",
        "ds": {
            "AA": 8,
            "BB": 9
        }
    }
    "#;
    let d: Data = from_str(json).unwrap();
    assert_eq!(d.t, "asdf");
    assert_eq!(d.ds.get(&TTs::AA), Some(&8));
    assert_eq!(d.ds.get(&TTs::BB), Some(&9));
    assert_eq!(d.ds.get(&TTs::CC), None);
}

// Test deserialize HashMap with i32 keys (not just serialize)
#[test]
fn deserialize_hashmap_i32_keys() {
    let json = r#"{"1": 2, "3": 4}"#;
    let map: HashMap<i32, i32> = from_str(json).unwrap();
    assert_eq!(map.get(&1), Some(&2));
    assert_eq!(map.get(&3), Some(&4));
}
