use eyre::Result;
use facet::Facet;
use facet_msgpack::from_slice;

#[test]
fn msgpack_deserialize_more_types() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Facet)]
    struct TestStructWithMoreTypes {
        u8_val: u8,
        u16_val: u16,
        i8_val: i8,
        i16_val: i16,
        u32_val: u32,
        i32_val: i32,
        u64_val: u64,
        i64_val: i64,
        f32_val: f32,
        f64_val: f64,
    }

    // MessagePack encoded data representing the struct with all the numeric values
    let data = [
        0x8a, // Map with 10 elements
        // u8_val: 255
        0xa6, 0x75, 0x38, 0x5f, 0x76, 0x61, 0x6c, // "u8_val"
        0xcc, 0xff, // unsigned 8-bit int (255)
        // u16_val: 65535
        0xa8, 0x75, 0x31, 0x36, 0x5f, 0x76, 0x61, 0x6c, // "u16_val"
        0xcd, 0xff, 0xff, // unsigned 16-bit int (65535)
        // i8_val: -128
        0xa7, 0x69, 0x38, 0x5f, 0x76, 0x61, 0x6c, // "i8_val"
        0xd0, 0x80, // signed 8-bit int (-128)
        // i16_val: -32768
        0xa8, 0x69, 0x31, 0x36, 0x5f, 0x76, 0x61, 0x6c, // "i16_val"
        0xd1, 0x80, 0x00, // signed 16-bit int (-32768)
        // u32_val: 4294967295
        0xa8, 0x75, 0x33, 0x32, 0x5f, 0x76, 0x61, 0x6c, // "u32_val"
        0xce, 0xff, 0xff, 0xff, 0xff, // unsigned 32-bit int (4294967295)
        // i32_val: -2147483648
        0xa8, 0x69, 0x33, 0x32, 0x5f, 0x76, 0x61, 0x6c, // "i32_val"
        0xd2, 0x80, 0x00, 0x00, 0x00, // signed 32-bit int (-2147483648)
        // u64_val: 18446744073709551615
        0xa8, 0x75, 0x36, 0x34, 0x5f, 0x76, 0x61, 0x6c, // "u64_val"
        0xcf, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
        0xff, // unsigned 64-bit int (18446744073709551615)
        // i64_val: -9223372036854775808
        0xa8, 0x69, 0x36, 0x34, 0x5f, 0x76, 0x61, 0x6c, // "i64_val"
        0xd3, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, // signed 64-bit int (-9223372036854775808)
        // f32_val: 3.14...
        0xa8, 0x66, 0x33, 0x32, 0x5f, 0x76, 0x61, 0x6c, // "f32_val"
        0xca, 0x40, 0x49, 0x0f, 0xdb, // float 32 (approx. PI)
        // f64_val: 3.14...
        0xa8, 0x66, 0x36, 0x34, 0x5f, 0x76, 0x61, 0x6c, // "f64_val"
        0xcb, 0x40, 0x09, 0x21, 0xfb, 0x54, 0x44, 0x2d, 0x18, // float 64 (more precise PI)
    ];

    let test_struct: TestStructWithMoreTypes = from_slice(&data)?;

    assert_eq!(test_struct.u8_val, 255);
    assert_eq!(test_struct.u16_val, 65535);
    assert_eq!(test_struct.i8_val, -128);
    assert_eq!(test_struct.i16_val, -32768);
    assert_eq!(test_struct.u32_val, 4294967295);
    assert_eq!(test_struct.i32_val, -2147483648);
    assert_eq!(test_struct.u64_val, 18446744073709551615);
    assert_eq!(test_struct.i64_val, -9223372036854775808);
    assert!((test_struct.f32_val - std::f32::consts::PI).abs() < f32::EPSILON);
    assert!((test_struct.f64_val - std::f64::consts::PI).abs() < f64::EPSILON);

    Ok(())
}
