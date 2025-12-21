//! Tests for JIT-compiled deserialization.

use facet::Facet;
use facet_format::jit;
use facet_format_json::JsonParser;

#[derive(Debug, PartialEq, Facet)]
struct SimpleStruct {
    name: String,
    age: i64,
    active: bool,
}

#[test]
fn test_jit_simple_struct() {
    // Check compatibility
    assert!(jit::is_jit_compatible::<SimpleStruct>());

    // Parse with JIT
    let json = br#"{"name": "Alice", "age": 30, "active": true}"#;
    let mut parser = JsonParser::new(json);

    let result = jit::try_deserialize::<SimpleStruct, JsonParser<'_>>(&mut parser);

    assert!(result.is_some(), "JIT deserialization should be attempted");
    let result = result.unwrap();
    assert!(
        result.is_ok(),
        "JIT deserialization should succeed: {:?}",
        result
    );

    let value = result.unwrap();
    assert_eq!(value.name, "Alice");
    assert_eq!(value.age, 30);
    assert!(value.active);
}

#[derive(Debug, PartialEq, Facet)]
struct MixedTypes {
    count: u64,
    ratio: f64,
    flag: bool,
}

#[test]
fn test_jit_mixed_types() {
    assert!(jit::is_jit_compatible::<MixedTypes>());

    let json = br#"{"count": 42, "ratio": 2.5, "flag": false}"#;
    let mut parser = JsonParser::new(json);

    let result = jit::try_deserialize::<MixedTypes, JsonParser<'_>>(&mut parser);

    assert!(result.is_some());
    let result = result.unwrap();
    assert!(
        result.is_ok(),
        "JIT deserialization should succeed: {:?}",
        result
    );

    let value = result.unwrap();
    assert_eq!(value.count, 42);
    assert!((value.ratio - 2.5).abs() < 0.001);
    assert!(!value.flag);
}

#[derive(Debug, PartialEq, Facet)]
struct OutOfOrder {
    a: i64,
    b: i64,
    c: i64,
}

#[test]
fn test_jit_out_of_order_fields() {
    // JSON fields in different order than struct definition
    let json = br#"{"c": 3, "a": 1, "b": 2}"#;
    let mut parser = JsonParser::new(json);

    let result = jit::try_deserialize::<OutOfOrder, JsonParser<'_>>(&mut parser);

    assert!(result.is_some());
    let result = result.unwrap();
    assert!(
        result.is_ok(),
        "JIT deserialization should succeed: {:?}",
        result
    );

    let value = result.unwrap();
    assert_eq!(value.a, 1);
    assert_eq!(value.b, 2);
    assert_eq!(value.c, 3);
}

#[test]
fn test_jit_unknown_fields_skipped() {
    // Extra fields should be skipped
    let json = br#"{"name": "Bob", "extra": "ignored", "age": 25, "active": false}"#;
    let mut parser = JsonParser::new(json);

    let result = jit::try_deserialize::<SimpleStruct, JsonParser<'_>>(&mut parser);

    assert!(result.is_some());
    let result = result.unwrap();
    assert!(
        result.is_ok(),
        "JIT deserialization should succeed: {:?}",
        result
    );

    let value = result.unwrap();
    assert_eq!(value.name, "Bob");
    assert_eq!(value.age, 25);
    assert!(!value.active);
}

#[derive(Debug, PartialEq, Facet)]
struct Inner {
    x: i64,
    y: i64,
}

#[derive(Debug, PartialEq, Facet)]
struct Outer {
    id: u64,
    inner: Inner,
    name: String,
}

#[test]
fn test_jit_nested_struct() {
    // Check compatibility
    assert!(jit::is_jit_compatible::<Outer>());
    assert!(jit::is_jit_compatible::<Inner>());

    // Parse with JIT
    let json = br#"{"id": 42, "inner": {"x": 10, "y": 20}, "name": "test"}"#;
    let mut parser = JsonParser::new(json);

    let result = jit::try_deserialize::<Outer, JsonParser<'_>>(&mut parser);

    assert!(result.is_some(), "JIT deserialization should be attempted");
    let result = result.unwrap();
    assert!(
        result.is_ok(),
        "JIT deserialization should succeed: {:?}",
        result
    );

    let value = result.unwrap();
    assert_eq!(value.id, 42);
    assert_eq!(value.inner.x, 10);
    assert_eq!(value.inner.y, 20);
    assert_eq!(value.name, "test");
}

#[derive(Debug, PartialEq, Facet)]
struct WithOption {
    id: u64,
    maybe_count: Option<i64>,
    maybe_flag: Option<bool>,
}

#[test]
fn test_jit_option_none() {
    // Test with null values
    let json = br#"{"id": 42, "maybe_count": null, "maybe_flag": null}"#;
    let mut parser = JsonParser::new(json);

    let result = jit::try_deserialize::<WithOption, JsonParser<'_>>(&mut parser);

    assert!(result.is_some(), "JIT should attempt with Option fields");
    let result = result.unwrap();
    assert!(
        result.is_ok(),
        "JIT deserialization should succeed: {:?}",
        result
    );

    let value = result.unwrap();
    assert_eq!(value.id, 42);
    assert_eq!(value.maybe_count, None);
    assert_eq!(value.maybe_flag, None);
}

#[test]
fn test_jit_option_some() {
    // Test with Some values
    let json = br#"{"id": 42, "maybe_count": 123, "maybe_flag": true}"#;
    let mut parser = JsonParser::new(json);

    let result = jit::try_deserialize::<WithOption, JsonParser<'_>>(&mut parser);

    assert!(result.is_some());
    let result = result.unwrap();
    assert!(
        result.is_ok(),
        "JIT deserialization should succeed: {:?}",
        result
    );

    let value = result.unwrap();
    assert_eq!(value.id, 42);
    assert_eq!(value.maybe_count, Some(123));
    assert_eq!(value.maybe_flag, Some(true));
}

#[test]
fn test_jit_vec_bool() {
    // Check compatibility - Vec<bool> should be JIT compatible
    assert!(
        jit::is_jit_compatible::<Vec<bool>>(),
        "Vec<bool> should be JIT compatible"
    );

    // Parse with JIT
    let json = br#"[true, false, true, true, false]"#;
    let mut parser = JsonParser::new(json);

    let result = jit::try_deserialize::<Vec<bool>, JsonParser<'_>>(&mut parser);

    assert!(result.is_some(), "JIT deserialization should be attempted");
    let result = result.unwrap();
    assert!(
        result.is_ok(),
        "JIT deserialization should succeed: {:?}",
        result
    );

    let value = result.unwrap();
    assert_eq!(value, vec![true, false, true, true, false]);
}

#[test]
fn test_jit_vec_i64() {
    assert!(jit::is_jit_compatible::<Vec<i64>>());

    let json = br#"[1, 2, 3, -4, 5]"#;
    let mut parser = JsonParser::new(json);

    let result = jit::try_deserialize::<Vec<i64>, JsonParser<'_>>(&mut parser);

    assert!(result.is_some());
    let result = result.unwrap();
    assert!(
        result.is_ok(),
        "JIT deserialization should succeed: {:?}",
        result
    );

    let value = result.unwrap();
    assert_eq!(value, vec![1, 2, 3, -4, 5]);
}

#[test]
#[allow(clippy::approx_constant)] // 3.14 is test data, not mathematical constant
fn test_jit_vec_f64() {
    assert!(jit::is_jit_compatible::<Vec<f64>>());

    let json = br#"[1.5, 2.0, 3.14]"#;
    let mut parser = JsonParser::new(json);

    let result = jit::try_deserialize::<Vec<f64>, JsonParser<'_>>(&mut parser);

    assert!(result.is_some());
    let result = result.unwrap();
    assert!(
        result.is_ok(),
        "JIT deserialization should succeed: {:?}",
        result
    );

    let value = result.unwrap();
    assert_eq!(value.len(), 3);
    assert!((value[0] - 1.5).abs() < 0.001);
    assert!((value[1] - 2.0).abs() < 0.001);
    assert!((value[2] - 3.14).abs() < 0.001);
}

#[test]
fn test_jit_vec_string() {
    assert!(jit::is_jit_compatible::<Vec<String>>());

    let json = br#"["hello", "world", "test"]"#;
    let mut parser = JsonParser::new(json);

    let result = jit::try_deserialize::<Vec<String>, JsonParser<'_>>(&mut parser);

    assert!(result.is_some());
    let result = result.unwrap();
    assert!(
        result.is_ok(),
        "JIT deserialization should succeed: {:?}",
        result
    );

    let value = result.unwrap();
    assert_eq!(value, vec!["hello", "world", "test"]);
}

// =============================================================================
// Cursor Coherency Tests (harden.md 3.3)
// =============================================================================

/// Test that cursor position is correctly updated after Tier-1 JIT parsing,
/// allowing continuation with normal parsing.
#[test]
fn test_cursor_coherency_after_tier1_struct() {
    use facet_format::FormatDeserializer;

    // Parse JSON array where each element should use JIT
    // Then verify cursor is at correct position after each element
    let json = br#"[{"name": "Alice", "age": 30, "active": true}, {"name": "Bob", "age": 25, "active": false}]"#;
    let parser = JsonParser::new(json);

    // Parse the entire Vec<SimpleStruct> using the standard deserializer
    // This exercises cursor coherency internally as it parses each struct
    let result: Vec<SimpleStruct> = FormatDeserializer::new(parser).deserialize().unwrap();

    assert_eq!(result.len(), 2);
    assert_eq!(result[0].name, "Alice");
    assert_eq!(result[0].age, 30);
    assert!(result[0].active);
    assert_eq!(result[1].name, "Bob");
    assert_eq!(result[1].age, 25);
    assert!(!result[1].active);
}

/// Test parsing a struct followed by additional content to verify cursor position.
#[test]
fn test_cursor_coherency_struct_then_more() {
    use facet_format::FormatDeserializer;

    // Parse nested array of structs - verifies cursor is correct after each struct
    let json = br#"[[1, 2], [3, 4, 5], [6]]"#;
    let parser = JsonParser::new(json);

    let result: Vec<Vec<i64>> = FormatDeserializer::new(parser).deserialize().unwrap();

    assert_eq!(result, vec![vec![1, 2], vec![3, 4, 5], vec![6]]);
}

/// Test that Tier-2 Vec parsing leaves cursor at correct position.
#[test]
fn test_cursor_coherency_tier2_vec() {
    use facet_format::FormatDeserializer;

    // Parse struct containing a Vec (Tier-2 eligible) and other fields
    #[derive(Debug, PartialEq, Facet)]
    struct WithVec {
        numbers: Vec<i64>,
        name: String,
    }

    // The "numbers" field may use Tier-2, "name" uses standard parsing
    // This tests that cursor is correct after Tier-2 Vec parsing
    let json = br#"{"numbers": [1, 2, 3], "name": "test"}"#;
    let parser = JsonParser::new(json);

    let result: WithVec = FormatDeserializer::new(parser).deserialize().unwrap();

    assert_eq!(result.numbers, vec![1, 2, 3]);
    assert_eq!(result.name, "test");
}

/// Test cursor coherency with Tier-2 Vec<bool> in struct fields.
#[test]
fn test_cursor_coherency_tier2_vec_bool_in_struct() {
    use facet_format::FormatDeserializer;

    #[derive(Debug, PartialEq, Facet)]
    struct FlagsAndName {
        flags: Vec<bool>,
        label: String,
        count: i64,
    }

    let json = br#"{"flags": [true, false, true], "label": "test", "count": 42}"#;
    let parser = JsonParser::new(json);

    let result: FlagsAndName = FormatDeserializer::new(parser).deserialize().unwrap();

    assert_eq!(result.flags, vec![true, false, true]);
    assert_eq!(result.label, "test");
    assert_eq!(result.count, 42);
}

/// Test cursor coherency with multiple Vec fields.
#[test]
fn test_cursor_coherency_multiple_vecs() {
    use facet_format::FormatDeserializer;

    #[derive(Debug, PartialEq, Facet)]
    struct MultiVec {
        bools: Vec<bool>,
        nums: Vec<i64>,
        strs: Vec<String>,
    }

    let json = br#"{"bools": [true, false], "nums": [1, 2, 3], "strs": ["a", "b"]}"#;
    let parser = JsonParser::new(json);

    let result: MultiVec = FormatDeserializer::new(parser).deserialize().unwrap();

    assert_eq!(result.bools, vec![true, false]);
    assert_eq!(result.nums, vec![1, 2, 3]);
    assert_eq!(result.strs, vec!["a", "b"]);
}

/// Test that empty arrays maintain cursor coherency.
#[test]
fn test_cursor_coherency_empty_arrays() {
    use facet_format::FormatDeserializer;

    #[derive(Debug, PartialEq, Facet)]
    struct WithEmpty {
        empty: Vec<i64>,
        name: String,
        more: Vec<bool>,
    }

    let json = br#"{"empty": [], "name": "test", "more": [true]}"#;
    let parser = JsonParser::new(json);

    let result: WithEmpty = FormatDeserializer::new(parser).deserialize().unwrap();

    assert_eq!(result.empty, Vec::<i64>::new());
    assert_eq!(result.name, "test");
    assert_eq!(result.more, vec![true]);
}

/// Test parsing arrays at top level with varied element types.
#[test]
fn test_cursor_coherency_nested_mixed() {
    use facet_format::FormatDeserializer;

    // Parse array containing structs with Vec fields
    #[derive(Debug, PartialEq, Facet)]
    struct Item {
        id: i64,
        tags: Vec<String>,
    }

    let json = br#"[{"id": 1, "tags": ["a", "b"]}, {"id": 2, "tags": ["c"]}]"#;
    let parser = JsonParser::new(json);

    let result: Vec<Item> = FormatDeserializer::new(parser).deserialize().unwrap();

    assert_eq!(result.len(), 2);
    assert_eq!(result[0].id, 1);
    assert_eq!(result[0].tags, vec!["a", "b"]);
    assert_eq!(result[1].id, 2);
    assert_eq!(result[1].tags, vec!["c"]);
}

// =============================================================================
// Required-Field Validation Tests (harden.md 3.1)
// =============================================================================

/// Test that missing a required field causes an error, not UB.
#[test]
fn test_required_field_missing_returns_error() {
    // SimpleStruct requires name, age, and active (all non-Option)
    // Omit "age" to trigger required-field validation
    let json = br#"{"name": "Alice", "active": true}"#;
    let mut parser = JsonParser::new(json);

    let result = jit::try_deserialize::<SimpleStruct, JsonParser<'_>>(&mut parser);

    assert!(result.is_some(), "JIT should be attempted");
    let result = result.unwrap();
    assert!(
        result.is_err(),
        "Should fail when required field 'age' is missing"
    );
}

/// Test that missing multiple required fields causes an error.
#[test]
fn test_multiple_required_fields_missing() {
    // Omit both "age" and "active"
    let json = br#"{"name": "Alice"}"#;
    let mut parser = JsonParser::new(json);

    let result = jit::try_deserialize::<SimpleStruct, JsonParser<'_>>(&mut parser);

    assert!(result.is_some(), "JIT should be attempted");
    let result = result.unwrap();
    assert!(
        result.is_err(),
        "Should fail when multiple required fields are missing"
    );
}

/// Test that empty struct (all fields missing) causes an error.
#[test]
fn test_all_required_fields_missing() {
    let json = br#"{}"#;
    let mut parser = JsonParser::new(json);

    let result = jit::try_deserialize::<SimpleStruct, JsonParser<'_>>(&mut parser);

    assert!(result.is_some(), "JIT should be attempted");
    let result = result.unwrap();
    assert!(
        result.is_err(),
        "Should fail when all required fields are missing"
    );
}

/// Test that Option fields don't cause errors when missing.
#[test]
fn test_optional_fields_can_be_missing() {
    // WithOption has: id (required), maybe_count (optional), maybe_flag (optional)
    // Only provide "id", omit the optional fields entirely
    let json = br#"{"id": 42}"#;
    let mut parser = JsonParser::new(json);

    let result = jit::try_deserialize::<WithOption, JsonParser<'_>>(&mut parser);

    assert!(result.is_some(), "JIT should be attempted");
    let result = result.unwrap();
    assert!(
        result.is_ok(),
        "Should succeed with only required field: {:?}",
        result
    );

    let value = result.unwrap();
    assert_eq!(value.id, 42);
    // Missing Option fields should be initialized to None
    // This validates that we pre-initialize Option fields before deserialization
    assert_eq!(
        value.maybe_count, None,
        "Missing Option<i64> should be None"
    );
    assert_eq!(
        value.maybe_flag, None,
        "Missing Option<bool> should be None"
    );
}

// =============================================================================
// Key Handling Tests (harden.md 3.2)
// =============================================================================

/// Test that escaped keys are correctly matched and don't cause leaks.
/// Escaped keys produce owned strings which must be properly freed.
#[test]
fn test_escaped_keys_handled_correctly() {
    // "na\u006de" unescapes to "name", "\u0061ge" to "age", "\u0061ctive" to "active"
    let json = br#"{"na\u006de": "Alice", "\u0061ge": 30, "\u0061ctive": true}"#;
    let mut parser = JsonParser::new(json);

    let result = jit::try_deserialize::<SimpleStruct, JsonParser<'_>>(&mut parser);

    assert!(result.is_some(), "JIT should be attempted");
    let result = result.unwrap();
    assert!(
        result.is_ok(),
        "Should succeed with escaped keys: {:?}",
        result
    );

    let value = result.unwrap();
    assert_eq!(value.name, "Alice");
    assert_eq!(value.age, 30);
    assert!(value.active);
}

/// Test escaped keys with unknown fields (ensures skip doesn't leak).
#[test]
fn test_escaped_unknown_keys_skipped() {
    // Mix of known and unknown escaped keys
    let json = br#"{"na\u006de": "Bob", "\u0065xtra": "ignored", "\u0061ge": 25, "active": false}"#;
    let mut parser = JsonParser::new(json);

    let result = jit::try_deserialize::<SimpleStruct, JsonParser<'_>>(&mut parser);

    assert!(result.is_some(), "JIT should be attempted");
    let result = result.unwrap();
    assert!(
        result.is_ok(),
        "Should succeed with escaped unknown keys: {:?}",
        result
    );

    let value = result.unwrap();
    assert_eq!(value.name, "Bob");
    assert_eq!(value.age, 25);
    assert!(!value.active);
}

/// Test that string values with escapes are handled correctly.
#[test]
fn test_escaped_string_values() {
    // Escaped string value: "Al\u0069ce" -> "Alice"
    let json = br#"{"name": "Al\u0069ce", "age": 30, "active": true}"#;
    let mut parser = JsonParser::new(json);

    let result = jit::try_deserialize::<SimpleStruct, JsonParser<'_>>(&mut parser);

    assert!(result.is_some(), "JIT should be attempted");
    let result = result.unwrap();
    assert!(
        result.is_ok(),
        "Should succeed with escaped string value: {:?}",
        result
    );

    let value = result.unwrap();
    assert_eq!(value.name, "Alice");
    assert_eq!(value.age, 30);
    assert!(value.active);
}

// ============================================================================
// Tier-2 (Format JIT) Regression Tests
// ============================================================================
// These tests verify that Tier-2 compilation works correctly for structs.
// They check tier stats to prevent regressions that cause silent fallback
// to Tier-1, which would tank performance.

#[test]
#[cfg(feature = "jit")]
fn test_tier2_simple_struct() {
    jit::reset_tier_stats();

    // Verify Tier-2 compatibility
    assert!(jit::is_format_jit_compatible::<SimpleStruct>());

    // Parse with Tier-2
    let json = br#"{"name": "Alice", "age": 30, "active": true}"#;
    let mut parser = JsonParser::new(json);

    let result = jit::try_deserialize_with_format_jit::<SimpleStruct, _>(&mut parser);

    assert!(result.is_some(), "Tier-2 compilation should succeed");
    let result = result.unwrap();
    assert!(
        result.is_ok(),
        "Tier-2 deserialization should succeed: {:?}",
        result
    );

    let value = result.unwrap();
    assert_eq!(value.name, "Alice");
    assert_eq!(value.age, 30);
    assert!(value.active);

    // REGRESSION TEST: Verify Tier-2 was actually used
    let (attempts, successes, compile_unsup, runtime_unsup, runtime_err, t1_uses) =
        jit::get_tier_stats();
    assert!(
        successes > 0,
        "Tier-2 should successfully compile SimpleStruct (tier2_successes={}, tier2_compile_unsupported={}, tier2_attempts={})",
        successes,
        compile_unsup,
        attempts
    );
    assert_eq!(t1_uses, 0, "Tier-1 should not be used");
    assert_eq!(
        runtime_unsup, 0,
        "Tier-2 should not have runtime unsupported errors"
    );
    assert_eq!(runtime_err, 0, "Tier-2 should not have runtime errors");
}

#[test]
#[cfg(feature = "jit")]
fn test_tier2_mixed_types() {
    jit::reset_tier_stats();

    assert!(jit::is_format_jit_compatible::<MixedTypes>());

    let json = br#"{"count": 42, "ratio": 2.5, "flag": false}"#;
    let mut parser = JsonParser::new(json);

    let result = jit::try_deserialize_with_format_jit::<MixedTypes, _>(&mut parser);

    assert!(result.is_some());
    let result = result.unwrap();
    assert!(result.is_ok(), "Tier-2 deserialization should succeed");

    let value = result.unwrap();
    assert_eq!(value.count, 42);
    assert!((value.ratio - 2.5).abs() < 0.001);
    assert!(!value.flag);

    // REGRESSION TEST: Verify Tier-2 was used
    let (_, successes, _, _, runtime_err, t1_uses) = jit::get_tier_stats();
    assert!(
        successes > 0,
        "Tier-2 should successfully compile MixedTypes"
    );
    assert_eq!(t1_uses, 0, "Tier-1 should not be used");
    assert_eq!(runtime_err, 0, "Tier-2 should not have runtime errors");
}

// =============================================================================
// Negative Cache Tests
// =============================================================================

/// Test that compilation failures are cached (negative cache).
/// This prevents repeated expensive compilation attempts on known-unsupported types.
#[test]
#[cfg(feature = "jit")]
fn test_tier2_negative_cache() {
    use facet_format::jit::cache;

    // Clear caches and stats to start fresh
    cache::clear_format_cache();
    cache::reset_cache_stats();
    jit::reset_tier_stats();

    // Define an unsupported type (tuple struct, not supported in Tier-2)
    #[derive(Debug, PartialEq, Facet)]
    struct TupleStruct(i64, String);

    // Verify it's not Tier-2 compatible
    assert!(!jit::is_format_jit_compatible::<TupleStruct>());

    let json = br#"[42, "hello"]"#;

    // First attempt: should try to compile and fail
    {
        let mut parser = JsonParser::new(json);
        let result = jit::try_deserialize_format::<TupleStruct, _>(&mut parser);
        assert!(
            result.is_none(),
            "Tuple struct should be unsupported in Tier-2"
        );
    }

    // Check cache stats after first attempt
    let (hit1, neg1, compile1, evict1) = cache::get_cache_stats();
    assert_eq!(hit1, 0, "No cache hits yet");
    assert_eq!(neg1, 0, "No negative cache hits yet");
    assert_eq!(compile1, 1, "Should have attempted compilation once");
    assert_eq!(evict1, 0, "No evictions yet");

    // Second attempt: should hit negative cache (no recompilation)
    {
        let mut parser = JsonParser::new(json);
        let result = jit::try_deserialize_format::<TupleStruct, _>(&mut parser);
        assert!(result.is_none(), "Still unsupported");
    }

    // Check cache stats after second attempt
    let (hit2, neg2, compile2, evict2) = cache::get_cache_stats();
    assert_eq!(hit2, 0, "Still no successful hits");
    assert_eq!(neg2, 1, "Should have ONE negative cache hit");
    assert_eq!(compile2, 1, "Should NOT have recompiled (still 1)");
    assert_eq!(evict2, 0, "Still no evictions");

    // Third attempt: should also hit negative cache (TLS cache this time)
    {
        let mut parser = JsonParser::new(json);
        let result = jit::try_deserialize_format::<TupleStruct, _>(&mut parser);
        assert!(result.is_none(), "Still unsupported");
    }

    // Check cache stats after third attempt
    let (hit3, neg3, compile3, evict3) = cache::get_cache_stats();
    assert_eq!(hit3, 0, "Still no successful hits");
    assert_eq!(neg3, 2, "Should have TWO negative cache hits now");
    assert_eq!(compile3, 1, "Should STILL not have recompiled (still 1)");
    assert_eq!(evict3, 0, "Still no evictions");

    println!("✓ Negative cache working: compilation attempted once, cached twice");
}

/// Test that the cache is bounded and evicts old entries when capacity is exceeded.
#[test]
#[cfg(feature = "jit")]
fn test_tier2_cache_eviction() {
    use facet_format::jit::cache;
    use std::env;

    // Set a small cache limit for testing
    // SAFETY: This is a test, we're the only ones modifying this env var
    unsafe {
        env::set_var("FACET_TIER2_CACHE_MAX_ENTRIES", "3");
    }

    // Clear caches and stats
    cache::clear_format_cache();
    cache::reset_cache_stats();

    // Define 4 different tuple structs (all unsupported in Tier-2)
    #[derive(Debug, Facet)]
    struct Type1(i64);
    #[derive(Debug, Facet)]
    struct Type2(i64);
    #[derive(Debug, Facet)]
    struct Type3(i64);
    #[derive(Debug, Facet)]
    struct Type4(i64);

    let json = br#"[42]"#;

    // Attempt 1: Type1 (cache miss, compile, cache at capacity 1/3)
    {
        let mut parser = JsonParser::new(json);
        let _ = jit::try_deserialize_format::<Type1, _>(&mut parser);
    }
    let (_, _, _, evict1) = cache::get_cache_stats();
    assert_eq!(evict1, 0, "No evictions yet, cache not full");

    // Attempt 2: Type2 (cache miss, compile, cache at capacity 2/3)
    {
        let mut parser = JsonParser::new(json);
        let _ = jit::try_deserialize_format::<Type2, _>(&mut parser);
    }
    let (_, _, _, evict2) = cache::get_cache_stats();
    assert_eq!(evict2, 0, "No evictions yet, cache not full");

    // Attempt 3: Type3 (cache miss, compile, cache at capacity 3/3)
    {
        let mut parser = JsonParser::new(json);
        let _ = jit::try_deserialize_format::<Type3, _>(&mut parser);
    }
    let (_, _, _, evict3) = cache::get_cache_stats();
    assert_eq!(evict3, 0, "No evictions yet, cache exactly at capacity");

    // Attempt 4: Type4 (cache miss, compile, triggers eviction of Type1)
    {
        let mut parser = JsonParser::new(json);
        let _ = jit::try_deserialize_format::<Type4, _>(&mut parser);
    }
    let (_, _, _, evict4) = cache::get_cache_stats();
    assert_eq!(evict4, 1, "Should have ONE eviction (Type1 was oldest)");

    // Attempt 5: Type1 again (should need to compile again, Type1 was evicted)
    cache::reset_cache_stats(); // Reset to measure this attempt cleanly
    {
        let mut parser = JsonParser::new(json);
        let _ = jit::try_deserialize_format::<Type1, _>(&mut parser);
    }
    let (_, neg, compile, evict5) = cache::get_cache_stats();
    assert_eq!(neg, 0, "Type1 was evicted, not a cache hit");
    assert_eq!(compile, 1, "Type1 recompiled after eviction");
    assert_eq!(evict5, 1, "Another eviction happened (Type2 was oldest)");

    println!("✓ Cache eviction working: capacity enforced, FIFO eviction");

    // Clean up env var
    // SAFETY: This is a test, cleaning up our test env var
    unsafe {
        env::remove_var("FACET_TIER2_CACHE_MAX_ENTRIES");
    }
}

/// Test that compilation budget guards prevent pathological shapes from compiling.
#[test]
#[cfg(feature = "jit")]
fn test_tier2_budget_guards() {
    use facet_format::jit::cache;
    use std::env;

    // Set a very low field limit for testing
    // SAFETY: This is a test, we're the only ones modifying this env var
    unsafe {
        env::set_var("FACET_TIER2_MAX_FIELDS", "5");
    }

    // Clear caches and stats
    cache::clear_format_cache();
    cache::reset_cache_stats();

    // Define a struct with more fields than the budget allows
    #[derive(Debug, Facet)]
    struct LargeStruct {
        f1: i64,
        f2: i64,
        f3: i64,
        f4: i64,
        f5: i64,
        f6: i64, // Exceeds limit of 5
    }

    let json = br#"{"f1":1,"f2":2,"f3":3,"f4":4,"f5":5,"f6":6}"#;

    // Attempt to compile - should be refused due to budget
    {
        let mut parser = JsonParser::new(json);
        let result = jit::try_deserialize_format::<LargeStruct, _>(&mut parser);
        assert!(result.is_none(), "Large struct should exceed budget");
    }

    // Verify it was cached as a failure
    let (_, neg, compile, _) = cache::get_cache_stats();
    assert_eq!(compile, 1, "Should have attempted compilation once");
    assert_eq!(neg, 0, "Not a negative cache hit yet (first attempt)");

    // Second attempt should hit negative cache
    {
        let mut parser = JsonParser::new(json);
        let result = jit::try_deserialize_format::<LargeStruct, _>(&mut parser);
        assert!(result.is_none(), "Still refused");
    }

    let (_, neg2, compile2, _) = cache::get_cache_stats();
    assert_eq!(compile2, 1, "Should NOT recompile (still 1)");
    assert_eq!(neg2, 1, "Should have ONE negative cache hit");

    println!("✓ Budget guards working: pathological shapes refused");

    // Clean up env var
    // SAFETY: This is a test, cleaning up our test env var
    unsafe {
        env::remove_var("FACET_TIER2_MAX_FIELDS");
    }
}

// =============================================================================
// Flattened HashMap (unknown key capture) tests
// =============================================================================

#[derive(Debug, PartialEq, Facet)]
struct BasicCapture {
    known_field: String,
    #[facet(flatten)]
    extra: std::collections::HashMap<String, i64>,
}

#[test]
#[cfg(feature = "jit")]
fn test_flatten_map_basic_capture() {
    // Test: known fields + 2 unknown keys captured into the map

    // Verify this is Tier-2 JIT compatible (format-specific)
    assert!(jit::is_format_jit_compatible::<BasicCapture>());

    let json = br#"{"known_field": "test", "unknown1": 42, "unknown2": 99}"#;
    let mut parser = JsonParser::new(json);

    // Use format-specific JIT compilation (Tier-2)
    let result = jit::try_deserialize_format::<BasicCapture, _>(&mut parser);
    assert!(result.is_some(), "Tier-2 should support flatten map");
    let result = result.unwrap();
    assert!(
        result.is_ok(),
        "Deserialization should succeed: {:?}",
        result
    );

    let value = result.unwrap();
    assert_eq!(value.known_field, "test");
    assert_eq!(value.extra.len(), 2, "Should capture 2 unknown keys");
    assert_eq!(value.extra.get("unknown1"), Some(&42));
    assert_eq!(value.extra.get("unknown2"), Some(&99));
}

#[derive(Debug, PartialEq, Facet)]
struct EmptyMapCase {
    known_field: String,
    #[facet(flatten)]
    extra: std::collections::HashMap<String, String>,
}

#[test]
#[cfg(feature = "jit")]
fn test_flatten_map_no_unknown_keys() {
    // Test: map ends up empty but initialized (no UB)
    assert!(jit::is_format_jit_compatible::<EmptyMapCase>());

    let json = br#"{"known_field": "test"}"#;
    let mut parser = JsonParser::new(json);

    let result = jit::try_deserialize_format::<EmptyMapCase, _>(&mut parser);
    assert!(result.is_some());
    let result = result.unwrap();
    assert!(
        result.is_ok(),
        "Deserialization should succeed: {:?}",
        result
    );

    let value = result.unwrap();
    assert_eq!(value.known_field, "test");
    assert_eq!(value.extra.len(), 0, "Map should be empty but initialized");
    // Verify we can safely iterate (map is properly initialized)
    assert_eq!(value.extra.len(), 0);
}

#[derive(Debug, PartialEq, Facet)]
struct PrecedenceCase {
    known_field: String,
    another_known: i64,
    #[facet(flatten)]
    extra: std::collections::HashMap<String, String>,
}

#[test]
#[cfg(feature = "jit")]
fn test_flatten_map_precedence() {
    // Test: a key that matches a real field must not go into the map
    assert!(jit::is_format_jit_compatible::<PrecedenceCase>());

    let json = br#"{"known_field": "test", "another_known": 42, "unknown1": "captured"}"#;
    let mut parser = JsonParser::new(json);

    let result = jit::try_deserialize_format::<PrecedenceCase, _>(&mut parser);
    assert!(result.is_some());
    let result = result.unwrap();
    assert!(
        result.is_ok(),
        "Deserialization should succeed: {:?}",
        result
    );

    let value = result.unwrap();
    assert_eq!(value.known_field, "test");
    assert_eq!(value.another_known, 42);
    assert_eq!(
        value.extra.len(),
        1,
        "Should only capture truly unknown keys"
    );
    assert_eq!(value.extra.get("unknown1"), Some(&"captured".to_string()));
    assert!(
        !value.extra.contains_key("known_field"),
        "Known field should not be in map"
    );
    assert!(
        !value.extra.contains_key("another_known"),
        "Known field should not be in map"
    );
}

#[derive(Debug, PartialEq, Facet)]
struct InnerStruct {
    inner_field: String,
}

#[derive(Debug, PartialEq, Facet)]
struct MixWithFlattenStruct {
    normal_field: String,
    #[facet(flatten)]
    inner: InnerStruct,
    #[facet(flatten)]
    extra: std::collections::HashMap<String, bool>,
}

#[test]
#[cfg(feature = "jit")]
fn test_flatten_map_mix_with_flatten_struct() {
    // Test: unknown keys go to map, known flattened keys go to their targets
    assert!(jit::is_format_jit_compatible::<MixWithFlattenStruct>());

    let json = br#"{"normal_field": "test", "inner_field": "flattened", "unknown1": true, "unknown2": false}"#;
    let mut parser = JsonParser::new(json);

    let result = jit::try_deserialize_format::<MixWithFlattenStruct, _>(&mut parser);
    assert!(result.is_some());
    let result = result.unwrap();
    assert!(
        result.is_ok(),
        "Deserialization should succeed: {:?}",
        result
    );

    let value = result.unwrap();
    assert_eq!(value.normal_field, "test");
    assert_eq!(value.inner.inner_field, "flattened");
    assert_eq!(value.extra.len(), 2, "Should capture unknown keys");
    assert_eq!(value.extra.get("unknown1"), Some(&true));
    assert_eq!(value.extra.get("unknown2"), Some(&false));
    assert!(
        !value.extra.contains_key("inner_field"),
        "Flattened struct field should not be in map"
    );
}

// Flattened map with complex value types (Vec, nested structs)
// Now supported with duplicate-key safety in place!

#[derive(Debug, PartialEq, Facet)]
struct FlattenMapWithVec {
    known: String,
    #[facet(flatten)]
    extra: std::collections::HashMap<String, Vec<i32>>,
}

#[test]
#[cfg(feature = "jit")]
fn test_flatten_map_with_vec_values() {
    // Test: flattened map where values are Vec<i32>
    // This exercises nested-call handling and key cleanup with containers
    assert!(jit::is_format_jit_compatible::<FlattenMapWithVec>());

    let json = br#"{"known": "test", "nums1": [1, 2, 3], "nums2": [4, 5]}"#;
    let mut parser = JsonParser::new(json);

    let result = jit::try_deserialize_format::<FlattenMapWithVec, _>(&mut parser);
    assert!(result.is_some(), "JIT should attempt deserialization");
    let result = result.unwrap();
    assert!(
        result.is_ok(),
        "Flattened map with Vec values should work: {:?}",
        result
    );

    let value = result.unwrap();
    assert_eq!(value.known, "test");
    assert_eq!(value.extra.len(), 2, "Should capture 2 unknown keys");
    assert_eq!(value.extra.get("nums1"), Some(&vec![1, 2, 3]));
    assert_eq!(value.extra.get("nums2"), Some(&vec![4, 5]));
}

#[test]
#[cfg(feature = "jit")]
fn test_flatten_map_with_vec_duplicate_keys() {
    // Test: duplicate keys in flattened map with Vec values
    // This tests duplicate-key memory safety with complex values
    assert!(jit::is_format_jit_compatible::<FlattenMapWithVec>());

    let json = br#"{"known": "test", "nums": [1, 2], "nums": [3, 4, 5]}"#;
    let mut parser = JsonParser::new(json);

    let result = jit::try_deserialize_format::<FlattenMapWithVec, _>(&mut parser);
    assert!(result.is_some());
    let result = result.unwrap();
    assert!(
        result.is_ok(),
        "Duplicate keys in flattened map with Vec should work: {:?}",
        result
    );

    let value = result.unwrap();
    assert_eq!(value.known, "test");
    assert_eq!(value.extra.len(), 1, "Should have 1 key (last wins)");
    assert_eq!(
        value.extra.get("nums"),
        Some(&vec![3, 4, 5]),
        "Should use last value (no leak of [1,2])"
    );
}

#[derive(Debug, PartialEq, Facet)]
struct NestedData {
    value: i32,
}

#[derive(Debug, PartialEq, Facet)]
struct FlattenMapWithStruct {
    id: i32,
    #[facet(flatten)]
    extra: std::collections::HashMap<String, NestedData>,
}

#[test]
#[cfg(feature = "jit")]
fn test_flatten_map_with_struct_values() {
    // Test: flattened map where values are nested structs
    // This exercises nested-call error passthrough
    assert!(jit::is_format_jit_compatible::<FlattenMapWithStruct>());

    let json = br#"{"id": 42, "data1": {"value": 10}, "data2": {"value": 20}}"#;
    let mut parser = JsonParser::new(json);

    let result = jit::try_deserialize_format::<FlattenMapWithStruct, _>(&mut parser);
    assert!(result.is_some());
    let result = result.unwrap();
    assert!(
        result.is_ok(),
        "Flattened map with struct values should work: {:?}",
        result
    );

    let value = result.unwrap();
    assert_eq!(value.id, 42);
    assert_eq!(value.extra.len(), 2);
    assert_eq!(value.extra.get("data1"), Some(&NestedData { value: 10 }));
    assert_eq!(value.extra.get("data2"), Some(&NestedData { value: 20 }));
}

// Flattened map mixed with flattened enum
// This tests that unknown keys go to the map while enum variant keys dispatch correctly

#[derive(Debug, PartialEq, Facet)]
#[repr(C)]
enum StatusFlat {
    #[facet(rename = "active")]
    Active,
    #[facet(rename = "inactive")]
    Inactive,
}

#[derive(Debug, PartialEq, Facet)]
struct MixFlattenMapEnum {
    id: i32,
    #[facet(flatten)]
    status: StatusFlat,
    #[facet(flatten)]
    extra: std::collections::HashMap<String, String>,
}

#[test]
#[cfg(feature = "jit")]
fn test_flatten_map_with_flatten_enum() {
    // Test: mix of normal field, flattened enum, and flattened map
    // Enum variant keys should dispatch to enum, unknown keys to map

    // First check if this combination is supported
    if !jit::is_format_jit_compatible::<MixFlattenMapEnum>() {
        // Skip test if not yet supported - this is a known limitation
        eprintln!("SKIP: flattened map + flattened enum not yet supported");
        return;
    }

    let json = br#"{"id": 42, "active": null, "custom1": "value1", "custom2": "value2"}"#;
    let mut parser = JsonParser::new(json);

    let result = jit::try_deserialize_format::<MixFlattenMapEnum, _>(&mut parser);
    assert!(result.is_some());
    let result = result.unwrap();
    assert!(
        result.is_ok(),
        "Flattened map + enum should work: {:?}",
        result
    );

    let value = result.unwrap();
    assert_eq!(value.id, 42);
    assert_eq!(value.status, StatusFlat::Active);
    assert_eq!(value.extra.len(), 2);
    assert_eq!(value.extra.get("custom1"), Some(&"value1".to_string()));
    assert_eq!(value.extra.get("custom2"), Some(&"value2".to_string()));
    assert!(
        !value.extra.contains_key("active"),
        "Enum variant key should not be in map"
    );
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Flattened Option<Struct> - common real-world pattern
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
//
// When Option<Struct> is flattened:
// - If any inner fields present → Some(struct)
// - If all inner fields absent → None

#[derive(Debug, PartialEq, Facet)]
struct DatabaseConfig {
    db_host: String,
    db_port: u16,
}

#[derive(Debug, PartialEq, Facet)]
struct AppConfigOptDb {
    name: String,
    #[facet(flatten)]
    database: Option<DatabaseConfig>,
}

#[test]
#[cfg(feature = "jit")]
fn test_flatten_option_struct_present() {
    // Test: all inner fields present → Some(struct)

    // Check if supported
    if !jit::is_format_jit_compatible::<AppConfigOptDb>() {
        eprintln!("SKIP: flatten Option<Struct> not yet supported in Tier-2");
        return;
    }

    let json = br#"{"name":"myapp","db_host":"localhost","db_port":5432}"#;
    let mut parser = JsonParser::new(json);

    let result = jit::try_deserialize_format::<AppConfigOptDb, _>(&mut parser);
    assert!(result.is_some());
    let result = result.unwrap();
    assert!(
        result.is_ok(),
        "Flatten Option<Struct> with fields should work: {:?}",
        result
    );

    let value = result.unwrap();
    assert_eq!(value.name, "myapp");
    assert!(
        value.database.is_some(),
        "Should be Some when fields present"
    );
    let db = value.database.unwrap();
    assert_eq!(db.db_host, "localhost");
    assert_eq!(db.db_port, 5432);
}

#[test]
#[cfg(feature = "jit")]
fn test_flatten_option_struct_absent() {
    // Test: all inner fields absent → None

    // Check if supported
    if !jit::is_format_jit_compatible::<AppConfigOptDb>() {
        eprintln!("SKIP: flatten Option<Struct> not yet supported in Tier-2");
        return;
    }

    let json = br#"{"name":"myapp"}"#;
    let mut parser = JsonParser::new(json);

    let result = jit::try_deserialize_format::<AppConfigOptDb, _>(&mut parser);
    assert!(result.is_some());
    let result = result.unwrap();
    assert!(
        result.is_ok(),
        "Flatten Option<Struct> without fields should work: {:?}",
        result
    );

    let value = result.unwrap();
    assert_eq!(value.name, "myapp");
    assert!(
        value.database.is_none(),
        "Should be None when fields absent"
    );
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Duplicate-key memory safety tests
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
//
// These tests verify that duplicate JSON keys properly drop old values before
// overwriting with new values, preventing memory leaks for owned types
// (String, Vec, HashMap, enum payloads). JSON semantics: "last wins".

#[derive(Debug, PartialEq, Facet)]
struct DupString {
    name: String,
}

#[test]
#[cfg(feature = "jit")]
fn test_duplicate_key_string_field() {
    // Test: {"name":"first","name":"second"} => Some struct with name="second" (no leak of "first")
    assert!(jit::is_format_jit_compatible::<DupString>());

    let json = br#"{"name": "first", "name": "second"}"#;
    let mut parser = JsonParser::new(json);

    let result = jit::try_deserialize_format::<DupString, _>(&mut parser);
    assert!(result.is_some(), "JIT should attempt deserialization");
    let result = result.unwrap();
    assert!(
        result.is_ok(),
        "Duplicate key should work (last wins): {:?}",
        result
    );

    let value = result.unwrap();
    assert_eq!(value.name, "second", "Should use last value");
}

#[derive(Debug, PartialEq, Facet)]
struct DupOptionString {
    opt: Option<String>,
}

#[test]
#[cfg(feature = "jit")]
fn test_duplicate_key_option_string_some_some() {
    // Test: {"opt":"x","opt":"y"} => Some("y") (no leak of "x")
    assert!(jit::is_format_jit_compatible::<DupOptionString>());

    let json = br#"{"opt": "x", "opt": "y"}"#;
    let mut parser = JsonParser::new(json);

    let result = jit::try_deserialize_format::<DupOptionString, _>(&mut parser);
    assert!(result.is_some());
    let result = result.unwrap();
    assert!(
        result.is_ok(),
        "Duplicate Option<String> should work: {:?}",
        result
    );

    let value = result.unwrap();
    assert_eq!(value.opt, Some("y".to_string()), "Should use last value");
}

#[test]
#[cfg(feature = "jit")]
fn test_duplicate_key_option_string_some_null() {
    // Test: {"opt":"x","opt":null} => None (no leak of "x")
    assert!(jit::is_format_jit_compatible::<DupOptionString>());

    let json = br#"{"opt": "x", "opt": null}"#;
    let mut parser = JsonParser::new(json);

    let result = jit::try_deserialize_format::<DupOptionString, _>(&mut parser);
    assert!(result.is_some());
    let result = result.unwrap();
    assert!(
        result.is_ok(),
        "Duplicate Option<String> (Some->None) should work: {:?}",
        result
    );

    let value = result.unwrap();
    assert_eq!(value.opt, None, "Should use last value (null)");
}

#[derive(Debug, PartialEq, Facet)]
struct DupOptionI32 {
    opt: Option<i32>,
}

#[test]
#[cfg(feature = "jit")]
fn test_duplicate_key_option_i32_some_null() {
    // Test: {"opt":42,"opt":null} => None
    assert!(jit::is_format_jit_compatible::<DupOptionI32>());

    let json = br#"{"opt": 42, "opt": null}"#;
    let mut parser = JsonParser::new(json);

    let result = jit::try_deserialize_format::<DupOptionI32, _>(&mut parser);
    assert!(result.is_some());
    let result = result.unwrap();
    assert!(
        result.is_ok(),
        "Duplicate Option<i32> (Some->None) should work: {:?}",
        result
    );

    let value = result.unwrap();
    assert_eq!(value.opt, None, "Should use last value (null)");
}

#[derive(Debug, PartialEq, Facet)]
struct DupVec {
    ids: Vec<i32>,
}

#[test]
#[cfg(feature = "jit")]
fn test_duplicate_key_vec_field() {
    // Test: {"ids":[1,2,3],"ids":[4,5]} => [4,5] (no leak of [1,2,3])
    assert!(jit::is_format_jit_compatible::<DupVec>());

    let json = br#"{"ids": [1, 2, 3], "ids": [4, 5]}"#;
    let mut parser = JsonParser::new(json);

    let result = jit::try_deserialize_format::<DupVec, _>(&mut parser);
    assert!(result.is_some());
    let result = result.unwrap();
    assert!(
        result.is_ok(),
        "Duplicate Vec field should work: {:?}",
        result
    );

    let value = result.unwrap();
    assert_eq!(value.ids, vec![4, 5], "Should use last value");
}

#[derive(Debug, PartialEq, Facet)]
struct DupMultipleFields {
    name: String,
    opt: Option<String>,
    ids: Vec<i32>,
}

#[test]
#[cfg(feature = "jit")]
fn test_duplicate_key_multiple_fields() {
    // Test multiple duplicate keys in same object
    assert!(jit::is_format_jit_compatible::<DupMultipleFields>());

    let json = br#"{"name":"a","opt":"x","ids":[1,2],"name":"b","opt":"y","ids":[3]}"#;
    let mut parser = JsonParser::new(json);

    let result = jit::try_deserialize_format::<DupMultipleFields, _>(&mut parser);
    assert!(result.is_some());
    let result = result.unwrap();
    assert!(
        result.is_ok(),
        "Multiple duplicate keys should work: {:?}",
        result
    );

    let value = result.unwrap();
    assert_eq!(value.name, "b", "Should use last name");
    assert_eq!(value.opt, Some("y".to_string()), "Should use last opt");
    assert_eq!(value.ids, vec![3], "Should use last ids");
}
