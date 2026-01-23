//! Cross-compatibility tests between facet-postcard and serde postcard.
//!
//! These tests verify:
//! 1. Byte-for-byte equality between facet and serde postcard serialization
//! 2. Facet-serialized data can be deserialized by serde postcard
//! 3. Serde postcard-serialized data can be deserialized by facet
//!
//! This is critical for ensuring interoperability in mixed codebases.

#![cfg(feature = "jit")]

use facet::Facet;
use facet_postcard::{from_slice, to_vec};
use postcard::from_bytes as postcard_from_slice;
use postcard::to_allocvec as postcard_to_vec;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Helper macro to test cross-compatibility for a type.
///
/// Creates tests that verify:
/// 1. Serialization produces identical bytes
/// 2. Facet can deserialize serde's output
/// 3. Serde can deserialize facet's output
macro_rules! test_cross_compat {
    ($name:ident, $facet_ty:ty, $serde_ty:ty, $values:expr) => {
        mod $name {
            use super::*;

            #[test]
            fn serialization_matches() {
                facet_testhelpers::setup();
                for value in $values {
                    let facet_bytes = to_vec(&value).expect("facet serialization failed");
                    let postcard_bytes =
                        postcard_to_vec(&value).expect("postcard serialization failed");
                    assert_eq!(
                        facet_bytes, postcard_bytes,
                        "Serialization mismatch for value {:?}",
                        value
                    );
                }
            }

            #[test]
            fn facet_to_serde() {
                facet_testhelpers::setup();
                for value in $values {
                    let facet_bytes = to_vec(&value).expect("facet serialization failed");
                    let decoded: $serde_ty = postcard_from_slice(&facet_bytes)
                        .expect("serde deserialization of facet bytes failed");
                    assert_eq!(
                        value, decoded,
                        "facet->serde roundtrip failed for {:?}",
                        value
                    );
                }
            }

            #[test]
            fn serde_to_facet() {
                facet_testhelpers::setup();
                for value in $values {
                    let postcard_bytes =
                        postcard_to_vec(&value).expect("postcard serialization failed");
                    let decoded: $facet_ty = from_slice(&postcard_bytes)
                        .expect("facet deserialization of serde bytes failed");
                    assert_eq!(
                        value, decoded,
                        "serde->facet roundtrip failed for {:?}",
                        value
                    );
                }
            }
        }
    };
}

/// Helper macro for types where Facet and Serde types are the same
macro_rules! test_cross_compat_same {
    ($name:ident, $ty:ty, $values:expr) => {
        test_cross_compat!($name, $ty, $ty, $values);
    };
}

// =============================================================================
// Primitive Types
// =============================================================================

mod primitives {
    use super::*;

    // Wrapper struct for primitives (needed for both Facet and Serde derives)
    #[derive(Debug, PartialEq, Clone, Facet, Serialize, Deserialize)]
    struct Wrap<T> {
        value: T,
    }

    fn wrap<T: Clone>(values: &[T]) -> Vec<Wrap<T>> {
        values.iter().map(|v| Wrap { value: v.clone() }).collect()
    }

    test_cross_compat_same!(u8_values, Wrap<u8>, wrap(&[0u8, 1, 127, 128, 255]));

    test_cross_compat_same!(
        u16_values,
        Wrap<u16>,
        wrap(&[0u16, 1, 127, 128, 255, 256, 1000, 65535])
    );

    test_cross_compat_same!(
        u32_values,
        Wrap<u32>,
        wrap(&[
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
        ])
    );

    test_cross_compat_same!(
        u64_values,
        Wrap<u64>,
        wrap(&[
            0u64,
            1,
            127,
            128,
            255,
            256,
            u32::MAX as u64,
            u32::MAX as u64 + 1,
            u64::MAX
        ])
    );

    test_cross_compat_same!(i8_values, Wrap<i8>, wrap(&[0i8, 1, -1, 127, -128]));

    test_cross_compat_same!(
        i16_values,
        Wrap<i16>,
        wrap(&[0i16, 1, -1, 127, -128, 1000, -1000, i16::MIN, i16::MAX])
    );

    test_cross_compat_same!(
        i32_values,
        Wrap<i32>,
        wrap(&[
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
        ])
    );

    test_cross_compat_same!(
        i64_values,
        Wrap<i64>,
        wrap(&[
            0i64,
            1,
            -1,
            i32::MIN as i64,
            i32::MAX as i64,
            i64::MIN,
            i64::MAX
        ])
    );

    test_cross_compat_same!(
        f32_values,
        Wrap<f32>,
        wrap(&[
            0.0f32,
            1.0,
            -1.0,
            1.5,
            -2.5,
            f32::MIN,
            f32::MAX,
            f32::EPSILON
        ])
    );

    test_cross_compat_same!(
        f64_values,
        Wrap<f64>,
        wrap(&[
            0.0f64,
            1.0,
            -1.0,
            1.23456789012345,
            f64::MIN,
            f64::MAX,
            f64::EPSILON
        ])
    );

    test_cross_compat_same!(bool_values, Wrap<bool>, wrap(&[true, false]));
}

// =============================================================================
// String Types
// =============================================================================

mod strings {
    use super::*;

    #[derive(Debug, PartialEq, Clone, Facet, Serialize, Deserialize)]
    struct StringWrapper {
        value: String,
    }

    test_cross_compat_same!(
        string_values,
        StringWrapper,
        [
            StringWrapper {
                value: String::new()
            },
            StringWrapper {
                value: "hello".to_string()
            },
            StringWrapper {
                value: "Hello, World!".to_string()
            },
            StringWrapper {
                value: "こんにちは世界".to_string()
            },
            StringWrapper {
                value: "🦀 Rust 🚀".to_string()
            },
            StringWrapper {
                value: "a".repeat(1000)
            }
        ]
    );
}

// =============================================================================
// Collection Types
// =============================================================================

mod collections {
    use super::*;

    #[derive(Debug, PartialEq, Clone, Facet, Serialize, Deserialize)]
    struct VecU32 {
        values: Vec<u32>,
    }

    test_cross_compat_same!(
        vec_u32,
        VecU32,
        [
            VecU32 { values: vec![] },
            VecU32 { values: vec![42] },
            VecU32 {
                values: vec![1, 2, 3, 4, 5]
            },
            VecU32 {
                values: (0..100).collect()
            }
        ]
    );

    #[derive(Debug, PartialEq, Clone, Facet, Serialize, Deserialize)]
    struct VecString {
        values: Vec<String>,
    }

    test_cross_compat_same!(
        vec_string,
        VecString,
        [
            VecString { values: vec![] },
            VecString {
                values: vec!["hello".to_string()]
            },
            VecString {
                values: vec!["a".to_string(), "b".to_string(), "c".to_string()]
            }
        ]
    );

    #[derive(Debug, PartialEq, Clone, Facet, Serialize, Deserialize)]
    struct VecBytes {
        values: Vec<u8>,
    }

    test_cross_compat_same!(
        vec_bytes,
        VecBytes,
        [
            VecBytes { values: vec![] },
            VecBytes {
                values: vec![0, 1, 2, 3]
            },
            VecBytes {
                values: (0..=255).collect()
            }
        ]
    );

    #[derive(Debug, PartialEq, Clone, Facet, Serialize, Deserialize)]
    struct NestedVec {
        values: Vec<Vec<u32>>,
    }

    test_cross_compat_same!(
        nested_vec,
        NestedVec,
        [
            NestedVec { values: vec![] },
            NestedVec {
                values: vec![vec![1, 2], vec![3, 4, 5], vec![]]
            }
        ]
    );

    // Note: HashMap order is not guaranteed, so we use BTreeMap for deterministic tests
    #[derive(Debug, PartialEq, Clone, Facet, Serialize, Deserialize)]
    struct BTreeMapWrapper {
        map: BTreeMap<String, u32>,
    }

    test_cross_compat_same!(
        btreemap,
        BTreeMapWrapper,
        [
            BTreeMapWrapper {
                map: BTreeMap::new()
            },
            BTreeMapWrapper {
                map: [("key".to_string(), 42)].into_iter().collect()
            },
            BTreeMapWrapper {
                map: [
                    ("alpha".to_string(), 1),
                    ("beta".to_string(), 2),
                    ("gamma".to_string(), 3),
                ]
                .into_iter()
                .collect()
            }
        ]
    );
}

// =============================================================================
// Option Types
// =============================================================================

mod options {
    use super::*;

    #[derive(Debug, PartialEq, Clone, Facet, Serialize, Deserialize)]
    struct OptionU32 {
        value: Option<u32>,
    }

    test_cross_compat_same!(
        option_u32,
        OptionU32,
        [
            OptionU32 { value: None },
            OptionU32 { value: Some(0) },
            OptionU32 { value: Some(42) },
            OptionU32 {
                value: Some(u32::MAX)
            }
        ]
    );

    #[derive(Debug, PartialEq, Clone, Facet, Serialize, Deserialize)]
    struct OptionString {
        value: Option<String>,
    }

    test_cross_compat_same!(
        option_string,
        OptionString,
        [
            OptionString { value: None },
            OptionString {
                value: Some(String::new())
            },
            OptionString {
                value: Some("hello".to_string())
            }
        ]
    );

    #[derive(Debug, PartialEq, Clone, Facet, Serialize, Deserialize)]
    struct NestedOption {
        value: Option<Option<u32>>,
    }

    test_cross_compat_same!(
        nested_option,
        NestedOption,
        [
            NestedOption { value: None },
            NestedOption { value: Some(None) },
            NestedOption {
                value: Some(Some(42))
            }
        ]
    );
}

// =============================================================================
// Result Types
// =============================================================================

mod results {
    use super::*;

    #[derive(Debug, PartialEq, Clone, Facet, Serialize, Deserialize)]
    struct ResultWrapper {
        value: Result<u32, String>,
    }

    test_cross_compat_same!(
        result_u32_string,
        ResultWrapper,
        [
            ResultWrapper { value: Ok(42) },
            ResultWrapper { value: Ok(0) },
            ResultWrapper {
                value: Err("error".to_string())
            },
            ResultWrapper {
                value: Err(String::new())
            }
        ]
    );
}

// =============================================================================
// Struct Types
// =============================================================================

mod structs {
    use super::*;

    #[derive(Debug, PartialEq, Clone, Facet, Serialize, Deserialize)]
    struct UnitStruct;

    test_cross_compat_same!(unit_struct, UnitStruct, [UnitStruct]);

    #[derive(Debug, PartialEq, Clone, Facet, Serialize, Deserialize)]
    struct Point {
        x: i32,
        y: i32,
    }

    test_cross_compat_same!(
        point,
        Point,
        [
            Point { x: 0, y: 0 },
            Point { x: 10, y: -20 },
            Point {
                x: i32::MIN,
                y: i32::MAX
            }
        ]
    );

    #[derive(Debug, PartialEq, Clone, Facet, Serialize, Deserialize)]
    struct Person {
        name: String,
        age: u32,
        active: bool,
    }

    test_cross_compat_same!(
        person,
        Person,
        [
            Person {
                name: "Alice".to_string(),
                age: 30,
                active: true
            },
            Person {
                name: "Bob".to_string(),
                age: 0,
                active: false
            },
            Person {
                name: "".to_string(),
                age: u32::MAX,
                active: true
            }
        ]
    );

    #[derive(Debug, PartialEq, Clone, Facet, Serialize, Deserialize)]
    struct TupleStruct(u32, String);

    test_cross_compat_same!(
        tuple_struct,
        TupleStruct,
        [
            TupleStruct(0, String::new()),
            TupleStruct(42, "hello".to_string())
        ]
    );

    #[derive(Debug, PartialEq, Clone, Facet, Serialize, Deserialize)]
    struct Newtype(u64);

    test_cross_compat_same!(
        newtype,
        Newtype,
        [Newtype(0), Newtype(42), Newtype(u64::MAX)]
    );

    #[derive(Debug, PartialEq, Clone, Facet, Serialize, Deserialize)]
    struct Nested {
        name: String,
        point: Point,
    }

    test_cross_compat_same!(
        nested,
        Nested,
        [
            Nested {
                name: "origin".to_string(),
                point: Point { x: 0, y: 0 }
            },
            Nested {
                name: "test".to_string(),
                point: Point { x: 100, y: -50 }
            }
        ]
    );
}

// =============================================================================
// Enum Types
// =============================================================================

mod enums {
    use super::*;

    #[derive(Debug, PartialEq, Clone, Facet, Serialize, Deserialize)]
    #[repr(u8)]
    enum Color {
        Red,
        Green,
        Blue,
    }

    test_cross_compat_same!(unit_enum, Color, [Color::Red, Color::Green, Color::Blue]);

    #[derive(Debug, PartialEq, Clone, Facet, Serialize, Deserialize)]
    #[repr(u8)]
    enum Message {
        Quit,
        Text(String),
        Number(u32),
    }

    test_cross_compat_same!(
        newtype_enum,
        Message,
        [
            Message::Quit,
            Message::Text("hello".to_string()),
            Message::Number(42)
        ]
    );

    #[derive(Debug, PartialEq, Clone, Facet, Serialize, Deserialize)]
    #[repr(u8)]
    enum TupleEnum {
        Empty,
        Pair(u32, String),
    }

    test_cross_compat_same!(
        tuple_enum,
        TupleEnum,
        [TupleEnum::Empty, TupleEnum::Pair(42, "test".to_string())]
    );

    #[derive(Debug, PartialEq, Clone, Facet, Serialize, Deserialize)]
    #[repr(u8)]
    enum StructEnum {
        Unit,
        Point { x: i32, y: i32 },
    }

    test_cross_compat_same!(
        struct_enum,
        StructEnum,
        [StructEnum::Unit, StructEnum::Point { x: 10, y: -20 }]
    );
}

// =============================================================================
// Tuple Types
// =============================================================================

mod tuples {
    use super::*;

    #[derive(Debug, PartialEq, Clone, Facet, Serialize, Deserialize)]
    struct WrapPair {
        value: (u32, String),
    }

    test_cross_compat_same!(
        pair,
        WrapPair,
        [
            WrapPair {
                value: (0, String::new())
            },
            WrapPair {
                value: (42, "hello".to_string())
            }
        ]
    );

    #[derive(Debug, PartialEq, Clone, Facet, Serialize, Deserialize)]
    struct WrapTriple {
        value: (u32, String, bool),
    }

    test_cross_compat_same!(
        triple,
        WrapTriple,
        [
            WrapTriple {
                value: (0, String::new(), false)
            },
            WrapTriple {
                value: (42, "test".to_string(), true)
            }
        ]
    );
}

// =============================================================================
// Complex Types (Kitchen Sink)
// =============================================================================

mod complex {
    use super::*;

    #[derive(Debug, PartialEq, Clone, Facet, Serialize, Deserialize)]
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

    test_cross_compat_same!(
        kitchen_sink,
        KitchenSink,
        [
            KitchenSink {
                u8_field: 255,
                u16_field: 65535,
                u32_field: u32::MAX,
                u64_field: u64::MAX,
                i8_field: -128,
                i16_field: -32768,
                i32_field: i32::MIN,
                i64_field: i64::MIN,
                f32_field: std::f32::consts::PI,
                f64_field: std::f64::consts::E,
                bool_field: true,
                string_field: "Hello, World!".to_string(),
                vec_field: vec![1, 2, 3, 4, 5],
                option_field: Some(42),
            },
            KitchenSink {
                u8_field: 0,
                u16_field: 0,
                u32_field: 0,
                u64_field: 0,
                i8_field: 0,
                i16_field: 0,
                i32_field: 0,
                i64_field: 0,
                f32_field: 0.0,
                f64_field: 0.0,
                bool_field: false,
                string_field: String::new(),
                vec_field: vec![],
                option_field: None,
            }
        ]
    );
}
