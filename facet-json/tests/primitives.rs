use facet::Facet;
use facet_json::from_str;
use facet_json::to_string;
use facet_testhelpers::test;
use std::collections::HashMap;
use std::f64::consts::PI;
use std::num::NonZero;

#[test]
fn json_read_empty_object_for_struct() {
    #[derive(Facet)]
    struct TestStruct {}
    let json = r#"{}"#;

    let _: TestStruct = from_str(json).unwrap();
}

#[test]
fn json_read_empty_object_for_hashmap() {
    let json = r#"{}"#;

    let _: HashMap<String, String> = from_str(json).unwrap();
}

#[test]
fn test_str_escaped() {
    #[derive(Facet, Debug)]
    struct S {
        foo: String,
    }

    let json_ok = r#"{"foo":"\"\\abc"}"#;
    let ok: S = from_str(json_ok).unwrap();
    assert_eq!(ok.foo, "\"\\abc");
}

#[test]
fn test_bool_serialization() {
    #[derive(Debug, Facet)]
    struct BoolStruct {
        flag: bool,
    }

    let true_value = BoolStruct { flag: true };
    let false_value = BoolStruct { flag: false };

    assert_eq!(to_string(&true_value), r#"{"flag":true}"#);
    assert_eq!(to_string(&false_value), r#"{"flag":false}"#);
}

#[test]
fn test_integer_types_serialization() {
    #[derive(Debug, Facet)]
    struct IntegerTypes {
        u8_val: u8,
        u16_val: u16,
        u32_val: u32,
        u64_val: u64,
        usize_val: usize,
        i8_val: i8,
        i16_val: i16,
        i64_val: i64,
        isize_val: isize,
    }

    let test_struct = IntegerTypes {
        u8_val: 255,
        u16_val: 65535,
        u32_val: 4294967295,
        u64_val: 18446744073709551615,
        usize_val: 12345,
        i8_val: -128,
        i16_val: -32768,
        i64_val: -9223372036854775808,
        isize_val: -54321,
    };

    let json = to_string(&test_struct);

    // Verify each integer type is serialized correctly
    assert!(json.contains(r#""u8_val":255"#));
    assert!(json.contains(r#""u16_val":65535"#));
    assert!(json.contains(r#""u32_val":4294967295"#));
    assert!(json.contains(r#""u64_val":18446744073709551615"#));
    assert!(json.contains(r#""usize_val":12345"#));
    assert!(json.contains(r#""i8_val":-128"#));
    assert!(json.contains(r#""i16_val":-32768"#));
    assert!(json.contains(r#""i64_val":-9223372036854775808"#));
    assert!(json.contains(r#""isize_val":-54321"#));
}

#[test]
fn test_nonzero_types_serialization() {
    #[derive(Debug, Facet)]
    struct NonZeroTypes {
        u16_val: NonZero<u16>,
        u32_val: NonZero<u32>,
        u64_val: NonZero<u64>,
        usize_val: NonZero<usize>,
        i8_val: NonZero<i8>,
        i16_val: NonZero<i16>,
        i32_val: NonZero<i32>,
        i64_val: NonZero<i64>,
        isize_val: NonZero<isize>,
    }

    let test_struct = NonZeroTypes {
        u16_val: NonZero::new(65535).unwrap(),
        u32_val: NonZero::new(4294967295).unwrap(),
        u64_val: NonZero::new(18446744073709551615).unwrap(),
        usize_val: NonZero::new(12345).unwrap(),
        i8_val: NonZero::new(127).unwrap(),
        i16_val: NonZero::new(32767).unwrap(),
        i32_val: NonZero::new(2147483647).unwrap(),
        i64_val: NonZero::new(9223372036854775807).unwrap(),
        isize_val: NonZero::new(54321).unwrap(),
    };

    let json = to_string(&test_struct);

    // Verify each NonZero type is serialized correctly
    assert!(json.contains(r#""u16_val":65535"#));
    assert!(json.contains(r#""u32_val":4294967295"#));
    assert!(json.contains(r#""u64_val":18446744073709551615"#));
    assert!(json.contains(r#""usize_val":12345"#));
    assert!(json.contains(r#""i8_val":127"#));
    assert!(json.contains(r#""i16_val":32767"#));
    assert!(json.contains(r#""i32_val":2147483647"#));
    assert!(json.contains(r#""i64_val":9223372036854775807"#));
    assert!(json.contains(r#""isize_val":54321"#));
}

#[test]
fn test_f64_serialization() {
    #[derive(Debug, Facet)]
    struct F64Struct {
        value: f64,
    }

    let test_struct = F64Struct { value: PI };
    let json = to_string(&test_struct);
    assert!(json.contains(r#""value":3.14159265358979"#));
}

#[test]
fn test_string_escaping() {
    #[derive(Debug, Facet)]
    struct EscapeTestStruct {
        quote: String,
        backslash: String,
        newline: String,
        carriage: String,
        tab: String,
        backspace: String,
        formfeed: String,
        control: String,
    }

    let test_struct = EscapeTestStruct {
        quote: "Contains \"quotes\"".to_string(),
        backslash: "Contains \\backslashes\\".to_string(),
        newline: "Contains\nnewlines".to_string(),
        carriage: "Contains\rreturns".to_string(),
        tab: "Contains\ttabs".to_string(),
        backspace: "Contains\u{08}backspace".to_string(),
        formfeed: "Contains\u{0C}formfeed".to_string(),
        control: "Contains\u{01}control".to_string(),
    };

    let json = to_string(&test_struct);

    assert!(json.contains(r#""quote":"Contains \"quotes\""#));
    assert!(json.contains(r#""backslash":"Contains \\backslashes\\"#));
    assert!(json.contains(r#""newline":"Contains\nnewlines"#));
    assert!(json.contains(r#""carriage":"Contains\rreturns"#));
    assert!(json.contains(r#""tab":"Contains\ttabs"#));
    assert!(json.contains(r#""backspace":"Contains\bbackspace"#));
    assert!(json.contains(r#""formfeed":"Contains\fformfeed"#));
    assert!(json.contains(r#""control":"Contains\u0001control"#));
}
