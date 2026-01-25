//! Builder API for layered configuration parsing.
//!
//! This module provides the main entry point for parsing layered configuration
//! from CLI arguments, environment variables, and config files.
//!
//! # Example
//!
//! ```rust,ignore
//! use facet_args::builder;
//!
//! let args: Args = builder()
//!     .cli(|cli| cli.args(std::env::args_os().skip(1)))
//!     .env(|env| env.prefix("REEF"))
//!     .file(|file| file.format(JsonFormat))
//!     .build()?;
//! ```

use alloc::string::String;
use alloc::vec::Vec;

use camino::Utf8PathBuf;

use crate::config_format::{ConfigFormatError, FormatRegistry};
use crate::config_value::ConfigValue;
use crate::env::{EnvConfig, EnvSource, StdEnv, parse_env_with_source};
use crate::merge::merge_layers;
use crate::provenance::{ConfigResult, FilePathStatus, FileResolution, Override, Provenance};

/// Create a new layered configuration builder.
pub fn builder() -> ConfigBuilder {
    ConfigBuilder::new()
}

/// Builder for layered configuration parsing.
pub struct ConfigBuilder<E: EnvSource = StdEnv> {
    cli_config: Option<CliConfig>,
    env_config: Option<EnvConfig>,
    file_config: Option<FileConfig>,
    env_source: E,
}

impl Default for ConfigBuilder<StdEnv> {
    fn default() -> Self {
        Self {
            cli_config: None,
            env_config: None,
            file_config: None,
            env_source: StdEnv,
        }
    }
}

impl ConfigBuilder<StdEnv> {
    /// Create a new builder with default settings.
    pub fn new() -> Self {
        Self::default()
    }
}

impl<E: EnvSource> ConfigBuilder<E> {
    /// Use a custom environment source (for testing).
    pub fn with_env_source<E2: EnvSource>(self, source: E2) -> ConfigBuilder<E2> {
        ConfigBuilder {
            cli_config: self.cli_config,
            env_config: self.env_config,
            file_config: self.file_config,
            env_source: source,
        }
    }

    /// Configure CLI argument parsing.
    pub fn cli<F>(mut self, f: F) -> Self
    where
        F: FnOnce(CliConfigBuilder) -> CliConfigBuilder,
    {
        self.cli_config = Some(f(CliConfigBuilder::new()).build());
        self
    }

    /// Configure environment variable parsing.
    pub fn env<F>(mut self, f: F) -> Self
    where
        F: FnOnce(EnvConfigBuilder) -> EnvConfigBuilder,
    {
        self.env_config = Some(f(EnvConfigBuilder::new()).build());
        self
    }

    /// Configure config file parsing.
    pub fn file<F>(mut self, f: F) -> Self
    where
        F: FnOnce(FileConfigBuilder) -> FileConfigBuilder,
    {
        self.file_config = Some(f(FileConfigBuilder::new()).build());
        self
    }

    /// Build the layered configuration, returning just the merged ConfigValue.
    ///
    /// This parses all configured layers and merges them in priority order:
    /// defaults < file < env < cli
    pub fn build_value(self) -> Result<ConfigValue, LayeredConfigError> {
        let result = self.build_traced()?;
        Ok(result.value)
    }

    /// Build the layered configuration with full provenance tracking.
    ///
    /// Returns a [`ConfigResult`] containing the merged value, provenance map,
    /// and override records.
    pub fn build_traced(self) -> Result<ConfigResult<ConfigValue>, LayeredConfigError> {
        let mut layers: Vec<ConfigValue> = Vec::new();
        let mut all_overrides: Vec<Override> = Vec::new();
        let mut file_resolution = FileResolution::new();

        // Layer 1: Config file (lowest priority after defaults)
        if let Some(ref file_config) = self.file_config {
            let (value_opt, resolution) = Self::load_config_file(file_config)?;
            file_resolution = resolution;
            if let Some(value) = value_opt {
                layers.push(value);
            }
        }

        // Layer 2: Environment variables
        if let Some(ref env_config) = self.env_config {
            let env_result = parse_env_with_source(env_config, &self.env_source);
            layers.push(env_result.value);
        }

        // Layer 3: CLI overrides (highest priority)
        if let Some(ref cli_config) = self.cli_config
            && let Some(value) = Self::parse_cli_overrides(cli_config)?
        {
            layers.push(value);
        }

        // Merge all layers
        let merge_result = merge_layers(layers);
        all_overrides.extend(merge_result.overrides);

        // Build provenance map by walking the merged tree
        let provenance = collect_provenance(&merge_result.value, "");

        Ok(ConfigResult::with_full_tracking(
            merge_result.value,
            provenance,
            all_overrides,
            file_resolution,
        ))
    }

    /// Load and parse the config file if specified.
    fn load_config_file(
        file_config: &FileConfig,
    ) -> Result<(Option<ConfigValue>, FileResolution), LayeredConfigError> {
        let mut resolution = FileResolution::new();

        // Check if explicit path was provided
        if let Some(ref explicit) = file_config.explicit_path {
            let exists = std::path::Path::new(explicit.as_str()).exists();
            resolution.add_explicit(explicit.clone(), exists);

            if !exists {
                return Err(LayeredConfigError::FileNotFound {
                    path: explicit.clone(),
                    resolution: resolution.clone(),
                });
            }

            // Mark default paths as not tried
            resolution.mark_defaults_not_tried(&file_config.default_paths);

            // Read and parse the explicit file
            let contents = std::fs::read_to_string(explicit.as_str())
                .map_err(|e| LayeredConfigError::FileRead(explicit.clone(), e.to_string()))?;

            let value = file_config
                .registry
                .parse_file(explicit, &contents)
                .map_err(|e| LayeredConfigError::FileParse(explicit.clone(), e))?;

            return Ok((Some(value), resolution));
        }

        // No explicit path, try defaults in order
        let mut found_path: Option<Utf8PathBuf> = None;

        for path in &file_config.default_paths {
            let exists = std::path::Path::new(path.as_str()).exists();

            if exists && found_path.is_none() {
                // This is the first one that exists - pick it
                resolution.add_default(path.clone(), FilePathStatus::Picked);
                found_path = Some(path.clone());
            } else {
                // Either doesn't exist, or we already found one
                let status = if exists {
                    FilePathStatus::NotTried // Exists but we picked an earlier one
                } else {
                    FilePathStatus::Absent
                };
                resolution.add_default(path.clone(), status);
            }
        }

        let Some(path) = found_path else {
            return Ok((None, resolution));
        };

        // Read and parse the picked file
        let contents = std::fs::read_to_string(path.as_str())
            .map_err(|e| LayeredConfigError::FileRead(path.clone(), e.to_string()))?;

        let value = file_config
            .registry
            .parse_file(&path, &contents)
            .map_err(|e| LayeredConfigError::FileParse(path, e))?;

        Ok((Some(value), resolution))
    }

    /// Parse CLI arguments into a ConfigValue tree.
    ///
    /// Handles:
    /// - Long flags: `--version` (bool true), `--name value`
    /// - Short flags: `-v` (bool true), `-n value`
    /// - Dotted paths: `--config.server.port 8080`
    /// - Boolean flags: `--flag` sets to true
    fn parse_cli_overrides(
        cli_config: &CliConfig,
    ) -> Result<Option<ConfigValue>, LayeredConfigError> {
        use crate::config_value::Sourced;
        use heck::ToSnakeCase;
        use indexmap::IndexMap;

        let mut root = IndexMap::default();
        let mut i = 0;

        while i < cli_config.args.len() {
            let arg = &cli_config.args[i];

            if let Some(flag) = arg.strip_prefix("--") {
                if flag.is_empty() {
                    // "--" separator, skip rest
                    break;
                }

                // Check for dotted path (e.g., --settings.server.port)
                if flag.contains('.') {
                    let parts: Vec<&str> = flag.split('.').collect();
                    // Get the value from the next argument
                    i += 1;
                    if i >= cli_config.args.len() {
                        return Err(LayeredConfigError::CliParse(format!(
                            "Missing value for --{}",
                            flag
                        )));
                    }
                    let value_str = &cli_config.args[i];
                    let arg_name = format!("--{}", flag);
                    let value = parse_cli_value(value_str, &arg_name);
                    insert_nested_value(&mut root, &parts, value);
                } else {
                    // Simple flag like --version or --name value
                    let key = flag.to_snake_case();

                    // Check if next arg looks like a value (not another flag)
                    let has_value =
                        i + 1 < cli_config.args.len() && !cli_config.args[i + 1].starts_with('-');

                    if has_value {
                        i += 1;
                        let arg_name = format!("--{}", flag);
                        let value = parse_cli_value(&cli_config.args[i], &arg_name);
                        root.insert(key, value);
                    } else {
                        // Boolean flag, set to true
                        let arg_name = format!("--{}", flag);
                        root.insert(
                            key,
                            ConfigValue::Bool(Sourced {
                                value: true,
                                span: None,
                                provenance: Some(Provenance::cli(arg_name, "true")),
                            }),
                        );
                    }
                }
            } else if let Some(flag) = arg.strip_prefix('-') {
                if flag.is_empty() {
                    // Bare "-" (stdin), treat as positional, skip for now
                    i += 1;
                    continue;
                }

                // Short flags like -v or -n value
                // For now, treat single char as boolean flag
                // TODO: Handle -vvv counting, -abc chaining
                for ch in flag.chars() {
                    let key = ch.to_string();
                    let arg_name = format!("-{}", ch);
                    root.insert(
                        key,
                        ConfigValue::Bool(Sourced {
                            value: true,
                            span: None,
                            provenance: Some(Provenance::cli(arg_name, "true")),
                        }),
                    );
                }
            } else {
                // Positional argument - skip for now
                // TODO: Handle positional args
            }

            i += 1;
        }

        if root.is_empty() {
            Ok(None)
        } else {
            Ok(Some(ConfigValue::Object(Sourced::new(root))))
        }
    }
}

/// Collect provenance from all values in a ConfigValue tree.
/// Parse a CLI value string and infer its type.
fn parse_cli_value(s: &str, arg_name: &str) -> ConfigValue {
    use crate::config_value::Sourced;

    let prov = Some(Provenance::cli(arg_name, s));

    // Try to parse as different types
    // 1. Boolean
    if s == "true" {
        return ConfigValue::Bool(Sourced {
            value: true,
            span: None,
            provenance: prov,
        });
    }
    if s == "false" {
        return ConfigValue::Bool(Sourced {
            value: false,
            span: None,
            provenance: prov,
        });
    }

    // 2. Integer
    if let Ok(i) = s.parse::<i64>() {
        return ConfigValue::Integer(Sourced {
            value: i,
            span: None,
            provenance: prov,
        });
    }

    // 3. Float
    if let Ok(f) = s.parse::<f64>() {
        return ConfigValue::Float(Sourced {
            value: f,
            span: None,
            provenance: prov,
        });
    }

    // 4. Default to string
    ConfigValue::String(Sourced {
        value: s.to_string(),
        span: None,
        provenance: prov,
    })
}

/// Insert a value into a nested map structure using a dotted path.
fn insert_nested_value(
    root: &mut indexmap::IndexMap<String, ConfigValue, std::hash::RandomState>,
    parts: &[&str],
    value: ConfigValue,
) {
    use crate::config_value::Sourced;
    use alloc::string::ToString;
    use indexmap::IndexMap;

    if parts.is_empty() {
        return;
    }

    if parts.len() == 1 {
        // Base case: insert the value
        root.insert(parts[0].to_string(), value);
    } else {
        // Recursive case: ensure intermediate object exists
        let key = parts[0].to_string();
        let entry = root
            .entry(key)
            .or_insert_with(|| ConfigValue::Object(Sourced::new(IndexMap::default())));

        // If it's already an object, recurse into it
        if let ConfigValue::Object(obj) = entry {
            insert_nested_value(&mut obj.value, &parts[1..], value);
        }
        // If it's not an object, we have a conflict - replace it with an object
        else {
            let mut new_map = IndexMap::default();
            insert_nested_value(&mut new_map, &parts[1..], value);
            *entry = ConfigValue::Object(Sourced::new(new_map));
        }
    }
}

fn collect_provenance(
    value: &ConfigValue,
    path: &str,
) -> indexmap::IndexMap<String, Provenance, std::hash::RandomState> {
    let mut map = indexmap::IndexMap::default();
    collect_provenance_inner(value, path, &mut map);
    map
}

fn collect_provenance_inner(
    value: &ConfigValue,
    path: &str,
    map: &mut indexmap::IndexMap<String, Provenance, std::hash::RandomState>,
) {
    let prov = match value {
        ConfigValue::Null(s) => s.provenance.as_ref(),
        ConfigValue::Bool(s) => s.provenance.as_ref(),
        ConfigValue::Integer(s) => s.provenance.as_ref(),
        ConfigValue::Float(s) => s.provenance.as_ref(),
        ConfigValue::String(s) => s.provenance.as_ref(),
        ConfigValue::Array(s) => s.provenance.as_ref(),
        ConfigValue::Object(s) => s.provenance.as_ref(),
        ConfigValue::Missing(_) => None, // Missing values have no provenance
    };

    if let Some(prov) = prov
        && !path.is_empty()
    {
        map.insert(path.to_string(), prov.clone());
    }

    // Recurse into children
    match value {
        ConfigValue::Array(arr) => {
            for (i, item) in arr.value.iter().enumerate() {
                let item_path = if path.is_empty() {
                    format!("{i}")
                } else {
                    format!("{path}[{i}]")
                };
                collect_provenance_inner(item, &item_path, map);
            }
        }
        ConfigValue::Object(obj) => {
            for (key, val) in &obj.value {
                let key_path = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                collect_provenance_inner(val, &key_path, map);
            }
        }
        _ => {}
    }
}

// ============================================================================
// CLI Configuration
// ============================================================================

/// Configuration for CLI argument parsing.
#[derive(Debug, Clone, Default)]
pub struct CliConfig {
    /// Raw CLI arguments.
    args: Vec<String>,
    /// Whether to error on unknown arguments.
    strict: bool,
}

/// Builder for CLI configuration.
#[derive(Debug, Default)]
pub struct CliConfigBuilder {
    config: CliConfig,
}

impl CliConfigBuilder {
    /// Create a new CLI config builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the CLI arguments to parse.
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.config.args = args.into_iter().map(|s| s.into()).collect();
        self
    }

    /// Set CLI arguments from OsString iterator (e.g., std::env::args_os()).
    pub fn args_os<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        self.config.args = args
            .into_iter()
            .filter_map(|s| s.as_ref().to_str().map(|s| s.to_string()))
            .collect();
        self
    }

    /// Enable strict mode - error on unknown arguments.
    pub fn strict(mut self) -> Self {
        self.config.strict = true;
        self
    }

    /// Build the CLI configuration.
    fn build(self) -> CliConfig {
        self.config
    }
}

// ============================================================================
// Environment Configuration
// ============================================================================

/// Builder for environment variable configuration.
#[derive(Debug, Default)]
pub struct EnvConfigBuilder {
    prefix: String,
    strict: bool,
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

    /// Build the env configuration.
    fn build(self) -> EnvConfig {
        let mut config = EnvConfig::new(self.prefix);
        if self.strict {
            config = config.strict();
        }
        config
    }
}

// ============================================================================
// File Configuration
// ============================================================================

/// Configuration for config file parsing.
#[derive(Default)]
pub struct FileConfig {
    /// Explicit path provided via CLI (e.g., --config path.json).
    explicit_path: Option<Utf8PathBuf>,
    /// Default paths to check if no explicit path is provided.
    default_paths: Vec<Utf8PathBuf>,
    /// Format registry for parsing different file types.
    registry: FormatRegistry,
    /// Whether to error on unknown keys in the config file.
    strict: bool,
}

/// Builder for file configuration.
#[derive(Default)]
pub struct FileConfigBuilder {
    config: FileConfig,
}

impl FileConfigBuilder {
    /// Create a new file config builder.
    pub fn new() -> Self {
        Self {
            config: FileConfig {
                registry: FormatRegistry::with_defaults(),
                ..Default::default()
            },
        }
    }

    /// Set an explicit config file path.
    pub fn path(mut self, path: impl Into<Utf8PathBuf>) -> Self {
        self.config.explicit_path = Some(path.into());
        self
    }

    /// Set default paths to check for config files.
    ///
    /// These are checked in order; the first existing file is used.
    pub fn default_paths<I, P>(mut self, paths: I) -> Self
    where
        I: IntoIterator<Item = P>,
        P: Into<Utf8PathBuf>,
    {
        self.config.default_paths = paths.into_iter().map(|p| p.into()).collect();
        self
    }

    /// Register an additional config file format.
    pub fn format<F: crate::config_format::ConfigFormat + 'static>(mut self, format: F) -> Self {
        self.config.registry.register(format);
        self
    }

    /// Enable strict mode - error on unknown keys in config file.
    pub fn strict(mut self) -> Self {
        self.config.strict = true;
        self
    }

    /// Build the file configuration.
    fn build(self) -> FileConfig {
        self.config
    }
}

// ============================================================================
// Errors
// ============================================================================

/// Errors that can occur during layered config parsing.
#[derive(Debug)]
pub enum LayeredConfigError {
    /// Config file not found at the specified path.
    FileNotFound {
        /// The path that was explicitly requested.
        path: Utf8PathBuf,
        /// File resolution information showing what was tried.
        resolution: FileResolution,
    },
    /// Error reading config file.
    FileRead(Utf8PathBuf, String),
    /// Error parsing config file.
    FileParse(Utf8PathBuf, ConfigFormatError),
    /// Error parsing CLI arguments.
    CliParse(String),
    /// Unknown configuration key (in strict mode).
    UnknownKey {
        /// The unknown key that was found.
        key: String,
        /// Where the key came from ("env", "file", "cli").
        source: &'static str,
        /// A suggested correction, if one was found.
        suggestion: Option<String>,
    },
    /// Missing required configuration value.
    MissingRequired(String),
}

impl core::fmt::Display for LayeredConfigError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::FileNotFound { path, resolution } => {
                writeln!(f, "config file not found: {path}")?;
                writeln!(f)?;
                writeln!(f, "File resolution:")?;

                if resolution.had_explicit {
                    writeln!(f, "  Explicit --config flag was used")?;
                } else {
                    writeln!(f, "  No --config flag provided, checked default paths:")?;
                }

                for path_info in &resolution.paths {
                    let status_str = match path_info.status {
                        FilePathStatus::Picked => "(picked)",
                        FilePathStatus::NotTried => "(not tried)",
                        FilePathStatus::Absent => "(absent)",
                    };
                    let explicit_str = if path_info.explicit {
                        " [via --config]"
                    } else {
                        ""
                    };
                    writeln!(f, "    {} {}{}", status_str, path_info.path, explicit_str)?;
                }

                Ok(())
            }
            Self::FileRead(path, err) => write!(f, "error reading {path}: {err}"),
            Self::FileParse(path, err) => write!(f, "error parsing {path}: {err}"),
            Self::CliParse(msg) => write!(f, "CLI parse error: {msg}"),
            Self::UnknownKey {
                key,
                source,
                suggestion,
            } => {
                write!(f, "unknown {source} key: {key}")?;
                if let Some(sug) = suggestion {
                    write!(f, " (did you mean {sug}?)")?;
                }
                Ok(())
            }
            Self::MissingRequired(key) => write!(f, "missing required configuration: {key}"),
        }
    }
}

impl core::error::Error for LayeredConfigError {}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_builder_empty() {
        let result = builder().build_value();
        assert!(result.is_ok());

        if let Ok(ConfigValue::Object(obj)) = result {
            assert!(obj.value.is_empty());
        } else {
            panic!("expected empty object");
        }
    }

    #[test]
    fn test_builder_env_only() {
        use crate::env::MockEnv;

        let env = MockEnv::from_pairs([
            ("TEST_BUILDER__PORT", "8080"),
            ("TEST_BUILDER__HOST", "localhost"),
        ]);

        let result = builder()
            .with_env_source(env)
            .env(|env| env.prefix("TEST_BUILDER"))
            .build_traced()
            .expect("should build");

        if let ConfigValue::Object(obj) = &result.value {
            assert!(obj.value.contains_key("port"));
            assert!(obj.value.contains_key("host"));

            if let Some(ConfigValue::String(port)) = obj.value.get("port") {
                assert_eq!(port.value, "8080");
            }
        } else {
            panic!("expected object");
        }

        // Check provenance was collected
        assert!(result.provenance.contains_key("port"));
        assert!(result.provenance.contains_key("host"));
    }

    #[test]
    fn test_builder_file_only() {
        // Create a temp config file
        let mut file = NamedTempFile::with_suffix(".json").unwrap();
        writeln!(file, r#"{{"port": 9000, "host": "filehost"}}"#).unwrap();
        let path = Utf8PathBuf::from_path_buf(file.path().to_path_buf()).unwrap();

        let result = builder()
            .file(|f| f.path(path))
            .build_traced()
            .expect("should build");

        if let ConfigValue::Object(obj) = &result.value {
            if let Some(ConfigValue::Integer(port)) = obj.value.get("port") {
                assert_eq!(port.value, 9000);
            }
            if let Some(ConfigValue::String(host)) = obj.value.get("host") {
                assert_eq!(host.value, "filehost");
            }
        } else {
            panic!("expected object");
        }
    }

    #[test]
    fn test_builder_file_and_env_merge() {
        use crate::env::MockEnv;

        // Create a temp config file
        let mut file = NamedTempFile::with_suffix(".json").unwrap();
        writeln!(file, r#"{{"port": 9000, "host": "filehost"}}"#).unwrap();
        let path = Utf8PathBuf::from_path_buf(file.path().to_path_buf()).unwrap();

        // Mock env var to override port
        let env = MockEnv::from_pairs([("TEST_MERGE__PORT", "8080")]);

        let result = builder()
            .with_env_source(env)
            .file(|f| f.path(path))
            .env(|e| e.prefix("TEST_MERGE"))
            .build_traced()
            .expect("should build");

        if let ConfigValue::Object(obj) = &result.value {
            // Port should be from env (higher priority)
            if let Some(ConfigValue::String(port)) = obj.value.get("port") {
                assert_eq!(port.value, "8080");
            }
            // Host should be from file (not in env)
            if let Some(ConfigValue::String(host)) = obj.value.get("host") {
                assert_eq!(host.value, "filehost");
            }
        } else {
            panic!("expected object");
        }

        // Should have override recorded
        assert!(!result.overrides.is_empty());
    }

    #[test]
    fn test_builder_file_not_found() {
        let result = builder()
            .file(|f| f.path("/nonexistent/path.json"))
            .build_value();

        assert!(matches!(
            result,
            Err(LayeredConfigError::FileNotFound { .. })
        ));
    }

    #[test]
    fn test_builder_default_paths() {
        // Create a temp config file
        let mut file = NamedTempFile::with_suffix(".json").unwrap();
        writeln!(file, r#"{{"found": true}}"#).unwrap();
        let path = Utf8PathBuf::from_path_buf(file.path().to_path_buf()).unwrap();

        let result = builder()
            .file(|f| {
                f.default_paths([
                    "/nonexistent/first.json",
                    path.as_str(),
                    "/nonexistent/last.json",
                ])
            })
            .build_value()
            .expect("should find file");

        if let ConfigValue::Object(obj) = result {
            assert!(obj.value.contains_key("found"));
        }
    }

    #[test]
    fn test_cli_config_builder() {
        let config = CliConfigBuilder::new()
            .args(["--port", "8080"])
            .strict()
            .build();

        assert_eq!(config.args, vec!["--port", "8080"]);
        assert!(config.strict);
    }

    #[test]
    fn test_env_config_builder() {
        let config = EnvConfigBuilder::new().prefix("MYAPP").strict().build();

        assert_eq!(config.prefix, "MYAPP");
        assert!(config.strict);
    }

    #[test]
    fn test_file_config_builder() {
        let config = FileConfigBuilder::new()
            .path("config.json")
            .default_paths(["./config.json", "~/.config/app.json"])
            .strict()
            .build();

        assert_eq!(config.explicit_path, Some(Utf8PathBuf::from("config.json")));
        assert_eq!(config.default_paths.len(), 2);
        assert!(config.strict);
    }
}
