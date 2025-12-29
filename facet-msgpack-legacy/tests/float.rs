use eyre::Result;
use facet::Facet;
use facet_msgpack_legacy::from_slice;

#[test]
fn msgpack_read_float() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Facet, Debug, PartialEq)]
    struct FloatStruct {
        foo: f32,
        bar: f64,
        baz: f32,
    }

    let data = [
        0x83, // Map with 3 elements
        0xa3, // Fixstr with length 3
        0x66, 0x6f, 0x6f, // "foo"
        0xca, // float32
        0x3f, 0x80, 0x00, 0x00, // 1.0
        0xa3, // Fixstr with length 3
        0x62, 0x61, 0x72, // "bar"
        0xcb, // float64
        0x7f, 0xef, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, // f64::MAX
        0xa3, // Fixstr with length 3
        0x62, 0x61, 0x7a, // "baz"
        0xcb, // float64, but will be cast to f32
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // 0
    ];

    let s: FloatStruct = from_slice(&data)?;
    assert_eq!(
        s,
        FloatStruct {
            foo: 1.,
            bar: f64::MAX,
            baz: 0.
        }
    );

    Ok(())
}

#[test]
#[should_panic = "FloatOverflow"]
fn msgpack_read_bad_floats() {
    facet_testhelpers::setup();

    #[derive(Facet, Debug, PartialEq)]
    struct FloatStruct {
        foo: f64,
        bar: f32,
    }

    let data = [
        0x82, // Map with 2 elements
        0xa3, // Fixstr with length 3
        0x66, 0x6f, 0x6f, // "foo"
        0xca, // float32, but will be cast to f64
        0x3f, 0x80, 0x00, 0x00, // 1.0
        0xa3, // Fixstr with length 3
        0x62, 0x61, 0x72, // "bar"
        0xcb, // float64, but will be cast to f32
        0x7f, 0xef, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, // f64::MAX
    ];

    let s: FloatStruct = from_slice(&data).unwrap();
    assert_eq!(
        s,
        FloatStruct {
            foo: 1.,
            bar: f64::MAX as f32,
        }
    );
}
