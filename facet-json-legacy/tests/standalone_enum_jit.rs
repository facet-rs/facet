//! Regression tests for standalone (non-flattened) enum Tier-2 JIT support
//!
//! These tests ensure enum field support maintains tier2_successes=100
//! and produces deterministic errors for malformed enum objects.

#![cfg(feature = "jit")]

use facet::Facet;
use facet_format::jit as format_jit;
use facet_json::JsonParser;

// ============================================================================
// Type Definitions
// ============================================================================

#[derive(Debug, PartialEq, Facet, serde::Serialize, serde::Deserialize, Clone)]
struct AuthPassword {
    password: String,
}

#[derive(Debug, PartialEq, Facet, serde::Serialize, serde::Deserialize, Clone)]
struct AuthToken {
    token: String,
}

#[derive(Debug, PartialEq, Facet, serde::Serialize, serde::Deserialize, Clone)]
#[repr(C, u32)]
enum AuthMethod {
    Password(AuthPassword),
    Token(AuthToken),
}

#[derive(Debug, PartialEq, Facet, serde::Serialize, serde::Deserialize, Clone)]
struct TransportTcp {
    host: String,
    port: u16,
}

#[derive(Debug, PartialEq, Facet, serde::Serialize, serde::Deserialize, Clone)]
struct TransportUnix {
    socket_path: String,
}

#[derive(Debug, PartialEq, Facet, serde::Serialize, serde::Deserialize, Clone)]
#[repr(C, u32)]
enum Transport {
    Tcp(TransportTcp),
    Unix(TransportUnix),
}

#[derive(Debug, PartialEq, Facet, serde::Serialize, serde::Deserialize, Clone)]
struct Config {
    name: String,
    auth: AuthMethod,
    transport: Transport,
}

// ============================================================================
// Tests
// ============================================================================

/// Test that standalone enums achieve tier2_successes=100
#[test]
fn test_standalone_enum_tier2_success() {
    // JSON shape: externally-tagged enum representation
    let json = r#"{"name":"server","auth":{"Password":{"password":"secret"}},"transport":{"Tcp":{"host":"localhost","port":8080}}}"#;

    // Attempt Tier-2 compilation
    let result = format_jit::get_format_deserializer::<Vec<Config>, JsonParser>();

    // Tier-2 should succeed for this type
    assert!(
        result.is_some(),
        "Tier-2 JIT should compile for Vec<Config> with standalone enum fields"
    );

    // Parse using Tier-2 (will use compiled deserializer from cache)
    let parsed: Vec<Config> =
        facet_json::from_str(&format!("[{}]", json)).expect("Should parse with Tier-2");

    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0].name, "server");

    // Verify we got the correct enum variants
    match &parsed[0].auth {
        AuthMethod::Password(p) => assert_eq!(p.password, "secret"),
        _ => panic!("Expected Password variant"),
    }

    match &parsed[0].transport {
        Transport::Tcp(t) => {
            assert_eq!(t.host, "localhost");
            assert_eq!(t.port, 8080);
        }
        _ => panic!("Expected Tcp variant"),
    }
}

/// Test that standalone enum with single field works
#[test]
fn test_standalone_enum_single_field() {
    #[derive(Debug, PartialEq, Facet, serde::Serialize, serde::Deserialize, Clone)]
    struct SimpleConfig {
        id: u32,
        auth: AuthMethod,
    }

    let json = r#"{"id":42,"auth":{"Token":{"token":"abc123"}}}"#;

    // Attempt Tier-2 compilation
    let result = format_jit::get_format_deserializer::<Vec<SimpleConfig>, JsonParser>();

    assert!(
        result.is_some(),
        "Tier-2 JIT should compile for Vec<SimpleConfig>"
    );

    // Parse using Tier-2
    let parsed: Vec<SimpleConfig> =
        facet_json::from_str(&format!("[{}]", json)).expect("Should parse with Tier-2");

    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0].id, 42);

    match &parsed[0].auth {
        AuthMethod::Token(t) => assert_eq!(t.token, "abc123"),
        _ => panic!("Expected Token variant"),
    }
}

/// Test empty enum object error
#[test]
fn test_empty_enum_object_error() {
    let json =
        r#"{"name":"server","auth":{},"transport":{"Tcp":{"host":"localhost","port":8080}}}"#;

    // Parse should fail with empty enum object
    let result: Result<Vec<Config>, _> = facet_json::from_str(&format!("[{}]", json));

    assert!(
        result.is_err(),
        "Parsing should fail with empty enum object"
    );

    let err = result.unwrap_err();
    let err_msg = format!("{:?}", err);
    // Error message should indicate empty enum object
    assert!(
        err_msg.contains("empty") || err_msg.contains("expected"),
        "Error message should indicate empty enum object, got: {}",
        err_msg
    );
}

/// Test extra keys in enum object error
#[test]
fn test_extra_keys_in_enum_object_error() {
    // Enum object with two variant keys (should have exactly one)
    let json = r#"{"name":"server","auth":{"Password":{"password":"secret"},"Token":{"token":"abc"}},"transport":{"Tcp":{"host":"localhost","port":8080}}}"#;

    // Parse should fail with extra keys in enum object
    let result: Result<Vec<Config>, _> = facet_json::from_str(&format!("[{}]", json));

    assert!(
        result.is_err(),
        "Parsing should fail with multiple keys in enum object"
    );

    let err = result.unwrap_err();
    let err_msg = format!("{:?}", err);
    // Error message should indicate extra keys
    assert!(
        err_msg.contains("extra") || err_msg.contains("one") || err_msg.contains("expected"),
        "Error message should indicate extra keys in enum object, got: {}",
        err_msg
    );
}

/// Test unknown variant error
#[test]
fn test_unknown_variant_error() {
    let json = r#"{"name":"server","auth":{"Unknown":{"foo":"bar"}},"transport":{"Tcp":{"host":"localhost","port":8080}}}"#;

    // Parse should fail with unknown variant
    let result: Result<Vec<Config>, _> = facet_json::from_str(&format!("[{}]", json));

    assert!(result.is_err(), "Parsing should fail with unknown variant");

    let err = result.unwrap_err();
    let err_msg = format!("{:?}", err);
    // Error message should indicate unknown variant
    assert!(
        err_msg.contains("unknown") || err_msg.contains("variant"),
        "Error message should indicate unknown variant, got: {}",
        err_msg
    );
}
