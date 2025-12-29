//! Tests for all struct kinds

use eyre::Result;
use facet::Facet;
use facet_postcard_legacy::{from_slice, to_vec};
use postcard::from_bytes as postcard_from_slice;
use postcard::to_allocvec as postcard_to_vec;
use serde::{Deserialize, Serialize};

// ============================================================================
// Unit struct tests
// ============================================================================

mod unit_struct_tests {
    use super::*;

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct UnitStruct;

    #[test]
    fn test_unit_struct() -> Result<()> {
        facet_testhelpers::setup();
        let value = UnitStruct;
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: UnitStruct = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct WrapperWithUnit {
        marker: UnitStruct,
        value: u32,
    }

    #[test]
    fn test_struct_containing_unit() -> Result<()> {
        facet_testhelpers::setup();
        let value = WrapperWithUnit {
            marker: UnitStruct,
            value: 42,
        };
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: WrapperWithUnit = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }
}

// ============================================================================
// Tuple struct tests (newtype and multi-field)
// ============================================================================

mod tuple_struct_tests {
    use super::*;

    // Newtype (single-field tuple struct)
    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct NewtypeU32(u32);

    #[test]
    fn test_newtype_u32() -> Result<()> {
        facet_testhelpers::setup();
        let value = NewtypeU32(42);
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: NewtypeU32 = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct NewtypeString(String);

    #[test]
    fn test_newtype_string() -> Result<()> {
        facet_testhelpers::setup();
        let value = NewtypeString("hello".to_string());
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: NewtypeString = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct NewtypeVec(Vec<u32>);

    #[test]
    fn test_newtype_vec() -> Result<()> {
        facet_testhelpers::setup();
        let value = NewtypeVec(vec![1, 2, 3]);
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: NewtypeVec = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    // Multi-field tuple struct
    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct TupleStruct2(u32, String);

    #[test]
    fn test_tuple_struct_2() -> Result<()> {
        facet_testhelpers::setup();
        let value = TupleStruct2(42, "hello".to_string());
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: TupleStruct2 = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct TupleStruct3(u8, u16, u32);

    #[test]
    fn test_tuple_struct_3() -> Result<()> {
        facet_testhelpers::setup();
        let value = TupleStruct3(1, 1000, 100000);
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: TupleStruct3 = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    // Nested newtype
    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct NestedNewtype(NewtypeU32);

    #[test]
    fn test_nested_newtype() -> Result<()> {
        facet_testhelpers::setup();
        let value = NestedNewtype(NewtypeU32(42));
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: NestedNewtype = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }
}

// ============================================================================
// Named struct tests
// ============================================================================

mod named_struct_tests {
    use super::*;

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct SingleField {
        value: u32,
    }

    #[test]
    fn test_single_field() -> Result<()> {
        facet_testhelpers::setup();
        let value = SingleField { value: 42 };
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: SingleField = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct MultipleFields {
        a: u32,
        b: String,
        c: bool,
        d: f64,
    }

    #[test]
    fn test_multiple_fields() -> Result<()> {
        facet_testhelpers::setup();
        let value = MultipleFields {
            a: 42,
            b: "hello".to_string(),
            c: true,
            d: 1.5,
        };
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: MultipleFields = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct NestedStruct {
        inner: SingleField,
        name: String,
    }

    #[test]
    fn test_nested_struct() -> Result<()> {
        facet_testhelpers::setup();
        let value = NestedStruct {
            inner: SingleField { value: 42 },
            name: "outer".to_string(),
        };
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: NestedStruct = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct DeeplyNested {
        level1: NestedStruct,
        extra: Vec<u32>,
    }

    #[test]
    fn test_deeply_nested() -> Result<()> {
        facet_testhelpers::setup();
        let value = DeeplyNested {
            level1: NestedStruct {
                inner: SingleField { value: 42 },
                name: "deep".to_string(),
            },
            extra: vec![1, 2, 3],
        };
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: DeeplyNested = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct WithOptions {
        required: u32,
        optional: Option<String>,
        also_optional: Option<Vec<u32>>,
    }

    #[test]
    fn test_struct_with_options_all_some() -> Result<()> {
        facet_testhelpers::setup();
        let value = WithOptions {
            required: 42,
            optional: Some("present".to_string()),
            also_optional: Some(vec![1, 2, 3]),
        };
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: WithOptions = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[test]
    fn test_struct_with_options_all_none() -> Result<()> {
        facet_testhelpers::setup();
        let value = WithOptions {
            required: 42,
            optional: None,
            also_optional: None,
        };
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: WithOptions = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    // Struct with all the different field types
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
        char_field: char,
        string_field: String,
        vec_field: Vec<u32>,
        option_field: Option<u32>,
    }

    #[test]
    fn test_kitchen_sink() -> Result<()> {
        facet_testhelpers::setup();
        let value = KitchenSink {
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
            char_field: '🦀',
            string_field: "hello world".to_string(),
            vec_field: vec![1, 2, 3, 4, 5],
            option_field: Some(42),
        };
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: KitchenSink = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }
}

// ============================================================================
// Cross-compatibility tests
// ============================================================================

mod cross_compatibility {
    use super::*;

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct TestStruct {
        id: u32,
        name: String,
        active: bool,
    }

    #[test]
    fn test_facet_to_postcard() -> Result<()> {
        facet_testhelpers::setup();
        let value = TestStruct {
            id: 123,
            name: "test".to_string(),
            active: true,
        };

        let facet_bytes = to_vec(&value)?;
        let decoded: TestStruct = postcard_from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[test]
    fn test_postcard_to_facet() -> Result<()> {
        facet_testhelpers::setup();
        let value = TestStruct {
            id: 456,
            name: "another test".to_string(),
            active: false,
        };

        let postcard_bytes = postcard_to_vec(&value)?;
        let decoded: TestStruct = from_slice(&postcard_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }
}
