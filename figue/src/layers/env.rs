//! Schema-driven environment variable parser that outputs ConfigValue with provenance.
//!
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

use std::string::{String, ToString};
use std::vec::Vec;

use indexmap::IndexMap;

use crate::driver::LayerOutput;
use crate::provenance::Provenance;
use crate::schema::{ConfigStructSchema, ConfigValueSchema, Schema};
use crate::value_builder::{LeafValue, ValueBuilder};

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

// ============================================================================
// EnvConfig
// ============================================================================

/// Configuration for environment variable parsing.
pub struct EnvConfig {
    /// The prefix to look for (e.g., `MYAPP`). For example, configuration variable
    /// foo.bar will be overrideable via `MYAPP__FOO__BAR`.
    pub prefix: String,

    /// Whether to error out if any env vars that start with `MYAPP__` should be reported
    /// as errors and stop the program entirely (to try and catch typos)
    pub strict: bool,

    /// Custom environment source (for testing). If None, uses StdEnv.
    pub source: Option<Box<dyn EnvSource>>,
}

impl EnvConfig {
    /// Create a new EnvConfig with the given prefix.
    pub fn new(prefix: impl Into<String>) -> Self {
        Self {
            prefix: prefix.into(),
            strict: false,
            source: None,
        }
    }

    /// Enable strict mode.
    pub fn strict(mut self) -> Self {
        self.strict = true;
        self
    }

    /// Get the env source, or StdEnv if none set.
    pub fn source(&self) -> &dyn EnvSource {
        self.source.as_ref().map(|s| s.as_ref()).unwrap_or(&StdEnv)
    }
}

/// Builder for environment variable configuration.
#[derive(Default)]
pub struct EnvConfigBuilder {
    prefix: String,
    strict: bool,
    source: Option<Box<dyn EnvSource>>,
}

impl EnvConfigBuilder {
    /// Create a new env config builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the environment variable prefix.
    pub fn prefix(mut self, prefix: impl Into<String>) -> Self {
        self.prefix = prefix.into();
        self
    }

    /// Enable strict mode - error on unknown env vars with the prefix.
    pub fn strict(mut self) -> Self {
        self.strict = true;
        self
    }

    /// Use a custom environment source (for testing).
    pub fn source(mut self, source: impl EnvSource + 'static) -> Self {
        self.source = Some(Box::new(source));
        self
    }

    /// Build the env configuration.
    pub fn build(self) -> EnvConfig {
        let mut config = EnvConfig::new(self.prefix);
        if self.strict {
            config = config.strict();
        }
        config.source = self.source;
        config
    }
}

/// Parse environment variables using the schema, returning a LayerOutput.
///
/// This reads env vars with the configured prefix and builds a ConfigValue tree
/// under the schema's config field.
pub fn parse_env(schema: &Schema, env_config: &EnvConfig, source: &dyn EnvSource) -> LayerOutput {
    // Get the config schema - if there's no config field, we can't parse env vars
    let Some(config_schema) = schema.config() else {
        return parse_env_no_config(env_config, source);
    };

    // Use explicit prefix from config, or fall back to schema's env_prefix
    let prefix = if env_config.prefix.is_empty() {
        config_schema.env_prefix().unwrap_or("")
    } else {
        &env_config.prefix
    };

    let prefix_with_sep = format!("{}__", prefix);

    // Create a ValueBuilder
    let mut builder = ValueBuilder::new(config_schema);

    // Track which paths were set by prefixed vars (so aliases don't override them)
    let mut prefixed_paths: Vec<Vec<String>> = Vec::new();

    // First pass: collect all prefixed env vars (higher priority)
    for (name, value) in source.vars() {
        // Check if this var matches our prefix
        if !name.starts_with(&prefix_with_sep) {
            continue;
        }

        // Extract the path after the prefix
        let rest = &name[prefix_with_sep.len()..];
        if rest.is_empty() {
            builder.warn(format!(
                "invalid environment variable name: {} (empty after prefix)",
                name
            ));
            continue;
        }

        // Parse the path segments
        let segments: Vec<&str> = rest.split("__").collect();

        // Check for empty segments
        if segments.iter().any(|s| s.is_empty()) {
            builder.warn(format!(
                "invalid environment variable name: {} (contains empty segment)",
                name
            ));
            continue;
        }

        // Convert to lowercase for field matching
        let path: Vec<String> = segments.iter().map(|s| s.to_lowercase()).collect();

        // Create provenance for this env var
        let prov = Provenance::env(&name, &value);

        // Validate enum values if the target is an enum
        validate_enum_value_if_applicable(&mut builder, config_schema, &path, &value, &name);

        // Parse the value (handle comma-separated lists)
        let leaf_value = parse_env_value(&value);

        // Set the value with its provenance
        if builder.set(&path, leaf_value, None, prov) {
            prefixed_paths.push(path);
        } else if env_config.strict {
            builder.error(format!("unknown configuration key: {}", path.join(".")));
        }
    }

    // Second pass: check env aliases (lower priority than prefixed vars)
    check_env_aliases(&mut builder, config_schema, source, &[], &prefixed_paths);

    builder.into_output(config_schema.field_name())
}

/// Recursively check env aliases for all fields in a config struct.
fn check_env_aliases(
    builder: &mut ValueBuilder,
    schema: &ConfigStructSchema,
    source: &dyn EnvSource,
    parent_path: &[String],
    prefixed_paths: &[Vec<String>],
) {
    for (field_name, field_schema) in schema.fields() {
        let mut field_path = parent_path.to_vec();
        field_path.push(field_name.clone());

        // Check if this field has aliases and wasn't already set by a prefixed var
        let already_set = prefixed_paths.contains(&field_path);
        if !already_set {
            for alias in field_schema.env_aliases() {
                if let Some(value) = source.get(alias) {
                    let prov = Provenance::env(alias, &value);
                    let leaf_value = parse_env_value(&value);
                    builder.set(&field_path, leaf_value, None, prov);
                    // Only use the first matching alias
                    break;
                }
            }
        }

        // Recurse into nested structs
        match field_schema.value() {
            ConfigValueSchema::Struct(nested) => {
                check_env_aliases(builder, nested, source, &field_path, prefixed_paths);
            }
            ConfigValueSchema::Option { value, .. } => {
                if let ConfigValueSchema::Struct(nested) = value.as_ref() {
                    check_env_aliases(builder, nested, source, &field_path, prefixed_paths);
                }
            }
            _ => {}
        }
    }
}

/// Handle the case where there's no config field in the schema.
fn parse_env_no_config(env_config: &EnvConfig, source: &dyn EnvSource) -> LayerOutput {
    use crate::config_value::{ConfigValue, Sourced};
    use crate::driver::UnusedKey;

    let prefix = &env_config.prefix;
    let prefix_with_sep = format!("{}__", prefix);

    let mut unused_keys = Vec::new();

    for (name, _value) in source.vars() {
        if name.starts_with(&prefix_with_sep) {
            let rest = &name[prefix_with_sep.len()..];
            if !rest.is_empty() {
                let segments: Vec<&str> = rest.split("__").collect();
                if !segments.iter().any(|s| s.is_empty()) {
                    let path: Vec<String> = segments.iter().map(|s| s.to_lowercase()).collect();
                    unused_keys.push(UnusedKey {
                        key: path,
                        provenance: Provenance::env(&name, ""),
                    });
                }
            }
        }
    }

    LayerOutput {
        value: Some(ConfigValue::Object(Sourced::new(IndexMap::default()))),
        unused_keys,
        diagnostics: Vec::new(),
    }
}

/// Parse an env var value, handling comma-separated lists.
fn parse_env_value(value: &str) -> LeafValue {
    if value.contains(',') {
        let elements = parse_comma_separated(value);
        if elements.len() > 1 {
            return LeafValue::StringArray(elements);
        } else if elements.len() == 1 {
            return LeafValue::String(elements.into_iter().next().unwrap());
        }
    }
    LeafValue::String(value.to_string())
}

/// Validate enum value if the target path points to an enum field.
fn validate_enum_value_if_applicable(
    builder: &mut ValueBuilder,
    schema: &ConfigStructSchema,
    path: &[String],
    value: &str,
    var_name: &str,
) {
    if let Some(value_schema) = schema.get_by_path(&path.to_vec()) {
        // Unwrap Option wrapper if present
        let inner_schema = match value_schema {
            ConfigValueSchema::Option { value: inner, .. } => inner.as_ref(),
            other => other,
        };

        // For enum fields, validate the value is a known variant
        if let ConfigValueSchema::Enum(enum_schema) = inner_schema {
            let variants = enum_schema.variants();
            if !variants.contains_key(value) {
                let valid_variants: Vec<&String> = variants.keys().collect();
                builder.warn(format!(
                    "{}: unknown variant '{}' for {}. Valid variants are: {}",
                    var_name,
                    value,
                    path.join("."),
                    valid_variants
                        .iter()
                        .map(|v| format!("'{}'", v))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
        }
    }
}

/// Parse a comma-separated string, handling escaping.
fn parse_comma_separated(input: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\\' {
            if let Some(&next) = chars.peek() {
                if next == ',' {
                    chars.next();
                    current.push(',');
                } else {
                    current.push(ch);
                }
            } else {
                current.push(ch);
            }
        } else if ch == ',' {
            let trimmed = current.trim().to_string();
            if !trimmed.is_empty() {
                result.push(trimmed);
            }
            current.clear();
        } else {
            current.push(ch);
        }
    }

    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        result.push(trimmed);
    }

    if result.is_empty() {
        result.push(input.to_string());
    }

    result
}

#[cfg(test)]
mod tests {
    use facet::Facet;
    use figue_attrs as args;

    use crate::config_value::ConfigValue;
    use crate::driver::Severity;
    use crate::schema::Schema;

    use super::*;

    // ========================================================================
    // Test schemas
    // ========================================================================

    #[derive(Facet)]
    struct ArgsWithConfig {
        #[facet(args::named)]
        verbose: bool,

        #[facet(args::config)]
        config: ServerConfig,
    }

    #[derive(Facet)]
    struct ServerConfig {
        port: u16,
        host: String,
    }

    #[derive(Facet)]
    struct ArgsWithNestedConfig {
        #[facet(args::config)]
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
        #[facet(args::config)]
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
        #[facet(args::named)]
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

    // ========================================================================
    // Tests: Flattened config fields
    // ========================================================================

    #[derive(Facet)]
    struct CommonConfig {
        log_level: String,
        debug: bool,
    }

    #[derive(Facet)]
    struct ServerConfigWithFlatten {
        port: u16,
        #[facet(flatten)]
        common: CommonConfig,
    }

    #[derive(Facet)]
    struct ArgsWithFlattenConfig {
        #[facet(args::named)]
        verbose: bool,

        #[facet(args::config)]
        config: ServerConfigWithFlatten,
    }

    #[test]
    fn test_flatten_config_parses_flattened_field() {
        // With flatten, REEF__LOG_LEVEL (not REEF__COMMON__LOG_LEVEL) sets log_level
        // The schema hoists flattened fields to the parent level
        let schema = Schema::from_shape(ArgsWithFlattenConfig::SHAPE).unwrap();
        let env = MockEnv::from_pairs([("REEF__LOG_LEVEL", "debug")]);
        let config = env_config("REEF");

        let output = parse_env(&schema, &config, &env);

        assert!(
            output.diagnostics.is_empty(),
            "diagnostics: {:?}",
            output.diagnostics
        );
        assert!(
            output.unused_keys.is_empty(),
            "unused keys: {:?}",
            output.unused_keys
        );

        let value = output.value.expect("should have value");
        // Flattened field appears at the flattened level, not nested
        let log_level = get_nested(&value, &["config", "log_level"]).expect("config.log_level");
        assert_eq!(get_string(log_level), Some("debug"));
    }

    #[test]
    fn test_flatten_config_top_level_and_flattened() {
        // Mix of top-level (port) and flattened (debug from common) config fields
        let schema = Schema::from_shape(ArgsWithFlattenConfig::SHAPE).unwrap();
        // PORT is not flattened, DEBUG is flattened from common
        let env = MockEnv::from_pairs([("REEF__PORT", "8080"), ("REEF__DEBUG", "true")]);
        let config = env_config("REEF");

        let output = parse_env(&schema, &config, &env);

        assert!(
            output.diagnostics.is_empty(),
            "diagnostics: {:?}",
            output.diagnostics
        );
        assert!(
            output.unused_keys.is_empty(),
            "unused keys: {:?}",
            output.unused_keys
        );

        let value = output.value.expect("should have value");
        let port = get_nested(&value, &["config", "port"]).expect("config.port");
        assert_eq!(get_string(port), Some("8080"));

        // Flattened field appears at the flattened level
        let debug = get_nested(&value, &["config", "debug"]).expect("config.debug");
        assert_eq!(get_string(debug), Some("true"));
    }

    // ------------------------------------------------------------------------
    // Two-level flatten tests
    // ------------------------------------------------------------------------

    #[derive(Facet)]
    struct DeepConfig {
        trace: bool,
    }

    #[derive(Facet)]
    struct MiddleConfig {
        #[facet(flatten)]
        deep: DeepConfig,
        verbose: bool,
    }

    #[derive(Facet)]
    struct OuterConfigWithDeepFlatten {
        name: String,
        #[facet(flatten)]
        middle: MiddleConfig,
    }

    #[derive(Facet)]
    struct ArgsWithDeepFlattenConfig {
        #[facet(args::config)]
        config: OuterConfigWithDeepFlatten,
    }

    #[test]
    fn test_two_level_flatten_config() {
        // trace is flattened from deep -> middle -> outer
        // So REEF__TRACE should work (not REEF__MIDDLE__DEEP__TRACE)
        let schema = Schema::from_shape(ArgsWithDeepFlattenConfig::SHAPE).unwrap();
        let env = MockEnv::from_pairs([
            ("REEF__NAME", "myapp"),
            ("REEF__VERBOSE", "true"),
            ("REEF__TRACE", "true"),
        ]);
        let config = env_config("REEF");

        let output = parse_env(&schema, &config, &env);

        assert!(
            output.diagnostics.is_empty(),
            "diagnostics: {:?}",
            output.diagnostics
        );
        assert!(
            output.unused_keys.is_empty(),
            "unused keys: {:?}",
            output.unused_keys
        );

        let value = output.value.expect("should have value");

        // All fields appear at the top config level due to flattening
        let name = get_nested(&value, &["config", "name"]).expect("config.name");
        assert_eq!(get_string(name), Some("myapp"));

        let verbose = get_nested(&value, &["config", "verbose"]).expect("config.verbose");
        assert_eq!(get_string(verbose), Some("true"));

        let trace = get_nested(&value, &["config", "trace"]).expect("config.trace");
        assert_eq!(get_string(trace), Some("true"));
    }

    #[test]
    fn test_nested_path_rejected_for_flattened_field() {
        // REEF__COMMON__LOG_LEVEL should be rejected because "common" doesn't exist
        // in the flattened schema (log_level is hoisted to the parent)
        let schema = Schema::from_shape(ArgsWithFlattenConfig::SHAPE).unwrap();
        let env = MockEnv::from_pairs([("REEF__COMMON__LOG_LEVEL", "debug")]);
        let config = env_config("REEF");

        let output = parse_env(&schema, &config, &env);

        // Should have an unused key because "common" doesn't exist in schema
        assert!(
            !output.unused_keys.is_empty(),
            "should reject nested path for flattened field"
        );
        assert!(
            output
                .unused_keys
                .iter()
                .any(|k| k.key.contains(&"common".to_string())),
            "unused key should contain 'common': {:?}",
            output.unused_keys
        );
    }

    // ========================================================================
    // Tests: Env aliases
    // ========================================================================

    #[derive(Facet)]
    struct ConfigWithAlias {
        /// Database URL with standard alias
        #[facet(args::env_alias = "DATABASE_URL")]
        database_url: String,

        /// Port without alias
        port: u16,
    }

    #[derive(Facet)]
    struct ArgsWithAliasConfig {
        #[facet(args::config)]
        config: ConfigWithAlias,
    }

    #[test]
    fn test_env_alias_basic() {
        // DATABASE_URL should be read via the alias
        let schema = Schema::from_shape(ArgsWithAliasConfig::SHAPE).unwrap();
        let env = MockEnv::from_pairs([("DATABASE_URL", "postgres://localhost/mydb")]);
        let config = env_config("REEF");

        let output = parse_env(&schema, &config, &env);

        assert!(
            output.diagnostics.is_empty(),
            "diagnostics: {:?}",
            output.diagnostics
        );
        let value = output.value.expect("should have value");

        let db_url = get_nested(&value, &["config", "database_url"]).expect("config.database_url");
        assert_eq!(get_string(db_url), Some("postgres://localhost/mydb"));
    }

    #[test]
    fn test_env_alias_prefixed_wins() {
        // When both prefixed and alias are set, prefixed wins
        let schema = Schema::from_shape(ArgsWithAliasConfig::SHAPE).unwrap();
        let env = MockEnv::from_pairs([
            ("DATABASE_URL", "alias_value"),
            ("REEF__DATABASE_URL", "prefixed_value"),
        ]);
        let config = env_config("REEF");

        let output = parse_env(&schema, &config, &env);

        assert!(
            output.diagnostics.is_empty(),
            "diagnostics: {:?}",
            output.diagnostics
        );
        let value = output.value.expect("should have value");

        let db_url = get_nested(&value, &["config", "database_url"]).expect("config.database_url");
        // Prefixed var should win over alias
        assert_eq!(get_string(db_url), Some("prefixed_value"));
    }

    #[test]
    fn test_env_alias_only_alias_set() {
        // Only alias set, no prefixed - alias should be used
        let schema = Schema::from_shape(ArgsWithAliasConfig::SHAPE).unwrap();
        let env = MockEnv::from_pairs([("DATABASE_URL", "alias_value"), ("REEF__PORT", "8080")]);
        let config = env_config("REEF");

        let output = parse_env(&schema, &config, &env);

        assert!(
            output.diagnostics.is_empty(),
            "diagnostics: {:?}",
            output.diagnostics
        );
        let value = output.value.expect("should have value");

        let db_url = get_nested(&value, &["config", "database_url"]).expect("config.database_url");
        assert_eq!(get_string(db_url), Some("alias_value"));

        let port = get_nested(&value, &["config", "port"]).expect("config.port");
        assert_eq!(get_string(port), Some("8080"));
    }

    #[derive(Facet)]
    struct ConfigWithMultipleAliases {
        /// Database URL with multiple aliases
        #[facet(args::env_alias = "DATABASE_URL", args::env_alias = "DB_URL")]
        database_url: String,
    }

    #[derive(Facet)]
    struct ArgsWithMultipleAliasConfig {
        #[facet(args::config)]
        config: ConfigWithMultipleAliases,
    }

    #[test]
    fn test_env_alias_multiple_aliases_first_wins() {
        // When multiple aliases exist, the first one found is used
        let schema = Schema::from_shape(ArgsWithMultipleAliasConfig::SHAPE).unwrap();
        // Only the second alias is set
        let env = MockEnv::from_pairs([("DB_URL", "second_alias_value")]);
        let config = env_config("REEF");

        let output = parse_env(&schema, &config, &env);

        assert!(
            output.diagnostics.is_empty(),
            "diagnostics: {:?}",
            output.diagnostics
        );
        let value = output.value.expect("should have value");

        let db_url = get_nested(&value, &["config", "database_url"]).expect("config.database_url");
        assert_eq!(get_string(db_url), Some("second_alias_value"));
    }

    #[test]
    fn test_env_alias_provenance() {
        // Provenance should show the actual alias var that was used
        use crate::provenance::Provenance;

        let schema = Schema::from_shape(ArgsWithAliasConfig::SHAPE).unwrap();
        let env = MockEnv::from_pairs([("DATABASE_URL", "postgres://localhost/mydb")]);
        let config = env_config("REEF");

        let output = parse_env(&schema, &config, &env);

        let value = output.value.expect("should have value");
        let db_url = get_nested(&value, &["config", "database_url"]).expect("config.database_url");

        if let ConfigValue::String(s) = db_url {
            let prov = s.provenance.as_ref().expect("should have provenance");
            if let Provenance::Env { var, value } = prov {
                // Should show DATABASE_URL, not REEF__DATABASE_URL
                assert_eq!(var, "DATABASE_URL");
                assert_eq!(value, "postgres://localhost/mydb");
            } else {
                panic!("expected Env provenance");
            }
        } else {
            panic!("expected string");
        }
    }

    #[derive(Facet)]
    struct NestedConfigWithAlias {
        db: DbConfig,
    }

    #[derive(Facet)]
    struct DbConfig {
        #[facet(args::env_alias = "DATABASE_URL")]
        url: String,
    }

    #[derive(Facet)]
    struct ArgsWithNestedAliasConfig {
        #[facet(args::config)]
        config: NestedConfigWithAlias,
    }

    #[test]
    fn test_env_alias_in_nested_struct() {
        // Alias should work even when field is in a nested struct
        let schema = Schema::from_shape(ArgsWithNestedAliasConfig::SHAPE).unwrap();
        let env = MockEnv::from_pairs([("DATABASE_URL", "postgres://localhost/mydb")]);
        let config = env_config("REEF");

        let output = parse_env(&schema, &config, &env);

        assert!(
            output.diagnostics.is_empty(),
            "diagnostics: {:?}",
            output.diagnostics
        );
        let value = output.value.expect("should have value");

        let url = get_nested(&value, &["config", "db", "url"]).expect("config.db.url");
        assert_eq!(get_string(url), Some("postgres://localhost/mydb"));
    }

    #[test]
    fn test_env_alias_nested_prefixed_wins() {
        // Prefixed var should win over alias even in nested struct
        let schema = Schema::from_shape(ArgsWithNestedAliasConfig::SHAPE).unwrap();
        let env = MockEnv::from_pairs([
            ("DATABASE_URL", "alias_value"),
            ("REEF__DB__URL", "prefixed_value"),
        ]);
        let config = env_config("REEF");

        let output = parse_env(&schema, &config, &env);

        let value = output.value.expect("should have value");
        let url = get_nested(&value, &["config", "db", "url"]).expect("config.db.url");
        assert_eq!(get_string(url), Some("prefixed_value"));
    }

    // ========================================================================
    // Tests: Schema-guided enum validation
    // ========================================================================

    /// Log level enum for testing enum validation
    #[derive(Facet)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum LogLevel {
        Debug,
        Info,
        Warn,
        Error,
    }

    #[derive(Facet)]
    struct ConfigWithEnum {
        log_level: LogLevel,
        port: u16,
    }

    #[derive(Facet)]
    struct ArgsWithEnumConfig {
        #[facet(args::config)]
        config: ConfigWithEnum,
    }

    #[test]
    fn test_enum_valid_variant_no_warning() {
        // Valid variant should not produce a warning
        let schema = Schema::from_shape(ArgsWithEnumConfig::SHAPE).unwrap();
        let env = MockEnv::from_pairs([("REEF__LOG_LEVEL", "Debug")]);
        let config = env_config("REEF");

        let output = parse_env(&schema, &config, &env);

        assert!(
            output.diagnostics.is_empty(),
            "valid enum variant should not produce warnings: {:?}",
            output.diagnostics
        );

        let value = output.value.expect("should have value");
        let log_level = get_nested(&value, &["config", "log_level"]).expect("config.log_level");
        assert_eq!(get_string(log_level), Some("Debug"));
    }

    #[test]
    fn test_enum_invalid_variant_produces_warning() {
        // Invalid variant should produce a warning with helpful message
        let schema = Schema::from_shape(ArgsWithEnumConfig::SHAPE).unwrap();
        let env = MockEnv::from_pairs([("REEF__LOG_LEVEL", "Debugg")]); // typo
        let config = env_config("REEF");

        let output = parse_env(&schema, &config, &env);

        // Should have a warning
        assert!(
            !output.diagnostics.is_empty(),
            "invalid enum variant should produce a warning"
        );

        // Warning should mention the invalid value and valid variants
        let warning = &output.diagnostics[0];
        assert!(
            warning.message.contains("Debugg"),
            "warning should mention the invalid value: {}",
            warning.message
        );
        assert!(
            warning.message.contains("Debug")
                && warning.message.contains("Info")
                && warning.message.contains("Warn")
                && warning.message.contains("Error"),
            "warning should list valid variants: {}",
            warning.message
        );

        // Value should still be set (the driver will do the actual parsing/rejection)
        let value = output.value.expect("should have value");
        let log_level = get_nested(&value, &["config", "log_level"]).expect("config.log_level");
        assert_eq!(get_string(log_level), Some("Debugg"));
    }

    #[derive(Facet)]
    struct ConfigWithOptionalEnum {
        log_level: Option<LogLevel>,
    }

    #[derive(Facet)]
    struct ArgsWithOptionalEnumConfig {
        #[facet(args::config)]
        config: ConfigWithOptionalEnum,
    }

    #[test]
    fn test_optional_enum_validation() {
        // Optional enum should also be validated
        let schema = Schema::from_shape(ArgsWithOptionalEnumConfig::SHAPE).unwrap();
        let env = MockEnv::from_pairs([("REEF__LOG_LEVEL", "invalid")]);
        let config = env_config("REEF");

        let output = parse_env(&schema, &config, &env);

        // Should have a warning even for optional enum
        assert!(
            !output.diagnostics.is_empty(),
            "invalid optional enum variant should produce a warning"
        );
    }

    #[derive(Facet)]
    struct NestedConfigWithEnum {
        logging: LoggingConfig,
    }

    #[derive(Facet)]
    struct LoggingConfig {
        level: LogLevel,
    }

    #[derive(Facet)]
    struct ArgsWithNestedEnumConfig {
        #[facet(args::config)]
        config: NestedConfigWithEnum,
    }

    #[test]
    fn test_nested_enum_validation() {
        // Enum in nested struct should be validated
        let schema = Schema::from_shape(ArgsWithNestedEnumConfig::SHAPE).unwrap();
        let env = MockEnv::from_pairs([("REEF__LOGGING__LEVEL", "unknown")]);
        let config = env_config("REEF");

        let output = parse_env(&schema, &config, &env);

        // Should have a warning
        assert!(
            !output.diagnostics.is_empty(),
            "invalid nested enum variant should produce a warning"
        );
    }

    // ========================================================================
    // Tests: Enum variant field setting (issue #37)
    // ========================================================================

    /// Storage enum with struct variants for testing enum variant field paths
    #[derive(Facet)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum Storage {
        S3 { bucket: String, region: String },
        Gcp { project: String, zone: String },
        Local { path: String },
    }

    #[derive(Facet)]
    struct ConfigWithEnumVariants {
        storage: Storage,
        port: u16,
    }

    #[derive(Facet)]
    struct ArgsWithEnumVariantConfig {
        #[facet(args::config)]
        config: ConfigWithEnumVariants,
    }

    #[test]
    fn test_enum_variant_field_single() {
        // Setting a single field within an enum variant: REEF__STORAGE__S3__BUCKET
        let schema = Schema::from_shape(ArgsWithEnumVariantConfig::SHAPE).unwrap();
        let env = MockEnv::from_pairs([("REEF__STORAGE__S3__BUCKET", "my-bucket")]);
        let config = env_config("REEF");

        let output = parse_env(&schema, &config, &env);

        assert!(
            output.diagnostics.is_empty(),
            "should not have diagnostics: {:?}",
            output.diagnostics
        );
        assert!(
            output.unused_keys.is_empty(),
            "should not have unused keys: {:?}",
            output.unused_keys
        );

        let value = output.value.expect("should have value");
        // Should produce: {config: {storage: {s3: {bucket: "my-bucket"}}}}
        let bucket = get_nested(&value, &["config", "storage", "S3", "bucket"])
            .expect("should have config.storage.S3.bucket");
        assert_eq!(get_string(bucket), Some("my-bucket"));
    }

    #[test]
    fn test_enum_variant_field_multiple() {
        // Setting multiple fields within the same enum variant
        let schema = Schema::from_shape(ArgsWithEnumVariantConfig::SHAPE).unwrap();
        let env = MockEnv::from_pairs([
            ("REEF__STORAGE__S3__BUCKET", "my-bucket"),
            ("REEF__STORAGE__S3__REGION", "us-east-1"),
        ]);
        let config = env_config("REEF");

        let output = parse_env(&schema, &config, &env);

        assert!(
            output.diagnostics.is_empty(),
            "should not have diagnostics: {:?}",
            output.diagnostics
        );
        assert!(
            output.unused_keys.is_empty(),
            "should not have unused keys: {:?}",
            output.unused_keys
        );

        let value = output.value.expect("should have value");
        let bucket = get_nested(&value, &["config", "storage", "S3", "bucket"])
            .expect("should have config.storage.S3.bucket");
        assert_eq!(get_string(bucket), Some("my-bucket"));

        let region = get_nested(&value, &["config", "storage", "S3", "region"])
            .expect("should have config.storage.S3.region");
        assert_eq!(get_string(region), Some("us-east-1"));
    }

    #[test]
    fn test_enum_variant_field_different_variant() {
        // Setting fields in a different variant (Gcp instead of S3)
        let schema = Schema::from_shape(ArgsWithEnumVariantConfig::SHAPE).unwrap();
        let env = MockEnv::from_pairs([("REEF__STORAGE__GCP__PROJECT", "my-project")]);
        let config = env_config("REEF");

        let output = parse_env(&schema, &config, &env);

        assert!(
            output.diagnostics.is_empty(),
            "should not have diagnostics: {:?}",
            output.diagnostics
        );
        assert!(
            output.unused_keys.is_empty(),
            "should not have unused keys: {:?}",
            output.unused_keys
        );

        let value = output.value.expect("should have value");
        let project = get_nested(&value, &["config", "storage", "Gcp", "project"])
            .expect("should have config.storage.Gcp.project");
        assert_eq!(get_string(project), Some("my-project"));
    }

    #[test]
    fn test_enum_variant_field_with_regular_field() {
        // Mix of enum variant field and regular config field
        let schema = Schema::from_shape(ArgsWithEnumVariantConfig::SHAPE).unwrap();
        let env = MockEnv::from_pairs([
            ("REEF__STORAGE__S3__BUCKET", "my-bucket"),
            ("REEF__PORT", "8080"),
        ]);
        let config = env_config("REEF");

        let output = parse_env(&schema, &config, &env);

        assert!(
            output.diagnostics.is_empty(),
            "should not have diagnostics: {:?}",
            output.diagnostics
        );
        assert!(
            output.unused_keys.is_empty(),
            "should not have unused keys: {:?}",
            output.unused_keys
        );

        let value = output.value.expect("should have value");
        let bucket = get_nested(&value, &["config", "storage", "S3", "bucket"])
            .expect("should have config.storage.S3.bucket");
        assert_eq!(get_string(bucket), Some("my-bucket"));

        let port = get_nested(&value, &["config", "port"]).expect("should have config.port");
        assert_eq!(get_string(port), Some("8080"));
    }

    #[test]
    fn test_enum_variant_unknown_variant_rejected() {
        // Unknown variant name should be rejected
        let schema = Schema::from_shape(ArgsWithEnumVariantConfig::SHAPE).unwrap();
        let env = MockEnv::from_pairs([("REEF__STORAGE__AZURE__CONTAINER", "my-container")]);
        let config = env_config("REEF");

        let output = parse_env(&schema, &config, &env);

        // Should have an unused key
        assert!(
            !output.unused_keys.is_empty(),
            "unknown variant should produce unused key"
        );
        assert!(
            output
                .unused_keys
                .iter()
                .any(|k| k.key.iter().any(|s| s == "azure")),
            "unused key should mention azure: {:?}",
            output.unused_keys
        );
    }

    #[test]
    fn test_enum_variant_unknown_field_rejected() {
        // Unknown field within a valid variant should be rejected
        let schema = Schema::from_shape(ArgsWithEnumVariantConfig::SHAPE).unwrap();
        let env = MockEnv::from_pairs([("REEF__STORAGE__S3__UNKNOWN_FIELD", "value")]);
        let config = env_config("REEF");

        let output = parse_env(&schema, &config, &env);

        // Should have an unused key
        assert!(
            !output.unused_keys.is_empty(),
            "unknown field in variant should produce unused key"
        );
    }

    #[derive(Facet)]
    struct ConfigWithOptionalEnumVariants {
        storage: Option<Storage>,
    }

    #[derive(Facet)]
    struct ArgsWithOptionalEnumVariantConfig {
        #[facet(args::config)]
        config: ConfigWithOptionalEnumVariants,
    }

    #[test]
    fn test_optional_enum_variant_field() {
        // Setting field within optional enum variant
        let schema = Schema::from_shape(ArgsWithOptionalEnumVariantConfig::SHAPE).unwrap();
        let env = MockEnv::from_pairs([("REEF__STORAGE__LOCAL__PATH", "/data")]);
        let config = env_config("REEF");

        let output = parse_env(&schema, &config, &env);

        assert!(
            output.diagnostics.is_empty(),
            "should not have diagnostics: {:?}",
            output.diagnostics
        );
        assert!(
            output.unused_keys.is_empty(),
            "should not have unused keys: {:?}",
            output.unused_keys
        );

        let value = output.value.expect("should have value");
        let path = get_nested(&value, &["config", "storage", "Local", "path"])
            .expect("should have config.storage.Local.path");
        assert_eq!(get_string(path), Some("/data"));
    }
}
