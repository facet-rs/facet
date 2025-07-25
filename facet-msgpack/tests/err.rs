use eyre::Result;
use facet::Facet;
use facet_msgpack::from_slice;

#[derive(Facet, Debug)]
struct FooBar {
    foo: u64,
    bar: String,
}

#[test]
fn bad_hex_1() -> Result<()> {
    facet_testhelpers::setup();

    let data = [
        0x82, // Fixmap with 2 elements
        0xa3, // Fixstr with length 3
        0x66, 0x6f, 0x6f, // "foo"
        0xce, // uint32 (correct prefix according to MessagePack spec)
        0x00, 0x00, 0x00, 0x2a, // 42
    ];
    let err = from_slice::<FooBar>(&data).unwrap_err();
    insta::assert_snapshot!(err);
    Ok(())
}

#[test]
fn bad_hex_2() -> Result<()> {
    facet_testhelpers::setup();

    let data = [
        0x82, // Fixmap with 2 elements
        0xa3, // Fixstr with length 3
        0x66, 0x6f, 0x6f, // "foo"
        0xce, // uint32 (correct prefix according to MessagePack spec)
        0x00, 0x00, 0x00, 0x2a, // 42
        0xa3, // Fixstr with length 3
        0x62, 0x61, 0x72, // "bar"
        0xce, // uint32 (correct prefix according to MessagePack spec)
        0x00, 0x00, 0x00, 0x2a, // 42
    ];
    let err = from_slice::<FooBar>(&data).unwrap_err();
    insta::assert_snapshot!(err);
    Ok(())
}
