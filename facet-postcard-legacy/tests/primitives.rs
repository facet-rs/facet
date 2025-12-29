//! Tests for all primitive types

use eyre::Result;
use facet::Facet;
use facet_postcard_legacy::{from_slice, to_vec};
use postcard::from_bytes as postcard_from_slice;
use postcard::to_allocvec as postcard_to_vec;
use serde::{Deserialize, Serialize};

// Helper macro to test a primitive type
macro_rules! test_primitive {
    ($name:ident, $ty:ty, $values:expr) => {
        mod $name {
            use super::*;

            #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
            struct Wrapper {
                value: $ty,
            }

            #[test]
            fn test_serialization_matches_postcard() -> Result<()> {
                facet_testhelpers::setup();
                for &value in &$values {
                    let wrapper = Wrapper { value };
                    let facet_bytes = to_vec(&wrapper)?;
                    let postcard_bytes = postcard_to_vec(&wrapper)?;
                    assert_eq!(
                        facet_bytes, postcard_bytes,
                        "Serialization mismatch for value {:?}",
                        value
                    );
                }
                Ok(())
            }

            #[test]
            fn test_roundtrip() -> Result<()> {
                facet_testhelpers::setup();
                for &value in &$values {
                    let wrapper = Wrapper { value };
                    let bytes = to_vec(&wrapper)?;
                    let decoded: Wrapper = from_slice(&bytes)?;
                    assert_eq!(wrapper, decoded, "Roundtrip failed for value {:?}", value);
                }
                Ok(())
            }

            #[test]
            fn test_cross_compatibility() -> Result<()> {
                facet_testhelpers::setup();
                for &value in &$values {
                    let wrapper = Wrapper { value };

                    // facet -> postcard
                    let facet_bytes = to_vec(&wrapper)?;
                    let decoded: Wrapper = postcard_from_slice(&facet_bytes)?;
                    assert_eq!(wrapper, decoded, "facet->postcard failed for {:?}", value);

                    // postcard -> facet
                    let postcard_bytes = postcard_to_vec(&wrapper)?;
                    let decoded: Wrapper = from_slice(&postcard_bytes)?;
                    assert_eq!(wrapper, decoded, "postcard->facet failed for {:?}", value);
                }
                Ok(())
            }
        }
    };
}

// Test u8
test_primitive!(u8_tests, u8, [0u8, 1, 127, 128, 255, u8::MIN, u8::MAX]);

// Test u16
test_primitive!(
    u16_tests,
    u16,
    [0u16, 1, 127, 128, 255, 256, 1000, 65535, u16::MIN, u16::MAX]
);

// Test u32
test_primitive!(
    u32_tests,
    u32,
    [
        0u32,
        1,
        127,
        128,
        255,
        256,
        65535,
        65536,
        100_000,
        1_000_000,
        u32::MAX
    ]
);

// Test u64
test_primitive!(
    u64_tests,
    u64,
    [
        0u64,
        1,
        127,
        128,
        255,
        256,
        65535,
        65536,
        u32::MAX as u64,
        u32::MAX as u64 + 1,
        u64::MAX
    ]
);

// Test u128
test_primitive!(
    u128_tests,
    u128,
    [
        0u128,
        1,
        127,
        128,
        u64::MAX as u128,
        u64::MAX as u128 + 1,
        u128::MAX
    ]
);

// Test i8
test_primitive!(i8_tests, i8, [0i8, 1, -1, 127, -128, i8::MIN, i8::MAX]);

// Test i16
test_primitive!(
    i16_tests,
    i16,
    [0i16, 1, -1, 127, -128, 1000, -1000, i16::MIN, i16::MAX]
);

// Test i32
test_primitive!(
    i32_tests,
    i32,
    [
        0i32,
        1,
        -1,
        127,
        -128,
        1000,
        -1000,
        100_000,
        -100_000,
        i32::MIN,
        i32::MAX
    ]
);

// Test i64
test_primitive!(
    i64_tests,
    i64,
    [
        0i64,
        1,
        -1,
        i32::MIN as i64,
        i32::MAX as i64,
        i64::MIN,
        i64::MAX
    ]
);

// Test i128
test_primitive!(
    i128_tests,
    i128,
    [
        0i128,
        1,
        -1,
        i64::MIN as i128,
        i64::MAX as i128,
        i128::MIN,
        i128::MAX
    ]
);

// Test usize (platform dependent, but should work)
test_primitive!(usize_tests, usize, [0usize, 1, 127, 128, 1000, usize::MAX]);

// Test isize
test_primitive!(isize_tests, isize, [0isize, 1, -1, 1000, -1000]);

// Test f32
test_primitive!(
    f32_tests,
    f32,
    [
        0.0f32,
        1.0,
        -1.0,
        1.5,
        -2.5,
        f32::MIN,
        f32::MAX,
        f32::EPSILON,
        f32::MIN_POSITIVE
    ]
);

// Test f64
test_primitive!(
    f64_tests,
    f64,
    [
        0.0f64,
        1.0,
        -1.0,
        1.23456789012345,
        -9.87654321098765,
        f64::MIN,
        f64::MAX,
        f64::EPSILON,
        f64::MIN_POSITIVE
    ]
);

// Test bool
test_primitive!(bool_tests, bool, [true, false]);

// Test char
test_primitive!(
    char_tests,
    char,
    [
        'a', 'Z', '0', ' ', '\n', '\t', '🦀', '日', '本', '\u{0}', '\u{FFFF}'
    ]
);

// Test unit type
mod unit_tests {
    use super::*;

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct UnitWrapper {
        value: (),
    }

    #[test]
    fn test_unit_serialization() -> Result<()> {
        facet_testhelpers::setup();
        let wrapper = UnitWrapper { value: () };
        let facet_bytes = to_vec(&wrapper)?;
        let postcard_bytes = postcard_to_vec(&wrapper)?;
        assert_eq!(facet_bytes, postcard_bytes);
        Ok(())
    }

    #[test]
    fn test_unit_roundtrip() -> Result<()> {
        facet_testhelpers::setup();
        let wrapper = UnitWrapper { value: () };
        let bytes = to_vec(&wrapper)?;
        let decoded: UnitWrapper = from_slice(&bytes)?;
        assert_eq!(wrapper, decoded);
        Ok(())
    }
}

// Test special float values
mod special_floats {
    use super::*;

    #[derive(Debug, Facet, Serialize, Deserialize)]
    struct F32Wrapper {
        value: f32,
    }

    #[derive(Debug, Facet, Serialize, Deserialize)]
    struct F64Wrapper {
        value: f64,
    }

    #[test]
    fn test_f32_infinity() -> Result<()> {
        facet_testhelpers::setup();
        let wrapper = F32Wrapper {
            value: f32::INFINITY,
        };
        let facet_bytes = to_vec(&wrapper)?;
        let postcard_bytes = postcard_to_vec(&wrapper)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: F32Wrapper = from_slice(&facet_bytes)?;
        assert!(decoded.value.is_infinite() && decoded.value.is_sign_positive());
        Ok(())
    }

    #[test]
    fn test_f32_neg_infinity() -> Result<()> {
        facet_testhelpers::setup();
        let wrapper = F32Wrapper {
            value: f32::NEG_INFINITY,
        };
        let facet_bytes = to_vec(&wrapper)?;
        let postcard_bytes = postcard_to_vec(&wrapper)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: F32Wrapper = from_slice(&facet_bytes)?;
        assert!(decoded.value.is_infinite() && decoded.value.is_sign_negative());
        Ok(())
    }

    #[test]
    fn test_f32_nan() -> Result<()> {
        facet_testhelpers::setup();
        let wrapper = F32Wrapper { value: f32::NAN };
        let facet_bytes = to_vec(&wrapper)?;
        let postcard_bytes = postcard_to_vec(&wrapper)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: F32Wrapper = from_slice(&facet_bytes)?;
        assert!(decoded.value.is_nan());
        Ok(())
    }

    #[test]
    fn test_f64_infinity() -> Result<()> {
        facet_testhelpers::setup();
        let wrapper = F64Wrapper {
            value: f64::INFINITY,
        };
        let facet_bytes = to_vec(&wrapper)?;
        let postcard_bytes = postcard_to_vec(&wrapper)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: F64Wrapper = from_slice(&facet_bytes)?;
        assert!(decoded.value.is_infinite() && decoded.value.is_sign_positive());
        Ok(())
    }

    #[test]
    fn test_f64_neg_infinity() -> Result<()> {
        facet_testhelpers::setup();
        let wrapper = F64Wrapper {
            value: f64::NEG_INFINITY,
        };
        let facet_bytes = to_vec(&wrapper)?;
        let postcard_bytes = postcard_to_vec(&wrapper)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: F64Wrapper = from_slice(&facet_bytes)?;
        assert!(decoded.value.is_infinite() && decoded.value.is_sign_negative());
        Ok(())
    }

    #[test]
    fn test_f64_nan() -> Result<()> {
        facet_testhelpers::setup();
        let wrapper = F64Wrapper { value: f64::NAN };
        let facet_bytes = to_vec(&wrapper)?;
        let postcard_bytes = postcard_to_vec(&wrapper)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: F64Wrapper = from_slice(&facet_bytes)?;
        assert!(decoded.value.is_nan());
        Ok(())
    }
}
