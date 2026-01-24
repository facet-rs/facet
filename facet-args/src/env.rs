//! Environment variable parsing for layered configuration.
//!
//! This module reads environment variables with a given prefix and converts them
//! into a [`ConfigValue`] tree that can be merged with other configuration sources.
//!
//! # Naming Convention
//!
//! Given a prefix like `"REEF"` and nested struct fields:
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
//! - `REEF__PORT`
//! - `REEF__SMTP__HOST`
//! - `REEF__SMTP__CONNECTION_TIMEOUT`
//!
//! Rules:
//! - Prefix + field path
//! - All SCREAMING_SNAKE_CASE
//! - Double underscore (`__`) as separator (to allow single `_` in field names)

use alloc::string::String;
use alloc::vec::Vec;

use indexmap::IndexMap;

use crate::config_value::{ConfigValue, Sourced};
use crate::provenance::Provenance;

// ============================================================================
// EnvSource trait
// ============================================================================

/// Trait for abstracting over environment variable sources.
///
/// This allows testing without modifying the actual environment.
pub trait EnvSource {
    /// Get the value of an environment variable by name.
    fn get(&self, name: &str) -> Option<String>;

    /// Iterate over all environment variables.
    fn vars(&self) -> Box<dyn Iterator<Item = (String, String)> + '_>;
}

/// Environment source that reads from the actual process environment.
#[derive(Debug, Clone, Copy, Default)]
pub struct StdEnv;

impl EnvSource for StdEnv {
    fn get(&self, name: &str) -> Option<String> {
        std::env::var(name).ok()
    }

    fn vars(&self) -> Box<dyn Iterator<Item = (String, String)> + '_> {
        Box::new(std::env::vars())
    }
}

/// Environment source backed by a map (for testing).
#[derive(Debug, Clone, Default)]
pub struct MockEnv {
    vars: IndexMap<String, String, std::hash::RandomState>,
}

impl MockEnv {
    /// Create a new empty mock environment.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a mock environment from an iterator of key-value pairs.
    pub fn from_pairs<I, K, V>(iter: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        Self {
            vars: iter
                .into_iter()
                .map(|(k, v)| (k.into(), v.into()))
                .collect(),
        }
    }

    /// Set an environment variable.
    pub fn set(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.vars.insert(name.into(), value.into());
    }
}

impl EnvSource for MockEnv {
    fn get(&self, name: &str) -> Option<String> {
        self.vars.get(name).cloned()
    }

    fn vars(&self) -> Box<dyn Iterator<Item = (String, String)> + '_> {
        Box::new(self.vars.iter().map(|(k, v)| (k.clone(), v.clone())))
    }
}

/// A parsed environment variable.
#[derive(Debug, Clone)]
pub struct EnvVar {
    /// The full variable name (e.g., "REEF__SMTP__HOST").
    pub name: String,
    /// The raw value from the environment.
    pub value: String,
    /// The key path derived from the name (e.g., "smtp.host").
    pub key_path: Vec<String>,
}

/// Configuration for environment variable parsing.
#[derive(Debug, Clone)]
pub struct EnvConfig {
    /// The prefix to look for (e.g., "REEF").
    pub prefix: String,
    /// Whether to error on unknown variables (strict mode).
    pub strict: bool,
}

impl EnvConfig {
    /// Create a new env config with the given prefix.
    pub fn new(prefix: impl Into<String>) -> Self {
        Self {
            prefix: prefix.into(),
            strict: false,
        }
    }

    /// Enable strict mode (error on unknown variables).
    pub fn strict(mut self) -> Self {
        self.strict = true;
        self
    }
}

/// Result of parsing environment variables.
#[derive(Debug)]
pub struct EnvParseResult {
    /// The parsed configuration value tree.
    pub value: ConfigValue,
    /// Variables that were found but couldn't be mapped to known fields.
    /// Only populated in non-strict mode; in strict mode these cause errors.
    pub unknown: Vec<EnvVar>,
}

/// Parse a single environment variable name into key path segments.
///
/// Given prefix "REEF" and name "REEF__SMTP__HOST", returns `["smtp", "host"]`.
/// Returns `None` if the name doesn't start with the prefix.
fn parse_env_var_name(name: &str, prefix: &str) -> Option<Vec<String>> {
    // Must start with PREFIX__
    let expected_prefix = format!("{prefix}__");
    if !name.starts_with(&expected_prefix) {
        return None;
    }

    // Split the rest by __
    let rest = &name[expected_prefix.len()..];
    if rest.is_empty() {
        return None;
    }

    let segments: Vec<String> = rest
        .split("__")
        .map(|s| s.to_lowercase()) // Convert SCREAMING_SNAKE to snake_case
        .collect();

    if segments.iter().any(|s| s.is_empty()) {
        return None; // Invalid: empty segment (e.g., REEF__FOO____BAR)
    }

    Some(segments)
}

/// Read all environment variables with the given prefix from an env source.
pub fn read_env_vars_from_source(source: &impl EnvSource, prefix: &str) -> Vec<EnvVar> {
    source
        .vars()
        .filter_map(|(name, value)| {
            let key_path = parse_env_var_name(&name, prefix)?;
            Some(EnvVar {
                name,
                value,
                key_path,
            })
        })
        .collect()
}

/// Read all environment variables with the given prefix.
///
/// This function reads from `std::env::vars()` and filters for variables
/// starting with `{prefix}__`.
pub fn read_env_vars(prefix: &str) -> Vec<EnvVar> {
    read_env_vars_from_source(&StdEnv, prefix)
}

/// Build a ConfigValue tree from parsed environment variables.
///
/// Each variable becomes a leaf in the tree. For example:
/// - `REEF__PORT=8080` → `{"port": 8080}`
/// - `REEF__SMTP__HOST=mail.example.com` → `{"smtp": {"host": "mail.example.com"}}`
pub fn build_config_value(vars: Vec<EnvVar>) -> ConfigValue {
    let mut root: IndexMap<String, ConfigValue, std::hash::RandomState> = IndexMap::default();

    for var in vars {
        insert_at_path(&mut root, &var.key_path, &var);
    }

    ConfigValue::Object(Sourced::new(root))
}

/// Insert a value at the given path in the config tree.
fn insert_at_path(
    root: &mut IndexMap<String, ConfigValue, std::hash::RandomState>,
    path: &[String],
    var: &EnvVar,
) {
    if path.is_empty() {
        return;
    }

    if path.len() == 1 {
        // Leaf node - insert the value
        let key = &path[0];
        let value = ConfigValue::String(Sourced {
            value: var.value.clone(),
            span: None,
            provenance: Some(Provenance::env(&var.name, &var.value)),
        });
        root.insert(key.clone(), value);
    } else {
        // Intermediate node - ensure object exists and recurse
        let key = &path[0];
        let rest = &path[1..];

        let entry = root
            .entry(key.clone())
            .or_insert_with(|| ConfigValue::Object(Sourced::new(IndexMap::default())));

        if let ConfigValue::Object(obj) = entry {
            insert_at_path(&mut obj.value, rest, var);
        }
        // If it's not an object, we have a conflict - later value wins
        // This could happen if REEF__FOO=1 and REEF__FOO__BAR=2 are both set
    }
}

/// Parse environment variables into a ConfigValue tree.
///
/// This is the main entry point for env var parsing.
///
/// # Example
///
/// ```rust,ignore
/// use facet_args::env::{EnvConfig, parse_env};
///
/// let config = EnvConfig::new("REEF");
/// let result = parse_env(&config);
/// // result.value contains the merged config tree
/// // result.unknown contains any unrecognized variables
/// ```
pub fn parse_env(config: &EnvConfig) -> EnvParseResult {
    parse_env_with_source(config, &StdEnv)
}

/// Parse environment variables from a custom source.
///
/// This allows using a [`MockEnv`] for testing without modifying the real environment.
pub fn parse_env_with_source(config: &EnvConfig, source: &impl EnvSource) -> EnvParseResult {
    let vars = read_env_vars_from_source(source, &config.prefix);
    let value = build_config_value(vars);
    EnvParseResult {
        value,
        unknown: Vec::new(), // TODO: validate against schema to find unknown vars
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Convert a key path to a dot-separated string.
    fn key_path_to_string(key_path: &[String]) -> String {
        key_path.join(".")
    }

    #[test]
    fn test_parse_env_var_name() {
        assert_eq!(
            parse_env_var_name("REEF__PORT", "REEF"),
            Some(vec!["port".to_string()])
        );
        assert_eq!(
            parse_env_var_name("REEF__SMTP__HOST", "REEF"),
            Some(vec!["smtp".to_string(), "host".to_string()])
        );
        assert_eq!(
            parse_env_var_name("REEF__SMTP__CONNECTION_TIMEOUT", "REEF"),
            Some(vec!["smtp".to_string(), "connection_timeout".to_string()])
        );

        // Wrong prefix
        assert_eq!(parse_env_var_name("OTHER__PORT", "REEF"), None);

        // No double underscore after prefix
        assert_eq!(parse_env_var_name("REEF_PORT", "REEF"), None);

        // Empty after prefix
        assert_eq!(parse_env_var_name("REEF__", "REEF"), None);

        // Empty segment
        assert_eq!(parse_env_var_name("REEF__FOO____BAR", "REEF"), None);
    }

    #[test]
    fn test_key_path_to_string() {
        assert_eq!(key_path_to_string(&["port".to_string()]), "port");
        assert_eq!(
            key_path_to_string(&["smtp".to_string(), "host".to_string()]),
            "smtp.host"
        );
    }

    #[test]
    fn test_read_env_vars_from() {
        let env = MockEnv::from_pairs([
            ("REEF__PORT", "8080"),
            ("REEF__HOST", "localhost"),
            ("OTHER__VAR", "ignored"),
            ("REEF_NOPE", "also ignored"),
        ]);

        let parsed = read_env_vars_from_source(&env, "REEF");
        assert_eq!(parsed.len(), 2);

        let port = parsed.iter().find(|v| v.name == "REEF__PORT").unwrap();
        assert_eq!(port.value, "8080");
        assert_eq!(port.key_path, vec!["port"]);

        let host = parsed.iter().find(|v| v.name == "REEF__HOST").unwrap();
        assert_eq!(host.value, "localhost");
        assert_eq!(host.key_path, vec!["host"]);
    }

    #[test]
    fn test_build_config_value_flat() {
        let vars = vec![
            EnvVar {
                name: "REEF__PORT".to_string(),
                value: "8080".to_string(),
                key_path: vec!["port".to_string()],
            },
            EnvVar {
                name: "REEF__HOST".to_string(),
                value: "localhost".to_string(),
                key_path: vec!["host".to_string()],
            },
        ];

        let value = build_config_value(vars);

        if let ConfigValue::Object(obj) = value {
            assert_eq!(obj.value.len(), 2);

            if let Some(ConfigValue::String(port)) = obj.value.get("port") {
                assert_eq!(port.value, "8080");
                assert!(port.provenance.is_some());
                assert!(port.provenance.as_ref().unwrap().is_env());
            } else {
                panic!("expected port string");
            }

            if let Some(ConfigValue::String(host)) = obj.value.get("host") {
                assert_eq!(host.value, "localhost");
            } else {
                panic!("expected host string");
            }
        } else {
            panic!("expected object");
        }
    }

    #[test]
    fn test_build_config_value_nested() {
        let vars = vec![
            EnvVar {
                name: "REEF__SMTP__HOST".to_string(),
                value: "mail.example.com".to_string(),
                key_path: vec!["smtp".to_string(), "host".to_string()],
            },
            EnvVar {
                name: "REEF__SMTP__PORT".to_string(),
                value: "587".to_string(),
                key_path: vec!["smtp".to_string(), "port".to_string()],
            },
        ];

        let value = build_config_value(vars);

        if let ConfigValue::Object(obj) = value {
            assert_eq!(obj.value.len(), 1);

            if let Some(ConfigValue::Object(smtp)) = obj.value.get("smtp") {
                assert_eq!(smtp.value.len(), 2);

                if let Some(ConfigValue::String(host)) = smtp.value.get("host") {
                    assert_eq!(host.value, "mail.example.com");
                } else {
                    panic!("expected smtp.host string");
                }

                if let Some(ConfigValue::String(port)) = smtp.value.get("port") {
                    assert_eq!(port.value, "587");
                } else {
                    panic!("expected smtp.port string");
                }
            } else {
                panic!("expected smtp object");
            }
        } else {
            panic!("expected object");
        }
    }

    #[test]
    fn test_parse_env_with_source() {
        let env = MockEnv::from_pairs([
            ("REEF__PORT", "8080"),
            ("REEF__SMTP__HOST", "mail.example.com"),
            ("REEF__SMTP__PORT", "587"),
        ]);

        let config = EnvConfig::new("REEF");
        let result = parse_env_with_source(&config, &env);

        if let ConfigValue::Object(obj) = result.value {
            assert_eq!(obj.value.len(), 2); // port and smtp

            if let Some(ConfigValue::String(port)) = obj.value.get("port") {
                assert_eq!(port.value, "8080");
            } else {
                panic!("expected port");
            }

            if let Some(ConfigValue::Object(smtp)) = obj.value.get("smtp") {
                assert_eq!(smtp.value.len(), 2);
            } else {
                panic!("expected smtp object");
            }
        } else {
            panic!("expected object");
        }
    }

    #[test]
    fn test_provenance_is_set() {
        let env = MockEnv::from_pairs([("REEF__PORT", "8080")]);
        let config = EnvConfig::new("REEF");
        let result = parse_env_with_source(&config, &env);

        if let ConfigValue::Object(obj) = result.value {
            if let Some(ConfigValue::String(port)) = obj.value.get("port") {
                let prov = port.provenance.as_ref().expect("should have provenance");
                assert!(prov.is_env());
                if let Provenance::Env { var, value } = prov {
                    assert_eq!(var, "REEF__PORT");
                    assert_eq!(value, "8080");
                }
            } else {
                panic!("expected port");
            }
        } else {
            panic!("expected object");
        }
    }

    #[test]
    fn test_env_config_strict() {
        let config = EnvConfig::new("REEF").strict();
        assert!(config.strict);
    }
}
