//! Roundtrip tests for facet-asn1.
//!
//! These tests verify that values can be serialized with facet-asn1
//! and then deserialized back to the same value.

use facet::Facet;
use facet_asn1::{from_slice, to_vec};

/// Helper macro to test round-trip serialization/deserialization.
macro_rules! test_roundtrip {
    ($name:ident, $ty:ty, $value:expr) => {
        #[test]
        fn $name() {
            facet_testhelpers::setup();

            let original: $ty = $value;

            // Serialize with facet-asn1
            let bytes = to_vec(&original).expect("serialization should succeed");

            // Deserialize with facet-asn1
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

    // Unit type - serialized as empty SEQUENCE
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
}

// =============================================================================
// String and Byte Types
// =============================================================================

mod strings_and_bytes {
    use super::*;

    // String
    test_roundtrip!(string_empty, String, String::new());
    test_roundtrip!(string_ascii, String, "Hello, World!".to_string());
    test_roundtrip!(string_unicode, String, "Hello World".to_string());

    // Vec<u8> (byte arrays)
    test_roundtrip!(bytes_empty, Vec<u8>, vec![]);
    test_roundtrip!(bytes_single, Vec<u8>, vec![42]);
    test_roundtrip!(bytes_sequence, Vec<u8>, vec![0, 1, 2, 3, 4, 5]);
}

// =============================================================================
// Collection Types - Vec
// =============================================================================

mod collections_vec {
    use super::*;

    test_roundtrip!(vec_u32_empty, Vec<u32>, vec![]);
    test_roundtrip!(vec_u32_single, Vec<u32>, vec![42]);
    test_roundtrip!(vec_u32_multiple, Vec<u32>, vec![1, 2, 3, 4, 5]);
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
}
