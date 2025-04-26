use eyre::Result;
use facet::Facet;
use facet_json::from_str;

#[test]
fn json_read_more_types() -> Result<()> {
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

    let json = r#"{
        "u8_val": 255,
        "u16_val": 65535,
        "i8_val": -128,
        "i16_val": -32768,
        "u32_val": 4294967295,
        "i32_val": -2147483648,
        "u64_val": 18446744073709551615,
        "i64_val": -9223372036854775808,
        "f32_val": 3.141592653589793,
        "f64_val": 3.141592653589793
    }"#;

    let test_struct: TestStructWithMoreTypes = from_str(json)?;

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
