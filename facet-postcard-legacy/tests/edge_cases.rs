//! Edge case tests - empty values, max values, deeply nested structures

use eyre::Result;
use facet::Facet;
use facet_postcard_legacy::{from_slice, to_vec};
use postcard::to_allocvec as postcard_to_vec;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ============================================================================
// Empty value tests
// ============================================================================

mod empty_tests {
    use super::*;

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct AllEmpty {
        string: String,
        vec: Vec<u32>,
        map: BTreeMap<String, u32>,
    }

    #[test]
    fn test_all_empty() -> Result<()> {
        facet_testhelpers::setup();
        let value = AllEmpty {
            string: String::new(),
            vec: vec![],
            map: BTreeMap::new(),
        };
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: AllEmpty = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct EmptyNested {
        outer: Vec<Vec<Vec<u32>>>,
    }

    #[test]
    fn test_empty_nested_vecs() -> Result<()> {
        facet_testhelpers::setup();
        let value = EmptyNested {
            outer: vec![vec![vec![]], vec![], vec![vec![], vec![]]],
        };
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: EmptyNested = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }
}

// ============================================================================
// Max value tests
// ============================================================================

mod max_value_tests {
    use super::*;

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct MaxIntegers {
        u8_max: u8,
        u16_max: u16,
        u32_max: u32,
        u64_max: u64,
        i8_max: i8,
        i8_min: i8,
        i16_max: i16,
        i16_min: i16,
        i32_max: i32,
        i32_min: i32,
        i64_max: i64,
        i64_min: i64,
    }

    #[test]
    fn test_max_integers() -> Result<()> {
        facet_testhelpers::setup();
        let value = MaxIntegers {
            u8_max: u8::MAX,
            u16_max: u16::MAX,
            u32_max: u32::MAX,
            u64_max: u64::MAX,
            i8_max: i8::MAX,
            i8_min: i8::MIN,
            i16_max: i16::MAX,
            i16_min: i16::MIN,
            i32_max: i32::MAX,
            i32_min: i32::MIN,
            i64_max: i64::MAX,
            i64_min: i64::MIN,
        };
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: MaxIntegers = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct Max128 {
        u128_max: u128,
        i128_max: i128,
        i128_min: i128,
    }

    #[test]
    fn test_max_128() -> Result<()> {
        facet_testhelpers::setup();
        let value = Max128 {
            u128_max: u128::MAX,
            i128_max: i128::MAX,
            i128_min: i128::MIN,
        };
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: Max128 = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }
}

// ============================================================================
// Deeply nested tests
// ============================================================================

mod deeply_nested_tests {
    use super::*;

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct Level1 {
        value: u32,
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct Level2 {
        inner: Level1,
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct Level3 {
        inner: Level2,
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct Level4 {
        inner: Level3,
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct Level5 {
        inner: Level4,
    }

    #[test]
    fn test_5_levels_deep() -> Result<()> {
        facet_testhelpers::setup();
        let value = Level5 {
            inner: Level4 {
                inner: Level3 {
                    inner: Level2 {
                        inner: Level1 { value: 42 },
                    },
                },
            },
        };
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: Level5 = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct DeeplyNestedOptions {
        value: Option<Option<Option<Option<Option<u32>>>>>,
    }

    #[test]
    fn test_deeply_nested_options_all_some() -> Result<()> {
        facet_testhelpers::setup();
        let value = DeeplyNestedOptions {
            value: Some(Some(Some(Some(Some(42))))),
        };
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: DeeplyNestedOptions = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[test]
    fn test_deeply_nested_options_partial() -> Result<()> {
        facet_testhelpers::setup();
        let value = DeeplyNestedOptions {
            value: Some(Some(Some(None))),
        };
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: DeeplyNestedOptions = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct DeeplyNestedVecs {
        value: Vec<Vec<Vec<Vec<u32>>>>,
    }

    #[test]
    fn test_deeply_nested_vecs() -> Result<()> {
        facet_testhelpers::setup();
        let value = DeeplyNestedVecs {
            value: vec![
                vec![vec![vec![1, 2], vec![3]], vec![vec![4, 5, 6]]],
                vec![vec![vec![7]]],
            ],
        };
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: DeeplyNestedVecs = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }
}

// ============================================================================
// Large data tests
// ============================================================================

mod large_data_tests {
    use super::*;

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct LargeVec {
        data: Vec<u32>,
    }

    #[test]
    fn test_large_vec_1000() -> Result<()> {
        facet_testhelpers::setup();
        let value = LargeVec {
            data: (0..1000).collect(),
        };
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: LargeVec = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[test]
    fn test_large_vec_10000() -> Result<()> {
        facet_testhelpers::setup();
        let value = LargeVec {
            data: (0..10000).collect(),
        };
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: LargeVec = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct LargeMap {
        data: BTreeMap<u32, String>,
    }

    #[test]
    fn test_large_map() -> Result<()> {
        facet_testhelpers::setup();
        let mut map = BTreeMap::new();
        for i in 0..1000 {
            map.insert(i, format!("value_{i}"));
        }
        let value = LargeMap { data: map };

        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: LargeMap = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }
}

// ============================================================================
// Varint boundary tests
// ============================================================================

mod varint_boundary_tests {
    use super::*;

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct VarintBoundary {
        value: u64,
    }

    // Test values at varint encoding boundaries
    const BOUNDARY_VALUES: [u64; 12] = [
        0,
        127,
        128,             // 2-byte varint starts
        16383,           // max 2-byte
        16384,           // 3-byte varint starts
        2097151,         // max 3-byte
        2097152,         // 4-byte varint starts
        268435455,       // max 4-byte
        268435456,       // 5-byte varint starts
        u32::MAX as u64, // 32-bit boundary
        u32::MAX as u64 + 1,
        u64::MAX,
    ];

    #[test]
    fn test_varint_boundaries() -> Result<()> {
        facet_testhelpers::setup();
        for &val in &BOUNDARY_VALUES {
            let value = VarintBoundary { value: val };
            let facet_bytes = to_vec(&value)?;
            let postcard_bytes = postcard_to_vec(&value)?;
            assert_eq!(
                facet_bytes, postcard_bytes,
                "Mismatch at boundary value {val}"
            );

            let decoded: VarintBoundary = from_slice(&facet_bytes)?;
            assert_eq!(value, decoded, "Roundtrip failed for value {val}");
        }
        Ok(())
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct SignedVarintBoundary {
        value: i64,
    }

    // Test zigzag encoding boundaries
    const SIGNED_BOUNDARY_VALUES: [i64; 16] = [
        0,
        -1,
        1,
        -64,
        63,
        -65,
        64,
        -8192,
        8191,
        -8193,
        8192,
        i32::MIN as i64,
        i32::MAX as i64,
        i64::MIN,
        i64::MAX,
        -i64::MAX,
    ];

    #[test]
    fn test_signed_varint_boundaries() -> Result<()> {
        facet_testhelpers::setup();
        for &val in &SIGNED_BOUNDARY_VALUES {
            let value = SignedVarintBoundary { value: val };
            let facet_bytes = to_vec(&value)?;
            let postcard_bytes = postcard_to_vec(&value)?;
            assert_eq!(
                facet_bytes, postcard_bytes,
                "Mismatch at signed boundary value {val}"
            );

            let decoded: SignedVarintBoundary = from_slice(&facet_bytes)?;
            assert_eq!(value, decoded, "Roundtrip failed for signed value {val}");
        }
        Ok(())
    }
}

// ============================================================================
// Special character tests
// ============================================================================

mod special_char_tests {
    use super::*;

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct SpecialStrings {
        value: String,
    }

    #[test]
    fn test_null_in_string() -> Result<()> {
        facet_testhelpers::setup();
        let value = SpecialStrings {
            value: "hello\0world".to_string(),
        };
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: SpecialStrings = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[test]
    fn test_all_control_chars() -> Result<()> {
        facet_testhelpers::setup();
        // All ASCII control characters (0x00-0x1F)
        let control: String = (0u8..32).map(|b| b as char).collect();
        let value = SpecialStrings { value: control };
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: SpecialStrings = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[test]
    fn test_emoji_string() -> Result<()> {
        facet_testhelpers::setup();
        let value = SpecialStrings {
            value: "🦀🔥💯🎉👍".to_string(),
        };
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: SpecialStrings = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[test]
    fn test_mixed_unicode() -> Result<()> {
        facet_testhelpers::setup();
        let value = SpecialStrings {
            value: "Hello 世界 مرحبا Привет 🌍".to_string(),
        };
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: SpecialStrings = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }
}

// ============================================================================
// Zero-sized types
// ============================================================================

mod zst_tests {
    use super::*;

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct ZeroSized;

    #[test]
    fn test_zero_sized_type() -> Result<()> {
        facet_testhelpers::setup();
        let value = ZeroSized;
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: ZeroSized = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct VecOfZst {
        values: Vec<ZeroSized>,
    }

    #[test]
    fn test_vec_of_zst() -> Result<()> {
        facet_testhelpers::setup();
        let value = VecOfZst {
            values: vec![ZeroSized, ZeroSized, ZeroSized],
        };
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: VecOfZst = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct WithPhantom<T> {
        value: u32,
        #[serde(skip)]
        #[facet(skip)]
        _marker: std::marker::PhantomData<T>,
    }

    impl<T> Default for WithPhantom<T> {
        fn default() -> Self {
            Self {
                value: 0,
                _marker: std::marker::PhantomData,
            }
        }
    }

    #[test]
    fn test_with_phantom_data() -> Result<()> {
        facet_testhelpers::setup();
        let value: WithPhantom<String> = WithPhantom {
            value: 42,
            _marker: std::marker::PhantomData,
        };
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: WithPhantom<String> = from_slice(&facet_bytes)?;
        assert_eq!(value.value, decoded.value);
        Ok(())
    }
}
