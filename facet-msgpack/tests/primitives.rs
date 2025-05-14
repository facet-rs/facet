use eyre::Result;
use facet::Facet;
use facet_msgpack::from_slice;
use std::collections::HashMap;

#[test]
fn msgpack_deserialize_empty_object_for_struct() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Facet)]
    struct TestStruct {}

    let data = [
        0x80, // Empty map
    ];

    let _: TestStruct = from_slice(&data)?;
    Ok(())
}

#[test]
fn msgpack_deserialize_empty_object_for_hashmap() -> Result<()> {
    facet_testhelpers::setup();

    let data = [
        0x80, // Empty map
    ];

    let _: HashMap<String, String> = from_slice(&data)?;
    Ok(())
}

#[test]
fn test_str_escaped() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Facet, Debug)]
    struct S {
        foo: String,
    }

    // {"foo":"\"\\abc"}
    let data = [
        0x81, // Map with 1 element
        0xa3, 0x66, 0x6f, 0x6f, // "foo"
        0xa5, 0x22, 0x5c, 0x61, 0x62, 0x63, // "\"\\abc"
    ];

    let ok: S = from_slice(&data)?;
    assert_eq!(ok.foo, "\"\\abc");

    Ok(())
}
