//! Tests for smart pointer types (Box, Rc, Arc)

// These are intentional test cases for smart pointer serialization
#![allow(clippy::box_collection)]
#![allow(clippy::redundant_allocation)]
#![allow(clippy::vec_box)]

use eyre::Result;
use facet::Facet;
use facet_postcard_legacy::{from_slice, to_vec};
use postcard::to_allocvec as postcard_to_vec;
use serde::{Deserialize, Serialize};
use std::rc::Rc;
use std::sync::Arc;

// ============================================================================
// Box tests
// ============================================================================

mod box_tests {
    use super::*;

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct BoxedU32 {
        value: Box<u32>,
    }

    #[test]
    fn test_boxed_u32() -> Result<()> {
        facet_testhelpers::setup();
        let value = BoxedU32 {
            value: Box::new(42),
        };
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: BoxedU32 = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct BoxedString {
        value: Box<String>,
    }

    #[test]
    fn test_boxed_string() -> Result<()> {
        facet_testhelpers::setup();
        let value = BoxedString {
            value: Box::new("hello".to_string()),
        };
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: BoxedString = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct BoxedVec {
        value: Box<Vec<u32>>,
    }

    #[test]
    fn test_boxed_vec() -> Result<()> {
        facet_testhelpers::setup();
        let value = BoxedVec {
            value: Box::new(vec![1, 2, 3]),
        };
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: BoxedVec = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct NestedBoxes {
        value: Box<Box<u32>>,
    }

    #[test]
    fn test_nested_boxes() -> Result<()> {
        facet_testhelpers::setup();
        let value = NestedBoxes {
            value: Box::new(Box::new(42)),
        };
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: NestedBoxes = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct BoxedOption {
        value: Box<Option<u32>>,
    }

    #[test]
    fn test_boxed_option_some() -> Result<()> {
        facet_testhelpers::setup();
        let value = BoxedOption {
            value: Box::new(Some(42)),
        };
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: BoxedOption = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[test]
    fn test_boxed_option_none() -> Result<()> {
        facet_testhelpers::setup();
        let value = BoxedOption {
            value: Box::new(None),
        };
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: BoxedOption = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct OptionBoxed {
        value: Option<Box<u32>>,
    }

    #[test]
    fn test_option_boxed_some() -> Result<()> {
        facet_testhelpers::setup();
        let value = OptionBoxed {
            value: Some(Box::new(42)),
        };
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: OptionBoxed = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[test]
    fn test_option_boxed_none() -> Result<()> {
        facet_testhelpers::setup();
        let value = OptionBoxed { value: None };
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: OptionBoxed = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }
}

// ============================================================================
// Rc tests (Note: Rc doesn't impl Serialize by default in serde, using Arc instead)
// ============================================================================

mod rc_tests {
    use super::*;

    // Note: Standard serde doesn't support Rc serialization by default
    // because Rc can create cycles. We test with simple cases.

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct RcU32 {
        #[serde(with = "arc_serde")]
        value: Rc<u32>,
    }

    // Helper module for Rc serialization (treating it like the inner value)
    mod arc_serde {
        use serde::{Deserialize, Deserializer, Serialize, Serializer};
        use std::rc::Rc;

        pub fn serialize<S, T>(value: &Rc<T>, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
            T: Serialize,
        {
            (**value).serialize(serializer)
        }

        pub fn deserialize<'de, D, T>(deserializer: D) -> Result<Rc<T>, D::Error>
        where
            D: Deserializer<'de>,
            T: Deserialize<'de>,
        {
            T::deserialize(deserializer).map(Rc::new)
        }
    }

    #[test]
    fn test_rc_u32() -> Result<()> {
        facet_testhelpers::setup();
        let value = RcU32 { value: Rc::new(42) };
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: RcU32 = from_slice(&facet_bytes)?;
        assert_eq!(*value.value, *decoded.value);
        Ok(())
    }
}

// ============================================================================
// Arc tests
// ============================================================================

mod arc_tests {
    use super::*;

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct ArcU32 {
        value: Arc<u32>,
    }

    #[test]
    fn test_arc_u32() -> Result<()> {
        facet_testhelpers::setup();
        let value = ArcU32 {
            value: Arc::new(42),
        };
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: ArcU32 = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct ArcString {
        value: Arc<String>,
    }

    #[test]
    fn test_arc_string() -> Result<()> {
        facet_testhelpers::setup();
        let value = ArcString {
            value: Arc::new("hello".to_string()),
        };
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: ArcString = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct ArcVec {
        value: Arc<Vec<u32>>,
    }

    #[test]
    fn test_arc_vec() -> Result<()> {
        facet_testhelpers::setup();
        let value = ArcVec {
            value: Arc::new(vec![1, 2, 3]),
        };
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: ArcVec = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct NestedArcs {
        value: Arc<Arc<u32>>,
    }

    #[test]
    fn test_nested_arcs() -> Result<()> {
        facet_testhelpers::setup();
        let value = NestedArcs {
            value: Arc::new(Arc::new(42)),
        };
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: NestedArcs = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }
}

// ============================================================================
// Mixed smart pointer tests
// ============================================================================

mod mixed_tests {
    use super::*;

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct MixedPointers {
        boxed: Box<u32>,
        arc: Arc<String>,
        vec_of_boxed: Vec<Box<u32>>,
    }

    #[test]
    fn test_mixed_pointers() -> Result<()> {
        facet_testhelpers::setup();
        let value = MixedPointers {
            boxed: Box::new(42),
            arc: Arc::new("hello".to_string()),
            vec_of_boxed: vec![Box::new(1), Box::new(2), Box::new(3)],
        };
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: MixedPointers = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }
}
