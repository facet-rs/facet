//! Tests for schema-based default handling.
//!
//! These tests verify that field-level `#[facet(default)]` attributes are:
//! 1. Properly extracted during schema building
//! 2. Used to fill in missing fields (so they don't appear as "missing")
//! 3. Not confused with fields that have no default and are truly missing

use crate::assert_diag_snapshot;
use facet::Facet;
use figue::{self as args, Driver, builder};

/// Test config with three fields demonstrating default behavior:
/// - one field is actually set in config
/// - one field is not set but has a default
/// - one field is not set and has no default (should be "missing")
#[derive(Facet, Debug)]
struct Args {
    #[facet(args::config, args::env_prefix = "TEST")]
    config: TestConfig,
}

#[derive(Facet, Debug)]
struct TestConfig {
    /// Field that will be set in the config file
    field_set: String,

    /// Field with a default - should NOT appear as missing
    #[facet(default = 42)]
    field_with_default: i32,

    /// Field without a default - SHOULD appear as missing
    field_required: String,
}

#[test]
fn test_schema_default_vs_missing() {
    // Only set one field, leave the other two unset
    let config_json = r#"{
        "field_set": "hello"
    }"#;

    let config = builder::<Args>()
        .unwrap()
        .file(|f| f.content(config_json, "config.json"))
        .build();

    let driver = Driver::new(config);
    let err = driver.run().unwrap_err();

    // The snapshot should show:
    // - field_set: "hello" (from file)
    // - field_with_default: 42 (from default, NOT marked as missing)
    // - field_required: MISSING (no default, no value)
    assert_diag_snapshot!(err);
}

/// Test with multiple default types to ensure they all serialize correctly
#[derive(Facet, Debug)]
struct ArgsMultipleDefaults {
    #[facet(args::config, args::env_prefix = "MULTI")]
    config: MultiDefaultConfig,
}

#[derive(Facet, Debug)]
struct MultiDefaultConfig {
    /// Required string - will be missing
    required_string: String,

    /// String with default
    #[facet(default = "default_value")]
    default_string: String,

    /// Integer with default
    #[facet(default = 100)]
    default_int: i32,

    /// Boolean with default true
    #[facet(default = true)]
    default_bool_true: bool,

    /// Boolean with default false (implicit)
    #[facet(default)]
    default_bool_false: bool,
}

#[test]
fn test_schema_multiple_default_types() {
    // Empty config - only required_string should be missing
    let config_json = r#"{}"#;

    let config = builder::<ArgsMultipleDefaults>()
        .unwrap()
        .file(|f| f.content(config_json, "config.json"))
        .build();

    let driver = Driver::new(config);
    let err = driver.run().unwrap_err();

    // The snapshot should show:
    // - required_string: MISSING
    // - default_string: "default_value" (from default)
    // - default_int: 100 (from default)
    // - default_bool_true: true (from default)
    // - default_bool_false: false (from default)
    assert_diag_snapshot!(err);
}
