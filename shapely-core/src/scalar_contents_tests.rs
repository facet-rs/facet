use crate::{Bytes, ScalarContents, Shapely};
use std::f32::consts::PI as PI_F32;
use std::f64::consts::E as E_F64;

/// Macro to define tests for scalar types
macro_rules! test_scalar_type {
    ($test_name:ident, $type:ty, $value:expr, $expected:expr) => {
        #[test]
        fn $test_name() {
            // Create a value
            let v: $type = $value;

            // Get a pointer to the value
            let v_addr: *const u8 = &v as *const $type as *const u8;

            // Get the shape for the type
            let v_shape = <$type>::shape();

            // Get the scalar contents (this is unsafe)
            let scalar_contents = unsafe { v_shape.get_scalar_contents(v_addr) };

            // Verify the contents
            assert_eq!(scalar_contents, $expected);
        }
    };
}

// Integer types
test_scalar_type!(test_i8_scalar_contents, i8, -42, ScalarContents::I8(-42));
test_scalar_type!(
    test_i16_scalar_contents,
    i16,
    -1000,
    ScalarContents::I16(-1000)
);
test_scalar_type!(test_i32_scalar_contents, i32, 23, ScalarContents::I32(23));
test_scalar_type!(
    test_i64_scalar_contents,
    i64,
    -1_000_000,
    ScalarContents::I64(-1_000_000)
);
test_scalar_type!(
    test_i128_scalar_contents,
    i128,
    123_456_789,
    ScalarContents::I128(123_456_789)
);

// Unsigned integer types
test_scalar_type!(test_u8_scalar_contents, u8, 255, ScalarContents::U8(255));
test_scalar_type!(
    test_u16_scalar_contents,
    u16,
    65535,
    ScalarContents::U16(65535)
);
test_scalar_type!(
    test_u32_scalar_contents,
    u32,
    4_294_967_295,
    ScalarContents::U32(4_294_967_295)
);
test_scalar_type!(
    test_u64_scalar_contents,
    u64,
    18_446_744_073_709_551_615,
    ScalarContents::U64(18_446_744_073_709_551_615)
);

// Floating point types
test_scalar_type!(
    test_f32_scalar_contents,
    f32,
    PI_F32,
    ScalarContents::F32(PI_F32)
);
test_scalar_type!(
    test_f64_scalar_contents,
    f64,
    E_F64,
    ScalarContents::F64(E_F64)
);

// Boolean type
test_scalar_type!(
    test_bool_scalar_contents,
    bool,
    true,
    ScalarContents::Boolean(true)
);

// Nothing type (unit)
test_scalar_type!(
    test_nothing_scalar_contents,
    (),
    (),
    ScalarContents::Nothing
);

// String and Bytes types need special handling due to Cow
#[test]
fn test_string_scalar_contents() {
    let v = String::from("hello");
    let v_addr: *const u8 = &v as *const String as *const u8;
    let v_shape = String::shape();
    let scalar_contents = unsafe { v_shape.get_scalar_contents(v_addr) };

    // We can't directly compare with ScalarContents::String because of Cow, so we match and compare the string content
    match scalar_contents {
        ScalarContents::String(s) => assert_eq!(s, "hello"),
        _ => panic!("Expected String contents"),
    }
}

#[test]
fn test_bytes_scalar_contents() {
    let v = Bytes(vec![72, 101, 108, 108, 111]); // "Hello" in ASCII
    let v_addr: *const u8 = &v as *const Bytes as *const u8;
    let v_shape = Bytes::shape();
    let scalar_contents = unsafe { v_shape.get_scalar_contents(v_addr) };

    // We can't directly compare with ScalarContents::Bytes because of Cow, so we match and compare the bytes content
    match scalar_contents {
        ScalarContents::Bytes(b) => assert_eq!(b.as_ref(), &[72, 101, 108, 108, 111]),
        _ => panic!("Expected Bytes contents"),
    }
}
