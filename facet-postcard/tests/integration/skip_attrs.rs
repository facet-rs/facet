//! Tests for skip attributes (`skip_serializing_if`, `skip_all_unless_truthy`)
//! with postcard serialization.
//!
//! These tests verify that skip predicates are correctly IGNORED for binary formats
//! like postcard, where fields are identified by position rather than name.
//!
//! The key invariant is: postcard roundtrip must work regardless of skip predicates,
//! because skipping fields during serialization would cause deserialization to fail
//! (it would expect fields at the wrong positions).
//!
//! ## Background
//!
//! This test suite was created in response to https://github.com/bearcove/roam/pull/67
//! where `skip_all_unless_truthy` was added to OTLP types in roam-telemetry.
//!
//! The concern was that skip attributes, while useful for self-describing formats
//! like JSON (where skipped fields can be reconstructed from field names), could
//! break binary formats like postcard (where fields are identified by position).
//!
//! ## Architecture
//!
//! facet-format handles this correctly by using different field iterators:
//! - `fields_for_serialize()` - evaluates skip predicates (for JSON, YAML)
//! - `fields_for_binary_serialize()` - ignores skip predicates (for postcard)
//!
//! The serializer chooses which to use based on `StructFieldMode`:
//! - `Named` → uses `fields_for_serialize()`
//! - `Unnamed` → uses `fields_for_binary_serialize()`

#![cfg(feature = "jit")]

use facet::Facet;
use facet_postcard::{from_slice, to_vec};

// =============================================================================
// skip_serializing_if tests
// =============================================================================

mod skip_serializing_if {
    use super::*;

    /// Test that `skip_serializing_if = Option::is_none` doesn't break postcard roundtrip.
    ///
    /// This is the most common use case from the roam-telemetry PR.
    #[test]
    fn option_is_none_single_field() {
        facet_testhelpers::setup();

        #[derive(Debug, Clone, PartialEq, Facet)]
        struct WithSkip {
            #[facet(skip_serializing_if = Option::is_none)]
            optional: Option<String>,
        }

        // Test with None
        let v = WithSkip { optional: None };
        let bytes = to_vec(&v).expect("serialize");
        let v2: WithSkip = from_slice(&bytes).expect("deserialize");
        assert_eq!(v, v2);

        // Test with Some
        let v = WithSkip {
            optional: Some("hello".into()),
        };
        let bytes = to_vec(&v).expect("serialize");
        let v2: WithSkip = from_slice(&bytes).expect("deserialize");
        assert_eq!(v, v2);
    }

    /// Test that `skip_serializing_if` on multiple fields doesn't break roundtrip.
    #[test]
    fn option_is_none_multiple_fields() {
        facet_testhelpers::setup();

        #[derive(Debug, Clone, PartialEq, Facet)]
        struct MultiSkip {
            #[facet(skip_serializing_if = Option::is_none)]
            first: Option<String>,
            #[facet(skip_serializing_if = Option::is_none)]
            second: Option<u32>,
            #[facet(skip_serializing_if = Option::is_none)]
            third: Option<bool>,
        }

        // All None
        let v = MultiSkip {
            first: None,
            second: None,
            third: None,
        };
        let bytes = to_vec(&v).expect("serialize");
        let v2: MultiSkip = from_slice(&bytes).expect("deserialize");
        assert_eq!(v, v2);

        // All Some
        let v = MultiSkip {
            first: Some("hello".into()),
            second: Some(42),
            third: Some(true),
        };
        let bytes = to_vec(&v).expect("serialize");
        let v2: MultiSkip = from_slice(&bytes).expect("deserialize");
        assert_eq!(v, v2);

        // Mixed: first None, others Some
        let v = MultiSkip {
            first: None,
            second: Some(42),
            third: Some(true),
        };
        let bytes = to_vec(&v).expect("serialize");
        let v2: MultiSkip = from_slice(&bytes).expect("deserialize");
        assert_eq!(v, v2);

        // Mixed: middle None
        let v = MultiSkip {
            first: Some("hello".into()),
            second: None,
            third: Some(false),
        };
        let bytes = to_vec(&v).expect("serialize");
        let v2: MultiSkip = from_slice(&bytes).expect("deserialize");
        assert_eq!(v, v2);

        // Mixed: last None
        let v = MultiSkip {
            first: Some("world".into()),
            second: Some(100),
            third: None,
        };
        let bytes = to_vec(&v).expect("serialize");
        let v2: MultiSkip = from_slice(&bytes).expect("deserialize");
        assert_eq!(v, v2);
    }

    /// Test `skip_serializing_if` with required and optional fields mixed.
    #[test]
    fn mixed_required_and_optional() {
        facet_testhelpers::setup();

        #[derive(Debug, Clone, PartialEq, Facet)]
        struct Mixed {
            required: String,
            #[facet(skip_serializing_if = Option::is_none)]
            optional: Option<String>,
            another_required: u32,
        }

        let v = Mixed {
            required: "must have".into(),
            optional: None,
            another_required: 42,
        };
        let bytes = to_vec(&v).expect("serialize");
        let v2: Mixed = from_slice(&bytes).expect("deserialize");
        assert_eq!(v, v2);

        let v = Mixed {
            required: "must have".into(),
            optional: Some("have this too".into()),
            another_required: 42,
        };
        let bytes = to_vec(&v).expect("serialize");
        let v2: Mixed = from_slice(&bytes).expect("deserialize");
        assert_eq!(v, v2);
    }

    /// Test custom predicate function.
    #[test]
    fn custom_predicate_function() {
        facet_testhelpers::setup();

        fn is_empty(s: &str) -> bool {
            s.is_empty()
        }

        #[derive(Debug, Clone, PartialEq, Facet)]
        struct CustomPredicate {
            #[facet(skip_serializing_if = is_empty)]
            value: String,
        }

        // Empty string (would be skipped in JSON)
        let v = CustomPredicate {
            value: String::new(),
        };
        let bytes = to_vec(&v).expect("serialize");
        let v2: CustomPredicate = from_slice(&bytes).expect("deserialize");
        assert_eq!(v, v2);

        // Non-empty string
        let v = CustomPredicate {
            value: "not empty".into(),
        };
        let bytes = to_vec(&v).expect("serialize");
        let v2: CustomPredicate = from_slice(&bytes).expect("deserialize");
        assert_eq!(v, v2);
    }

    /// Test predicate with Vec::is_empty.
    #[test]
    fn vec_is_empty_predicate() {
        facet_testhelpers::setup();

        #[derive(Debug, Clone, PartialEq, Facet)]
        struct WithVec {
            name: String,
            #[facet(skip_serializing_if = Vec::is_empty)]
            items: Vec<u32>,
        }

        // Empty vec
        let v = WithVec {
            name: "test".into(),
            items: vec![],
        };
        let bytes = to_vec(&v).expect("serialize");
        let v2: WithVec = from_slice(&bytes).expect("deserialize");
        assert_eq!(v, v2);

        // Non-empty vec
        let v = WithVec {
            name: "test".into(),
            items: vec![1, 2, 3],
        };
        let bytes = to_vec(&v).expect("serialize");
        let v2: WithVec = from_slice(&bytes).expect("deserialize");
        assert_eq!(v, v2);
    }
}

// =============================================================================
// skip_all_unless_truthy tests
// =============================================================================

mod skip_all_unless_truthy {
    use super::*;

    /// Basic test: container-level attribute with Option fields.
    #[test]
    fn basic_option_fields() {
        facet_testhelpers::setup();

        #[derive(Debug, Clone, PartialEq, Facet)]
        #[facet(skip_all_unless_truthy)]
        struct AllOptional {
            tag: Option<String>,
            payload: Option<String>,
        }

        // Both None
        let v = AllOptional {
            tag: None,
            payload: None,
        };
        let bytes = to_vec(&v).expect("serialize");
        let v2: AllOptional = from_slice(&bytes).expect("deserialize");
        assert_eq!(v, v2);

        // First None, second Some
        let v = AllOptional {
            tag: None,
            payload: Some("hello".into()),
        };
        let bytes = to_vec(&v).expect("serialize");
        let v2: AllOptional = from_slice(&bytes).expect("deserialize");
        assert_eq!(v, v2);

        // First Some, second None
        let v = AllOptional {
            tag: Some("mytag".into()),
            payload: None,
        };
        let bytes = to_vec(&v).expect("serialize");
        let v2: AllOptional = from_slice(&bytes).expect("deserialize");
        assert_eq!(v, v2);

        // Both Some
        let v = AllOptional {
            tag: Some("mytag".into()),
            payload: Some("mypayload".into()),
        };
        let bytes = to_vec(&v).expect("serialize");
        let v2: AllOptional = from_slice(&bytes).expect("deserialize");
        assert_eq!(v, v2);
    }

    /// Test with String fields (falsy when empty).
    #[test]
    fn string_fields() {
        facet_testhelpers::setup();

        #[derive(Debug, Clone, PartialEq, Facet)]
        #[facet(skip_all_unless_truthy)]
        struct StringStruct {
            name: String,
            description: String,
        }

        // Both empty
        let v = StringStruct {
            name: String::new(),
            description: String::new(),
        };
        let bytes = to_vec(&v).expect("serialize");
        let v2: StringStruct = from_slice(&bytes).expect("deserialize");
        assert_eq!(v, v2);

        // First empty, second non-empty
        let v = StringStruct {
            name: String::new(),
            description: "has content".into(),
        };
        let bytes = to_vec(&v).expect("serialize");
        let v2: StringStruct = from_slice(&bytes).expect("deserialize");
        assert_eq!(v, v2);

        // Both non-empty
        let v = StringStruct {
            name: "name".into(),
            description: "description".into(),
        };
        let bytes = to_vec(&v).expect("serialize");
        let v2: StringStruct = from_slice(&bytes).expect("deserialize");
        assert_eq!(v, v2);
    }

    /// Test with Vec fields (falsy when empty).
    #[test]
    fn vec_fields() {
        facet_testhelpers::setup();

        #[derive(Debug, Clone, PartialEq, Facet)]
        #[facet(skip_all_unless_truthy)]
        struct VecStruct {
            items: Vec<u32>,
            more_items: Vec<String>,
        }

        // Both empty
        let v = VecStruct {
            items: vec![],
            more_items: vec![],
        };
        let bytes = to_vec(&v).expect("serialize");
        let v2: VecStruct = from_slice(&bytes).expect("deserialize");
        assert_eq!(v, v2);

        // First empty, second non-empty
        let v = VecStruct {
            items: vec![],
            more_items: vec!["hello".into()],
        };
        let bytes = to_vec(&v).expect("serialize");
        let v2: VecStruct = from_slice(&bytes).expect("deserialize");
        assert_eq!(v, v2);

        // Both non-empty
        let v = VecStruct {
            items: vec![1, 2, 3],
            more_items: vec!["a".into(), "b".into()],
        };
        let bytes = to_vec(&v).expect("serialize");
        let v2: VecStruct = from_slice(&bytes).expect("deserialize");
        assert_eq!(v, v2);
    }

    /// Test with mixed field types.
    #[test]
    fn mixed_field_types() {
        facet_testhelpers::setup();

        #[derive(Debug, Clone, PartialEq, Facet)]
        #[facet(skip_all_unless_truthy)]
        struct MixedStruct {
            opt_string: Option<String>,
            string_val: String,
            vec_val: Vec<u32>,
            opt_int: Option<i64>,
        }

        // All falsy
        let v = MixedStruct {
            opt_string: None,
            string_val: String::new(),
            vec_val: vec![],
            opt_int: None,
        };
        let bytes = to_vec(&v).expect("serialize");
        let v2: MixedStruct = from_slice(&bytes).expect("deserialize");
        assert_eq!(v, v2);

        // All truthy
        let v = MixedStruct {
            opt_string: Some("hello".into()),
            string_val: "world".into(),
            vec_val: vec![1, 2, 3],
            opt_int: Some(42),
        };
        let bytes = to_vec(&v).expect("serialize");
        let v2: MixedStruct = from_slice(&bytes).expect("deserialize");
        assert_eq!(v, v2);

        // Alternating pattern
        let v = MixedStruct {
            opt_string: None,
            string_val: "has value".into(),
            vec_val: vec![],
            opt_int: Some(-1),
        };
        let bytes = to_vec(&v).expect("serialize");
        let v2: MixedStruct = from_slice(&bytes).expect("deserialize");
        assert_eq!(v, v2);
    }
}

// =============================================================================
// Combined attribute tests
// =============================================================================

mod combined_attrs {
    use super::*;

    /// Test struct with both container-level and field-level attributes.
    #[test]
    fn container_and_field_level() {
        facet_testhelpers::setup();

        fn is_zero(n: &u32) -> bool {
            *n == 0
        }

        #[derive(Debug, Clone, PartialEq, Facet)]
        #[facet(skip_all_unless_truthy)]
        struct Combined {
            opt: Option<String>,
            #[facet(skip_serializing_if = is_zero)]
            count: u32,
            name: String,
        }

        // All falsy/zero
        let v = Combined {
            opt: None,
            count: 0,
            name: String::new(),
        };
        let bytes = to_vec(&v).expect("serialize");
        let v2: Combined = from_slice(&bytes).expect("deserialize");
        assert_eq!(v, v2);

        // All truthy/non-zero
        let v = Combined {
            opt: Some("value".into()),
            count: 42,
            name: "myname".into(),
        };
        let bytes = to_vec(&v).expect("serialize");
        let v2: Combined = from_slice(&bytes).expect("deserialize");
        assert_eq!(v, v2);
    }

    /// Test nested structs with skip attributes.
    #[test]
    fn nested_structs() {
        facet_testhelpers::setup();

        #[derive(Debug, Clone, PartialEq, Facet)]
        #[facet(skip_all_unless_truthy)]
        struct Inner {
            value: Option<i32>,
        }

        #[derive(Debug, Clone, PartialEq, Facet)]
        struct Outer {
            name: String,
            #[facet(skip_serializing_if = Option::is_none)]
            inner: Option<Inner>,
        }

        // Inner is None
        let v = Outer {
            name: "test".into(),
            inner: None,
        };
        let bytes = to_vec(&v).expect("serialize");
        let v2: Outer = from_slice(&bytes).expect("deserialize");
        assert_eq!(v, v2);

        // Inner is Some with None value
        let v = Outer {
            name: "test".into(),
            inner: Some(Inner { value: None }),
        };
        let bytes = to_vec(&v).expect("serialize");
        let v2: Outer = from_slice(&bytes).expect("deserialize");
        assert_eq!(v, v2);

        // Inner is Some with Some value
        let v = Outer {
            name: "test".into(),
            inner: Some(Inner { value: Some(42) }),
        };
        let bytes = to_vec(&v).expect("serialize");
        let v2: Outer = from_slice(&bytes).expect("deserialize");
        assert_eq!(v, v2);
    }
}

// =============================================================================
// Enum variant tests
// =============================================================================

mod enum_variants {
    use super::*;

    /// Test enum with skip attributes on variant fields.
    #[test]
    fn enum_variant_with_skip() {
        facet_testhelpers::setup();

        #[derive(Debug, Clone, PartialEq, Facet)]
        #[repr(u8)]
        enum Message {
            Simple(String),
            Complex {
                id: u64,
                #[facet(skip_serializing_if = Option::is_none)]
                metadata: Option<String>,
            },
        }

        // Simple variant
        let v = Message::Simple("hello".into());
        let bytes = to_vec(&v).expect("serialize");
        let v2: Message = from_slice(&bytes).expect("deserialize");
        assert_eq!(v, v2);

        // Complex with None metadata
        let v = Message::Complex {
            id: 1,
            metadata: None,
        };
        let bytes = to_vec(&v).expect("serialize");
        let v2: Message = from_slice(&bytes).expect("deserialize");
        assert_eq!(v, v2);

        // Complex with Some metadata
        let v = Message::Complex {
            id: 2,
            metadata: Some("extra info".into()),
        };
        let bytes = to_vec(&v).expect("serialize");
        let v2: Message = from_slice(&bytes).expect("deserialize");
        assert_eq!(v, v2);
    }

    /// Test enum with skip_all_unless_truthy on variant struct.
    #[test]
    fn enum_variant_skip_all_unless_truthy() {
        facet_testhelpers::setup();

        #[derive(Debug, Clone, PartialEq, Facet)]
        #[facet(skip_all_unless_truthy)]
        struct Data {
            tag: Option<String>,
            value: Option<i32>,
        }

        #[derive(Debug, Clone, PartialEq, Facet)]
        #[repr(u8)]
        enum Wrapper {
            Empty,
            WithData(Data),
        }

        // Empty variant
        let v = Wrapper::Empty;
        let bytes = to_vec(&v).expect("serialize");
        let v2: Wrapper = from_slice(&bytes).expect("deserialize");
        assert_eq!(v, v2);

        // WithData with all None
        let v = Wrapper::WithData(Data {
            tag: None,
            value: None,
        });
        let bytes = to_vec(&v).expect("serialize");
        let v2: Wrapper = from_slice(&bytes).expect("deserialize");
        assert_eq!(v, v2);

        // WithData with mixed
        let v = Wrapper::WithData(Data {
            tag: Some("key".into()),
            value: None,
        });
        let bytes = to_vec(&v).expect("serialize");
        let v2: Wrapper = from_slice(&bytes).expect("deserialize");
        assert_eq!(v, v2);

        // WithData with all Some
        let v = Wrapper::WithData(Data {
            tag: Some("key".into()),
            value: Some(100),
        });
        let bytes = to_vec(&v).expect("serialize");
        let v2: Wrapper = from_slice(&bytes).expect("deserialize");
        assert_eq!(v, v2);
    }
}

// =============================================================================
// Cross-format comparison tests
// =============================================================================

mod cross_format {
    use super::*;

    /// Demonstrate that JSON skips falsy fields while postcard includes them.
    ///
    /// This is the core behavioral difference:
    /// - JSON (self-describing): can omit fields, they're reconstructed from names
    /// - Postcard (positional): must include all fields to maintain order
    #[test]
    fn json_skips_but_postcard_includes() {
        facet_testhelpers::setup();

        #[derive(Debug, Clone, PartialEq, Facet)]
        #[facet(skip_all_unless_truthy)]
        struct SkipTest {
            opt: Option<String>,
            list: Vec<u32>,
        }

        let with_falsy = SkipTest {
            opt: None,
            list: vec![],
        };

        // JSON output: should NOT contain the falsy fields
        let json = facet_json::to_string(&with_falsy).expect("json serialize");
        assert_eq!(json, "{}", "JSON should skip all falsy fields");

        // Postcard output: should contain Option discriminants
        let postcard = to_vec(&with_falsy).expect("postcard serialize");
        assert!(
            !postcard.is_empty(),
            "postcard should include Option discriminant"
        );

        // Postcard should roundtrip correctly
        let rt: SkipTest = from_slice(&postcard).expect("postcard roundtrip");
        assert_eq!(rt, with_falsy);

        // With truthy values
        let with_truthy = SkipTest {
            opt: Some("hello".into()),
            list: vec![1, 2, 3],
        };

        // JSON output: should contain the fields
        let json = facet_json::to_string(&with_truthy).expect("json serialize");
        assert!(
            json.contains("opt") && json.contains("list"),
            "JSON should include truthy fields"
        );

        // Postcard should also work
        let postcard = to_vec(&with_truthy).expect("postcard serialize");
        let rt: SkipTest = from_slice(&postcard).expect("postcard roundtrip");
        assert_eq!(rt, with_truthy);
    }

    /// Verify that postcard produces different byte lengths than JSON would
    /// for skip_serializing_if fields, because postcard DOES include the fields
    /// while JSON skips them.
    ///
    /// This test ensures we're actually testing the right behavior - if postcard
    /// was incorrectly skipping fields, the bytes would be different between
    /// Some and None cases.
    #[test]
    fn postcard_includes_all_fields() {
        facet_testhelpers::setup();

        #[derive(Debug, Clone, PartialEq, Facet)]
        struct TestStruct {
            before: u8,
            #[facet(skip_serializing_if = Option::is_none)]
            middle: Option<u8>,
            after: u8,
        }

        // With None - postcard should still serialize Option discriminant
        let with_none = TestStruct {
            before: 1,
            middle: None,
            after: 3,
        };
        let none_bytes = to_vec(&with_none).expect("serialize None");

        // With Some - postcard should serialize Option discriminant + value
        let with_some = TestStruct {
            before: 1,
            middle: Some(2),
            after: 3,
        };
        let some_bytes = to_vec(&with_some).expect("serialize Some");

        // The bytes should be different (Some has the value, None doesn't)
        assert_ne!(
            none_bytes, some_bytes,
            "postcard should produce different bytes for None vs Some"
        );

        // None should be shorter (discriminant but no value)
        assert!(
            none_bytes.len() < some_bytes.len(),
            "None bytes ({}) should be shorter than Some bytes ({})",
            none_bytes.len(),
            some_bytes.len()
        );

        // Both should roundtrip
        let rt_none: TestStruct = from_slice(&none_bytes).expect("roundtrip None");
        let rt_some: TestStruct = from_slice(&some_bytes).expect("roundtrip Some");
        assert_eq!(rt_none, with_none);
        assert_eq!(rt_some, with_some);
    }

    /// Verify that the field order is preserved correctly.
    ///
    /// If postcard incorrectly skipped fields, deserializing would put values
    /// in the wrong fields.
    #[test]
    fn field_order_preserved() {
        facet_testhelpers::setup();

        #[derive(Debug, Clone, PartialEq, Facet)]
        #[facet(skip_all_unless_truthy)]
        struct OrderTest {
            first: Option<u32>,
            second: Option<u32>,
            third: Option<u32>,
        }

        // Pattern: None, Some(2), None
        // If skipping was broken, second's value might end up in first or third
        let v = OrderTest {
            first: None,
            second: Some(2),
            third: None,
        };
        let bytes = to_vec(&v).expect("serialize");
        let v2: OrderTest = from_slice(&bytes).expect("deserialize");
        assert_eq!(v2.first, None, "first should be None");
        assert_eq!(v2.second, Some(2), "second should be Some(2)");
        assert_eq!(v2.third, None, "third should be None");

        // Pattern: Some(1), None, Some(3)
        let v = OrderTest {
            first: Some(1),
            second: None,
            third: Some(3),
        };
        let bytes = to_vec(&v).expect("serialize");
        let v2: OrderTest = from_slice(&bytes).expect("deserialize");
        assert_eq!(v2.first, Some(1), "first should be Some(1)");
        assert_eq!(v2.second, None, "second should be None");
        assert_eq!(v2.third, Some(3), "third should be Some(3)");
    }
}

// =============================================================================
// OTLP-style types (from roam-telemetry PR)
// =============================================================================

mod otlp_style {
    use super::*;

    /// Mimics the AnyValue type from OTLP/roam-telemetry.
    #[derive(Debug, Clone, PartialEq, Facet)]
    #[facet(skip_all_unless_truthy)]
    struct AnyValue {
        string_value: Option<String>,
        int_value: Option<i64>,
        bool_value: Option<bool>,
    }

    /// Mimics the Span type from OTLP/roam-telemetry.
    #[derive(Debug, Clone, PartialEq, Facet)]
    struct Span {
        trace_id: String,
        span_id: String,
        #[facet(skip_serializing_if = Option::is_none)]
        parent_span_id: Option<String>,
        name: String,
    }

    #[test]
    fn any_value_string() {
        facet_testhelpers::setup();

        let v = AnyValue {
            string_value: Some("test".into()),
            int_value: None,
            bool_value: None,
        };
        let bytes = to_vec(&v).expect("serialize");
        let v2: AnyValue = from_slice(&bytes).expect("deserialize");
        assert_eq!(v, v2);
    }

    #[test]
    fn any_value_int() {
        facet_testhelpers::setup();

        let v = AnyValue {
            string_value: None,
            int_value: Some(42),
            bool_value: None,
        };
        let bytes = to_vec(&v).expect("serialize");
        let v2: AnyValue = from_slice(&bytes).expect("deserialize");
        assert_eq!(v, v2);
    }

    #[test]
    fn any_value_bool() {
        facet_testhelpers::setup();

        let v = AnyValue {
            string_value: None,
            int_value: None,
            bool_value: Some(true),
        };
        let bytes = to_vec(&v).expect("serialize");
        let v2: AnyValue = from_slice(&bytes).expect("deserialize");
        assert_eq!(v, v2);
    }

    #[test]
    fn span_with_parent() {
        facet_testhelpers::setup();

        let v = Span {
            trace_id: "trace123".into(),
            span_id: "span456".into(),
            parent_span_id: Some("parent789".into()),
            name: "my-span".into(),
        };
        let bytes = to_vec(&v).expect("serialize");
        let v2: Span = from_slice(&bytes).expect("deserialize");
        assert_eq!(v, v2);
    }

    #[test]
    fn span_without_parent() {
        facet_testhelpers::setup();

        let v = Span {
            trace_id: "trace123".into(),
            span_id: "span456".into(),
            parent_span_id: None,
            name: "root-span".into(),
        };
        let bytes = to_vec(&v).expect("serialize");
        let v2: Span = from_slice(&bytes).expect("deserialize");
        assert_eq!(v, v2);
    }
}
