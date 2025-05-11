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

#[test]
fn json_read_float_extremes() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Facet)]
    struct FloatExtremes {
        max: f64,
        min: f64,
        very_small: f64,
        very_large: f64,
        special: f64,
    }

    let json = r#"{
        "max": 1.7976931348623157e+308,
        "min": 4.9e-324,
        "very_small": 1.0e-100,
        "very_large": 1.0e+100,
        "special": 1.234567890123456789
    }"#;

    let values: FloatExtremes = from_str(json)?;

    // Different parsers may have slightly different precision
    // especially for extreme values, so test approximately
    assert!(f64::abs(values.max - f64::MAX) < f64::EPSILON * 100.0);

    // MIN_POSITIVE is tricky, as some parsers round differently
    // The lexical-parse library claims to parse "4.9e-324" as 5e-324 which is close enough
    let min_positive_ratio = values.min / f64::MIN_POSITIVE;
    assert!(
        min_positive_ratio > 0.9 && min_positive_ratio < 3.0,
        "min value {} should be close to MIN_POSITIVE {}",
        values.min,
        f64::MIN_POSITIVE
    );

    assert_eq!(values.very_small, 1.0e-100);
    assert_eq!(values.very_large, 1.0e+100);

    // High-precision floating point values may differ between parsers
    // so we test approximately
    let expected_special = 1.234567890123456789;
    assert!(f64::abs(values.special - expected_special) < f64::EPSILON * 100.0);

    Ok(())
}
