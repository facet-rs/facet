use core::marker::PhantomData;

use alloc::string::String;
use alloc::vec::Vec;

use camino::Utf8PathBuf;
use facet::Facet;
use facet_error as error;
use facet_reflect::{Partial, ReflectError};

use crate::{
    config_format::{ConfigFormat, ConfigFormatError, FormatRegistry},
    config_value::ConfigValue,
    env::{EnvConfig, EnvSource, StdEnv, parse_env_with_source},
    provenance::{ConfigResult, FilePathStatus, FileResolution, Provenance},
    schema::{Schema, SchemaError},
};

/// Start configuring an args/config parser for a given type.
///
/// At this stage all you need to pass in is your `Args` type, which must implement [`Facet`].
///
/// This call is fallible because it makes sure that the struct that you pass in, is actually a struct
/// and not an enum, and that all its fields and subfields are actually properly annotated with
/// [`facet(args::positional)`], [`facet(args::named)`], etc. — see [`crate::Attr`]
///
/// This function also already allocates the destination shape, to avoid unsafe code later on.
/// If this allocation fails, then another error is returned.
pub fn builder<T>() -> Result<ConfigBuilder<T>, BuilderError>
where
    T: Facet<'static>,
{
    let schema = Schema::from_shape(T::SHAPE)?;
    let destination = Partial::alloc::<T>().map_err(BuilderError::Alloc)?;
    Ok(ConfigBuilder {
        _phantom: PhantomData,
        partial: destination,
        cli_config: None,
        env_config: None,
        file_config: None,
        env_source: Box::new(StdEnv),
    })
}

/// Builder for layered configuration parsing.
pub struct ConfigBuilder<T> {
    _phantom: PhantomData<T>,
    /// The partially allocated destination structure where parsed values land.
    partial: Partial<'static>,
    /// CLI parsing settings, if the user configured that layer.
    cli_config: Option<CliConfig>,
    /// Environment parsing settings, if provided.
    env_config: Option<EnvConfig>,
    /// File parsing settings for the file layer.
    file_config: Option<FileConfig>,
    /// Source for environment variables (typically `StdEnv`).
    env_source: Box<dyn EnvSource>,
}

impl<T> ConfigBuilder<T> {
    /// Use a custom environment source (for testing).
    pub fn with_env_source(mut self, source: impl EnvSource + 'static) -> Self {
        self.env_source = Box::new(source);
        self
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
    pub fn build_value(self) -> Result<ConfigValue, BuilderError> {
        panic!("build_value is being moved to the driver API")
    }

    /// Build the layered configuration with full provenance tracking.
    ///
    /// Returns a [`ConfigResult`] containing the merged value, provenance map,
    /// and override records.
    pub fn build_traced(self) -> Result<ConfigResult<ConfigValue>, BuilderError> {
        panic!("build_traced is being moved to the driver API")
    }

    /// Load and parse the config file if specified.
    fn load_config_file(
        file_config: &FileConfig,
    ) -> Result<(Option<ConfigValue>, FileResolution), BuilderError> {
        let mut resolution = FileResolution::new();

        // Check if explicit path was provided
        if let Some(ref explicit) = file_config.explicit_path {
            let exists = std::path::Path::new(explicit.as_str()).exists();
            resolution.add_explicit(explicit.clone(), exists);

            if !exists {
                return Err(BuilderError::FileNotFound {
                    path: explicit.clone(),
                    resolution: resolution.clone(),
                });
            }

            // Mark default paths as not tried
            resolution.mark_defaults_not_tried(&file_config.default_paths);

            // Read and parse the explicit file
            let contents = std::fs::read_to_string(explicit.as_str())
                .map_err(|e| BuilderError::FileRead(explicit.clone(), e.to_string()))?;

            let value = file_config
                .registry
                .parse_file(explicit, &contents)
                .map_err(|e| BuilderError::FileParse(explicit.clone(), e))?;

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
            .map_err(|e| BuilderError::FileRead(path.clone(), e.to_string()))?;

        let value = file_config
            .registry
            .parse_file(&path, &contents)
            .map_err(|e| BuilderError::FileParse(path, e))?;

        Ok((Some(value), resolution))
    }

    /// Parse CLI arguments into a ConfigValue tree.
    ///
    /// Handles:
    /// - Long flags: `--version` (bool true), `--name value`
    /// - Short flags: `-v` (bool true), `-n value`
    /// - Dotted paths: `--config.server.port 8080`
    /// - Boolean flags: `--flag` sets to true
    #[deprecated(note = "this has nothing to do in builder and it's a duplicate parser")]
    fn parse_cli_overrides(cli_config: &CliConfig) -> Result<Option<ConfigValue>, BuilderError> {
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
                        return Err(BuilderError::CliParse(format!(
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

use crate::config_value::{insert_nested_value, parse_cli_value};

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
struct FileConfig {
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
    pub fn format<F: ConfigFormat + 'static>(mut self, format: F) -> Self {
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

#[derive(Facet, Debug)]
#[facet(derive(Error))]
#[repr(u8)]
pub enum BuilderError {
    /// The schema provided to BuilderError was invalid.
    SchemaError(#[facet(error::from)] SchemaError),

    /// Allocation failed while constructing the builder.
    Alloc(#[facet(opaque)] ReflectError),

    /// Config file not found at the specified path: {path}
    FileNotFound {
        /// The path that was explicitly requested.
        path: Utf8PathBuf,
        /// File resolution information showing what was tried.
        resolution: FileResolution,
    },

    /// Error reading config file: {0}: {1}
    FileRead(Utf8PathBuf, String),

    /// Error parsing config file: {0}: {1}
    FileParse(Utf8PathBuf, ConfigFormatError),

    /// Error parsing CLI arguments: {0}
    CliParse(String),

    /// Unknown configuration key (in strict mode): {key} (source: {source}, suggestion: {suggestion:?})
    UnknownKey {
        /// The unknown key that was found.
        key: String,
        /// Where the key came from ("env", "file", "cli").
        source: &'static str,
        /// A suggested correction, if one was found.
        suggestion: Option<String>,
    },

    /// Missing required configuration value: {0}
    MissingRequired(String),
}

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
    fn test_builder_file_not_found() {
        let result = builder()
            .file(|f| f.path("/nonexistent/path.json"))
            .build_value();

        assert!(matches!(result, Err(BuilderError::FileNotFound { .. })));
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
