use eyre::Result;
use facet::Facet;
use facet_msgpack::from_slice;

#[test]
fn msgpack_deserialize_unit_enum_variant() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    enum FontStyle {
        Italic,
        Oblique,
    }

    // "Italic"
    let data_italic = [
        0xa6, 0x49, 0x74, 0x61, 0x6c, 0x69, 0x63, // "Italic"
    ];

    // "Oblique"
    let data_oblique = [
        0xa7, 0x4f, 0x62, 0x6c, 0x69, 0x71, 0x75, 0x65, // "Oblique"
    ];

    let s_italic: FontStyle = from_slice(&data_italic)?;
    assert_eq!(s_italic, FontStyle::Italic);

    let s_oblique: FontStyle = from_slice(&data_oblique)?;
    assert_eq!(s_oblique, FontStyle::Oblique);

    Ok(())
}

#[test]
fn msgpack_deserialize_tuple_variant() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    enum Point {
        X(u64),
        Y(String, bool),
    }

    // { "X": 123 }
    let data_x = [
        0x81, // Map with 1 element
        0xa1, 0x58, // "X"
        0x7b, // 123 (positive fixint)
    ];

    // { "Y": ["hello", true] }
    let data_y = [
        0x81, // Map with 1 element
        0xa1, 0x59, // "Y"
        0x92, // Array with 2 elements
        0xa5, 0x68, 0x65, 0x6c, 0x6c, 0x6f, // "hello"
        0xc3, // true
    ];

    let p_x: Point = from_slice(&data_x)?;
    assert_eq!(p_x, Point::X(123));

    let p_y: Point = from_slice(&data_y)?;
    assert_eq!(p_y, Point::Y("hello".to_string(), true));

    Ok(())
}

#[test]
fn msgpack_deserialize_struct_variant() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum Point {
        Thing,
        Well { made: String, i: bool, guess: i32 },
        Other(i32),
    }

    // { "Well": { "made": "in germany", "i": false, "guess": 3 } }
    let data = [
        0x81, // Map with 1 element
        0xa4, 0x57, 0x65, 0x6c, 0x6c, // "Well"
        0x83, // Map with 3 elements
        0xa4, 0x6d, 0x61, 0x64, 0x65, // "made"
        0xaa, 0x69, 0x6e, 0x20, 0x67, 0x65, 0x72, 0x6d, 0x61, 0x6e, 0x79, // "in germany"
        0xa1, 0x69, // "i"
        0xc2, // false
        0xa5, 0x67, 0x75, 0x65, 0x73, 0x73, // "guess"
        0x03, // 3 (positive fixint)
    ];

    let point: Point = from_slice(&data)?;
    assert_eq!(
        point,
        Point::Well {
            made: "in germany".to_string(),
            i: false,
            guess: 3
        }
    );

    Ok(())
}
