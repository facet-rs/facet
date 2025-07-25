use eyre::Result;
use facet_msgpack::from_slice;

#[test]
fn test_read_tuple_string() -> Result<()> {
    facet_testhelpers::setup();

    // [""]
    let data_empty = [
        0x91, // Array with 1 element
        0xa0, // Empty string
    ];

    let ok: (String,) = from_slice(&data_empty)?;
    assert_eq!(ok.0, "");

    // ["un", "deux", "trois"]
    let data_three = [
        0x93, // Array with 3 elements
        0xa2, 0x75, 0x6e, // "un"
        0xa4, 0x64, 0x65, 0x75, 0x78, // "deux"
        0xa5, 0x74, 0x72, 0x6f, 0x69, 0x73, // "trois"
    ];

    let ok: (String, String, String) = from_slice(&data_three)?;
    assert_eq!(ok.0, "un");
    assert_eq!(ok.1, "deux");
    assert_eq!(ok.2, "trois");

    Ok(())
}

#[test]
fn test_read_tuple_i32() -> Result<()> {
    facet_testhelpers::setup();

    // [10]
    let data_one = [
        0x91, // Array with 1 element
        0x0a, // 10 (positive fixint)
    ];

    let ok: (i32,) = from_slice(&data_one)?;
    assert_eq!(ok.0, 10);

    // [10, 20]
    let data_two = [
        0x92, // Array with 2 elements
        0x0a, // 10 (positive fixint)
        0x14, // 20 (positive fixint)
    ];

    let ok: (i32, i32) = from_slice(&data_two)?;
    assert_eq!(ok.0, 10);
    assert_eq!(ok.1, 20);

    Ok(())
}

#[test]
fn test_read_tuple_mixed() -> Result<()> {
    facet_testhelpers::setup();

    // ["aaa", 100]
    let data = [
        0x92, // Array with 2 elements
        0xa3, 0x61, 0x61, 0x61, // "aaa"
        0x64, // 100 (positive fixint)
    ];

    let ok: (String, i32) = from_slice(&data)?;
    assert_eq!(ok.0, "aaa");
    assert_eq!(ok.1, 100);

    Ok(())
}
