//! Comprehensive round-trip tests for facet-postcard.
//!
//! These tests verify that values can be serialized with facet-postcard
//! and then deserialized back to the same value using facet assertion helpers.
//!
//! Each test follows this pattern:
//! 1. Create a value
//! 2. Serialize it with `to_vec()`
//! 3. Deserialize it with `from_slice()`
//! 4. Assert the deserialized value equals the original

#![cfg(feature = "jit")]

use facet::Facet;
use facet_postcard::{from_slice, to_vec};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

/// Helper macro to test round-trip serialization/deserialization.
///
/// Creates a test that:
/// 1. Serializes the value with facet-postcard
/// 2. Deserializes it back with facet-postcard
/// 3. Asserts they're equal
macro_rules! test_roundtrip {
    ($name:ident, $ty:ty, $value:expr) => {
        #[test]
        fn $name() {
            facet_testhelpers::setup();

            let original: $ty = $value;

            // Serialize with facet-postcard
            let bytes = to_vec(&original).expect("serialization should succeed");

            // Deserialize with facet-postcard
            let deserialized: $ty = from_slice(&bytes).expect("deserialization should succeed");

            // Assert equality
            assert_eq!(
                deserialized, original,
                "round-trip failed: deserialized value doesn't match original"
            );
        }
    };
}

// =============================================================================
// Primitive Types
// =============================================================================

mod primitives {
    use super::*;

    // Unit type
    test_roundtrip!(unit_type, (), ());

    // Boolean
    test_roundtrip!(bool_true, bool, true);
    test_roundtrip!(bool_false, bool, false);

    // Unsigned integers
    test_roundtrip!(u8_zero, u8, 0);
    test_roundtrip!(u8_max, u8, u8::MAX);
    test_roundtrip!(u8_mid, u8, 128);

    test_roundtrip!(u16_zero, u16, 0);
    test_roundtrip!(u16_max, u16, u16::MAX);
    test_roundtrip!(u16_boundary, u16, 256);

    test_roundtrip!(u32_zero, u32, 0);
    test_roundtrip!(u32_max, u32, u32::MAX);
    test_roundtrip!(u32_large, u32, 1_000_000);

    test_roundtrip!(u64_zero, u64, 0);
    test_roundtrip!(u64_max, u64, u64::MAX);
    test_roundtrip!(u64_large, u64, u64::MAX / 2);

    // u128
    test_roundtrip!(u128_zero, u128, 0);
    test_roundtrip!(u128_max, u128, u128::MAX);
    test_roundtrip!(u128_large, u128, u128::MAX / 2);

    // usize
    test_roundtrip!(usize_zero, usize, 0);
    test_roundtrip!(usize_max, usize, usize::MAX);
    test_roundtrip!(usize_mid, usize, usize::MAX / 2);

    // Signed integers
    test_roundtrip!(i8_zero, i8, 0);
    test_roundtrip!(i8_positive, i8, i8::MAX);
    test_roundtrip!(i8_negative, i8, i8::MIN);

    test_roundtrip!(i16_zero, i16, 0);
    test_roundtrip!(i16_positive, i16, i16::MAX);
    test_roundtrip!(i16_negative, i16, i16::MIN);

    test_roundtrip!(i32_zero, i32, 0);
    test_roundtrip!(i32_positive, i32, i32::MAX);
    test_roundtrip!(i32_negative, i32, i32::MIN);

    test_roundtrip!(i64_zero, i64, 0);
    test_roundtrip!(i64_positive, i64, i64::MAX);
    test_roundtrip!(i64_negative, i64, i64::MIN);

    // i128
    test_roundtrip!(i128_zero, i128, 0);
    test_roundtrip!(i128_max, i128, i128::MAX);
    test_roundtrip!(i128_min, i128, i128::MIN);

    // isize
    test_roundtrip!(isize_zero, isize, 0);
    test_roundtrip!(isize_max, isize, isize::MAX);
    test_roundtrip!(isize_min, isize, isize::MIN);

    // Floating point
    test_roundtrip!(f32_zero, f32, 0.0);
    test_roundtrip!(f32_positive, f32, std::f32::consts::PI);
    test_roundtrip!(f32_negative, f32, -std::f32::consts::E);
    test_roundtrip!(f32_infinity, f32, f32::INFINITY);
    test_roundtrip!(f32_neg_infinity, f32, f32::NEG_INFINITY);

    test_roundtrip!(f64_zero, f64, 0.0);
    test_roundtrip!(f64_positive, f64, std::f64::consts::PI);
    test_roundtrip!(f64_negative, f64, -std::f64::consts::E);
    test_roundtrip!(f64_infinity, f64, f64::INFINITY);
    test_roundtrip!(f64_neg_infinity, f64, f64::NEG_INFINITY);

    // char
    test_roundtrip!(char_ascii, char, 'a');
    test_roundtrip!(char_unicode, char, '日');
    test_roundtrip!(char_emoji, char, '🦀');
    test_roundtrip!(char_null, char, '\0');
    test_roundtrip!(char_max, char, char::MAX);
}

// =============================================================================
// NonZero Types
// =============================================================================

mod nonzero {
    use super::*;
    use std::num::{
        NonZeroI8, NonZeroI16, NonZeroI32, NonZeroI64, NonZeroI128, NonZeroIsize, NonZeroU8,
        NonZeroU16, NonZeroU32, NonZeroU64, NonZeroU128, NonZeroUsize,
    };

    // Unsigned NonZero
    test_roundtrip!(nonzero_u8_one, NonZeroU8, NonZeroU8::new(1).unwrap());
    test_roundtrip!(nonzero_u8_max, NonZeroU8, NonZeroU8::new(u8::MAX).unwrap());

    test_roundtrip!(nonzero_u16_one, NonZeroU16, NonZeroU16::new(1).unwrap());
    test_roundtrip!(
        nonzero_u16_max,
        NonZeroU16,
        NonZeroU16::new(u16::MAX).unwrap()
    );

    test_roundtrip!(nonzero_u32_one, NonZeroU32, NonZeroU32::new(1).unwrap());
    test_roundtrip!(
        nonzero_u32_max,
        NonZeroU32,
        NonZeroU32::new(u32::MAX).unwrap()
    );

    test_roundtrip!(nonzero_u64_one, NonZeroU64, NonZeroU64::new(1).unwrap());
    test_roundtrip!(
        nonzero_u64_max,
        NonZeroU64,
        NonZeroU64::new(u64::MAX).unwrap()
    );

    test_roundtrip!(nonzero_u128_one, NonZeroU128, NonZeroU128::new(1).unwrap());
    test_roundtrip!(
        nonzero_u128_max,
        NonZeroU128,
        NonZeroU128::new(u128::MAX).unwrap()
    );

    test_roundtrip!(
        nonzero_usize_one,
        NonZeroUsize,
        NonZeroUsize::new(1).unwrap()
    );
    test_roundtrip!(
        nonzero_usize_max,
        NonZeroUsize,
        NonZeroUsize::new(usize::MAX).unwrap()
    );

    // Signed NonZero
    test_roundtrip!(nonzero_i8_one, NonZeroI8, NonZeroI8::new(1).unwrap());
    test_roundtrip!(nonzero_i8_neg_one, NonZeroI8, NonZeroI8::new(-1).unwrap());
    test_roundtrip!(nonzero_i8_max, NonZeroI8, NonZeroI8::new(i8::MAX).unwrap());
    test_roundtrip!(nonzero_i8_min, NonZeroI8, NonZeroI8::new(i8::MIN).unwrap());

    test_roundtrip!(nonzero_i16_one, NonZeroI16, NonZeroI16::new(1).unwrap());
    test_roundtrip!(
        nonzero_i16_max,
        NonZeroI16,
        NonZeroI16::new(i16::MAX).unwrap()
    );
    test_roundtrip!(
        nonzero_i16_min,
        NonZeroI16,
        NonZeroI16::new(i16::MIN).unwrap()
    );

    test_roundtrip!(nonzero_i32_one, NonZeroI32, NonZeroI32::new(1).unwrap());
    test_roundtrip!(
        nonzero_i32_max,
        NonZeroI32,
        NonZeroI32::new(i32::MAX).unwrap()
    );
    test_roundtrip!(
        nonzero_i32_min,
        NonZeroI32,
        NonZeroI32::new(i32::MIN).unwrap()
    );

    test_roundtrip!(nonzero_i64_one, NonZeroI64, NonZeroI64::new(1).unwrap());
    test_roundtrip!(
        nonzero_i64_max,
        NonZeroI64,
        NonZeroI64::new(i64::MAX).unwrap()
    );
    test_roundtrip!(
        nonzero_i64_min,
        NonZeroI64,
        NonZeroI64::new(i64::MIN).unwrap()
    );

    test_roundtrip!(nonzero_i128_one, NonZeroI128, NonZeroI128::new(1).unwrap());
    test_roundtrip!(
        nonzero_i128_max,
        NonZeroI128,
        NonZeroI128::new(i128::MAX).unwrap()
    );
    test_roundtrip!(
        nonzero_i128_min,
        NonZeroI128,
        NonZeroI128::new(i128::MIN).unwrap()
    );

    test_roundtrip!(
        nonzero_isize_one,
        NonZeroIsize,
        NonZeroIsize::new(1).unwrap()
    );
    test_roundtrip!(
        nonzero_isize_max,
        NonZeroIsize,
        NonZeroIsize::new(isize::MAX).unwrap()
    );
    test_roundtrip!(
        nonzero_isize_min,
        NonZeroIsize,
        NonZeroIsize::new(isize::MIN).unwrap()
    );

    // NonZero in structs
    #[derive(Debug, PartialEq, Facet)]
    struct WithNonZero {
        id: NonZeroU64,
        count: NonZeroU32,
    }

    test_roundtrip!(
        struct_with_nonzero,
        WithNonZero,
        WithNonZero {
            id: NonZeroU64::new(12345).unwrap(),
            count: NonZeroU32::new(42).unwrap(),
        }
    );

    // Option<NonZero> (niche optimization)
    test_roundtrip!(
        option_nonzero_u64_some,
        Option<NonZeroU64>,
        Some(NonZeroU64::new(42).unwrap())
    );
    test_roundtrip!(option_nonzero_u64_none, Option<NonZeroU64>, None);
}

// =============================================================================
// String and Byte Types
// =============================================================================

mod strings_and_bytes {
    use super::*;

    // String
    test_roundtrip!(string_empty, String, String::new());
    test_roundtrip!(string_ascii, String, "Hello, World!".to_string());
    test_roundtrip!(string_unicode, String, "こんにちは世界".to_string());
    test_roundtrip!(string_emoji, String, "🦀 Rust 🚀".to_string());
    test_roundtrip!(string_long, String, "a".repeat(10000));

    // Vec<u8> (byte arrays)
    test_roundtrip!(bytes_empty, Vec<u8>, vec![]);
    test_roundtrip!(bytes_single, Vec<u8>, vec![42]);
    test_roundtrip!(bytes_sequence, Vec<u8>, vec![0, 1, 2, 3, 4, 5]);
    test_roundtrip!(bytes_full_range, Vec<u8>, (0..=255).collect());

    // Box<str>
    test_roundtrip!(box_str_empty, Box<str>, "".into());
    test_roundtrip!(box_str_value, Box<str>, "boxed string".into());

    // Box<[u8]>
    test_roundtrip!(box_bytes_empty, Box<[u8]>, vec![].into_boxed_slice());
    test_roundtrip!(box_bytes_value, Box<[u8]>, vec![1, 2, 3].into_boxed_slice());
}

// =============================================================================
// Optimized String Types
// =============================================================================

#[cfg(feature = "compact_str")]
mod compact_str_tests {
    use super::*;
    use compact_str::CompactString;

    test_roundtrip!(compact_string_empty, CompactString, CompactString::new(""));
    test_roundtrip!(
        compact_string_ascii,
        CompactString,
        CompactString::from("Hello, World!")
    );
    test_roundtrip!(
        compact_string_unicode,
        CompactString,
        CompactString::from("こんにちは世界")
    );
    test_roundtrip!(
        compact_string_emoji,
        CompactString,
        CompactString::from("🦀 Rust 🚀")
    );
    test_roundtrip!(
        compact_string_long,
        CompactString,
        CompactString::from("a".repeat(10000))
    );
}

#[cfg(feature = "smartstring")]
mod smartstring_tests {
    use super::*;
    use smartstring::{LazyCompact, SmartString};

    test_roundtrip!(
        smartstring_empty,
        SmartString<LazyCompact>,
        SmartString::from("")
    );
    test_roundtrip!(
        smartstring_ascii,
        SmartString<LazyCompact>,
        SmartString::from("Hello, World!")
    );
    test_roundtrip!(
        smartstring_unicode,
        SmartString<LazyCompact>,
        SmartString::from("こんにちは世界")
    );
    test_roundtrip!(
        smartstring_emoji,
        SmartString<LazyCompact>,
        SmartString::from("🦀 Rust 🚀")
    );
    test_roundtrip!(
        smartstring_long,
        SmartString<LazyCompact>,
        SmartString::from("a".repeat(10000))
    );
}

#[cfg(feature = "bytestring")]
mod bytestring_tests {
    use super::*;
    use bytestring::ByteString;

    test_roundtrip!(bytestring_empty, ByteString, ByteString::from(""));
    test_roundtrip!(
        bytestring_ascii,
        ByteString,
        ByteString::from("Hello, World!")
    );
    test_roundtrip!(
        bytestring_unicode,
        ByteString,
        ByteString::from("こんにちは世界")
    );
    test_roundtrip!(bytestring_emoji, ByteString, ByteString::from("🦀 Rust 🚀"));
    test_roundtrip!(
        bytestring_long,
        ByteString,
        ByteString::from("a".repeat(10000))
    );
}

// =============================================================================
// Collection Types - Vec
// =============================================================================

mod collections_vec {
    use super::*;

    // Vec<T> for various T
    test_roundtrip!(vec_bool_empty, Vec<bool>, vec![]);
    test_roundtrip!(vec_bool_values, Vec<bool>, vec![true, false, true, false]);

    test_roundtrip!(vec_u32_empty, Vec<u32>, vec![]);
    test_roundtrip!(vec_u32_single, Vec<u32>, vec![42]);
    test_roundtrip!(vec_u32_multiple, Vec<u32>, vec![1, 2, 3, 4, 5]);
    test_roundtrip!(vec_u32_large, Vec<u32>, (0..1000).collect());

    test_roundtrip!(
        vec_i64_values,
        Vec<i64>,
        vec![-100, 0, 100, i64::MIN, i64::MAX]
    );

    test_roundtrip!(vec_f64_values, Vec<f64>, vec![1.1, 2.2, 3.3, -4.4]);

    test_roundtrip!(vec_string_empty, Vec<String>, vec![]);
    test_roundtrip!(
        vec_string_values,
        Vec<String>,
        vec!["hello".to_string(), "world".to_string(), "🦀".to_string()]
    );

    // Vec<char>
    test_roundtrip!(vec_char_empty, Vec<char>, vec![]);
    test_roundtrip!(vec_char_ascii, Vec<char>, vec!['a', 'b', 'c']);
    test_roundtrip!(
        vec_char_unicode,
        Vec<char>,
        vec!['こ', 'ん', 'に', 'ち', 'は']
    );
    test_roundtrip!(vec_char_emoji, Vec<char>, vec!['🦀', '🚀', '✨']);

    // Nested Vec
    test_roundtrip!(vec_vec_empty, Vec<Vec<u32>>, vec![]);
    test_roundtrip!(
        vec_vec_nested,
        Vec<Vec<u32>>,
        vec![vec![1, 2], vec![3, 4, 5], vec![]]
    );

    // Vec<()>
    test_roundtrip!(vec_unit_empty, Vec<()>, vec![]);
    test_roundtrip!(vec_unit_five, Vec<()>, vec![(), (), (), (), ()]);

    // Triple nested Vec
    test_roundtrip!(
        vec_vec_vec_u32,
        Vec<Vec<Vec<u32>>>,
        vec![vec![vec![1, 2], vec![3]], vec![vec![4, 5, 6]],]
    );
}

// =============================================================================
// Collection Types - HashMap/BTreeMap
// =============================================================================

mod collections_map {
    use super::*;

    // HashMap<String, T>
    test_roundtrip!(hashmap_empty, HashMap<String, u32>, HashMap::new());
    test_roundtrip!(
        hashmap_single,
        HashMap<String, u32>,
        [("key".to_string(), 42)].into_iter().collect()
    );
    test_roundtrip!(
        hashmap_multiple,
        HashMap<String, u32>,
        [
            ("one".to_string(), 1),
            ("two".to_string(), 2),
            ("three".to_string(), 3),
        ]
        .into_iter()
        .collect()
    );

    // BTreeMap<String, T>
    test_roundtrip!(btreemap_empty, BTreeMap<String, u32>, BTreeMap::new());
    test_roundtrip!(
        btreemap_single,
        BTreeMap<String, u32>,
        [("key".to_string(), 42)].into_iter().collect()
    );
    test_roundtrip!(
        btreemap_multiple,
        BTreeMap<String, u32>,
        [
            ("alpha".to_string(), 1),
            ("beta".to_string(), 2),
            ("gamma".to_string(), 3),
        ]
        .into_iter()
        .collect()
    );

    // HashMap with Vec<char> values (dodeca font analysis case)
    test_roundtrip!(
        hashmap_vec_char_empty,
        HashMap<String, Vec<char>>,
        HashMap::new()
    );
    test_roundtrip!(
        hashmap_vec_char_single,
        HashMap<String, Vec<char>>,
        [("font1".to_string(), vec!['a', 'b', 'c'])]
            .into_iter()
            .collect()
    );
    test_roundtrip!(
        hashmap_vec_char_multiple,
        HashMap<String, Vec<char>>,
        [
            ("Arial".to_string(), vec!['H', 'e', 'l', 'l', 'o']),
            ("Roboto".to_string(), vec!['W', 'o', 'r', 'l', 'd']),
        ]
        .into_iter()
        .collect()
    );

    // BTreeMap with Vec<char> values
    test_roundtrip!(
        btreemap_vec_char_single,
        BTreeMap<String, Vec<char>>,
        [("font1".to_string(), vec!['a', 'b', 'c'])]
            .into_iter()
            .collect()
    );

    // HashMap with Vec<u32> values
    test_roundtrip!(
        hashmap_vec_u32_values,
        HashMap<String, Vec<u32>>,
        [
            ("nums".to_string(), vec![1, 2, 3]),
            ("more".to_string(), vec![4, 5]),
        ]
        .into_iter()
        .collect()
    );

    // Maps with integer keys
    test_roundtrip!(
        hashmap_u32_keys,
        HashMap<u32, String>,
        [(1, "one".to_string()), (2, "two".to_string())]
            .into_iter()
            .collect()
    );
    test_roundtrip!(
        btreemap_i64_keys,
        BTreeMap<i64, String>,
        [(-1, "neg".to_string()), (0, "zero".to_string()), (1, "pos".to_string())]
            .into_iter()
            .collect()
    );

    // Nested maps
    test_roundtrip!(
        hashmap_nested,
        HashMap<String, HashMap<String, u32>>,
        [(
            "outer".to_string(),
            [("inner".to_string(), 42)].into_iter().collect()
        )]
        .into_iter()
        .collect()
    );
}

// =============================================================================
// Collection Types - HashSet/BTreeSet
// =============================================================================

mod collections_set {
    use super::*;

    // HashSet
    test_roundtrip!(hashset_empty, HashSet<String>, HashSet::new());
    test_roundtrip!(
        hashset_strings,
        HashSet<String>,
        ["a".to_string(), "b".to_string(), "c".to_string()]
            .into_iter()
            .collect()
    );
    test_roundtrip!(
        hashset_u32,
        HashSet<u32>,
        [1, 2, 3, 4, 5].into_iter().collect()
    );

    // BTreeSet
    test_roundtrip!(btreeset_empty, BTreeSet<String>, BTreeSet::new());
    test_roundtrip!(
        btreeset_strings,
        BTreeSet<String>,
        ["x".to_string(), "y".to_string(), "z".to_string()]
            .into_iter()
            .collect()
    );
    test_roundtrip!(
        btreeset_i32,
        BTreeSet<i32>,
        [-10, -5, 0, 5, 10].into_iter().collect()
    );
}

// =============================================================================
// Collection Types - Fixed-size Arrays
// =============================================================================

mod collections_array {
    use super::*;

    test_roundtrip!(array_u8_empty, [u8; 0], []);
    test_roundtrip!(array_u8_small, [u8; 4], [1, 2, 3, 4]);
    test_roundtrip!(array_u32_small, [u32; 3], [100, 200, 300]);
    test_roundtrip!(array_bool, [bool; 5], [true, false, true, false, true]);
    test_roundtrip!(array_i64, [i64; 2], [i64::MIN, i64::MAX]);

    // Nested arrays
    test_roundtrip!(array_nested, [[u8; 2]; 3], [[1, 2], [3, 4], [5, 6]]);
}

// =============================================================================
// Option Types
// =============================================================================

mod options {
    use super::*;

    // Option<primitive>
    test_roundtrip!(option_u32_none, Option<u32>, None);
    test_roundtrip!(option_u32_some, Option<u32>, Some(42));

    test_roundtrip!(option_string_none, Option<String>, None);
    test_roundtrip!(
        option_string_some,
        Option<String>,
        Some("hello".to_string())
    );

    test_roundtrip!(option_bool_none, Option<bool>, None);
    test_roundtrip!(option_bool_some_true, Option<bool>, Some(true));
    test_roundtrip!(option_bool_some_false, Option<bool>, Some(false));

    test_roundtrip!(option_char_none, Option<char>, None);
    test_roundtrip!(option_char_some, Option<char>, Some('🦀'));

    // Nested Option
    test_roundtrip!(option_option_none, Option<Option<u32>>, None);
    test_roundtrip!(option_option_some_none, Option<Option<u32>>, Some(None));
    test_roundtrip!(option_option_some_some, Option<Option<u32>>, Some(Some(42)));

    // Option<Vec>
    test_roundtrip!(option_vec_none, Option<Vec<u32>>, None);
    test_roundtrip!(option_vec_some_empty, Option<Vec<u32>>, Some(vec![]));
    test_roundtrip!(
        option_vec_some_values,
        Option<Vec<u32>>,
        Some(vec![1, 2, 3])
    );

    // Option<()>
    test_roundtrip!(option_unit_none, Option<()>, None);
    test_roundtrip!(option_unit_some, Option<()>, Some(()));

    // Option<Box<T>>
    test_roundtrip!(option_box_none, Option<Box<u32>>, None);
    test_roundtrip!(option_box_some, Option<Box<u32>>, Some(Box::new(42)));
}

// =============================================================================
// Result Types
// =============================================================================

mod results {
    use super::*;

    // Result<T, E> with primitive types
    test_roundtrip!(result_u32_ok, Result<u32, String>, Ok(42));
    test_roundtrip!(
        result_u32_err,
        Result<u32, String>,
        Err("error message".to_string())
    );

    test_roundtrip!(
        result_string_ok,
        Result<String, u32>,
        Ok("success".to_string())
    );
    test_roundtrip!(result_string_err, Result<String, u32>, Err(404));

    // Result with Vec
    test_roundtrip!(result_vec_ok, Result<Vec<u32>, String>, Ok(vec![1, 2, 3]));
    test_roundtrip!(
        result_vec_err,
        Result<Vec<u32>, String>,
        Err("failed".to_string())
    );

    // Result with custom error type
    #[derive(Debug, PartialEq, Facet)]
    struct CustomError {
        code: u32,
        message: String,
    }

    test_roundtrip!(result_custom_ok, Result<i32, CustomError>, Ok(42));
    test_roundtrip!(
        result_custom_err,
        Result<i32, CustomError>,
        Err(CustomError {
            code: 500,
            message: "Internal Server Error".to_string()
        })
    );

    // Result<(), E> and Result<T, ()>
    test_roundtrip!(result_unit_ok, Result<(), String>, Ok(()));
    test_roundtrip!(
        result_unit_err,
        Result<(), String>,
        Err("error".to_string())
    );
    test_roundtrip!(result_ok_unit_err, Result<u32, ()>, Ok(42));
    test_roundtrip!(result_err_unit, Result<u32, ()>, Err(()));

    // Nested Result
    test_roundtrip!(
        result_nested_ok_ok,
        Result<Result<u32, String>, String>,
        Ok(Ok(42))
    );
    test_roundtrip!(
        result_nested_ok_err,
        Result<Result<u32, String>, String>,
        Ok(Err("inner".to_string()))
    );
    test_roundtrip!(
        result_nested_err,
        Result<Result<u32, String>, String>,
        Err("outer".to_string())
    );
}

// =============================================================================
// Box Types
// =============================================================================

mod box_types {
    use super::*;

    test_roundtrip!(box_u32, Box<u32>, Box::new(42));
    test_roundtrip!(box_string, Box<String>, Box::new("hello".to_string()));
    test_roundtrip!(box_vec, Box<Vec<u32>>, Box::new(vec![1, 2, 3]));

    // Box<dyn T> not supported, but Box<concrete> should work
    #[derive(Debug, PartialEq, Facet)]
    struct Point {
        x: i32,
        y: i32,
    }

    test_roundtrip!(box_struct, Box<Point>, Box::new(Point { x: 10, y: 20 }));

    // Nested Box
    test_roundtrip!(box_box, Box<Box<u32>>, Box::new(Box::new(42)));
}

// =============================================================================
// Struct Types
// =============================================================================

mod structs {
    use super::*;

    // Unit struct
    #[derive(Debug, PartialEq, Facet)]
    struct UnitStruct;

    test_roundtrip!(unit_struct, UnitStruct, UnitStruct);

    // Named field struct
    #[derive(Debug, PartialEq, Facet)]
    struct Point {
        x: i32,
        y: i32,
    }

    test_roundtrip!(struct_point, Point, Point { x: 10, y: -20 });

    #[derive(Debug, PartialEq, Facet)]
    struct Person {
        name: String,
        age: u32,
        active: bool,
    }

    test_roundtrip!(
        struct_person,
        Person,
        Person {
            name: "Alice".to_string(),
            age: 30,
            active: true
        }
    );

    // Tuple struct
    #[derive(Debug, PartialEq, Facet)]
    struct Color(u8, u8, u8);

    test_roundtrip!(tuple_struct_color, Color, Color(255, 128, 0));

    #[derive(Debug, PartialEq, Facet)]
    struct Newtype(u64);

    test_roundtrip!(newtype_struct, Newtype, Newtype(12345));

    // Struct with Option fields
    #[derive(Debug, PartialEq, Facet)]
    struct WithOptions {
        required: String,
        optional: Option<u32>,
    }

    test_roundtrip!(
        struct_with_option_some,
        WithOptions,
        WithOptions {
            required: "test".to_string(),
            optional: Some(42)
        }
    );
    test_roundtrip!(
        struct_with_option_none,
        WithOptions,
        WithOptions {
            required: "test".to_string(),
            optional: None
        }
    );

    // Struct with Vec fields
    #[derive(Debug, PartialEq, Facet)]
    struct WithVec {
        name: String,
        values: Vec<u32>,
    }

    test_roundtrip!(
        struct_with_vec,
        WithVec,
        WithVec {
            name: "data".to_string(),
            values: vec![1, 2, 3, 4, 5]
        }
    );

    // Struct with HashMap field
    #[derive(Debug, PartialEq, Facet)]
    struct WithMap {
        name: String,
        data: HashMap<String, u32>,
    }

    test_roundtrip!(
        struct_with_map,
        WithMap,
        WithMap {
            name: "test".to_string(),
            data: [("key".to_string(), 42)].into_iter().collect()
        }
    );

    // Nested structs
    #[derive(Debug, PartialEq, Facet)]
    struct Inner {
        value: u32,
    }

    #[derive(Debug, PartialEq, Facet)]
    struct Outer {
        name: String,
        inner: Inner,
    }

    test_roundtrip!(
        nested_struct,
        Outer,
        Outer {
            name: "outer".to_string(),
            inner: Inner { value: 42 }
        }
    );

    // Deeply nested
    #[derive(Debug, PartialEq, Facet)]
    struct Level3 {
        data: Vec<u32>,
    }

    #[derive(Debug, PartialEq, Facet)]
    struct Level2 {
        level3: Level3,
    }

    #[derive(Debug, PartialEq, Facet)]
    struct Level1 {
        level2: Level2,
    }

    test_roundtrip!(
        deeply_nested,
        Level1,
        Level1 {
            level2: Level2 {
                level3: Level3 {
                    data: vec![1, 2, 3]
                }
            }
        }
    );

    // Struct with Box field
    #[derive(Debug, PartialEq, Facet)]
    struct WithBox {
        boxed: Box<u32>,
    }

    test_roundtrip!(
        struct_with_box,
        WithBox,
        WithBox {
            boxed: Box::new(42)
        }
    );

    // Struct mimicking FontAnalysis from dodeca
    #[derive(Debug, PartialEq, Clone, Facet)]
    struct FontFace {
        family: String,
        src: String,
        weight: Option<String>,
        style: Option<String>,
    }

    #[derive(Debug, PartialEq, Facet)]
    struct FontAnalysis {
        chars_per_font: HashMap<String, Vec<char>>,
        font_faces: Vec<FontFace>,
    }

    test_roundtrip!(
        font_analysis_empty,
        FontAnalysis,
        FontAnalysis {
            chars_per_font: HashMap::new(),
            font_faces: vec![],
        }
    );

    test_roundtrip!(
        font_analysis_with_data,
        FontAnalysis,
        FontAnalysis {
            chars_per_font: [
                ("Arial".to_string(), vec!['H', 'e', 'l', 'l', 'o']),
                ("Roboto".to_string(), vec!['W', 'o', 'r', 'l', 'd']),
            ]
            .into_iter()
            .collect(),
            font_faces: vec![
                FontFace {
                    family: "Arial".to_string(),
                    src: "/fonts/arial.woff2".to_string(),
                    weight: Some("400".to_string()),
                    style: None,
                },
                FontFace {
                    family: "Roboto".to_string(),
                    src: "/fonts/roboto.woff2".to_string(),
                    weight: Some("700".to_string()),
                    style: Some("italic".to_string()),
                },
            ],
        }
    );
}

// =============================================================================
// Enum Types
// =============================================================================

mod enums {
    use super::*;

    // Unit variants only
    #[derive(Debug, PartialEq, Facet)]
    #[repr(u8)]
    enum Color {
        Red,
        Green,
        Blue,
    }

    test_roundtrip!(enum_unit_red, Color, Color::Red);
    test_roundtrip!(enum_unit_green, Color, Color::Green);
    test_roundtrip!(enum_unit_blue, Color, Color::Blue);

    // Mixed variants - newtype
    #[derive(Debug, PartialEq, Facet)]
    #[repr(u8)]
    enum Message {
        Quit,
        Text(String),
        Number(u32),
    }

    test_roundtrip!(enum_newtype_quit, Message, Message::Quit);
    test_roundtrip!(
        enum_newtype_text,
        Message,
        Message::Text("hello".to_string())
    );
    test_roundtrip!(enum_newtype_number, Message, Message::Number(42));

    // Tuple variants
    #[derive(Debug, PartialEq, Facet)]
    #[repr(u8)]
    enum TupleEnum {
        Empty,
        Pair(u32, String),
        Triple(u8, u16, u32),
    }

    test_roundtrip!(enum_tuple_empty, TupleEnum, TupleEnum::Empty);
    test_roundtrip!(
        enum_tuple_pair,
        TupleEnum,
        TupleEnum::Pair(42, "test".to_string())
    );
    test_roundtrip!(
        enum_tuple_triple,
        TupleEnum,
        TupleEnum::Triple(1, 256, 100000)
    );

    // Struct variants
    #[derive(Debug, PartialEq, Facet)]
    #[repr(u8)]
    enum StructEnum {
        Unit,
        Point { x: i32, y: i32 },
        Person { name: String, age: u32 },
    }

    test_roundtrip!(enum_struct_unit, StructEnum, StructEnum::Unit);
    test_roundtrip!(
        enum_struct_point,
        StructEnum,
        StructEnum::Point { x: 10, y: -20 }
    );
    test_roundtrip!(
        enum_struct_person,
        StructEnum,
        StructEnum::Person {
            name: "Bob".to_string(),
            age: 25
        }
    );

    // Enum with Option in variant
    #[derive(Debug, PartialEq, Facet)]
    #[repr(u8)]
    enum WithOption {
        None,
        Some(Option<u32>),
    }

    test_roundtrip!(enum_with_option_none, WithOption, WithOption::None);
    test_roundtrip!(
        enum_with_option_some_none,
        WithOption,
        WithOption::Some(None)
    );
    test_roundtrip!(
        enum_with_option_some_some,
        WithOption,
        WithOption::Some(Some(42))
    );

    // Enum with Vec in variant
    #[derive(Debug, PartialEq, Facet)]
    #[repr(u8)]
    enum WithVec {
        Empty,
        Values(Vec<u32>),
    }

    test_roundtrip!(enum_with_vec_empty, WithVec, WithVec::Empty);
    test_roundtrip!(
        enum_with_vec_values,
        WithVec,
        WithVec::Values(vec![1, 2, 3])
    );

    // Nested enum
    #[derive(Debug, PartialEq, Facet)]
    #[repr(u8)]
    enum InnerEnum {
        A,
        B(u32),
    }

    #[derive(Debug, PartialEq, Facet)]
    #[repr(u8)]
    enum OuterEnum {
        None,
        Inner(InnerEnum),
    }

    test_roundtrip!(enum_nested_none, OuterEnum, OuterEnum::None);
    test_roundtrip!(enum_nested_a, OuterEnum, OuterEnum::Inner(InnerEnum::A));
    test_roundtrip!(enum_nested_b, OuterEnum, OuterEnum::Inner(InnerEnum::B(42)));

    // Enum mimicking FontResult from dodeca
    #[derive(Debug, PartialEq, Facet)]
    #[repr(u8)]
    enum FontResult {
        AnalysisSuccess { chars: Vec<char> },
        CssSuccess { css: String },
        DecompressSuccess { data: Vec<u8> },
        Error { message: String },
    }

    test_roundtrip!(
        font_result_analysis,
        FontResult,
        FontResult::AnalysisSuccess {
            chars: vec!['a', 'b', 'c']
        }
    );
    test_roundtrip!(
        font_result_css,
        FontResult,
        FontResult::CssSuccess {
            css: "body { }".to_string()
        }
    );
    test_roundtrip!(
        font_result_error,
        FontResult,
        FontResult::Error {
            message: "failed".to_string()
        }
    );
}

// =============================================================================
// Tuple Types
// =============================================================================

mod tuples {
    use super::*;

    // Unit tuple (same as unit type, but tested in tuples module for clarity)
    test_roundtrip!(tuple_unit, (), ());

    // Single-element tuple
    test_roundtrip!(tuple_single, (u32,), (42u32,));

    // Pair tuple
    test_roundtrip!(tuple_pair, (u32, String), (42u32, "hello".to_string()));

    // Triple tuple
    test_roundtrip!(
        tuple_triple,
        (u32, String, bool),
        (42u32, "test".to_string(), true)
    );

    // Quadruple tuple
    test_roundtrip!(tuple_quad, (u8, u16, u32, u64), (1u8, 2u16, 3u32, 4u64));

    // Nested tuple
    test_roundtrip!(
        tuple_nested,
        ((u32, String), (bool, Vec<u32>)),
        ((42u32, "hello".to_string()), (true, vec![1, 2, 3]))
    );

    // Tuple with Option
    test_roundtrip!(
        tuple_with_option,
        (Option<u32>, Option<String>),
        (Some(42), None)
    );
}

// =============================================================================
// Camino Path Types
// =============================================================================

#[cfg(feature = "camino")]
mod camino_types {
    use super::*;
    use camino::Utf8PathBuf;

    // Simple Utf8PathBuf
    test_roundtrip!(
        utf8pathbuf_simple,
        Utf8PathBuf,
        Utf8PathBuf::from("/path/to/file")
    );
    test_roundtrip!(utf8pathbuf_empty, Utf8PathBuf, Utf8PathBuf::new());
    test_roundtrip!(
        utf8pathbuf_relative,
        Utf8PathBuf,
        Utf8PathBuf::from("relative/path")
    );
    test_roundtrip!(
        utf8pathbuf_with_extension,
        Utf8PathBuf,
        Utf8PathBuf::from("/path/to/file.txt")
    );
    test_roundtrip!(
        utf8pathbuf_unicode,
        Utf8PathBuf,
        Utf8PathBuf::from("/путь/к/файлу")
    );

    // Option<Utf8PathBuf>
    test_roundtrip!(option_utf8pathbuf_none, Option<Utf8PathBuf>, None);
    test_roundtrip!(
        option_utf8pathbuf_some,
        Option<Utf8PathBuf>,
        Some(Utf8PathBuf::from("/path"))
    );

    // Vec<Utf8PathBuf>
    test_roundtrip!(
        vec_utf8pathbuf,
        Vec<Utf8PathBuf>,
        vec![
            Utf8PathBuf::from("/a"),
            Utf8PathBuf::from("/b/c"),
            Utf8PathBuf::from("/d/e/f"),
        ]
    );

    // Struct with Utf8PathBuf fields
    #[derive(Debug, PartialEq, Facet)]
    struct WithPath {
        name: String,
        path: Utf8PathBuf,
    }

    test_roundtrip!(
        struct_with_path,
        WithPath,
        WithPath {
            name: "test".to_string(),
            path: Utf8PathBuf::from("/home/user"),
        }
    );

    // Struct with Option<Utf8PathBuf> - mirrors BuildResult
    #[derive(Debug, PartialEq, Facet)]
    struct WithOptionalPath {
        success: bool,
        message: String,
        output_path: Option<Utf8PathBuf>,
    }

    test_roundtrip!(
        struct_with_optional_path_some,
        WithOptionalPath,
        WithOptionalPath {
            success: true,
            message: "done".to_string(),
            output_path: Some(Utf8PathBuf::from("/output/binary")),
        }
    );

    test_roundtrip!(
        struct_with_optional_path_none,
        WithOptionalPath,
        WithOptionalPath {
            success: false,
            message: "failed".to_string(),
            output_path: None,
        }
    );
}

// =============================================================================
// Network Types
// =============================================================================

#[cfg(feature = "net")]
mod net_types {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};

    // Ipv4Addr
    test_roundtrip!(ipv4_localhost, Ipv4Addr, Ipv4Addr::LOCALHOST);
    test_roundtrip!(ipv4_broadcast, Ipv4Addr, Ipv4Addr::BROADCAST);
    test_roundtrip!(ipv4_custom, Ipv4Addr, Ipv4Addr::new(192, 168, 1, 1));

    // Ipv6Addr
    test_roundtrip!(ipv6_localhost, Ipv6Addr, Ipv6Addr::LOCALHOST);
    test_roundtrip!(
        ipv6_custom,
        Ipv6Addr,
        Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1)
    );

    // IpAddr
    test_roundtrip!(ipaddr_v4, IpAddr, IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)));
    test_roundtrip!(ipaddr_v6, IpAddr, IpAddr::V6(Ipv6Addr::LOCALHOST));

    // SocketAddrV4
    test_roundtrip!(
        socketaddr_v4,
        SocketAddrV4,
        SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 8080)
    );

    // SocketAddrV6
    test_roundtrip!(
        socketaddr_v6,
        SocketAddrV6,
        SocketAddrV6::new(Ipv6Addr::LOCALHOST, 8080, 0, 0)
    );

    // SocketAddr
    test_roundtrip!(
        socketaddr_from_v4,
        SocketAddr,
        SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 1), 443))
    );
    test_roundtrip!(
        socketaddr_from_v6,
        SocketAddr,
        SocketAddr::V6(SocketAddrV6::new(Ipv6Addr::LOCALHOST, 8080, 0, 0))
    );

    // In structs
    #[derive(Debug, PartialEq, Facet)]
    struct ServerConfig {
        bind_addr: SocketAddr,
        public_ip: Option<IpAddr>,
    }

    test_roundtrip!(
        struct_with_net_types,
        ServerConfig,
        ServerConfig {
            bind_addr: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), 8080)),
            public_ip: Some(IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4))),
        }
    );
}

// =============================================================================
// Complex/Kitchen Sink Types
// =============================================================================

mod complex {
    use super::*;

    // A struct that uses many different types
    #[derive(Debug, PartialEq, Facet)]
    struct KitchenSink {
        // Primitives
        u8_field: u8,
        u16_field: u16,
        u32_field: u32,
        u64_field: u64,
        i8_field: i8,
        i16_field: i16,
        i32_field: i32,
        i64_field: i64,
        f32_field: f32,
        f64_field: f64,
        bool_field: bool,
        char_field: char,
        // Strings
        string_field: String,
        // Collections
        vec_field: Vec<u32>,
        // Option
        option_field: Option<u32>,
        // Result
        result_field: Result<String, u32>,
    }

    test_roundtrip!(
        kitchen_sink_full,
        KitchenSink,
        KitchenSink {
            u8_field: 255,
            u16_field: 65535,
            u32_field: 4294967295,
            u64_field: 18446744073709551615,
            i8_field: -128,
            i16_field: -32768,
            i32_field: -2147483648,
            i64_field: -9223372036854775808,
            f32_field: std::f32::consts::PI,
            f64_field: std::f64::consts::E,
            bool_field: true,
            char_field: '🦀',
            string_field: "Hello, World!".to_string(),
            vec_field: vec![1, 2, 3, 4, 5],
            option_field: Some(42),
            result_field: Ok("success".to_string()),
        }
    );

    // Generic struct
    #[derive(Debug, PartialEq, Facet)]
    struct Container<T> {
        value: T,
        metadata: String,
    }

    test_roundtrip!(
        generic_u32,
        Container<u32>,
        Container {
            value: 42,
            metadata: "integer".to_string()
        }
    );

    test_roundtrip!(
        generic_vec,
        Container<Vec<String>>,
        Container {
            value: vec!["a".to_string(), "b".to_string()],
            metadata: "string list".to_string()
        }
    );

    test_roundtrip!(
        generic_option,
        Container<Option<u32>>,
        Container {
            value: Some(42),
            metadata: "optional".to_string()
        }
    );
}

// =============================================================================
// Edge Cases and Stress Tests
// =============================================================================

mod edge_cases {
    use super::*;

    // Large numbers
    test_roundtrip!(u64_almost_max, u64, u64::MAX - 1);
    test_roundtrip!(i64_almost_min, i64, i64::MIN + 1);
    test_roundtrip!(i64_almost_max, i64, i64::MAX - 1);

    // Floating point edge cases
    test_roundtrip!(f32_min_positive, f32, f32::MIN_POSITIVE);
    test_roundtrip!(f64_min_positive, f64, f64::MIN_POSITIVE);
    test_roundtrip!(f32_epsilon, f32, f32::EPSILON);
    test_roundtrip!(f64_epsilon, f64, f64::EPSILON);

    // Very long strings
    test_roundtrip!(string_10k_chars, String, "x".repeat(10_000));
    test_roundtrip!(string_unicode_repeated, String, "🦀".repeat(100));

    // Large vectors
    test_roundtrip!(vec_1000_u32, Vec<u32>, (0..1000).collect());
    test_roundtrip!(vec_1000_bool, Vec<bool>, vec![true; 1000]);

    // Deeply nested Options
    #[derive(Debug, PartialEq, Facet)]
    struct Deep4Option {
        value: Option<Option<Option<Option<u32>>>>,
    }

    test_roundtrip!(
        deep_option_all_some,
        Deep4Option,
        Deep4Option {
            value: Some(Some(Some(Some(42))))
        }
    );

    test_roundtrip!(
        deep_option_none_at_level_2,
        Deep4Option,
        Deep4Option {
            value: Some(Some(None))
        }
    );

    // Mixed Result and Option
    #[derive(Debug, PartialEq, Facet)]
    struct MixedResultOption {
        result_of_option: Result<Option<u32>, String>,
        option_of_result: Option<Result<u32, String>>,
    }

    test_roundtrip!(
        mixed_result_option_ok_some,
        MixedResultOption,
        MixedResultOption {
            result_of_option: Ok(Some(42)),
            option_of_result: Some(Ok(100)),
        }
    );

    test_roundtrip!(
        mixed_result_option_err_none,
        MixedResultOption,
        MixedResultOption {
            result_of_option: Err("error".to_string()),
            option_of_result: None,
        }
    );

    // Vec of Options
    test_roundtrip!(
        vec_of_options,
        Vec<Option<u32>>,
        vec![Some(1), None, Some(2), None, Some(3)]
    );

    // Vec of Results
    test_roundtrip!(
        vec_of_results,
        Vec<Result<u32, String>>,
        vec![Ok(1), Err("e1".to_string()), Ok(2), Err("e2".to_string())]
    );

    // Struct with many Option fields
    #[derive(Debug, PartialEq, Facet)]
    struct ManyOptions {
        a: Option<u8>,
        b: Option<u16>,
        c: Option<u32>,
        d: Option<u64>,
        e: Option<String>,
        f: Option<bool>,
    }

    test_roundtrip!(
        many_options_all_some,
        ManyOptions,
        ManyOptions {
            a: Some(1),
            b: Some(2),
            c: Some(3),
            d: Some(4),
            e: Some("test".to_string()),
            f: Some(true),
        }
    );

    test_roundtrip!(
        many_options_all_none,
        ManyOptions,
        ManyOptions {
            a: None,
            b: None,
            c: None,
            d: None,
            e: None,
            f: None,
        }
    );

    test_roundtrip!(
        many_options_mixed,
        ManyOptions,
        ManyOptions {
            a: Some(1),
            b: None,
            c: Some(3),
            d: None,
            e: Some("test".to_string()),
            f: None,
        }
    );

    // BTreeMap wrapped in struct
    #[derive(Debug, PartialEq, Facet)]
    struct ManyEntries {
        map: BTreeMap<String, u32>,
    }

    test_roundtrip!(
        btreemap_many_entries,
        ManyEntries,
        ManyEntries {
            map: [
                ("a".to_string(), 1),
                ("b".to_string(), 2),
                ("c".to_string(), 3),
                ("d".to_string(), 4),
                ("e".to_string(), 5),
            ]
            .into_iter()
            .collect()
        }
    );

    // Vec of structs
    #[derive(Debug, PartialEq, Facet)]
    struct SimplePoint {
        x: i32,
        y: i32,
    }

    test_roundtrip!(
        vec_of_structs,
        Vec<SimplePoint>,
        vec![
            SimplePoint { x: 1, y: 2 },
            SimplePoint { x: 3, y: 4 },
            SimplePoint { x: 5, y: 6 },
        ]
    );

    // Vec of enums
    #[derive(Debug, PartialEq, Facet)]
    #[repr(u8)]
    enum Status {
        Active,
        Inactive,
        Pending,
    }

    test_roundtrip!(
        vec_of_enums,
        Vec<Status>,
        vec![
            Status::Active,
            Status::Inactive,
            Status::Pending,
            Status::Active
        ]
    );

    // Struct with multiple collection types
    #[derive(Debug, PartialEq, Facet)]
    struct MultiCollection {
        vec: Vec<u32>,
        hash: HashMap<String, u32>,
        btree: BTreeMap<String, bool>,
    }

    test_roundtrip!(
        multi_collection,
        MultiCollection,
        MultiCollection {
            vec: vec![1, 2, 3],
            hash: [("a".to_string(), 1)].into_iter().collect(),
            btree: [("x".to_string(), true)].into_iter().collect(),
        }
    );

    // All integer types in one struct
    #[derive(Debug, PartialEq, Facet)]
    struct AllIntegers {
        u8_val: u8,
        u16_val: u16,
        u32_val: u32,
        u64_val: u64,
        u128_val: u128,
        i8_val: i8,
        i16_val: i16,
        i32_val: i32,
        i64_val: i64,
        i128_val: i128,
    }

    test_roundtrip!(
        all_integers_max,
        AllIntegers,
        AllIntegers {
            u8_val: u8::MAX,
            u16_val: u16::MAX,
            u32_val: u32::MAX,
            u64_val: u64::MAX,
            u128_val: u128::MAX,
            i8_val: i8::MAX,
            i16_val: i16::MAX,
            i32_val: i32::MAX,
            i64_val: i64::MAX,
            i128_val: i128::MAX,
        }
    );

    test_roundtrip!(
        all_integers_min,
        AllIntegers,
        AllIntegers {
            u8_val: u8::MIN,
            u16_val: u16::MIN,
            u32_val: u32::MIN,
            u64_val: u64::MIN,
            u128_val: u128::MIN,
            i8_val: i8::MIN,
            i16_val: i16::MIN,
            i32_val: i32::MIN,
            i64_val: i64::MIN,
            i128_val: i128::MIN,
        }
    );

    // All float types in one struct
    #[derive(Debug, PartialEq, Facet)]
    struct AllFloats {
        f32_val: f32,
        f64_val: f64,
    }

    test_roundtrip!(
        all_floats_positive,
        AllFloats,
        AllFloats {
            f32_val: 1.23,
            f64_val: 4.56,
        }
    );

    test_roundtrip!(
        all_floats_negative,
        AllFloats,
        AllFloats {
            f32_val: -1.23,
            f64_val: -4.56,
        }
    );

    test_roundtrip!(
        all_floats_infinity,
        AllFloats,
        AllFloats {
            f32_val: f32::INFINITY,
            f64_val: f64::NEG_INFINITY,
        }
    );

    // Enum with Vec in variant containing struct with chars
    #[derive(Debug, PartialEq, Facet)]
    struct CharData {
        name: String,
        chars: Vec<char>,
    }

    #[derive(Debug, PartialEq, Facet)]
    #[repr(u8)]
    enum DataEnum {
        Empty,
        CharData(CharData),
        Values(Vec<u32>),
    }

    test_roundtrip!(enum_with_char_data_empty, DataEnum, DataEnum::Empty);
    test_roundtrip!(
        enum_with_char_data_values,
        DataEnum,
        DataEnum::CharData(CharData {
            name: "test".to_string(),
            chars: vec!['a', 'b', 'c'],
        })
    );
}

// =============================================================================
// Arc and Rc Types
// =============================================================================

mod arc_rc {
    use super::*;
    use std::rc::Rc;
    use std::sync::Arc;

    // Arc<T>
    test_roundtrip!(arc_u32, Arc<u32>, Arc::new(42));
    test_roundtrip!(arc_string, Arc<String>, Arc::new("hello".to_string()));
    test_roundtrip!(arc_vec, Arc<Vec<u32>>, Arc::new(vec![1, 2, 3]));

    // Arc<str>
    test_roundtrip!(arc_str, Arc<str>, Arc::from("hello world"));

    // Arc<[T]>
    test_roundtrip!(arc_slice_u8, Arc<[u8]>, Arc::from([1u8, 2, 3].as_slice()));
    test_roundtrip!(
        arc_slice_u32,
        Arc<[u32]>,
        Arc::from([10u32, 20, 30].as_slice())
    );

    // Arc<struct>
    #[derive(Debug, PartialEq, Facet)]
    struct ArcPoint {
        x: i32,
        y: i32,
    }

    test_roundtrip!(
        arc_struct,
        Arc<ArcPoint>,
        Arc::new(ArcPoint { x: 10, y: 20 })
    );

    // Nested Arc
    test_roundtrip!(arc_arc, Arc<Arc<u32>>, Arc::new(Arc::new(42)));

    // Rc<T>
    test_roundtrip!(rc_u32, Rc<u32>, Rc::new(42));
    test_roundtrip!(rc_string, Rc<String>, Rc::new("hello".to_string()));
    test_roundtrip!(rc_vec, Rc<Vec<u32>>, Rc::new(vec![1, 2, 3]));

    // Rc<str>
    test_roundtrip!(rc_str, Rc<str>, Rc::from("hello world"));

    // Rc<[T]>
    test_roundtrip!(rc_slice_u8, Rc<[u8]>, Rc::from([1u8, 2, 3].as_slice()));

    // Rc<struct>
    test_roundtrip!(rc_struct, Rc<ArcPoint>, Rc::new(ArcPoint { x: 10, y: 20 }));

    // Option<Arc<T>>
    test_roundtrip!(option_arc_some, Option<Arc<u32>>, Some(Arc::new(42)));
    test_roundtrip!(option_arc_none, Option<Arc<u32>>, None);

    // Vec<Arc<T>>
    test_roundtrip!(
        vec_arc,
        Vec<Arc<String>>,
        vec![Arc::new("a".to_string()), Arc::new("b".to_string())]
    );

    // Arc in struct
    #[derive(Debug, PartialEq, Facet)]
    struct WithArc {
        data: Arc<String>,
        count: u32,
    }

    test_roundtrip!(
        struct_with_arc,
        WithArc,
        WithArc {
            data: Arc::new("shared".to_string()),
            count: 42,
        }
    );
}

// =============================================================================
// Vec<Struct> and HashMap<String, Struct> - Complex Collection Tests
// =============================================================================

mod collections_of_structs {
    use super::*;

    #[derive(Debug, PartialEq, Clone, Facet)]
    struct Person {
        name: String,
        age: u32,
        active: bool,
    }

    #[derive(Debug, PartialEq, Clone, Facet)]
    struct Address {
        street: String,
        city: String,
        zip: Option<String>,
    }

    #[derive(Debug, PartialEq, Clone, Facet)]
    struct Company {
        name: String,
        employees: Vec<Person>,
        headquarters: Address,
    }

    // Vec<Struct>
    test_roundtrip!(vec_person_empty, Vec<Person>, vec![]);
    test_roundtrip!(
        vec_person_single,
        Vec<Person>,
        vec![Person {
            name: "Alice".to_string(),
            age: 30,
            active: true
        }]
    );
    test_roundtrip!(
        vec_person_multiple,
        Vec<Person>,
        vec![
            Person {
                name: "Alice".to_string(),
                age: 30,
                active: true
            },
            Person {
                name: "Bob".to_string(),
                age: 25,
                active: false
            },
            Person {
                name: "Charlie".to_string(),
                age: 35,
                active: true
            },
        ]
    );

    // HashMap<String, Struct>
    test_roundtrip!(
        hashmap_string_person_empty,
        HashMap<String, Person>,
        HashMap::new()
    );
    test_roundtrip!(
        hashmap_string_person_single,
        HashMap<String, Person>,
        [(
            "alice".to_string(),
            Person {
                name: "Alice".to_string(),
                age: 30,
                active: true
            }
        )]
        .into_iter()
        .collect()
    );
    test_roundtrip!(
        hashmap_string_person_multiple,
        HashMap<String, Person>,
        [
            (
                "alice".to_string(),
                Person {
                    name: "Alice".to_string(),
                    age: 30,
                    active: true
                }
            ),
            (
                "bob".to_string(),
                Person {
                    name: "Bob".to_string(),
                    age: 25,
                    active: false
                }
            ),
        ]
        .into_iter()
        .collect()
    );

    // BTreeMap<String, Struct>
    test_roundtrip!(
        btreemap_string_person,
        BTreeMap<String, Person>,
        [
            (
                "alice".to_string(),
                Person {
                    name: "Alice".to_string(),
                    age: 30,
                    active: true
                }
            ),
            (
                "bob".to_string(),
                Person {
                    name: "Bob".to_string(),
                    age: 25,
                    active: false
                }
            ),
        ]
        .into_iter()
        .collect()
    );

    // Struct containing Vec<Struct>
    test_roundtrip!(
        company_with_employees,
        Company,
        Company {
            name: "Acme Corp".to_string(),
            employees: vec![
                Person {
                    name: "Alice".to_string(),
                    age: 30,
                    active: true
                },
                Person {
                    name: "Bob".to_string(),
                    age: 25,
                    active: true
                },
            ],
            headquarters: Address {
                street: "123 Main St".to_string(),
                city: "Springfield".to_string(),
                zip: Some("12345".to_string()),
            },
        }
    );

    // Struct containing HashMap<String, Struct>
    #[derive(Debug, PartialEq, Facet)]
    struct Directory {
        people: HashMap<String, Person>,
        addresses: HashMap<String, Address>,
    }

    test_roundtrip!(
        directory_with_data,
        Directory,
        Directory {
            people: [(
                "alice".to_string(),
                Person {
                    name: "Alice".to_string(),
                    age: 30,
                    active: true
                }
            ),]
            .into_iter()
            .collect(),
            addresses: [(
                "home".to_string(),
                Address {
                    street: "123 Home St".to_string(),
                    city: "Hometown".to_string(),
                    zip: None,
                }
            ),]
            .into_iter()
            .collect(),
        }
    );

    // Vec<Vec<Struct>>
    test_roundtrip!(
        vec_vec_person,
        Vec<Vec<Person>>,
        vec![
            vec![Person {
                name: "A".to_string(),
                age: 1,
                active: true
            }],
            vec![
                Person {
                    name: "B".to_string(),
                    age: 2,
                    active: false
                },
                Person {
                    name: "C".to_string(),
                    age: 3,
                    active: true
                },
            ],
        ]
    );

    // Option<Vec<Struct>>
    test_roundtrip!(
        option_vec_person_some,
        Option<Vec<Person>>,
        Some(vec![Person {
            name: "Alice".to_string(),
            age: 30,
            active: true
        }])
    );
    test_roundtrip!(option_vec_person_none, Option<Vec<Person>>, None);

    // Vec<Option<Struct>>
    test_roundtrip!(
        vec_option_person,
        Vec<Option<Person>>,
        vec![
            Some(Person {
                name: "Alice".to_string(),
                age: 30,
                active: true
            }),
            None,
            Some(Person {
                name: "Bob".to_string(),
                age: 25,
                active: false
            }),
        ]
    );
}

// =============================================================================
// External/Esoteric Types - UUID, ULID, Chrono, Jiff, etc.
// =============================================================================

#[cfg(feature = "uuid")]
mod uuid_roundtrip {
    use super::*;
    use uuid::Uuid;

    test_roundtrip!(uuid_nil, Uuid, Uuid::nil());
    test_roundtrip!(uuid_max, Uuid, Uuid::max());

    #[test]
    fn uuid_v4_roundtrip() {
        facet_testhelpers::setup();
        let original = Uuid::new_v4();
        let bytes = to_vec(&original).expect("serialization should succeed");
        let deserialized: Uuid = from_slice(&bytes).expect("deserialization should succeed");
        assert_eq!(deserialized, original);
    }

    // Vec<Uuid>
    test_roundtrip!(vec_uuid, Vec<Uuid>, vec![Uuid::nil(), Uuid::max()]);

    // Option<Uuid>
    test_roundtrip!(option_uuid_some, Option<Uuid>, Some(Uuid::nil()));
    test_roundtrip!(option_uuid_none, Option<Uuid>, None);

    // HashMap<String, Uuid>
    test_roundtrip!(
        hashmap_uuid,
        HashMap<String, Uuid>,
        [("id".to_string(), Uuid::nil())].into_iter().collect()
    );

    // Struct with Uuid
    #[derive(Debug, PartialEq, Facet)]
    struct EntityWithUuid {
        id: Uuid,
        name: String,
    }

    test_roundtrip!(
        struct_with_uuid,
        EntityWithUuid,
        EntityWithUuid {
            id: Uuid::nil(),
            name: "test".to_string(),
        }
    );
}

#[cfg(feature = "ulid")]
mod ulid_roundtrip {
    use super::*;
    use ulid::Ulid;

    test_roundtrip!(ulid_nil, Ulid, Ulid::nil());

    #[test]
    fn ulid_new_roundtrip() {
        facet_testhelpers::setup();
        let original = Ulid::new();
        let bytes = to_vec(&original).expect("serialization should succeed");
        let deserialized: Ulid = from_slice(&bytes).expect("deserialization should succeed");
        assert_eq!(deserialized, original);
    }

    // Vec<Ulid>
    test_roundtrip!(vec_ulid, Vec<Ulid>, vec![Ulid::nil()]);

    // Struct with Ulid
    #[derive(Debug, PartialEq, Facet)]
    struct EntityWithUlid {
        id: Ulid,
        name: String,
    }

    test_roundtrip!(
        struct_with_ulid,
        EntityWithUlid,
        EntityWithUlid {
            id: Ulid::nil(),
            name: "test".to_string(),
        }
    );
}

#[cfg(feature = "chrono")]
mod chrono_roundtrip {
    use super::*;
    use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, Utc};

    #[test]
    fn datetime_utc_now() {
        facet_testhelpers::setup();
        let original = Utc::now();
        let bytes = to_vec(&original).expect("serialization should succeed");
        let deserialized: DateTime<Utc> =
            from_slice(&bytes).expect("deserialization should succeed");
        assert_eq!(deserialized, original);
    }

    #[test]
    fn naive_date_roundtrip() {
        facet_testhelpers::setup();
        let original = NaiveDate::from_ymd_opt(2024, 12, 25).unwrap();
        let bytes = to_vec(&original).expect("serialization should succeed");
        let deserialized: NaiveDate = from_slice(&bytes).expect("deserialization should succeed");
        assert_eq!(deserialized, original);
    }

    #[test]
    fn naive_time_roundtrip() {
        facet_testhelpers::setup();
        let original = NaiveTime::from_hms_opt(12, 30, 45).unwrap();
        let bytes = to_vec(&original).expect("serialization should succeed");
        let deserialized: NaiveTime = from_slice(&bytes).expect("deserialization should succeed");
        assert_eq!(deserialized, original);
    }

    #[test]
    fn naive_datetime_roundtrip() {
        facet_testhelpers::setup();
        let date = NaiveDate::from_ymd_opt(2024, 12, 25).unwrap();
        let time = NaiveTime::from_hms_opt(12, 30, 45).unwrap();
        let original = NaiveDateTime::new(date, time);
        let bytes = to_vec(&original).expect("serialization should succeed");
        let deserialized: NaiveDateTime =
            from_slice(&bytes).expect("deserialization should succeed");
        assert_eq!(deserialized, original);
    }

    // Vec<DateTime>
    #[test]
    fn vec_datetime() {
        facet_testhelpers::setup();
        let original: Vec<DateTime<Utc>> = vec![Utc::now()];
        let bytes = to_vec(&original).expect("serialization should succeed");
        let deserialized: Vec<DateTime<Utc>> =
            from_slice(&bytes).expect("deserialization should succeed");
        assert_eq!(deserialized, original);
    }

    // Struct with DateTime
    #[derive(Debug, PartialEq, Facet)]
    struct Event {
        timestamp: DateTime<Utc>,
        name: String,
    }

    #[test]
    fn struct_with_datetime() {
        facet_testhelpers::setup();
        let original = Event {
            timestamp: Utc::now(),
            name: "test event".to_string(),
        };
        let bytes = to_vec(&original).expect("serialization should succeed");
        let deserialized: Event = from_slice(&bytes).expect("deserialization should succeed");
        assert_eq!(deserialized, original);
    }
}

#[cfg(feature = "jiff02")]
mod jiff_roundtrip {
    use super::*;
    use jiff::Timestamp;

    #[test]
    fn timestamp_now() {
        facet_testhelpers::setup();
        let original = Timestamp::now();
        let bytes = to_vec(&original).expect("serialization should succeed");
        let deserialized: Timestamp = from_slice(&bytes).expect("deserialization should succeed");
        assert_eq!(deserialized, original);
    }

    // Struct with Timestamp
    #[derive(Debug, PartialEq, Facet)]
    struct JiffEvent {
        timestamp: Timestamp,
        name: String,
    }

    #[test]
    fn struct_with_timestamp() {
        facet_testhelpers::setup();
        let original = JiffEvent {
            timestamp: Timestamp::now(),
            name: "test event".to_string(),
        };
        let bytes = to_vec(&original).expect("serialization should succeed");
        let deserialized: JiffEvent = from_slice(&bytes).expect("deserialization should succeed");
        assert_eq!(deserialized, original);
    }
}

#[cfg(feature = "time")]
mod time_roundtrip {
    use super::*;
    use time::OffsetDateTime;

    #[test]
    fn offset_datetime_now() {
        facet_testhelpers::setup();
        let original = OffsetDateTime::now_utc();
        let bytes = to_vec(&original).expect("serialization should succeed");
        let deserialized: OffsetDateTime =
            from_slice(&bytes).expect("deserialization should succeed");
        // Compare with some tolerance due to subsecond precision
        assert_eq!(deserialized.unix_timestamp(), original.unix_timestamp());
    }
}

#[cfg(feature = "bytes")]
mod bytes_roundtrip {
    use super::*;
    use bytes::{Bytes, BytesMut};

    test_roundtrip!(bytes_empty, Bytes, Bytes::new());
    test_roundtrip!(bytes_static, Bytes, Bytes::from_static(b"hello world"));

    test_roundtrip!(bytesmut_empty, BytesMut, BytesMut::new());
    test_roundtrip!(bytesmut_data, BytesMut, BytesMut::from(&b"hello"[..]));

    // Vec<Bytes>
    test_roundtrip!(
        vec_bytes,
        Vec<Bytes>,
        vec![Bytes::from_static(b"a"), Bytes::from_static(b"b")]
    );

    // Struct with Bytes
    #[derive(Debug, PartialEq, Facet)]
    struct BinaryData {
        data: Bytes,
        name: String,
    }

    test_roundtrip!(
        struct_with_bytes,
        BinaryData,
        BinaryData {
            data: Bytes::from_static(b"binary content"),
            name: "test".to_string(),
        }
    );
}

#[cfg(feature = "ordered-float")]
mod ordered_float_roundtrip {
    use super::*;
    use ordered_float::{NotNan, OrderedFloat};

    test_roundtrip!(ordered_f32, OrderedFloat<f32>, OrderedFloat(1.5f32));
    test_roundtrip!(ordered_f64, OrderedFloat<f64>, OrderedFloat(2.5f64));
    test_roundtrip!(
        ordered_infinity,
        OrderedFloat<f64>,
        OrderedFloat(f64::INFINITY)
    );

    test_roundtrip!(notnan_f32, NotNan<f32>, NotNan::new(1.5f32).unwrap());
    test_roundtrip!(notnan_f64, NotNan<f64>, NotNan::new(2.5f64).unwrap());

    // Vec<OrderedFloat>
    test_roundtrip!(
        vec_ordered_float,
        Vec<OrderedFloat<f64>>,
        vec![OrderedFloat(1.0), OrderedFloat(2.0), OrderedFloat(3.0)]
    );

    // Struct with OrderedFloat
    #[derive(Debug, PartialEq, Facet)]
    struct Measurement {
        value: OrderedFloat<f64>,
        unit: String,
    }

    test_roundtrip!(
        struct_with_ordered_float,
        Measurement,
        Measurement {
            value: OrderedFloat(42.5),
            unit: "meters".to_string(),
        }
    );

    // HashMap with OrderedFloat values (useful for storing measurements)
    test_roundtrip!(
        hashmap_ordered_float,
        HashMap<String, OrderedFloat<f64>>,
        [
            ("temp".to_string(), OrderedFloat(98.6)),
            ("pressure".to_string(), OrderedFloat(1013.25)),
        ]
        .into_iter()
        .collect()
    );
}
