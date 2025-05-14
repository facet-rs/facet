use eyre::Result;
use facet_msgpack::from_slice;
use std::collections::HashMap;

#[test]
fn msgpack_deserialize_hashmap() -> Result<()> {
    facet_testhelpers::setup();

    // { "key1": "value1", "key2": "value2", "key3": "value3" }
    let data = [
        0x83, // Map with 3 elements
        // key1: value1
        0xa4, 0x6b, 0x65, 0x79, 0x31, // "key1"
        0xa6, 0x76, 0x61, 0x6c, 0x75, 0x65, 0x31, // "value1"
        // key2: value2
        0xa4, 0x6b, 0x65, 0x79, 0x32, // "key2"
        0xa6, 0x76, 0x61, 0x6c, 0x75, 0x65, 0x32, // "value2"
        // key3: value3
        0xa4, 0x6b, 0x65, 0x79, 0x33, // "key3"
        0xa6, 0x76, 0x61, 0x6c, 0x75, 0x65, 0x33, // "value3"
    ];

    let m: HashMap<String, String> = from_slice(&data)?;
    assert_eq!(m.get("key1").unwrap(), "value1");
    assert_eq!(m.get("key2").unwrap(), "value2");
    assert_eq!(m.get("key3").unwrap(), "value3");

    Ok(())
}
