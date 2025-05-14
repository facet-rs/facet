use eyre::Result;
use facet::Facet;
use facet_msgpack::from_slice;

#[test]
fn msgpack_read_struct_two_fields() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, PartialEq, Facet)]
    struct TestStruct {
        name: String,
        age: u64,
    }

    let data = [
        0x82, // Fixmap with 2 elements
        0xa4, // Fixstr with length 4
        0x6e, 0x61, 0x6d, 0x65, // "name"
        0xa5, // Fixstr with length 5
        0x41, 0x6c, 0x69, 0x63, 0x65, // "Alice"
        0xa3, // Fixstr with length 3
        0x61, 0x67, 0x65, // "age"
        0xce, // uint32 (correct prefix according to MessagePack spec)
        0x00, 0x00, 0x00, 0x1e, // 30
    ];

    let result: TestStruct = from_slice(&data)?;
    assert_eq!(
        result,
        TestStruct {
            name: "Alice".to_string(),
            age: 30,
        }
    );

    Ok(())
}
