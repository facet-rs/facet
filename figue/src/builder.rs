//! Builder API for layered configuration.
//!
//! This module provides the [`builder`] function and [`ConfigBuilder`] type for
//! constructing layered configuration parsers. Use this when you need to combine
//! multiple configuration sources (CLI, environment variables, config files).
//!
//! # Overview
//!
//! The builder pattern allows you to:
//! - Configure CLI argument parsing
//! - Set up environment variable parsing with custom prefixes
//! - Load configuration files in various formats
//! - Customize help text and version information
//!
//! # Example
//!
//! ```rust
//! use facet::Facet;
//! use figue::{self as args, builder, Driver};
//!
//! #[derive(Facet, Debug)]
//! struct Args {
//!     #[facet(args::config, args::env_prefix = "MYAPP")]
//!     config: Config,
//! }
//!
//! #[derive(Facet, Debug)]
//! struct Config {
//!     #[facet(default = 8080)]
//!     port: u16,
//!     #[facet(default = "localhost")]
//!     host: String,
//! }
//!
//! // Build the configuration
//! let config = builder::<Args>()
//!     .unwrap()
//!     .cli(|cli| cli.args(["--config.port", "3000"]))
//!     .help(|h| h.program_name("myapp").version("1.0.0"))
//!     .build();
//!
//! // Run the driver to get the parsed value
//! let output = Driver::new(config).run().into_result().unwrap();
//! assert_eq!(output.value.config.port, 3000);
//! ```
//!
//! # Layer Priority
//!
//! When the same field is set in multiple sources, the priority order is:
//! 1. CLI arguments (highest)
//! 2. Environment variables
//! 3. Config files
//! 4. Code defaults (lowest)
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
/// This is the main entry point for building layered configuration. The type `T`
/// must implement [`Facet`] and be properly annotated with figue attributes.
///
/// # Example
///
/// ```rust
/// use facet::Facet;
/// use figue::{self as args, builder, Driver};
///
/// #[derive(Facet)]
/// struct Args {
///     #[facet(args::named, default)]
///     verbose: bool,
///     #[facet(args::positional)]
///     file: String,
/// }
///
/// let config = builder::<Args>()
///     .expect("schema should be valid")
///     .cli(|cli| cli.args(["--verbose", "input.txt"]))
///     .build();
///
/// let args: Args = Driver::new(config).run().unwrap();
/// assert!(args.verbose);
/// ```
///
/// # Errors
///
/// # Errors
///
/// Returns an error if:
/// - The type is not a struct (enums cannot be root types)
/// - Fields are missing required annotations (`args::positional`, `args::named`, etc.)
/// - Schema validation fails
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
///
/// Use the fluent API to configure each layer:
/// - [`.cli()`](Self::cli) - Configure CLI argument parsing
/// - [`.env()`](Self::env) - Configure environment variable parsing
/// - [`.file()`](Self::file) - Configure config file loading
/// - [`.help()`](Self::help) - Configure help text generation
/// - [`.build()`](Self::build) - Finalize and create the config
///
/// # Example
///
/// ```rust
/// use facet::Facet;
/// use figue::{self as args, builder, Driver};
///
/// #[derive(Facet)]
/// struct Args {
///     #[facet(args::config, args::env_prefix = "APP")]
///     config: AppConfig,
/// }
///
/// #[derive(Facet)]
/// struct AppConfig {
///     #[facet(default = 8080)]
///     port: u16,
/// }
///
/// let config = builder::<Args>()
///     .unwrap()
///     .cli(|cli| cli.args(["--config.port", "9000"]))  // CLI takes priority
///     .help(|h| h.program_name("myapp"))
///     .build();
///
/// let output = Driver::new(config).run().into_result().unwrap();
/// assert_eq!(output.value.config.port, 9000);
/// ```
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
    ///
    /// Use this to specify where CLI arguments come from and how they're parsed.
    ///
    /// # Example
    ///
    /// ```rust
    /// use facet::Facet;
    /// use figue::{self as args, builder, Driver};
    ///
    /// #[derive(Facet)]
    /// struct Args {
    ///     #[facet(args::named)]
    ///     verbose: bool,
    /// }
    ///
    /// // Parse specific arguments (useful for testing)
    /// let config = builder::<Args>()
    ///     .unwrap()
    ///     .cli(|cli| cli.args(["--verbose"]))
    ///     .build();
    ///
    /// let args: Args = Driver::new(config).run().unwrap();
    /// assert!(args.verbose);
    /// ```
    ///
    /// For production use, parse from `std::env::args()`:
    ///
    /// ```rust,no_run
    /// # use facet::Facet;
    /// # use figue::{self as args, builder, Driver};
    /// # #[derive(Facet)]
    /// # struct Args { #[facet(args::named)] verbose: bool }
    /// let config = builder::<Args>()
    ///     .unwrap()
    ///     .cli(|cli| cli.args(std::env::args().skip(1)))
    ///     .build();
    /// ```
    pub fn cli<F>(mut self, f: F) -> Self
    where
        F: FnOnce(CliConfigBuilder) -> CliConfigBuilder,
    {
        self.cli_config = Some(f(CliConfigBuilder::new()).build());
        self
    }

    /// Configure help text generation.
    ///
    /// Use this to set the program name, version, and additional description
    /// shown in help output and version output.
    ///
    /// # Example
    ///
    /// ```rust
    /// use facet::Facet;
    /// use figue::{self as args, builder, Driver, DriverError};
    ///
    /// #[derive(Facet)]
    /// struct Args {
    ///     #[facet(args::named, args::help, default)]
    ///     help: bool,
    /// }
    ///
    /// let config = builder::<Args>()
    ///     .unwrap()
    ///     .cli(|cli| cli.args(["--help"]))
    ///     .help(|h| h
    ///         .program_name("myapp")
    ///         .version("1.2.3")
    ///         .description("A helpful description"))
    ///     .build();
    ///
    /// let result = Driver::new(config).run().into_result();
    /// match result {
    ///     Err(DriverError::Help { text }) => {
    ///         assert!(text.contains("myapp"));
    ///     }
    ///     _ => panic!("expected help"),
    /// }
    /// ```
    pub fn help<F>(mut self, f: F) -> Self
    where
        F: FnOnce(HelpConfigBuilder) -> HelpConfigBuilder,
    {
        self.help_config = Some(f(HelpConfigBuilder::new()).build());
        self
    }

    /// Configure environment variable parsing.
    ///
    /// Environment variables are parsed according to the schema's `args::env_prefix`
    /// attribute. For example, with prefix "MYAPP" and a field `port`, the env var
    /// `MYAPP__PORT` will be read.
    ///
    /// # Example
    ///
    /// ```rust
    /// use facet::Facet;
    /// use figue::{self as args, builder, Driver, MockEnv};
    ///
    /// #[derive(Facet)]
    /// struct Args {
    ///     #[facet(args::config, args::env_prefix = "APP")]
    ///     config: Config,
    /// }
    ///
    /// #[derive(Facet)]
    /// struct Config {
    ///     #[facet(default = 8080)]
    ///     port: u16,
    /// }
    ///
    /// // Use MockEnv for testing (to avoid modifying real environment)
    /// let config = builder::<Args>()
    ///     .unwrap()
    ///     .env(|env| env.source(MockEnv::from_pairs([
    ///         ("APP__PORT", "9000"),
    ///     ])))
    ///     .build();
    ///
    /// let output = Driver::new(config).run().into_result().unwrap();
    /// assert_eq!(output.value.config.port, 9000);
    /// ```
    pub fn env<F>(mut self, f: F) -> Self
    where
        F: FnOnce(EnvConfigBuilder) -> EnvConfigBuilder,
    {
        self.env_config = Some(f(EnvConfigBuilder::new()).build());
        self
    }

    /// Configure config file parsing.
    ///
    /// Load configuration from JSON, or other formats via the format registry.
    ///
    /// # Example
    ///
    /// ```rust
    /// use facet::Facet;
    /// use figue::{self as args, builder, Driver};
    ///
    /// #[derive(Facet)]
    /// struct Args {
    ///     #[facet(args::config)]
    ///     config: Config,
    /// }
    ///
    /// #[derive(Facet)]
    /// struct Config {
    ///     #[facet(default = 8080)]
    ///     port: u16,
    /// }
    ///
    /// // Use inline content for testing (avoids file I/O)
    /// let config = builder::<Args>()
    ///     .unwrap()
    ///     .file(|f| f.content(r#"{"port": 9000}"#, "config.json"))
    ///     .build();
    ///
    /// let output = Driver::new(config).run().into_result().unwrap();
    /// assert_eq!(output.value.config.port, 9000);
    /// ```
    pub fn file<F>(mut self, f: F) -> Self
    where
        F: FnOnce(FileConfigBuilder) -> FileConfigBuilder,
    {
        self.file_config = Some(f(FileConfigBuilder::new()).build());
        self
    }

    /// Finalize the builder and return a [`Config`] for use with [`Driver`](crate::Driver).
    ///
    /// After calling this, create a `Driver` and call `run()`:
    ///
    /// ```rust
    /// use facet::Facet;
    /// use figue::{self as args, builder, Driver};
    ///
    /// #[derive(Facet)]
    /// struct Args {
    ///     #[facet(args::positional)]
    ///     file: String,
    /// }
    ///
    /// let config = builder::<Args>()
    ///     .unwrap()
    ///     .cli(|cli| cli.args(["input.txt"]))
    ///     .build();
    ///
    /// let output = Driver::new(config).run().into_result().unwrap();
    /// assert_eq!(output.value.file, "input.txt");
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

/// Builder for help text configuration.
///
/// Configure how help and version information is displayed.
///
/// # Example
///
/// ```rust
/// use facet::Facet;
/// use figue::{self as args, builder, Driver, DriverError};
///
/// #[derive(Facet)]
/// struct Args {
///     #[facet(args::named, args::version, default)]
///     version: bool,
/// }
///
/// let config = builder::<Args>()
///     .unwrap()
///     .cli(|cli| cli.args(["--version"]))
///     .help(|h| h.program_name("myapp").version("1.0.0"))
///     .build();
///
/// let result = Driver::new(config).run().into_result();
/// match result {
///     Err(DriverError::Version { text }) => {
///         assert!(text.contains("myapp 1.0.0"));
///     }
///     _ => panic!("expected version"),
/// }
/// ```
#[derive(Debug, Default)]
pub struct HelpConfigBuilder {
    config: HelpConfig,
}

impl HelpConfigBuilder {
    /// Create a new help config builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the program name shown in help and version output.
    ///
    /// If not set, defaults to the executable name from `std::env::args()`.
    pub fn program_name(mut self, name: impl Into<String>) -> Self {
        self.config.program_name = Some(name.into());
        self
    }

    /// Set the program version shown by `--version`.
    ///
    /// Use `env!("CARGO_PKG_VERSION")` to capture your crate's version:
    ///
    /// ```rust,no_run
    /// # use figue::builder;
    /// # use facet::Facet;
    /// # #[derive(Facet)] struct Args { #[facet(figue::positional)] f: String }
    /// let config = builder::<Args>()
    ///     .unwrap()
    ///     .help(|h| h.version(env!("CARGO_PKG_VERSION")))
    ///     .build();
    /// ```
    ///
    /// If not set, `--version` will display "unknown".
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.config.version = Some(version.into());
        self
    }

    /// Set an additional description shown after the auto-generated help.
    ///
    /// This appears below the program name and doc comment, useful for
    /// additional context or examples.
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.config.description = Some(description.into());
        self
    }

    /// Set the text wrapping width for help output.
    ///
    /// Set to 0 to disable wrapping. Default is 80 columns.
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

/// Builder for config file parsing configuration.
///
/// Configure how configuration files are loaded and parsed.
///
/// # Example
///
/// ```rust
/// use facet::Facet;
/// use figue::{self as args, builder, Driver};
///
/// #[derive(Facet)]
/// struct Args {
///     #[facet(args::config)]
///     config: Config,
/// }
///
/// #[derive(Facet)]
/// struct Config {
///     #[facet(default = "localhost")]
///     host: String,
///     #[facet(default = 8080)]
///     port: u16,
/// }
///
/// // Load from inline JSON (useful for testing)
/// let config = builder::<Args>()
///     .unwrap()
///     .file(|f| f.content(r#"{"host": "0.0.0.0", "port": 3000}"#, "config.json"))
///     .build();
///
/// let output = Driver::new(config).run().into_result().unwrap();
/// assert_eq!(output.value.config.host, "0.0.0.0");
/// assert_eq!(output.value.config.port, 3000);
/// ```
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
    ///
    /// This path takes priority over default paths. Use when the user
    /// specifies a config file via CLI (e.g., `--config path/to/config.json`).
    pub fn path(mut self, path: impl Into<Utf8PathBuf>) -> Self {
        self.config.explicit_path = Some(path.into());
        self
    }

    /// Set default paths to check for config files.
    ///
    /// These are checked in order; the first existing file is used.
    /// Common patterns include `./config.json`, `~/.config/app/config.json`, etc.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use figue::builder;
    /// # use facet::Facet;
    /// # #[derive(Facet)] struct Args { #[facet(figue::config)] config: Config }
    /// # #[derive(Facet)] struct Config { #[facet(default = 0)] port: u16 }
    /// let config = builder::<Args>()
    ///     .unwrap()
    ///     .file(|f| f.default_paths([
    ///         "./config.json",
    ///         "~/.config/myapp/config.json",
    ///         "/etc/myapp/config.json",
    ///     ]))
    ///     .build();
    /// ```
    pub fn default_paths<I, P>(mut self, paths: I) -> Self
    where
        I: IntoIterator<Item = P>,
        P: Into<Utf8PathBuf>,
    {
        self.config.default_paths = paths.into_iter().map(|p| p.into()).collect();
        self
    }

    /// Register an additional config file format.
    ///
    /// By default, JSON is supported. Use this to add TOML, YAML, or custom formats.
    /// See [`ConfigFormat`](crate::ConfigFormat) for implementing custom formats.
    pub fn format<F: ConfigFormat + 'static>(mut self, format: F) -> Self {
        self.config.registry.register(format);
        self
    }

    /// Enable strict mode - error on unknown keys in config file.
    ///
    /// By default, unknown keys are ignored. In strict mode, any key in the
    /// config file that doesn't match a schema field causes an error.
    pub fn strict(mut self) -> Self {
        self.config.strict = true;
        self
    }

    /// Set inline content for testing (avoids disk I/O).
    ///
    /// The filename is used for format detection (e.g., "config.toml" or "settings.json").
    /// This is useful for unit tests that don't want to create actual files.
    ///
    /// # Example
    ///
    /// ```rust
    /// use facet::Facet;
    /// use figue::{self as args, builder, Driver};
    ///
    /// #[derive(Facet)]
    /// struct Args {
    ///     #[facet(args::config)]
    ///     config: Config,
    /// }
    ///
    /// #[derive(Facet)]
    /// struct Config {
    ///     #[facet(default = 8080)]
    ///     port: u16,
    /// }
    ///
    /// let config = builder::<Args>()
    ///     .unwrap()
    ///     .file(|f| f.content(r#"{"port": 9000}"#, "test.json"))
    ///     .build();
    ///
    /// let output = Driver::new(config).run().into_result().unwrap();
    /// assert_eq!(output.value.config.port, 9000);
    /// ```
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

/// Errors that can occur when building configuration.
///
/// These errors happen during the setup phase, before actual parsing begins.
/// They typically indicate problems with the schema definition or file loading.
#[derive(Facet)]
#[repr(u8)]
pub enum BuilderError {
    /// Schema validation failed.
    ///
    /// The type definition has errors, such as missing required attributes
    /// or invalid combinations of attributes.
    SchemaError(#[facet(opaque)] SchemaError),

    /// Memory allocation failed when preparing the destination type.
    Alloc(#[facet(opaque)] ReflectError),

    /// Config file was not found at the specified path.
    FileNotFound {
        /// The path that was checked.
        path: Utf8PathBuf,
    },

    /// Failed to read the config file.
    FileRead(Utf8PathBuf, String),

    /// Failed to parse the config file content.
    FileParse(Utf8PathBuf, ConfigFormatError),

    /// CLI argument parsing failed.
    CliParse(String),

    /// An unknown key was found in configuration.
    ///
    /// Only reported in strict mode.
    UnknownKey {
        /// The unknown key.
        key: String,
        /// Where the key came from (e.g., "config file", "environment").
        source: &'static str,
        /// A suggested correction if the key appears to be a typo.
        suggestion: Option<String>,
    },

    /// A required field was not provided.
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
