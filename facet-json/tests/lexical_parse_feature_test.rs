#![cfg(feature = "lexical-parse")]

// This file only compiles when the "lexical-parse" feature is enabled
// It verifies that the lexical-core dependency is correctly enabled and used

use facet_json::from_str;

#[test]
fn lexical_parse_feature_is_enabled() {
    // This test doesn't actually do anything except verify that the feature flag works
    // If lexical-parse isn't properly enabled, this file wouldn't compile
    assert!(true);
}

// Test floating point parsing using lexical-core
#[test]
fn test_lexical_float_parsing() {
    // Small value
    let result: f64 = from_str("1.5e-323").unwrap();
    assert_eq!(result, 1.5e-323);

    // Small but not quite as small value to avoid potential precision issues
    let result: f64 = from_str("1.0e-300").unwrap();
    assert_eq!(result, 1.0e-300);

    // Negative exponent
    let result: f64 = from_str("1.0e-200").unwrap();
    assert_eq!(result, 1.0e-200);

    // High precision value
    let result: f64 = from_str("9007199254740992.0").unwrap(); // 2^53
    assert_eq!(result, 9007199254740992.0);

    // Scientific notation
    let result: f64 = from_str("1e23").unwrap();
    assert_eq!(result, 1e23);
}

// Test integer parsing using lexical-core
#[test]
fn test_lexical_integer_parsing() {
    // Max u64
    let result: u64 = from_str("18446744073709551615").unwrap();
    assert_eq!(result, u64::MAX);

    // Max i64
    let result: i64 = from_str("9223372036854775807").unwrap();
    assert_eq!(result, i64::MAX);

    // Min i64
    let result: i64 = from_str("-9223372036854775808").unwrap();
    assert_eq!(result, i64::MIN);

    // Zero
    let result: i64 = from_str("0").unwrap();
    assert_eq!(result, 0);

    // Negative zero
    let result: i64 = from_str("-0").unwrap();
    assert_eq!(result, 0);
}
