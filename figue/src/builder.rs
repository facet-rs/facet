//! Builder API for layered configuration.
//!
//! This module is under active development.
#![allow(private_interfaces)]

use std::marker::PhantomData;
use std::string::String;

use camino::Utf8PathBuf;
use facet::Facet;
use facet_reflect::ReflectError;

use crate::{
    config_format::{ConfigFormat, ConfigFormatError},
    help::HelpConfig,
    layers::{
        cli::{CliConfig, CliConfigBuilder},
        env::{EnvConfig, EnvConfigBuilder},
        file::FileConfig,
    },
    schema::{Schema, error::SchemaError},
};

/// Start configuring an args/config parser for a given type.
///
/// At this stage all you need to pass in is your `Args` type, which must implement [`Facet`].
///
/// This call is fallible because it makes sure that the struct that you pass in, is actually a struct
/// and not an enum, and that all its fields and subfields are actually properly annotated with
/// `#[facet(args::positional)]`, `#[facet(args::named)]`, etc. — see [`crate::Attr`]
///
/// This function also already allocates the destination shape, to avoid unsafe code later on.
/// If this allocation fails, then another error is returned.
pub fn builder<T>() -> Result<ConfigBuilder<T>, BuilderError>
where
    T: Facet<'static>,
{
    let schema = Schema::from_shape(T::SHAPE)?;
    Ok(ConfigBuilder {
        _phantom: PhantomData,
        schema,
        cli_config: None,
        help_config: None,
        env_config: None,
        file_config: None,
    })
}

/// Builder for layered configuration parsing.
pub struct ConfigBuilder<T> {
    _phantom: PhantomData<T>,
    /// Parsed schema for the target type.
    schema: Schema,
    /// CLI parsing settings, if the user configured that layer.
    cli_config: Option<CliConfig>,
    /// Help text settings, if provided.
    help_config: Option<HelpConfig>,
    /// Environment parsing settings, if provided.
    env_config: Option<EnvConfig>,
    /// File parsing settings for the file layer.
    file_config: Option<FileConfig>,
}

/// Fully built configuration (schema + sources) for the driver.
pub struct Config<T> {
    /// Parsed schema for the target type.
    pub schema: Schema,
    /// CLI parsing settings, if the user configured that layer.
    pub cli_config: Option<CliConfig>,
    /// Help text settings, if provided.
    pub help_config: Option<HelpConfig>,
    /// Environment parsing settings, if provided.
    pub env_config: Option<EnvConfig>,
    /// File parsing settings for the file layer.
    pub file_config: Option<FileConfig>,
    /// Type marker.
    _phantom: PhantomData<T>,
}

impl<T> ConfigBuilder<T> {
    /// Configure CLI argument parsing.
    pub fn cli<F>(mut self, f: F) -> Self
    where
        F: FnOnce(CliConfigBuilder) -> CliConfigBuilder,
    {
        self.cli_config = Some(f(CliConfigBuilder::new()).build());
        self
    }

    /// Configure help text generation.
    pub fn help<F>(mut self, f: F) -> Self
    where
        F: FnOnce(HelpConfigBuilder) -> HelpConfigBuilder,
    {
        self.help_config = Some(f(HelpConfigBuilder::new()).build());
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

    /// Finalize the builder and return a Config for use with the Driver.
    ///
    /// After calling this, create a `Driver` and call `run()`:
    /// ```ignore
    /// let config = builder::<MyArgs>()?.cli(...).env(...).build();
    /// let output = Driver::new(config).run().unwrap();
    /// ```
    pub fn build(self) -> Config<T> {
        Config {
            schema: self.schema,
            cli_config: self.cli_config,
            help_config: self.help_config,
            env_config: self.env_config,
            file_config: self.file_config,
            _phantom: PhantomData,
        }
    }
}

// ============================================================================
// Help Configuration
// ============================================================================

/// Builder for help configuration.
#[derive(Debug, Default)]
pub struct HelpConfigBuilder {
    config: HelpConfig,
}

impl HelpConfigBuilder {
    /// Create a new help config builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the program name shown in help.
    pub fn program_name(mut self, name: impl Into<String>) -> Self {
        self.config.program_name = Some(name.into());
        self
    }

    /// Set the program version shown by `--version`.
    ///
    /// Use `env!("CARGO_PKG_VERSION")` to capture your crate's version:
    ///
    /// ```rust,ignore
    /// .help(|h| h.version(env!("CARGO_PKG_VERSION")))
    /// ```
    ///
    /// If not set, `--version` will display "unknown".
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.config.version = Some(version.into());
        self
    }

    /// Set an additional description shown after the auto-generated one.
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.config.description = Some(description.into());
        self
    }

    /// Set the text wrapping width (0 = no wrapping).
    pub fn width(mut self, width: usize) -> Self {
        self.config.width = width;
        self
    }

    /// Build the help configuration.
    fn build(self) -> HelpConfig {
        self.config
    }
}

// ============================================================================
// File Configuration Builder
// ============================================================================

/// Builder for file configuration.
#[derive(Default)]
pub struct FileConfigBuilder {
    config: FileConfig,
}

impl FileConfigBuilder {
    /// Create a new file config builder.
    pub fn new() -> Self {
        Self {
            config: FileConfig::default(),
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

    /// Set inline content for testing (avoids disk I/O).
    ///
    /// The filename is used for format detection (e.g., "config.toml" or "settings.json").
    pub fn content(mut self, content: impl Into<String>, filename: impl Into<String>) -> Self {
        self.config.inline_content = Some((content.into(), filename.into()));
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

#[derive(Facet)]
#[repr(u8)]
pub enum BuilderError {
    SchemaError(#[facet(opaque)] SchemaError),
    Alloc(#[facet(opaque)] ReflectError),
    FileNotFound {
        path: Utf8PathBuf,
    },
    FileRead(Utf8PathBuf, String),
    FileParse(Utf8PathBuf, ConfigFormatError),
    CliParse(String),
    UnknownKey {
        key: String,
        source: &'static str,
        suggestion: Option<String>,
    },
    MissingRequired(String),
}

impl std::fmt::Display for BuilderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuilderError::SchemaError(e) => write!(f, "{e}"),
            BuilderError::Alloc(e) => write!(f, "allocation failed: {e}"),
            BuilderError::FileNotFound { path } => {
                write!(f, "config file not found: {path}")
            }
            BuilderError::FileRead(path, msg) => {
                write!(f, "error reading {path}: {msg}")
            }
            BuilderError::FileParse(path, e) => {
                write!(f, "error parsing {path}: {e}")
            }
            BuilderError::CliParse(msg) => write!(f, "{msg}"),
            BuilderError::UnknownKey {
                key,
                source,
                suggestion,
            } => {
                write!(f, "unknown configuration key '{key}' from {source}")?;
                if let Some(suggestion) = suggestion {
                    write!(f, " (did you mean '{suggestion}'?)")?;
                }
                Ok(())
            }
            BuilderError::MissingRequired(field) => {
                write!(f, "missing required configuration: {field}")
            }
        }
    }
}

impl std::fmt::Debug for BuilderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

impl std::error::Error for BuilderError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            BuilderError::SchemaError(e) => Some(e),
            BuilderError::Alloc(e) => Some(e),
            BuilderError::FileParse(_, e) => Some(e),
            _ => None,
        }
    }
}

impl From<SchemaError> for BuilderError {
    fn from(e: SchemaError) -> Self {
        BuilderError::SchemaError(e)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate as args;
    use facet::Facet;

    #[derive(Facet)]
    struct TestConfig {
        #[facet(args::config)]
        config: TestConfigLayer,
    }

    #[derive(Facet)]
    struct TestConfigLayer {
        #[facet(args::named)]
        port: u16,
        #[facet(args::named)]
        host: String,
    }

    #[test]
    fn test_cli_config_builder() {
        let config = CliConfigBuilder::new()
            .args(["--port", "8080"])
            .strict()
            .build();

        assert_eq!(config.args(), &["--port", "8080"]);
        assert!(config.strict());
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
