//! Tests for facet attributes
//!
//! Note: Some attributes like #[facet(rename)] and #[facet(tag)] are for
//! text-based formats and don't affect postcard serialization since postcard
//! uses field order, not field names. We still test them to ensure they
//! don't break anything.

use eyre::Result;
use facet::Facet;
use facet_postcard_legacy::{from_slice, to_vec};
use postcard::to_allocvec as postcard_to_vec;
use serde::{Deserialize, Serialize};

// ============================================================================
// #[facet(default)] tests
// ============================================================================

mod default_tests {
    use super::*;

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct WithDefault {
        required: u32,
        #[facet(default)]
        #[serde(default)]
        optional: String,
    }

    #[test]
    fn test_default_with_value() -> Result<()> {
        facet_testhelpers::setup();
        let value = WithDefault {
            required: 42,
            optional: "hello".to_string(),
        };
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: WithDefault = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[test]
    fn test_default_with_empty() -> Result<()> {
        facet_testhelpers::setup();
        let value = WithDefault {
            required: 42,
            optional: String::new(), // default value
        };
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: WithDefault = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    fn default_number() -> u32 {
        100
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct WithCustomDefault {
        name: String,
        #[facet(default = default_number())]
        #[serde(default = "default_number")]
        count: u32,
    }

    #[test]
    fn test_custom_default() -> Result<()> {
        facet_testhelpers::setup();
        let value = WithCustomDefault {
            name: "test".to_string(),
            count: 42,
        };
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: WithCustomDefault = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct MultipleDefaults {
        #[facet(default)]
        #[serde(default)]
        a: u32,
        #[facet(default)]
        #[serde(default)]
        b: String,
        #[facet(default)]
        #[serde(default)]
        c: Vec<u32>,
    }

    #[test]
    fn test_multiple_defaults() -> Result<()> {
        facet_testhelpers::setup();
        let value = MultipleDefaults {
            a: 0,
            b: String::new(),
            c: vec![],
        };
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: MultipleDefaults = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }
}

// ============================================================================
// #[facet(skip)] tests
// ============================================================================

mod skip_tests {
    use super::*;

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct WithSkip {
        included: u32,
        #[facet(skip)]
        #[serde(skip)]
        skipped: u32,
    }

    impl Default for WithSkip {
        fn default() -> Self {
            Self {
                included: 0,
                skipped: 999, // This should be the default after deserialization
            }
        }
    }

    #[test]
    fn test_skip_field() -> Result<()> {
        facet_testhelpers::setup();
        let value = WithSkip {
            included: 42,
            skipped: 100, // This won't be serialized
        };
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        // Note: After roundtrip, skipped field will have default value
        let decoded: WithSkip = from_slice(&facet_bytes)?;
        assert_eq!(decoded.included, 42);
        // skipped field uses Default::default() which is 999
        Ok(())
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct WithSkipSerializing {
        included: u32,
        #[facet(skip_serializing)]
        #[serde(skip_serializing)]
        #[facet(default)]
        #[serde(default)]
        skip_ser: u32,
    }

    #[test]
    fn test_skip_serializing() -> Result<()> {
        facet_testhelpers::setup();
        let value = WithSkipSerializing {
            included: 42,
            skip_ser: 100,
        };
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);
        Ok(())
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct WithSkipDeserializing {
        included: u32,
        #[facet(skip_deserializing)]
        #[serde(skip_deserializing)]
        #[facet(default)]
        #[serde(default)]
        skip_deser: u32,
    }

    #[test]
    fn test_skip_deserializing() -> Result<()> {
        facet_testhelpers::setup();
        let value = WithSkipDeserializing {
            included: 42,
            skip_deser: 100,
        };
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);
        Ok(())
    }
}

// ============================================================================
// #[facet(rename)] tests - Note: doesn't affect postcard binary format
// ============================================================================

mod rename_tests {
    use super::*;

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct WithRename {
        #[facet(rename = "different_name")]
        #[serde(rename = "different_name")]
        original: u32,
    }

    #[test]
    fn test_rename_doesnt_affect_binary() -> Result<()> {
        facet_testhelpers::setup();
        // Postcard uses field order, not names, so rename shouldn't matter
        let value = WithRename { original: 42 };
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: WithRename = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    #[facet(rename_all = "camelCase")]
    #[serde(rename_all = "camelCase")]
    struct WithRenameAll {
        first_field: u32,
        second_field: String,
    }

    #[test]
    fn test_rename_all_doesnt_affect_binary() -> Result<()> {
        facet_testhelpers::setup();
        let value = WithRenameAll {
            first_field: 42,
            second_field: "hello".to_string(),
        };
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: WithRenameAll = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }
}

// ============================================================================
// #[facet(transparent)] tests
// ============================================================================

mod transparent_tests {
    use super::*;

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    #[facet(transparent)]
    #[serde(transparent)]
    struct TransparentWrapper(u32);

    #[test]
    fn test_transparent_wrapper() -> Result<()> {
        facet_testhelpers::setup();
        let value = TransparentWrapper(42);
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: TransparentWrapper = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    #[facet(transparent)]
    #[serde(transparent)]
    struct TransparentString(String);

    #[test]
    fn test_transparent_string() -> Result<()> {
        facet_testhelpers::setup();
        let value = TransparentString("hello".to_string());
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: TransparentString = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }
}

// ============================================================================
// Combination of attributes
// ============================================================================

mod combined_attributes {
    use super::*;

    fn default_count() -> u32 {
        10
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct CombinedStruct {
        required: u32,

        #[facet(default)]
        #[serde(default)]
        with_default: String,

        #[facet(default = default_count())]
        #[serde(default = "default_count")]
        with_custom_default: u32,

        #[facet(rename = "renamed")]
        #[serde(rename = "renamed")]
        original_name: bool,
    }

    #[test]
    fn test_combined_attributes() -> Result<()> {
        facet_testhelpers::setup();
        let value = CombinedStruct {
            required: 42,
            with_default: "hello".to_string(),
            with_custom_default: 100,
            original_name: true,
        };
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: CombinedStruct = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }
}

// ============================================================================
// Enum attributes
// ============================================================================

mod enum_attributes {
    use super::*;

    // Note: #[facet(tag)] and #[facet(content)] are for text formats
    // They don't affect postcard's binary format which uses variant indices

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum RenamedVariants {
        #[facet(rename = "first")]
        #[serde(rename = "first")]
        A,
        #[facet(rename = "second")]
        #[serde(rename = "second")]
        B(u32),
    }

    #[test]
    fn test_renamed_variants() -> Result<()> {
        facet_testhelpers::setup();
        let value = RenamedVariants::B(42);
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: RenamedVariants = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }
}
