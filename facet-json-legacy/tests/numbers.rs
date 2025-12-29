use facet::Facet;
use facet_json_legacy::from_str;
use facet_testhelpers::test;

#[test]
fn json_read_more_types() {
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
        u128_val: u128,
        i128_val: i128,
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
        "u128_val": 340282366920938463463374607431768211455,
        "i128_val": -170141183460469231731687303715884105728,
        "f32_val": 3.141592653589793,
        "f64_val": 3.141592653589793
    }"#;

    let test_struct: TestStructWithMoreTypes = from_str(json).unwrap();

    assert_eq!(test_struct.u8_val, 255);
    assert_eq!(test_struct.u16_val, 65535);
    assert_eq!(test_struct.i8_val, -128);
    assert_eq!(test_struct.i16_val, -32768);
    assert_eq!(test_struct.u32_val, 4294967295);
    assert_eq!(test_struct.i32_val, -2147483648);
    assert_eq!(test_struct.u64_val, 18446744073709551615);
    assert_eq!(test_struct.i64_val, -9223372036854775808);
    assert_eq!(
        test_struct.u128_val,
        340282366920938463463374607431768211455
    );
    assert_eq!(
        test_struct.i128_val,
        -170141183460469231731687303715884105728
    );
    assert!((test_struct.f32_val - std::f32::consts::PI).abs() < f32::EPSILON);
    assert!((test_struct.f64_val - std::f64::consts::PI).abs() < f64::EPSILON);
}

#[test]
fn json_read_usize_isize() {
    #[derive(Facet, Debug, PartialEq)]
    struct TestStructWithSize {
        usize_val: usize,
        isize_val: isize,
    }

    let json = r#"{"usize_val": 42, "isize_val": -17}"#;
    let test_struct: TestStructWithSize = from_str(json).unwrap();

    assert_eq!(test_struct.usize_val, 42);
    assert_eq!(test_struct.isize_val, -17);
}

#[test]
fn json_roundtrip_usize_isize() {
    #[derive(Facet, Debug, PartialEq)]
    struct TestStructWithSize {
        usize_val: usize,
        isize_val: isize,
    }

    let original = TestStructWithSize {
        usize_val: 12345,
        isize_val: -6789,
    };

    let json = facet_json_legacy::to_string(&original);
    let deserialized: TestStructWithSize = from_str(&json).unwrap();

    assert_eq!(original, deserialized);
}
