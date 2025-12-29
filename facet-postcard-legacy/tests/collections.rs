//! Tests for collection types

use eyre::Result;
use facet::Facet;
use facet_postcard_legacy::{from_slice, to_vec};
use postcard::to_allocvec as postcard_to_vec;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

// ============================================================================
// Vec tests
// ============================================================================

mod vec_tests {
    use super::*;

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct VecU32 {
        values: Vec<u32>,
    }

    #[test]
    fn test_empty_vec() -> Result<()> {
        facet_testhelpers::setup();
        let wrapper = VecU32 { values: vec![] };
        let facet_bytes = to_vec(&wrapper)?;
        let postcard_bytes = postcard_to_vec(&wrapper)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: VecU32 = from_slice(&facet_bytes)?;
        assert_eq!(wrapper, decoded);
        Ok(())
    }

    #[test]
    fn test_single_element_vec() -> Result<()> {
        facet_testhelpers::setup();
        let wrapper = VecU32 { values: vec![42] };
        let facet_bytes = to_vec(&wrapper)?;
        let postcard_bytes = postcard_to_vec(&wrapper)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: VecU32 = from_slice(&facet_bytes)?;
        assert_eq!(wrapper, decoded);
        Ok(())
    }

    #[test]
    fn test_multiple_elements() -> Result<()> {
        facet_testhelpers::setup();
        let wrapper = VecU32 {
            values: vec![1, 2, 3, 4, 5],
        };
        let facet_bytes = to_vec(&wrapper)?;
        let postcard_bytes = postcard_to_vec(&wrapper)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: VecU32 = from_slice(&facet_bytes)?;
        assert_eq!(wrapper, decoded);
        Ok(())
    }

    #[test]
    fn test_large_vec() -> Result<()> {
        facet_testhelpers::setup();
        let wrapper = VecU32 {
            values: (0..1000).collect(),
        };
        let facet_bytes = to_vec(&wrapper)?;
        let postcard_bytes = postcard_to_vec(&wrapper)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: VecU32 = from_slice(&facet_bytes)?;
        assert_eq!(wrapper, decoded);
        Ok(())
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct VecString {
        values: Vec<String>,
    }

    #[test]
    fn test_vec_of_strings() -> Result<()> {
        facet_testhelpers::setup();
        let wrapper = VecString {
            values: vec!["hello".to_string(), "world".to_string(), "!".to_string()],
        };
        let facet_bytes = to_vec(&wrapper)?;
        let postcard_bytes = postcard_to_vec(&wrapper)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: VecString = from_slice(&facet_bytes)?;
        assert_eq!(wrapper, decoded);
        Ok(())
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct VecU8 {
        bytes: Vec<u8>,
    }

    #[test]
    fn test_vec_u8_empty() -> Result<()> {
        facet_testhelpers::setup();
        let wrapper = VecU8 { bytes: vec![] };
        let facet_bytes = to_vec(&wrapper)?;
        let postcard_bytes = postcard_to_vec(&wrapper)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: VecU8 = from_slice(&facet_bytes)?;
        assert_eq!(wrapper, decoded);
        Ok(())
    }

    #[test]
    fn test_vec_u8_with_data() -> Result<()> {
        facet_testhelpers::setup();
        let wrapper = VecU8 {
            bytes: vec![0, 1, 2, 255, 128, 64],
        };
        let facet_bytes = to_vec(&wrapper)?;
        let postcard_bytes = postcard_to_vec(&wrapper)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: VecU8 = from_slice(&facet_bytes)?;
        assert_eq!(wrapper, decoded);
        Ok(())
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct NestedVec {
        matrix: Vec<Vec<u32>>,
    }

    #[test]
    fn test_nested_vec() -> Result<()> {
        facet_testhelpers::setup();
        let wrapper = NestedVec {
            matrix: vec![vec![1, 2, 3], vec![4, 5], vec![6, 7, 8, 9]],
        };
        let facet_bytes = to_vec(&wrapper)?;
        let postcard_bytes = postcard_to_vec(&wrapper)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: NestedVec = from_slice(&facet_bytes)?;
        assert_eq!(wrapper, decoded);
        Ok(())
    }
}

// ============================================================================
// HashMap tests
// ============================================================================

mod hashmap_tests {
    use super::*;

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct HashMapWrapper {
        map: HashMap<String, u32>,
    }

    #[test]
    fn test_empty_hashmap() -> Result<()> {
        facet_testhelpers::setup();
        let wrapper = HashMapWrapper {
            map: HashMap::new(),
        };
        let facet_bytes = to_vec(&wrapper)?;
        let postcard_bytes = postcard_to_vec(&wrapper)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: HashMapWrapper = from_slice(&facet_bytes)?;
        assert_eq!(wrapper, decoded);
        Ok(())
    }

    #[test]
    fn test_single_entry_hashmap() -> Result<()> {
        facet_testhelpers::setup();
        let mut map = HashMap::new();
        map.insert("key".to_string(), 42);
        let wrapper = HashMapWrapper { map };

        // For HashMap, order is not guaranteed, so we just test roundtrip
        let facet_bytes = to_vec(&wrapper)?;
        let decoded: HashMapWrapper = from_slice(&facet_bytes)?;
        assert_eq!(wrapper, decoded);
        Ok(())
    }

    // Note: HashMap iteration order is not deterministic, so we can't compare
    // byte-for-byte with postcard for multi-entry maps. We test roundtrip instead.
    #[test]
    fn test_multi_entry_hashmap_roundtrip() -> Result<()> {
        facet_testhelpers::setup();
        let mut map = HashMap::new();
        map.insert("one".to_string(), 1);
        map.insert("two".to_string(), 2);
        map.insert("three".to_string(), 3);
        let wrapper = HashMapWrapper { map };

        let bytes = to_vec(&wrapper)?;
        let decoded: HashMapWrapper = from_slice(&bytes)?;
        assert_eq!(wrapper, decoded);
        Ok(())
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct IntKeyMap {
        map: HashMap<u32, String>,
    }

    #[test]
    fn test_int_key_hashmap() -> Result<()> {
        facet_testhelpers::setup();
        let mut map = HashMap::new();
        map.insert(1, "one".to_string());
        map.insert(2, "two".to_string());
        let wrapper = IntKeyMap { map };

        let bytes = to_vec(&wrapper)?;
        let decoded: IntKeyMap = from_slice(&bytes)?;
        assert_eq!(wrapper, decoded);
        Ok(())
    }
}

// ============================================================================
// BTreeMap tests (ordered, so we can compare bytes)
// ============================================================================

mod btreemap_tests {
    use super::*;

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct BTreeMapWrapper {
        map: BTreeMap<String, u32>,
    }

    #[test]
    fn test_empty_btreemap() -> Result<()> {
        facet_testhelpers::setup();
        let wrapper = BTreeMapWrapper {
            map: BTreeMap::new(),
        };
        let facet_bytes = to_vec(&wrapper)?;
        let postcard_bytes = postcard_to_vec(&wrapper)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: BTreeMapWrapper = from_slice(&facet_bytes)?;
        assert_eq!(wrapper, decoded);
        Ok(())
    }

    #[test]
    fn test_btreemap_ordered() -> Result<()> {
        facet_testhelpers::setup();
        let mut map = BTreeMap::new();
        map.insert("alpha".to_string(), 1);
        map.insert("beta".to_string(), 2);
        map.insert("gamma".to_string(), 3);
        let wrapper = BTreeMapWrapper { map };

        let facet_bytes = to_vec(&wrapper)?;
        let postcard_bytes = postcard_to_vec(&wrapper)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: BTreeMapWrapper = from_slice(&facet_bytes)?;
        assert_eq!(wrapper, decoded);
        Ok(())
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct NestedBTreeMap {
        map: BTreeMap<String, BTreeMap<String, u32>>,
    }

    #[test]
    fn test_nested_btreemap() -> Result<()> {
        facet_testhelpers::setup();
        let mut inner1 = BTreeMap::new();
        inner1.insert("a".to_string(), 1);
        inner1.insert("b".to_string(), 2);

        let mut inner2 = BTreeMap::new();
        inner2.insert("x".to_string(), 10);

        let mut outer = BTreeMap::new();
        outer.insert("first".to_string(), inner1);
        outer.insert("second".to_string(), inner2);

        let wrapper = NestedBTreeMap { map: outer };

        let facet_bytes = to_vec(&wrapper)?;
        let postcard_bytes = postcard_to_vec(&wrapper)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: NestedBTreeMap = from_slice(&facet_bytes)?;
        assert_eq!(wrapper, decoded);
        Ok(())
    }
}

// ============================================================================
// HashSet tests
// ============================================================================

mod hashset_tests {
    use super::*;

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct HashSetWrapper {
        set: HashSet<u32>,
    }

    #[test]
    fn test_empty_hashset() -> Result<()> {
        facet_testhelpers::setup();
        let wrapper = HashSetWrapper {
            set: HashSet::new(),
        };
        let facet_bytes = to_vec(&wrapper)?;
        let postcard_bytes = postcard_to_vec(&wrapper)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: HashSetWrapper = from_slice(&facet_bytes)?;
        assert_eq!(wrapper, decoded);
        Ok(())
    }

    #[test]
    fn test_hashset_roundtrip() -> Result<()> {
        facet_testhelpers::setup();
        let mut set = HashSet::new();
        set.insert(1);
        set.insert(2);
        set.insert(3);
        let wrapper = HashSetWrapper { set };

        let bytes = to_vec(&wrapper)?;
        let decoded: HashSetWrapper = from_slice(&bytes)?;
        assert_eq!(wrapper, decoded);
        Ok(())
    }
}

// ============================================================================
// BTreeSet tests
// ============================================================================

mod btreeset_tests {
    use super::*;

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct BTreeSetWrapper {
        set: BTreeSet<u32>,
    }

    #[test]
    fn test_empty_btreeset() -> Result<()> {
        facet_testhelpers::setup();
        let wrapper = BTreeSetWrapper {
            set: BTreeSet::new(),
        };
        let facet_bytes = to_vec(&wrapper)?;
        let postcard_bytes = postcard_to_vec(&wrapper)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: BTreeSetWrapper = from_slice(&facet_bytes)?;
        assert_eq!(wrapper, decoded);
        Ok(())
    }

    #[test]
    fn test_btreeset_ordered() -> Result<()> {
        facet_testhelpers::setup();
        let mut set = BTreeSet::new();
        set.insert(3);
        set.insert(1);
        set.insert(2);
        let wrapper = BTreeSetWrapper { set };

        let facet_bytes = to_vec(&wrapper)?;
        let postcard_bytes = postcard_to_vec(&wrapper)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: BTreeSetWrapper = from_slice(&facet_bytes)?;
        assert_eq!(wrapper, decoded);
        Ok(())
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct StringBTreeSet {
        set: BTreeSet<String>,
    }

    #[test]
    fn test_string_btreeset() -> Result<()> {
        facet_testhelpers::setup();
        let mut set = BTreeSet::new();
        set.insert("zebra".to_string());
        set.insert("apple".to_string());
        set.insert("mango".to_string());
        let wrapper = StringBTreeSet { set };

        let facet_bytes = to_vec(&wrapper)?;
        let postcard_bytes = postcard_to_vec(&wrapper)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: StringBTreeSet = from_slice(&facet_bytes)?;
        assert_eq!(wrapper, decoded);
        Ok(())
    }
}

// ============================================================================
// Option tests
// ============================================================================

mod option_tests {
    use super::*;

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct OptionU32 {
        value: Option<u32>,
    }

    #[test]
    fn test_option_none() -> Result<()> {
        facet_testhelpers::setup();
        let wrapper = OptionU32 { value: None };
        let facet_bytes = to_vec(&wrapper)?;
        let postcard_bytes = postcard_to_vec(&wrapper)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: OptionU32 = from_slice(&facet_bytes)?;
        assert_eq!(wrapper, decoded);
        Ok(())
    }

    #[test]
    fn test_option_some() -> Result<()> {
        facet_testhelpers::setup();
        let wrapper = OptionU32 { value: Some(42) };
        let facet_bytes = to_vec(&wrapper)?;
        let postcard_bytes = postcard_to_vec(&wrapper)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: OptionU32 = from_slice(&facet_bytes)?;
        assert_eq!(wrapper, decoded);
        Ok(())
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct OptionString {
        value: Option<String>,
    }

    #[test]
    fn test_option_string_none() -> Result<()> {
        facet_testhelpers::setup();
        let wrapper = OptionString { value: None };
        let facet_bytes = to_vec(&wrapper)?;
        let postcard_bytes = postcard_to_vec(&wrapper)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: OptionString = from_slice(&facet_bytes)?;
        assert_eq!(wrapper, decoded);
        Ok(())
    }

    #[test]
    fn test_option_string_some() -> Result<()> {
        facet_testhelpers::setup();
        let wrapper = OptionString {
            value: Some("hello".to_string()),
        };
        let facet_bytes = to_vec(&wrapper)?;
        let postcard_bytes = postcard_to_vec(&wrapper)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: OptionString = from_slice(&facet_bytes)?;
        assert_eq!(wrapper, decoded);
        Ok(())
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct NestedOption {
        value: Option<Option<u32>>,
    }

    #[test]
    fn test_nested_option_none() -> Result<()> {
        facet_testhelpers::setup();
        let wrapper = NestedOption { value: None };
        let facet_bytes = to_vec(&wrapper)?;
        let postcard_bytes = postcard_to_vec(&wrapper)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: NestedOption = from_slice(&facet_bytes)?;
        assert_eq!(wrapper, decoded);
        Ok(())
    }

    #[test]
    fn test_nested_option_some_none() -> Result<()> {
        facet_testhelpers::setup();
        let wrapper = NestedOption { value: Some(None) };
        let facet_bytes = to_vec(&wrapper)?;
        let postcard_bytes = postcard_to_vec(&wrapper)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: NestedOption = from_slice(&facet_bytes)?;
        assert_eq!(wrapper, decoded);
        Ok(())
    }

    #[test]
    fn test_nested_option_some_some() -> Result<()> {
        facet_testhelpers::setup();
        let wrapper = NestedOption {
            value: Some(Some(42)),
        };
        let facet_bytes = to_vec(&wrapper)?;
        let postcard_bytes = postcard_to_vec(&wrapper)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: NestedOption = from_slice(&facet_bytes)?;
        assert_eq!(wrapper, decoded);
        Ok(())
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct MultipleOptions {
        a: Option<u32>,
        b: Option<String>,
        c: Option<bool>,
    }

    #[test]
    fn test_multiple_options_all_none() -> Result<()> {
        facet_testhelpers::setup();
        let wrapper = MultipleOptions {
            a: None,
            b: None,
            c: None,
        };
        let facet_bytes = to_vec(&wrapper)?;
        let postcard_bytes = postcard_to_vec(&wrapper)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: MultipleOptions = from_slice(&facet_bytes)?;
        assert_eq!(wrapper, decoded);
        Ok(())
    }

    #[test]
    fn test_multiple_options_all_some() -> Result<()> {
        facet_testhelpers::setup();
        let wrapper = MultipleOptions {
            a: Some(42),
            b: Some("hello".to_string()),
            c: Some(true),
        };
        let facet_bytes = to_vec(&wrapper)?;
        let postcard_bytes = postcard_to_vec(&wrapper)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: MultipleOptions = from_slice(&facet_bytes)?;
        assert_eq!(wrapper, decoded);
        Ok(())
    }

    #[test]
    fn test_multiple_options_mixed() -> Result<()> {
        facet_testhelpers::setup();
        let wrapper = MultipleOptions {
            a: Some(42),
            b: None,
            c: Some(false),
        };
        let facet_bytes = to_vec(&wrapper)?;
        let postcard_bytes = postcard_to_vec(&wrapper)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: MultipleOptions = from_slice(&facet_bytes)?;
        assert_eq!(wrapper, decoded);
        Ok(())
    }
}
