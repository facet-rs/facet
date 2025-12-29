//! Tests for fixed-size arrays and tuples

// Allow non-camel-case names like ArrayU8_4 to indicate array element type and size
#![allow(non_camel_case_types)]

use eyre::Result;
use facet::Facet;
use facet_postcard_legacy::{from_slice, to_vec};
use postcard::to_allocvec as postcard_to_vec;
use serde::{Deserialize, Serialize};

// ============================================================================
// Fixed-size array tests
// ============================================================================

mod array_tests {
    use super::*;

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct ArrayU8_4 {
        data: [u8; 4],
    }

    #[test]
    fn test_array_u8_4() -> Result<()> {
        facet_testhelpers::setup();
        let wrapper = ArrayU8_4 { data: [1, 2, 3, 4] };
        let facet_bytes = to_vec(&wrapper)?;
        let postcard_bytes = postcard_to_vec(&wrapper)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: ArrayU8_4 = from_slice(&facet_bytes)?;
        assert_eq!(wrapper, decoded);
        Ok(())
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct ArrayU32_3 {
        data: [u32; 3],
    }

    #[test]
    fn test_array_u32_3() -> Result<()> {
        facet_testhelpers::setup();
        let wrapper = ArrayU32_3 {
            data: [100, 200, 300],
        };
        let facet_bytes = to_vec(&wrapper)?;
        let postcard_bytes = postcard_to_vec(&wrapper)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: ArrayU32_3 = from_slice(&facet_bytes)?;
        assert_eq!(wrapper, decoded);
        Ok(())
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct ArrayU8_0 {
        data: [u8; 0],
    }

    #[test]
    fn test_empty_array() -> Result<()> {
        facet_testhelpers::setup();
        let wrapper = ArrayU8_0 { data: [] };
        let facet_bytes = to_vec(&wrapper)?;
        let postcard_bytes = postcard_to_vec(&wrapper)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: ArrayU8_0 = from_slice(&facet_bytes)?;
        assert_eq!(wrapper, decoded);
        Ok(())
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct ArrayU8_1 {
        data: [u8; 1],
    }

    #[test]
    fn test_single_element_array() -> Result<()> {
        facet_testhelpers::setup();
        let wrapper = ArrayU8_1 { data: [42] };
        let facet_bytes = to_vec(&wrapper)?;
        let postcard_bytes = postcard_to_vec(&wrapper)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: ArrayU8_1 = from_slice(&facet_bytes)?;
        assert_eq!(wrapper, decoded);
        Ok(())
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct ArrayU8_32 {
        data: [u8; 32],
    }

    #[test]
    fn test_larger_array() -> Result<()> {
        facet_testhelpers::setup();
        let wrapper = ArrayU8_32 {
            data: [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22,
                23, 24, 25, 26, 27, 28, 29, 30, 31,
            ],
        };
        let facet_bytes = to_vec(&wrapper)?;
        let postcard_bytes = postcard_to_vec(&wrapper)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: ArrayU8_32 = from_slice(&facet_bytes)?;
        assert_eq!(wrapper, decoded);
        Ok(())
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct ArrayString_2 {
        data: [String; 2],
    }

    #[test]
    fn test_array_of_strings() -> Result<()> {
        facet_testhelpers::setup();
        let wrapper = ArrayString_2 {
            data: ["hello".to_string(), "world".to_string()],
        };
        let facet_bytes = to_vec(&wrapper)?;
        let postcard_bytes = postcard_to_vec(&wrapper)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: ArrayString_2 = from_slice(&facet_bytes)?;
        assert_eq!(wrapper, decoded);
        Ok(())
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct NestedArray {
        data: [[u8; 3]; 2],
    }

    #[test]
    fn test_nested_array() -> Result<()> {
        facet_testhelpers::setup();
        let wrapper = NestedArray {
            data: [[1, 2, 3], [4, 5, 6]],
        };
        let facet_bytes = to_vec(&wrapper)?;
        let postcard_bytes = postcard_to_vec(&wrapper)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: NestedArray = from_slice(&facet_bytes)?;
        assert_eq!(wrapper, decoded);
        Ok(())
    }
}

// ============================================================================
// Tuple tests
// ============================================================================

mod tuple_tests {
    use super::*;

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct Tuple2 {
        data: (u32, String),
    }

    #[test]
    fn test_tuple_2() -> Result<()> {
        facet_testhelpers::setup();
        let wrapper = Tuple2 {
            data: (42, "hello".to_string()),
        };
        let facet_bytes = to_vec(&wrapper)?;
        let postcard_bytes = postcard_to_vec(&wrapper)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: Tuple2 = from_slice(&facet_bytes)?;
        assert_eq!(wrapper, decoded);
        Ok(())
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct Tuple3 {
        data: (u8, u16, u32),
    }

    #[test]
    fn test_tuple_3() -> Result<()> {
        facet_testhelpers::setup();
        let wrapper = Tuple3 {
            data: (1, 1000, 100000),
        };
        let facet_bytes = to_vec(&wrapper)?;
        let postcard_bytes = postcard_to_vec(&wrapper)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: Tuple3 = from_slice(&facet_bytes)?;
        assert_eq!(wrapper, decoded);
        Ok(())
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct Tuple1 {
        data: (u32,),
    }

    #[test]
    fn test_tuple_1() -> Result<()> {
        facet_testhelpers::setup();
        let wrapper = Tuple1 { data: (42,) };
        let facet_bytes = to_vec(&wrapper)?;
        let postcard_bytes = postcard_to_vec(&wrapper)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: Tuple1 = from_slice(&facet_bytes)?;
        assert_eq!(wrapper, decoded);
        Ok(())
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct Tuple4 {
        data: (bool, char, f32, f64),
    }

    #[test]
    fn test_tuple_4_mixed() -> Result<()> {
        facet_testhelpers::setup();
        let wrapper = Tuple4 {
            data: (true, 'X', 1.5, 9.99),
        };
        let facet_bytes = to_vec(&wrapper)?;
        let postcard_bytes = postcard_to_vec(&wrapper)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: Tuple4 = from_slice(&facet_bytes)?;
        assert_eq!(wrapper, decoded);
        Ok(())
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct NestedTuple {
        data: ((u32, u32), (String, String)),
    }

    #[test]
    fn test_nested_tuple() -> Result<()> {
        facet_testhelpers::setup();
        let wrapper = NestedTuple {
            data: ((1, 2), ("a".to_string(), "b".to_string())),
        };
        let facet_bytes = to_vec(&wrapper)?;
        let postcard_bytes = postcard_to_vec(&wrapper)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: NestedTuple = from_slice(&facet_bytes)?;
        assert_eq!(wrapper, decoded);
        Ok(())
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct TupleWithOption {
        data: (Option<u32>, Option<String>),
    }

    #[test]
    fn test_tuple_with_options() -> Result<()> {
        facet_testhelpers::setup();

        let wrapper = TupleWithOption {
            data: (Some(42), None),
        };
        let facet_bytes = to_vec(&wrapper)?;
        let postcard_bytes = postcard_to_vec(&wrapper)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: TupleWithOption = from_slice(&facet_bytes)?;
        assert_eq!(wrapper, decoded);
        Ok(())
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct TupleWithVec {
        data: (Vec<u32>, Vec<String>),
    }

    #[test]
    fn test_tuple_with_vecs() -> Result<()> {
        facet_testhelpers::setup();
        let wrapper = TupleWithVec {
            data: (vec![1, 2, 3], vec!["a".to_string(), "b".to_string()]),
        };
        let facet_bytes = to_vec(&wrapper)?;
        let postcard_bytes = postcard_to_vec(&wrapper)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: TupleWithVec = from_slice(&facet_bytes)?;
        assert_eq!(wrapper, decoded);
        Ok(())
    }
}

// ============================================================================
// Unit tuple / empty tuple tests
// ============================================================================

mod unit_tuple_tests {
    use super::*;

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct UnitTuple {
        data: (),
    }

    #[test]
    fn test_unit_tuple() -> Result<()> {
        facet_testhelpers::setup();
        let wrapper = UnitTuple { data: () };
        let facet_bytes = to_vec(&wrapper)?;
        let postcard_bytes = postcard_to_vec(&wrapper)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: UnitTuple = from_slice(&facet_bytes)?;
        assert_eq!(wrapper, decoded);
        Ok(())
    }
}
