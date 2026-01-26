//! Snapshot tests for deserialization error messages.
//!
//! These tests verify that error messages from type mismatches, invalid values,
//! etc. are properly formatted with Ariadne and point to the correct source
//! location (env vars, config files, CLI args).

use crate::assert_diag_snapshot;
use facet::Facet;
use figue::{self as args, builder, Driver, DriverError, MockEnv};

// ============================================================================
// Test schemas
// ============================================================================

#[derive(Facet, Debug)]
struct SimpleConfig {
    /// Port number
    port: u16,
    /// Hostname
    host: String,
}

#[derive(Facet, Debug)]
struct ArgsWithSimpleConfig {
    #[facet(args::config, args::env_prefix = "APP")]
    config: SimpleConfig,
}

#[derive(Facet, Debug)]
struct NestedConfig {
    /// Database settings
    database: DatabaseConfig,
}

#[derive(Facet, Debug)]
struct DatabaseConfig {
    /// Database URL
    url: String,
    /// Max connections
    max_connections: u32,
}

#[derive(Facet, Debug)]
struct ArgsWithNestedConfig {
    #[facet(args::config, args::env_prefix = "APP")]
    config: NestedConfig,
}

// ============================================================================
// Tests: Type mismatch errors from env vars
// ============================================================================

#[test]
fn test_deser_error_env_wrong_type_for_port() {
    // Port expects u16 but we provide a string
    let env = MockEnv::from_pairs([
        ("APP__PORT", "not_a_number"),
        ("APP__HOST", "localhost"),
    ]);

    let config = builder::<ArgsWithSimpleConfig>()
        .unwrap()
        .env(|e| e.source(env))
        .build();

    let result = Driver::new(config).run().into_result();

    match result {
        Err(DriverError::Failed { report }) => {
            assert_diag_snapshot!(report);
        }
        other => panic!("expected DriverError::Failed, got {:?}", other),
    }
}

#[test]
fn test_deser_error_env_negative_for_unsigned() {
    // Port expects u16 but we provide a negative number
    let env = MockEnv::from_pairs([("APP__PORT", "-1"), ("APP__HOST", "localhost")]);

    let config = builder::<ArgsWithSimpleConfig>()
        .unwrap()
        .env(|e| e.source(env))
        .build();

    let result = Driver::new(config).run().into_result();

    match result {
        Err(DriverError::Failed { report }) => {
            assert_diag_snapshot!(report);
        }
        other => panic!("expected DriverError::Failed, got {:?}", other),
    }
}

#[test]
fn test_deser_error_env_number_out_of_range() {
    // Port expects u16 (max 65535) but we provide a larger number
    let env = MockEnv::from_pairs([("APP__PORT", "99999"), ("APP__HOST", "localhost")]);

    let config = builder::<ArgsWithSimpleConfig>()
        .unwrap()
        .env(|e| e.source(env))
        .build();

    let result = Driver::new(config).run().into_result();

    match result {
        Err(DriverError::Failed { report }) => {
            assert_diag_snapshot!(report);
        }
        other => panic!("expected DriverError::Failed, got {:?}", other),
    }
}

// ============================================================================
// Tests: Type mismatch errors from config files
// ============================================================================

#[test]
fn test_deser_error_file_wrong_type_for_port() {
    // Port expects u16 but JSON provides a string
    let config_json = r#"{
        "port": "not_a_number",
        "host": "localhost"
    }"#;

    let config = builder::<ArgsWithSimpleConfig>()
        .unwrap()
        .file(|f| f.content(config_json, "config.json"))
        .build();

    let result = Driver::new(config).run().into_result();

    match result {
        Err(DriverError::Failed { report }) => {
            assert_diag_snapshot!(report);
        }
        other => panic!("expected DriverError::Failed, got {:?}", other),
    }
}

#[test]
fn test_deser_error_file_nested_wrong_type() {
    // max_connections expects u32 but JSON provides a string
    let config_json = r#"{
        "database": {
            "url": "postgres://localhost/db",
            "max_connections": "lots"
        }
    }"#;

    let config = builder::<ArgsWithNestedConfig>()
        .unwrap()
        .file(|f| f.content(config_json, "config.json"))
        .build();

    let result = Driver::new(config).run().into_result();

    match result {
        Err(DriverError::Failed { report }) => {
            assert_diag_snapshot!(report);
        }
        other => panic!("expected DriverError::Failed, got {:?}", other),
    }
}

// ============================================================================
// Tests: Errors from CLI config overrides
// ============================================================================

#[test]
fn test_deser_error_cli_override_wrong_type() {
    // --config.port expects u16 but we provide a string
    let config = builder::<ArgsWithSimpleConfig>()
        .unwrap()
        .cli(|cli| cli.args(["--config.port", "oops", "--config.host", "localhost"]))
        .build();

    let result = Driver::new(config).run().into_result();

    match result {
        Err(DriverError::Failed { report }) => {
            assert_diag_snapshot!(report);
        }
        other => panic!("expected DriverError::Failed, got {:?}", other),
    }
}

// ============================================================================
// Tests: Layered errors (env overrides file, CLI overrides env)
// ============================================================================

#[test]
fn test_deser_error_env_overrides_valid_file() {
    // File has valid port, but env overrides with invalid value
    let config_json = r#"{
        "port": 8080,
        "host": "localhost"
    }"#;

    let env = MockEnv::from_pairs([("APP__PORT", "invalid")]);

    let config = builder::<ArgsWithSimpleConfig>()
        .unwrap()
        .file(|f| f.content(config_json, "config.json"))
        .env(|e| e.source(env))
        .build();

    let result = Driver::new(config).run().into_result();

    match result {
        Err(DriverError::Failed { report }) => {
            assert_diag_snapshot!(report);
        }
        other => panic!("expected DriverError::Failed, got {:?}", other),
    }
}
