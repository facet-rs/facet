//! Schema-driven environment variable parser that outputs ConfigValue with provenance.
//!
//! This module is under active development and not yet wired into the main API.
#![allow(dead_code)]
//!
//! This parser:
//! - Uses the pre-built Schema to know the config field structure
//! - Outputs LayerOutput (ConfigValue + diagnostics), not a Partial
//! - Does NOT validate types (that's the driver's job)
//! - Reports malformed env var names as diagnostics
//! - Tracks unused keys (env vars that don't match schema fields)
//!
//! # Naming Convention
//!
//! Given a prefix like `"REEF"` and a config struct:
//!
//! ```rust,ignore
//! struct ServerConfig {
//!     port: u16,
//!     smtp: SmtpConfig,
//! }
//!
//! struct SmtpConfig {
//!     host: String,
//!     connection_timeout: u64,
//! }
//! ```
//!
//! The corresponding environment variable names are:
//! - `REEF__PORT` → config.port
//! - `REEF__SMTP__HOST` → config.smtp.host
//! - `REEF__SMTP__CONNECTION_TIMEOUT` → config.smtp.connection_timeout
//!
//! Rules:
//! - Prefix implies the config field (env vars only set config, not CLI args)
//! - All SCREAMING_SNAKE_CASE
//! - Double underscore (`__`) as separator (to allow single `_` in field names)

use crate::driver::LayerOutput;
use crate::env::{EnvConfig, EnvSource};
use crate::schema::Schema;

/// Parse environment variables using the schema, returning a LayerOutput.
///
/// This reads env vars with the configured prefix and builds a ConfigValue tree
/// under the schema's config field.
pub fn parse_env(
    _schema: &Schema,
    _env_config: &EnvConfig,
    _source: &dyn EnvSource,
) -> LayerOutput {
    todo!("implement env2::parse_env")
}

#[cfg(test)]
mod tests {
    use facet::Facet;

    use crate::builder::EnvConfigBuilder;
    use crate::config_value::ConfigValue;
    use crate::driver::Severity;
    use crate::env::{EnvConfig, MockEnv};
    use crate::schema::Schema;

    use super::*;

    // ========================================================================
    // Test schemas
    // ========================================================================

    #[derive(Facet)]
    struct ArgsWithConfig {
        #[facet(crate::named)]
        verbose: bool,

        #[facet(crate::config)]
        config: ServerConfig,
    }

    #[derive(Facet)]
    struct ServerConfig {
        port: u16,
        host: String,
    }

    #[derive(Facet)]
    struct ArgsWithNestedConfig {
        #[facet(crate::config)]
        settings: AppSettings,
    }

    #[derive(Facet)]
    struct AppSettings {
        port: u16,
        smtp: SmtpConfig,
    }

    #[derive(Facet)]
    struct SmtpConfig {
        host: String,
        connection_timeout: u64,
    }

    #[derive(Facet)]
    struct ArgsWithListConfig {
        #[facet(crate::config)]
        config: ListConfig,
    }

    #[derive(Facet)]
    struct ListConfig {
        ports: Vec<u16>,
        allowed_hosts: Vec<String>,
    }

    // ========================================================================
    // Helper functions
    // ========================================================================

    fn env_config(prefix: &str) -> EnvConfig {
        EnvConfigBuilder::new().prefix(prefix).build()
    }

    fn env_config_strict(prefix: &str) -> EnvConfig {
        EnvConfigBuilder::new().prefix(prefix).strict().build()
    }

    fn get_nested<'a>(cv: &'a ConfigValue, path: &[&str]) -> Option<&'a ConfigValue> {
        let mut current = cv;
        for key in path {
            match current {
                ConfigValue::Object(obj) => {
                    current = obj.value.get(*key)?;
                }
                _ => return None,
            }
        }
        Some(current)
    }

    fn get_string(cv: &ConfigValue) -> Option<&str> {
        match cv {
            ConfigValue::String(s) => Some(&s.value),
            _ => None,
        }
    }

    fn get_array_len(cv: &ConfigValue) -> Option<usize> {
        match cv {
            ConfigValue::Array(arr) => Some(arr.value.len()),
            _ => None,
        }
    }

    // ========================================================================
    // Tests: Basic parsing
    // ========================================================================

    #[test]
    fn test_empty_env() {
        let schema = Schema::from_shape(ArgsWithConfig::SHAPE).unwrap();
        let env = MockEnv::new();
        let config = env_config("REEF");

        let output = parse_env(&schema, &config, &env);

        assert!(output.diagnostics.is_empty());
        assert!(output.unused_keys.is_empty());
        // Empty env should produce an empty object (or None?)
        // Let's say it produces an object with just the config field as empty object
    }

    #[test]
    fn test_single_flat_field() {
        let schema = Schema::from_shape(ArgsWithConfig::SHAPE).unwrap();
        let env = MockEnv::from_pairs([("REEF__PORT", "8080")]);
        let config = env_config("REEF");

        let output = parse_env(&schema, &config, &env);

        assert!(output.diagnostics.is_empty());
        let value = output.value.expect("should have value");

        // Should be {config: {port: "8080"}}
        let port = get_nested(&value, &["config", "port"]).expect("should have config.port");
        assert_eq!(get_string(port), Some("8080"));
    }

    #[test]
    fn test_multiple_flat_fields() {
        let schema = Schema::from_shape(ArgsWithConfig::SHAPE).unwrap();
        let env = MockEnv::from_pairs([("REEF__PORT", "8080"), ("REEF__HOST", "localhost")]);
        let config = env_config("REEF");

        let output = parse_env(&schema, &config, &env);

        assert!(output.diagnostics.is_empty());
        let value = output.value.expect("should have value");

        let port = get_nested(&value, &["config", "port"]).expect("should have config.port");
        assert_eq!(get_string(port), Some("8080"));

        let host = get_nested(&value, &["config", "host"]).expect("should have config.host");
        assert_eq!(get_string(host), Some("localhost"));
    }

    #[test]
    fn test_nested_field() {
        let schema = Schema::from_shape(ArgsWithNestedConfig::SHAPE).unwrap();
        let env = MockEnv::from_pairs([("REEF__SMTP__HOST", "mail.example.com")]);
        let config = env_config("REEF");

        let output = parse_env(&schema, &config, &env);

        assert!(output.diagnostics.is_empty());
        let value = output.value.expect("should have value");

        // Config field is named "settings" in this schema
        let host = get_nested(&value, &["settings", "smtp", "host"])
            .expect("should have settings.smtp.host");
        assert_eq!(get_string(host), Some("mail.example.com"));
    }

    #[test]
    fn test_deeply_nested() {
        let schema = Schema::from_shape(ArgsWithNestedConfig::SHAPE).unwrap();
        let env = MockEnv::from_pairs([
            ("REEF__PORT", "8080"),
            ("REEF__SMTP__HOST", "mail.example.com"),
            ("REEF__SMTP__CONNECTION_TIMEOUT", "30"),
        ]);
        let config = env_config("REEF");

        let output = parse_env(&schema, &config, &env);

        assert!(output.diagnostics.is_empty());
        let value = output.value.expect("should have value");

        let port = get_nested(&value, &["settings", "port"]).expect("port");
        assert_eq!(get_string(port), Some("8080"));

        let host = get_nested(&value, &["settings", "smtp", "host"]).expect("smtp.host");
        assert_eq!(get_string(host), Some("mail.example.com"));

        let timeout = get_nested(&value, &["settings", "smtp", "connection_timeout"])
            .expect("smtp.connection_timeout");
        assert_eq!(get_string(timeout), Some("30"));
    }

    // ========================================================================
    // Tests: Value handling
    // ========================================================================

    #[test]
    fn test_comma_separated_list() {
        let schema = Schema::from_shape(ArgsWithListConfig::SHAPE).unwrap();
        let env = MockEnv::from_pairs([("REEF__PORTS", "8080,8081,8082")]);
        let config = env_config("REEF");

        let output = parse_env(&schema, &config, &env);

        assert!(output.diagnostics.is_empty());
        let value = output.value.expect("should have value");

        let ports = get_nested(&value, &["config", "ports"]).expect("config.ports");
        assert_eq!(get_array_len(ports), Some(3));
    }

    #[test]
    fn test_escaped_comma() {
        let schema = Schema::from_shape(ArgsWithConfig::SHAPE).unwrap();
        let env = MockEnv::from_pairs([("REEF__HOST", r"hello\, world")]);
        let config = env_config("REEF");

        let output = parse_env(&schema, &config, &env);

        assert!(output.diagnostics.is_empty());
        let value = output.value.expect("should have value");

        let host = get_nested(&value, &["config", "host"]).expect("config.host");
        assert_eq!(get_string(host), Some("hello, world"));
    }

    #[test]
    fn test_values_stay_as_strings() {
        // We don't parse "8080" into an integer - that's the driver's job
        let schema = Schema::from_shape(ArgsWithConfig::SHAPE).unwrap();
        let env = MockEnv::from_pairs([("REEF__PORT", "8080")]);
        let config = env_config("REEF");

        let output = parse_env(&schema, &config, &env);

        let value = output.value.expect("should have value");
        let port = get_nested(&value, &["config", "port"]).expect("config.port");

        // Should be a string, not an integer
        assert!(matches!(port, ConfigValue::String(_)));
    }

    // ========================================================================
    // Tests: Provenance
    // ========================================================================

    #[test]
    fn test_provenance_is_set() {
        use crate::provenance::Provenance;

        let schema = Schema::from_shape(ArgsWithConfig::SHAPE).unwrap();
        let env = MockEnv::from_pairs([("REEF__PORT", "8080")]);
        let config = env_config("REEF");

        let output = parse_env(&schema, &config, &env);
        let value = output.value.expect("should have value");

        let port = get_nested(&value, &["config", "port"]).expect("config.port");
        if let ConfigValue::String(s) = port {
            let prov = s.provenance.as_ref().expect("should have provenance");
            assert!(matches!(prov, Provenance::Env { .. }));
            if let Provenance::Env { var, value } = prov {
                assert_eq!(var, "REEF__PORT");
                assert_eq!(value, "8080");
            }
        } else {
            panic!("expected string");
        }
    }

    // ========================================================================
    // Tests: Malformed names (diagnostics)
    // ========================================================================

    #[test]
    fn test_empty_segment_diagnostic() {
        let schema = Schema::from_shape(ArgsWithConfig::SHAPE).unwrap();
        let env = MockEnv::from_pairs([("REEF__FOO____BAR", "x")]);
        let config = env_config("REEF");

        let output = parse_env(&schema, &config, &env);

        // Should have a diagnostic about invalid env var name
        assert!(!output.diagnostics.is_empty());
        assert!(
            output
                .diagnostics
                .iter()
                .any(|d| d.message.contains("empty segment") || d.message.contains("invalid"))
        );
    }

    #[test]
    fn test_just_prefix_diagnostic() {
        let schema = Schema::from_shape(ArgsWithConfig::SHAPE).unwrap();
        let env = MockEnv::from_pairs([("REEF__", "x")]);
        let config = env_config("REEF");

        let output = parse_env(&schema, &config, &env);

        // Should have a diagnostic
        assert!(!output.diagnostics.is_empty());
    }

    #[test]
    fn test_wrong_prefix_ignored() {
        let schema = Schema::from_shape(ArgsWithConfig::SHAPE).unwrap();
        let env = MockEnv::from_pairs([("OTHER__PORT", "8080")]);
        let config = env_config("REEF");

        let output = parse_env(&schema, &config, &env);

        // No diagnostics - it's just a different prefix
        assert!(output.diagnostics.is_empty());
        assert!(output.unused_keys.is_empty());
        // No value or empty object
    }

    #[test]
    fn test_single_underscore_ignored() {
        let schema = Schema::from_shape(ArgsWithConfig::SHAPE).unwrap();
        let env = MockEnv::from_pairs([("REEF_PORT", "8080")]);
        let config = env_config("REEF");

        let output = parse_env(&schema, &config, &env);

        // Single underscore doesn't match PREFIX__ pattern
        assert!(output.diagnostics.is_empty());
        assert!(output.unused_keys.is_empty());
    }

    // ========================================================================
    // Tests: Unused keys (schema-aware)
    // ========================================================================

    #[test]
    fn test_unknown_field_unused_key() {
        let schema = Schema::from_shape(ArgsWithConfig::SHAPE).unwrap();
        // Typo: PORTT instead of PORT
        let env = MockEnv::from_pairs([("REEF__PORTT", "8080")]);
        let config = env_config("REEF");

        let output = parse_env(&schema, &config, &env);

        // Should be in unused_keys
        assert!(!output.unused_keys.is_empty());
        assert!(output.unused_keys.iter().any(|k| {
            // The key path should contain "portt"
            k.key.iter().any(|s| s == "portt")
        }));
    }

    #[test]
    fn test_unknown_nested_field_unused_key() {
        let schema = Schema::from_shape(ArgsWithNestedConfig::SHAPE).unwrap();
        // Typo: HOSTT instead of HOST
        let env = MockEnv::from_pairs([("REEF__SMTP__HOSTT", "x")]);
        let config = env_config("REEF");

        let output = parse_env(&schema, &config, &env);

        assert!(!output.unused_keys.is_empty());
    }

    #[test]
    fn test_strict_mode_unknown_field_error() {
        let schema = Schema::from_shape(ArgsWithConfig::SHAPE).unwrap();
        let env = MockEnv::from_pairs([("REEF__PORTT", "8080")]);
        let config = env_config_strict("REEF");

        let output = parse_env(&schema, &config, &env);

        // In strict mode, unknown keys should produce an error diagnostic
        assert!(
            output
                .diagnostics
                .iter()
                .any(|d| d.severity == Severity::Error)
        );
    }

    // ========================================================================
    // Tests: Edge cases
    // ========================================================================

    #[test]
    fn test_case_matching() {
        // Env vars are SCREAMING_SNAKE, schema fields are snake_case
        let schema = Schema::from_shape(ArgsWithConfig::SHAPE).unwrap();
        let env = MockEnv::from_pairs([("REEF__PORT", "8080")]);
        let config = env_config("REEF");

        let output = parse_env(&schema, &config, &env);

        assert!(output.diagnostics.is_empty());
        let value = output.value.expect("should have value");
        // "PORT" should match "port"
        assert!(get_nested(&value, &["config", "port"]).is_some());
    }

    #[test]
    fn test_field_with_underscore() {
        // connection_timeout in schema should match CONNECTION_TIMEOUT in env
        let schema = Schema::from_shape(ArgsWithNestedConfig::SHAPE).unwrap();
        let env = MockEnv::from_pairs([("REEF__SMTP__CONNECTION_TIMEOUT", "30")]);
        let config = env_config("REEF");

        let output = parse_env(&schema, &config, &env);

        assert!(output.diagnostics.is_empty());
        let value = output.value.expect("should have value");
        assert!(get_nested(&value, &["settings", "smtp", "connection_timeout"]).is_some());
    }

    #[test]
    fn test_empty_value() {
        let schema = Schema::from_shape(ArgsWithConfig::SHAPE).unwrap();
        let env = MockEnv::from_pairs([("REEF__PORT", "")]);
        let config = env_config("REEF");

        let output = parse_env(&schema, &config, &env);

        // Empty value should still be set (as empty string), not skipped
        assert!(output.diagnostics.is_empty());
        let value = output.value.expect("should have value");
        let port = get_nested(&value, &["config", "port"]).expect("config.port");
        assert_eq!(get_string(port), Some(""));
    }

    // ========================================================================
    // Tests: No config field in schema
    // ========================================================================

    #[derive(Facet)]
    struct ArgsWithoutConfig {
        #[facet(crate::named)]
        verbose: bool,
    }

    #[test]
    fn test_no_config_field_in_schema() {
        // If the schema has no config field, env vars matching the prefix
        // should all be unused keys (or we could emit a warning?)
        let schema = Schema::from_shape(ArgsWithoutConfig::SHAPE).unwrap();
        let env = MockEnv::from_pairs([("REEF__PORT", "8080")]);
        let config = env_config("REEF");

        let output = parse_env(&schema, &config, &env);

        // No config field means all env vars are unused
        assert!(!output.unused_keys.is_empty());
    }
}
