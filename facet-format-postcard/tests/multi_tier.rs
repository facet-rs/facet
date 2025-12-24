//! Multi-tier tests for facet-format-postcard.
//!
//! This test file runs the same test cases against all three deserialization tiers:
//! - **Tier 0**: Pure reflection/event-based deserialization via `FormatDeserializer`
//! - **Tier 1**: Shape JIT - compiles the event consumer (uses `FormatParser` events)
//! - **Tier 2**: Format JIT - compiles format-specific byte parsing directly
//!
//! The tests are designed to identify which features work at which tier, helping
//! guide implementation priorities.

#![cfg(feature = "jit")]

use facet::Facet;
use facet_format_postcard::{PostcardError, PostcardParser, from_slice};
use postcard::to_allocvec as postcard_to_vec;
use serde::{Deserialize, Serialize};

/// Helper to test deserialization at a specific tier
mod tier_helpers {
    use super::*;
    use facet_format::{DeserializeError, FormatDeserializer};

    /// Deserialize using Tier-0 (pure reflection, no JIT)
    pub fn deserialize_tier0<'de, T>(input: &'de [u8]) -> Result<T, DeserializeError<PostcardError>>
    where
        T: Facet<'de>,
    {
        let parser = PostcardParser::new(input);
        let mut de = FormatDeserializer::new(parser);
        de.deserialize()
    }

    /// Deserialize using Tier-1 (shape JIT with event stream)
    #[allow(dead_code)]
    pub fn deserialize_tier1<'de, T>(input: &'de [u8]) -> Result<T, DeserializeError<PostcardError>>
    where
        T: Facet<'de> + core::fmt::Debug,
    {
        let mut parser = PostcardParser::new(input);
        match facet_format::jit::try_deserialize::<T, _>(&mut parser) {
            Some(result) => result,
            None => Err(DeserializeError::Unsupported(
                "Tier-1 JIT not supported for this type".into(),
            )),
        }
    }

    /// Deserialize using Tier-2 (format JIT - direct byte parsing)
    pub fn deserialize_tier2<'de, T>(input: &'de [u8]) -> Result<T, DeserializeError<PostcardError>>
    where
        T: Facet<'de>,
    {
        from_slice(input)
    }
}

use tier_helpers::*;

// =============================================================================
// Macro for multi-tier testing
// =============================================================================

/// Test a type at all tiers, comparing against postcard reference implementation.
///
/// Usage: test_all_tiers!(test_name, Type, value);
macro_rules! test_all_tiers {
    ($name:ident, $ty:ty, $value:expr) => {
        mod $name {
            use super::*;

            fn get_value() -> $ty {
                $value
            }

            fn get_encoded() -> Vec<u8> {
                postcard_to_vec(&get_value()).expect("postcard should encode")
            }

            #[test]
            fn tier0_reflection() {
                facet_testhelpers::setup();
                let encoded = get_encoded();
                let result: Result<$ty, _> = deserialize_tier0(&encoded);
                match result {
                    Ok(decoded) => assert_eq!(decoded, get_value(), "Tier-0 decoded wrong value"),
                    Err(e) => panic!("Tier-0 failed: {}", e),
                }
            }

            // Tier-1 tests are commented out until FormatParser is implemented
            // #[test]
            // fn tier1_shape_jit() {
            //     facet_testhelpers::setup();
            //     let encoded = get_encoded();
            //     let result: Result<$ty, _> = deserialize_tier1(&encoded);
            //     match result {
            //         Ok(decoded) => assert_eq!(decoded, get_value(), "Tier-1 decoded wrong value"),
            //         Err(e) => panic!("Tier-1 failed: {}", e),
            //     }
            // }

            #[test]
            fn tier2_format_jit() {
                facet_testhelpers::setup();
                let encoded = get_encoded();
                let result: Result<$ty, _> = deserialize_tier2(&encoded);
                match result {
                    Ok(decoded) => assert_eq!(decoded, get_value(), "Tier-2 decoded wrong value"),
                    Err(e) => panic!("Tier-2 failed: {}", e),
                }
            }
        }
    };
}

/// Test a type only at Tier-2 (for types where Tier-0/1 aren't implemented yet)
macro_rules! test_tier2_only {
    ($name:ident, $ty:ty, $value:expr) => {
        mod $name {
            use super::*;

            fn get_value() -> $ty {
                $value
            }

            fn get_encoded() -> Vec<u8> {
                postcard_to_vec(&get_value()).expect("postcard should encode")
            }

            #[test]
            fn tier2_format_jit() {
                facet_testhelpers::setup();
                let encoded = get_encoded();
                let result: Result<$ty, _> = deserialize_tier2(&encoded);
                match result {
                    Ok(decoded) => assert_eq!(decoded, get_value(), "Tier-2 decoded wrong value"),
                    Err(e) => panic!("Tier-2 failed: {}", e),
                }
            }
        }
    };
}

// =============================================================================
// Primitive types
// =============================================================================

mod primitives {
    use super::*;

    // Wrapper structs for testing primitives in struct context
    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct WrapU8 {
        value: u8,
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct WrapU16 {
        value: u16,
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct WrapU32 {
        value: u32,
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct WrapU64 {
        value: u64,
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct WrapI8 {
        value: i8,
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct WrapI16 {
        value: i16,
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct WrapI32 {
        value: i32,
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct WrapI64 {
        value: i64,
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct WrapBool {
        value: bool,
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct WrapF32 {
        value: f32,
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct WrapF64 {
        value: f64,
    }

    // u8 tests
    test_all_tiers!(u8_zero, WrapU8, WrapU8 { value: 0 });
    test_all_tiers!(u8_one, WrapU8, WrapU8 { value: 1 });
    test_all_tiers!(u8_max, WrapU8, WrapU8 { value: u8::MAX });
    test_all_tiers!(u8_mid, WrapU8, WrapU8 { value: 128 });

    // u16 tests
    test_all_tiers!(u16_zero, WrapU16, WrapU16 { value: 0 });
    test_all_tiers!(u16_small, WrapU16, WrapU16 { value: 127 });
    test_all_tiers!(u16_boundary, WrapU16, WrapU16 { value: 128 });
    test_all_tiers!(u16_max, WrapU16, WrapU16 { value: u16::MAX });

    // u32 tests
    test_all_tiers!(u32_zero, WrapU32, WrapU32 { value: 0 });
    test_all_tiers!(u32_small, WrapU32, WrapU32 { value: 42 });
    test_all_tiers!(u32_large, WrapU32, WrapU32 { value: 100_000 });
    test_all_tiers!(u32_max, WrapU32, WrapU32 { value: u32::MAX });

    // u64 tests
    test_all_tiers!(u64_zero, WrapU64, WrapU64 { value: 0 });
    test_all_tiers!(u64_small, WrapU64, WrapU64 { value: 255 });
    test_all_tiers!(
        u64_large,
        WrapU64,
        WrapU64 {
            value: u64::MAX / 2
        }
    );
    test_all_tiers!(u64_max, WrapU64, WrapU64 { value: u64::MAX });

    // i8 tests
    test_all_tiers!(i8_zero, WrapI8, WrapI8 { value: 0 });
    test_all_tiers!(i8_positive, WrapI8, WrapI8 { value: 127 });
    test_all_tiers!(i8_negative, WrapI8, WrapI8 { value: -128 });

    // i32 tests
    test_all_tiers!(i32_zero, WrapI32, WrapI32 { value: 0 });
    test_all_tiers!(i32_positive, WrapI32, WrapI32 { value: 1000 });
    test_all_tiers!(i32_negative, WrapI32, WrapI32 { value: -1000 });
    test_all_tiers!(i32_min, WrapI32, WrapI32 { value: i32::MIN });
    test_all_tiers!(i32_max, WrapI32, WrapI32 { value: i32::MAX });

    // i64 tests
    test_all_tiers!(i64_zero, WrapI64, WrapI64 { value: 0 });
    test_all_tiers!(
        i64_positive,
        WrapI64,
        WrapI64 {
            value: i64::MAX / 2
        }
    );
    test_all_tiers!(
        i64_negative,
        WrapI64,
        WrapI64 {
            value: i64::MIN / 2
        }
    );

    // bool tests
    test_all_tiers!(bool_true, WrapBool, WrapBool { value: true });
    test_all_tiers!(bool_false, WrapBool, WrapBool { value: false });

    // f32 tests
    test_all_tiers!(f32_zero, WrapF32, WrapF32 { value: 0.0 });
    test_all_tiers!(f32_positive, WrapF32, WrapF32 { value: 1.5 });
    test_all_tiers!(f32_negative, WrapF32, WrapF32 { value: -2.5 });

    // f64 tests
    test_all_tiers!(f64_zero, WrapF64, WrapF64 { value: 0.0 });
    test_all_tiers!(f64_positive, WrapF64, WrapF64 { value: 1.23456789 });
    test_all_tiers!(f64_negative, WrapF64, WrapF64 { value: -9.87654321 });
}

// =============================================================================
// Vec types
// =============================================================================

mod vecs {
    use super::*;

    // Vec<bool> - now Tier-0 supported with hint_sequence
    test_all_tiers!(vec_bool_empty, Vec<bool>, vec![]);
    test_all_tiers!(vec_bool_single, Vec<bool>, vec![true]);
    test_all_tiers!(vec_bool_multiple, Vec<bool>, vec![true, false, true, false]);

    // Vec<u8> - now Tier-0 supported with hint_sequence
    test_all_tiers!(vec_u8_empty, Vec<u8>, vec![]);
    test_all_tiers!(vec_u8_single, Vec<u8>, vec![42]);
    test_all_tiers!(vec_u8_multiple, Vec<u8>, vec![0, 128, 255]);

    // Vec<u32> - now Tier-0 supported with hint_sequence
    test_all_tiers!(vec_u32_empty, Vec<u32>, vec![]);
    test_all_tiers!(vec_u32_single, Vec<u32>, vec![42]);
    test_all_tiers!(vec_u32_multiple, Vec<u32>, vec![1, 2, 3, 4, 5]);
    test_all_tiers!(vec_u32_large, Vec<u32>, (0..100).collect::<Vec<_>>());

    // Vec<u64> - now Tier-0 supported with hint_sequence
    test_all_tiers!(vec_u64_empty, Vec<u64>, vec![]);
    test_all_tiers!(vec_u64_single, Vec<u64>, vec![u64::MAX]);
    test_all_tiers!(
        vec_u64_multiple,
        Vec<u64>,
        vec![0, 1, u64::MAX / 2, u64::MAX]
    );

    // Vec<i32>
    test_tier2_only!(vec_i32_empty, Vec<i32>, vec![]);
    test_tier2_only!(vec_i32_positive, Vec<i32>, vec![1, 2, 3]);
    test_tier2_only!(vec_i32_negative, Vec<i32>, vec![-1, -2, -3]);
    test_tier2_only!(
        vec_i32_mixed,
        Vec<i32>,
        vec![-100, 0, 100, i32::MIN, i32::MAX]
    );

    // Vec<i64>
    test_tier2_only!(vec_i64_empty, Vec<i64>, vec![]);
    test_tier2_only!(vec_i64_mixed, Vec<i64>, vec![i64::MIN, -1, 0, 1, i64::MAX]);
}

// =============================================================================
// String types
// =============================================================================

mod strings {
    use super::*;

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct WrapString {
        value: String,
    }

    test_tier2_only!(
        string_empty,
        WrapString,
        WrapString {
            value: String::new()
        }
    );
    test_tier2_only!(
        string_ascii,
        WrapString,
        WrapString {
            value: "hello".to_string()
        }
    );
    test_tier2_only!(
        string_unicode,
        WrapString,
        WrapString {
            value: "こんにちは🦀".to_string()
        }
    );
    test_tier2_only!(
        string_long,
        WrapString,
        WrapString {
            value: "a".repeat(1000)
        }
    );
}

// =============================================================================
// Struct types
// =============================================================================

mod structs {
    use super::*;

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct UnitStruct;

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct SingleField {
        value: u32,
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct MultiField {
        a: u32,
        b: String,
        c: bool,
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct Nested {
        inner: SingleField,
        name: String,
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct WithOption {
        required: u32,
        optional: Option<String>,
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct WithVec {
        values: Vec<u32>,
    }

    test_tier2_only!(unit_struct, UnitStruct, UnitStruct);
    test_tier2_only!(single_field, SingleField, SingleField { value: 42 });
    test_tier2_only!(
        multi_field,
        MultiField,
        MultiField {
            a: 1,
            b: "test".to_string(),
            c: true
        }
    );
    test_tier2_only!(
        nested_struct,
        Nested,
        Nested {
            inner: SingleField { value: 42 },
            name: "outer".to_string()
        }
    );
    // Structs with Option and Vec - now Tier-0 supported
    test_all_tiers!(
        option_some,
        WithOption,
        WithOption {
            required: 42,
            optional: Some("present".to_string())
        }
    );
    test_all_tiers!(
        option_none,
        WithOption,
        WithOption {
            required: 42,
            optional: None
        }
    );
    test_all_tiers!(
        with_vec,
        WithVec,
        WithVec {
            values: vec![1, 2, 3]
        }
    );
}

// =============================================================================
// Tuple struct types
// =============================================================================

mod tuple_structs {
    use super::*;

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct Newtype(u32);

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct TupleTwo(u32, String);

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct TupleThree(u8, u16, u32);

    test_tier2_only!(newtype_u32, Newtype, Newtype(42));
    test_tier2_only!(tuple_two, TupleTwo, TupleTwo(42, "hello".to_string()));
    test_tier2_only!(tuple_three, TupleThree, TupleThree(1, 1000, 100000));
}

// =============================================================================
// Enum types
// =============================================================================

mod enums {
    use super::*;

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    #[repr(u8)]
    enum UnitEnum {
        A,
        B,
        C,
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum NewtypeEnum {
        Unit,
        Number(u32),
        Text(String),
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum TupleEnum {
        Unit,
        Pair(u32, String),
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum StructEnum {
        Unit,
        Named { x: i32, y: i32 },
    }

    // Enum types - now Tier-0 supported with hint_enum
    test_all_tiers!(unit_enum_a, UnitEnum, UnitEnum::A);
    test_all_tiers!(unit_enum_b, UnitEnum, UnitEnum::B);
    test_all_tiers!(unit_enum_c, UnitEnum, UnitEnum::C);

    test_all_tiers!(newtype_enum_unit, NewtypeEnum, NewtypeEnum::Unit);
    test_all_tiers!(newtype_enum_number, NewtypeEnum, NewtypeEnum::Number(42));
    test_all_tiers!(
        newtype_enum_text,
        NewtypeEnum,
        NewtypeEnum::Text("hello".to_string())
    );

    test_all_tiers!(tuple_enum_unit, TupleEnum, TupleEnum::Unit);
    test_all_tiers!(
        tuple_enum_pair,
        TupleEnum,
        TupleEnum::Pair(42, "hello".to_string())
    );

    test_all_tiers!(struct_enum_unit, StructEnum, StructEnum::Unit);
    test_all_tiers!(
        struct_enum_named,
        StructEnum,
        StructEnum::Named { x: 10, y: -20 }
    );
}

// =============================================================================
// Collection types
// =============================================================================

mod collections {
    use super::*;
    use std::collections::{BTreeMap, HashMap};

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct WithHashMap {
        map: HashMap<String, u32>,
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct WithBTreeMap {
        map: BTreeMap<String, u32>,
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct NestedVec {
        matrix: Vec<Vec<u32>>,
    }

    test_tier2_only!(
        hashmap_empty,
        WithHashMap,
        WithHashMap {
            map: HashMap::new()
        }
    );
    test_tier2_only!(
        hashmap_single,
        WithHashMap,
        WithHashMap {
            map: [("key".to_string(), 42)].into_iter().collect()
        }
    );

    test_tier2_only!(
        btreemap_empty,
        WithBTreeMap,
        WithBTreeMap {
            map: BTreeMap::new()
        }
    );
    test_tier2_only!(
        btreemap_ordered,
        WithBTreeMap,
        WithBTreeMap {
            map: [
                ("alpha".to_string(), 1),
                ("beta".to_string(), 2),
                ("gamma".to_string(), 3),
            ]
            .into_iter()
            .collect()
        }
    );

    test_tier2_only!(nested_vec_empty, NestedVec, NestedVec { matrix: vec![] });
    test_tier2_only!(
        nested_vec_with_data,
        NestedVec,
        NestedVec {
            matrix: vec![vec![1, 2], vec![3, 4, 5], vec![6]]
        }
    );
}

// =============================================================================
// Option types
// =============================================================================

mod options {
    use super::*;

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct OptU32 {
        value: Option<u32>,
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct OptString {
        value: Option<String>,
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct OptVec {
        value: Option<Vec<u32>>,
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct NestedOpt {
        value: Option<Option<u32>>,
    }

    // Option types - now Tier-0 supported with hint_option
    test_all_tiers!(opt_u32_some, OptU32, OptU32 { value: Some(42) });
    test_all_tiers!(opt_u32_none, OptU32, OptU32 { value: None });

    test_all_tiers!(
        opt_string_some,
        OptString,
        OptString {
            value: Some("hello".to_string())
        }
    );
    test_all_tiers!(opt_string_none, OptString, OptString { value: None });

    test_all_tiers!(
        opt_vec_some,
        OptVec,
        OptVec {
            value: Some(vec![1, 2, 3])
        }
    );
    test_all_tiers!(opt_vec_none, OptVec, OptVec { value: None });

    test_all_tiers!(nested_opt_none, NestedOpt, NestedOpt { value: None });
    test_all_tiers!(
        nested_opt_some_none,
        NestedOpt,
        NestedOpt { value: Some(None) }
    );
    test_all_tiers!(
        nested_opt_some_some,
        NestedOpt,
        NestedOpt {
            value: Some(Some(42))
        }
    );
}

// =============================================================================
// Result types
// =============================================================================

mod results {
    use super::*;

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct ResU32 {
        value: Result<u32, String>,
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct ResString {
        value: Result<String, u32>,
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct ResVec {
        value: Result<Vec<u32>, String>,
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct CustomError {
        code: u32,
        message: String,
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct ResCustom {
        value: Result<i32, CustomError>,
    }

    // Result types - should work with Tier-0 like Option does
    test_all_tiers!(res_u32_ok, ResU32, ResU32 { value: Ok(42) });
    test_all_tiers!(
        res_u32_err,
        ResU32,
        ResU32 {
            value: Err("error message".to_string())
        }
    );

    test_all_tiers!(
        res_string_ok,
        ResString,
        ResString {
            value: Ok("success".to_string())
        }
    );
    test_all_tiers!(res_string_err, ResString, ResString { value: Err(404) });

    test_all_tiers!(
        res_vec_ok,
        ResVec,
        ResVec {
            value: Ok(vec![1, 2, 3])
        }
    );
    test_all_tiers!(
        res_vec_err,
        ResVec,
        ResVec {
            value: Err("failed".to_string())
        }
    );

    test_all_tiers!(res_custom_ok, ResCustom, ResCustom { value: Ok(42) });
    test_all_tiers!(
        res_custom_err,
        ResCustom,
        ResCustom {
            value: Err(CustomError {
                code: 500,
                message: "Internal Server Error".to_string()
            })
        }
    );
}

// =============================================================================
// Kitchen sink test (complex nested type)
// =============================================================================

mod kitchen_sink {
    use super::*;

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct KitchenSink {
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
        string_field: String,
        vec_field: Vec<u32>,
        option_field: Option<u32>,
    }

    test_tier2_only!(
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
            f32_field: 1.5,
            f64_field: 9.87654321,
            bool_field: true,
            string_field: "hello world".to_string(),
            vec_field: vec![1, 2, 3, 4, 5],
            option_field: Some(42),
        }
    );
}
