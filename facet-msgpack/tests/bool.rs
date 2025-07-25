use eyre::Result;
use facet::Facet;
use facet_msgpack::from_slice;

#[test]
fn msgpack_read_bool() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Facet, Debug, PartialEq)]
    struct BoolStruct {
        yes: bool,
        no: bool,
    }

    let data = [
        0x82, // Map with 2 elements
        0xa3, // Fixstr with length 3
        0x79, 0x65, 0x73, // "yes"
        0xc3, // true
        0xa2, // Fixstr with length 2
        0x6e, 0x6f, // "no"
        0xc2, // false
    ];

    let s: BoolStruct = from_slice(&data)?;
    assert_eq!(
        s,
        BoolStruct {
            yes: true,
            no: false
        }
    );

    Ok(())
}
