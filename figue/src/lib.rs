#![warn(missing_docs)]
#![deny(unsafe_code)]
// Allow deprecated during transition to new driver-based API
//! # figue - Layered Configuration for Rust
//!
//! figue provides type-safe, layered configuration parsing with support for:
//! - **CLI arguments** - Standard command-line argument parsing
//! - **Environment variables** - Configure apps via environment
//! - **Config files** - JSON, and more formats via plugins
//! - **Defaults from code** - Compile-time defaults
//!
//! Built on [facet](https://docs.rs/facet) reflection, figue uses derive macros
//! to generate parsers at compile time with zero runtime reflection overhead.
//!
//! ## Quick Start
//!
//! For simple CLI-only parsing, use [`from_slice`] or [`from_std_args`]:
//!
//! ```rust
//! use facet::Facet;
//! use figue::{self as args, FigueBuiltins};
//!
//! #[derive(Facet, Debug)]
//! struct Args {
//!     /// Enable verbose output
//!     #[facet(args::named, args::short = 'v', default)]
//!     verbose: bool,
//!
//!     /// Input file to process
//!     #[facet(args::positional)]
//!     input: String,
//!
//!     /// Standard CLI options (--help, --version, --completions)
//!     #[facet(flatten)]
//!     builtins: FigueBuiltins,
//! }
//!
//! // Parse from a slice (useful for testing)
//! let args: Args = figue::from_slice(&["--verbose", "input.txt"]).unwrap();
//! assert!(args.verbose);
//! assert_eq!(args.input, "input.txt");
//! ```
//!
//! ## Layered Configuration
//!
//! For applications that need config files and environment variables, use the
//! [`builder`] API with [`Driver`]:
//!
//! ```rust
//! use facet::Facet;
//! use figue::{self as args, builder, Driver};
//!
//! #[derive(Facet, Debug)]
//! struct Args {
//!     /// Application configuration
//!     #[facet(args::config, args::env_prefix = "MYAPP")]
//!     config: AppConfig,
//! }
//!
//! #[derive(Facet, Debug)]
//! struct AppConfig {
//!     /// Server port
//!     #[facet(default = 8080)]
//!     port: u16,
//!
//!     /// Server host
//!     #[facet(default = "localhost")]
//!     host: String,
//! }
//!
//! // Build layered configuration
//! let config = builder::<Args>()
//!     .unwrap()
//!     .cli(|cli| cli.args(["--config.port", "3000"]))
//!     .build();
//!
//! let output = Driver::new(config).run().into_result().unwrap();
//! assert_eq!(output.value.config.port, 3000);
//! assert_eq!(output.value.config.host, "localhost"); // from default
//! ```
//!
//! ## Subcommands
//!
//! figue supports subcommands via enum types:
//!
//! ```rust
//! use facet::Facet;
//! use figue::{self as args, FigueBuiltins};
//!
//! #[derive(Facet, Debug)]
//! struct Cli {
//!     #[facet(args::subcommand)]
//!     command: Command,
//!
//!     #[facet(flatten)]
//!     builtins: FigueBuiltins,
//! }
//!
//! #[derive(Facet, Debug)]
//! #[repr(u8)]
//! enum Command {
//!     /// Build the project
//!     Build {
//!         /// Build in release mode
//!         #[facet(args::named, args::short = 'r')]
//!         release: bool,
//!     },
//!     /// Run the project
//!     Run {
//!         /// Arguments to pass through
//!         #[facet(args::positional)]
//!         args: Vec<String>,
//!     },
//! }
//!
//! let cli: Cli = figue::from_slice(&["build", "--release"]).unwrap();
//! match cli.command {
//!     Command::Build { release } => assert!(release),
//!     Command::Run { .. } => unreachable!(),
//! }
//! ```
//!
//! ## Attribute Reference
//!
//! figue uses `#[facet(...)]` attributes to configure parsing behavior:
//!
//! | Attribute | Description |
//! |-----------|-------------|
//! | `args::positional` | Mark field as positional argument |
//! | `args::named` | Mark field as named flag (--flag) |
//! | `args::short = 'x'` | Add short flag (-x) |
//! | `args::counted` | Count occurrences (-vvv = 3) |
//! | `args::subcommand` | Mark field as subcommand selector |
//! | `args::config` | Mark field as layered config struct |
//! | `args::env_prefix = "X"` | Set env var prefix for config |
//! | `args::help` | Mark as help flag (exits with code 0) |
//! | `args::version` | Mark as version flag (exits with code 0) |
//! | `args::completions` | Mark as shell completions flag |
//! | `flatten` | Flatten nested struct fields |
//! | `default` / `default = x` | Provide default value |
//! | `rename = "x"` | Rename field in CLI/config |
//! | `sensitive` | Redact field in debug output |
//!
//! ## Entry Points
//!
//! - [`from_std_args`] - Parse from `std::env::args()` (CLI-only)
//! - [`from_slice`] - Parse from a string slice (CLI-only, good for testing)
//! - [`builder`] - Start building layered configuration (CLI + env + files)
//!
//! For most CLI applications, start with [`FigueBuiltins`] flattened into your
//! args struct to get `--help`, `--version`, and `--completions` for free.

extern crate self as figue;

// Re-export attribute macros from figue-attrs.
// This allows users to write `#[facet(figue::named)]` or `use figue as args; #[facet(args::named)]`
pub use figue_attrs::*;

// Alias for internal use - allows `#[facet(args::named)]` syntax
use figue_attrs as args;

#[macro_use]
mod macros;

pub(crate) mod builder;
pub(crate) mod color;
pub(crate) mod completions;
pub(crate) mod config_format;
pub(crate) mod config_value;
pub(crate) mod config_value_parser;
pub(crate) mod diagnostics;
pub(crate) mod driver;
pub(crate) mod dump;
pub(crate) mod enum_conflicts;
pub(crate) mod env_subst;
pub(crate) mod error;
pub(crate) mod extract;
pub(crate) mod help;
pub(crate) mod layers;
pub(crate) mod merge;
pub(crate) mod missing;
pub(crate) mod path;
pub(crate) mod provenance;
pub(crate) mod reflection;
pub(crate) mod schema;
pub(crate) mod span;
pub(crate) mod span_registry;
pub(crate) mod suggest;
pub(crate) mod value_builder;

use facet_core::Facet;

// ==========================================
// PUBLIC INTERFACE
// ==========================================

pub use crate::completions::Shell;
pub use builder::builder;
pub use config_format::{ConfigFormat, ConfigFormatError, JsonFormat};
pub use config_value::ConfigValue;
pub use driver::{Driver, DriverError, DriverOutcome, DriverOutput, DriverReport};
pub use error::{ArgsErrorKind, ArgsErrorWithInput};
pub use extract::{ExtractError, ExtractMissingField};
pub use help::{HelpConfig, generate_help, generate_help_for_shape};
pub use layers::env::MockEnv;
pub use layers::file::FormatRegistry;

/// Parse command-line arguments from `std::env::args()`.
///
/// This is a convenience function for CLI-only parsing (no env vars, no config files).
/// For layered configuration, use [`builder`] instead.
///
/// Returns a [`DriverOutcome`] which handles `--help`, `--version`, and errors gracefully.
/// Use `.unwrap()` for automatic exit handling, or `.into_result()` for manual control.
///
/// # Example
///
/// ```rust,no_run
/// use facet::Facet;
/// use figue::{self as args, FigueBuiltins};
///
/// #[derive(Facet)]
/// struct Args {
///     #[facet(args::positional)]
///     input: String,
///
///     #[facet(flatten)]
///     builtins: FigueBuiltins,
/// }
///
/// let args: Args = figue::from_std_args().unwrap();
/// println!("Processing: {}", args.input);
/// ```
pub fn from_std_args<T: Facet<'static>>() -> DriverOutcome<T> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    from_slice(&args_ref)
}

/// Parse command-line arguments from a slice.
///
/// This is a convenience function for CLI-only parsing (no env vars, no config files).
/// For layered configuration, use [`builder`] instead.
///
/// This function is particularly useful for testing, as you can provide arguments
/// directly without modifying `std::env::args()`.
///
/// # Example
///
/// ```rust
/// use facet::Facet;
/// use figue::{self as args, FigueBuiltins};
///
/// #[derive(Facet, Debug)]
/// struct Args {
///     /// Enable verbose mode
///     #[facet(args::named, args::short = 'v', default)]
///     verbose: bool,
///
///     /// Input file
///     #[facet(args::positional)]
///     input: String,
///
///     #[facet(flatten)]
///     builtins: FigueBuiltins,
/// }
///
/// // Parse with long flag
/// let args: Args = figue::from_slice(&["--verbose", "file.txt"]).unwrap();
/// assert!(args.verbose);
/// assert_eq!(args.input, "file.txt");
///
/// // Parse with short flag
/// let args: Args = figue::from_slice(&["-v", "file.txt"]).unwrap();
/// assert!(args.verbose);
///
/// // Parse without optional flag
/// let args: Args = figue::from_slice(&["file.txt"]).unwrap();
/// assert!(!args.verbose);
/// ```
///
/// # Errors
///
/// Returns an error (via [`DriverOutcome`]) if:
/// - Required arguments are missing
/// - Unknown flags are provided
/// - Type conversion fails (e.g., "abc" for a number)
/// - `--help`, `--version`, or `--completions` is requested (success exit)
pub fn from_slice<T: Facet<'static>>(args: &[&str]) -> DriverOutcome<T> {
    use crate::driver::{Driver, DriverError, DriverOutcome};

    let config = match builder::<T>() {
        Ok(b) => b
            .cli(|cli| cli.args(args.iter().map(|s| s.to_string())))
            .build(),
        Err(e) => return DriverOutcome::err(DriverError::Builder { error: e }),
    };

    Driver::new(config).run()
}

/// Standard CLI builtins that can be flattened into your Args struct.
///
/// This provides the standard `--help`, `--version`, and `--completions` flags
/// that most CLI applications need. Flatten it into your Args struct:
///
/// ```rust
/// use figue::{self as args, FigueBuiltins};
/// use facet::Facet;
///
/// #[derive(Facet, Debug)]
/// struct Args {
///     /// Your actual arguments
///     #[facet(args::positional)]
///     input: String,
///
///     /// Standard CLI options
///     #[facet(flatten)]
///     builtins: FigueBuiltins,
/// }
///
/// // The builtins are automatically available
/// let args: Args = figue::from_slice(&["myfile.txt"]).unwrap();
/// assert_eq!(args.input, "myfile.txt");
/// assert!(!args.builtins.help);
/// assert!(!args.builtins.version);
/// ```
///
/// The driver automatically handles these fields:
/// - `--help` / `-h`: Shows help and exits with code 0
/// - `--version` / `-V`: Shows version and exits with code 0
/// - `--completions <SHELL>`: Generates shell completions and exits with code 0
///
/// # Setting the Version
///
/// By default, `--version` displays "unknown" because figue cannot automatically
/// capture your crate's version at compile time. To display your crate's version,
/// configure it via the builder:
///
/// ```rust,no_run
/// use figue::{self as args, builder, Driver, FigueBuiltins};
/// use facet::Facet;
///
/// #[derive(Facet)]
/// struct Args {
///     #[facet(args::positional)]
///     input: String,
///
///     #[facet(flatten)]
///     builtins: FigueBuiltins,
/// }
///
/// let config = figue::builder::<Args>()
///     .unwrap()
///     .cli(|cli| cli.args(std::env::args().skip(1)))
///     .help(|h| h
///         .program_name(env!("CARGO_PKG_NAME"))
///         .version(env!("CARGO_PKG_VERSION")))
///     .build();
///
/// let args: Args = figue::Driver::new(config).run().unwrap();
/// // use args...
/// ```
///
/// The `env!("CARGO_PKG_VERSION")` macro is evaluated at *your* crate's compile time,
/// capturing the correct version from your `Cargo.toml`.
///
/// # Handling Help and Version Manually
///
/// If you need to handle these cases yourself (e.g., for custom formatting),
/// use `into_result()` instead of `unwrap()`:
///
/// ```rust
/// use figue::{self as args, FigueBuiltins, DriverError};
/// use facet::Facet;
///
/// #[derive(Facet)]
/// struct Args {
///     #[facet(args::positional, default)]
///     input: Option<String>,
///
///     #[facet(flatten)]
///     builtins: FigueBuiltins,
/// }
///
/// let result = figue::from_slice::<Args>(&["--help"]).into_result();
/// match result {
///     Err(DriverError::Help { text }) => {
///         assert!(text.contains("--help"));
///     }
///     _ => panic!("expected help"),
/// }
/// ```
#[derive(facet::Facet, Default, Debug)]
pub struct FigueBuiltins {
    /// Show help message and exit.
    #[facet(args::named, args::short = 'h', args::help, default)]
    pub help: bool,

    /// Show version and exit.
    #[facet(args::named, args::short = 'V', args::version, default)]
    pub version: bool,

    /// Generate shell completions.
    #[facet(args::named, args::completions, default)]
    pub completions: Option<Shell>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::help::generate_help;
    use crate::schema::Schema;

    #[derive(facet::Facet)]
    struct ArgsWithBuiltins {
        /// Input file
        #[facet(args::positional)]
        input: String,

        /// Standard options
        #[facet(flatten)]
        builtins: FigueBuiltins,
    }

    #[test]
    fn test_figue_builtins_flatten_in_schema() {
        let schema = Schema::from_shape(ArgsWithBuiltins::SHAPE);
        assert!(schema.is_ok(), "Schema should build: {:?}", schema.err());
    }

    #[test]
    fn test_figue_builtins_in_help() {
        let help = generate_help::<ArgsWithBuiltins>(&HelpConfig::default());
        assert!(help.contains("--help"), "help should contain --help");
        assert!(help.contains("-h"), "help should contain -h");
        assert!(help.contains("--version"), "help should contain --version");
        assert!(help.contains("-V"), "help should contain -V");
        assert!(
            help.contains("--completions"),
            "help should contain --completions"
        );
        assert!(
            help.contains("<bash,zsh,fish>"),
            "help should show enum variants for --completions: {}",
            help
        );
    }

    #[test]
    fn test_figue_builtins_special_fields_detected() {
        let schema = Schema::from_shape(ArgsWithBuiltins::SHAPE).unwrap();
        let special = schema.special();

        // With flatten, fields appear at top level - path is just ["help"]
        assert!(special.help.is_some(), "help should be detected");
        assert_eq!(special.help.as_ref().unwrap(), &vec!["help".to_string()]);

        // Version at top level
        assert!(special.version.is_some(), "version should be detected");
        assert_eq!(
            special.version.as_ref().unwrap(),
            &vec!["version".to_string()]
        );

        // Completions at top level
        assert!(
            special.completions.is_some(),
            "completions should be detected"
        );
        assert_eq!(
            special.completions.as_ref().unwrap(),
            &vec!["completions".to_string()]
        );
    }

    // ========================================================================
    // Tests: Special fields with custom names and nesting
    // ========================================================================

    /// Special fields can be renamed - detection works via attribute, not field name
    #[derive(facet::Facet)]
    struct ArgsWithRenamedHelp {
        /// Print documentation and exit
        #[facet(args::named, args::help, rename = "print-docs")]
        show_help: bool,

        /// Show program version
        #[facet(args::named, args::version, rename = "show-version")]
        show_ver: bool,
    }

    #[test]
    fn test_special_fields_renamed() {
        let schema = Schema::from_shape(ArgsWithRenamedHelp::SHAPE).unwrap();
        let special = schema.special();

        // Detection is by ATTRIBUTE (crate::help), not field name.
        // The path uses the EFFECTIVE name (after rename).
        assert!(
            special.help.is_some(),
            "help should be detected via attribute"
        );
        assert_eq!(
            special.help.as_ref().unwrap(),
            &vec!["print-docs".to_string()],
            "path should use effective name"
        );

        assert!(
            special.version.is_some(),
            "version should be detected via attribute"
        );
        assert_eq!(
            special.version.as_ref().unwrap(),
            &vec!["show-version".to_string()],
            "path should use effective name"
        );
    }

    /// Deeply nested special fields (flatten inside flatten)
    #[derive(facet::Facet)]
    struct DeepInner {
        #[facet(args::named, args::help, default)]
        help: bool,
    }

    #[derive(facet::Facet)]
    struct DeepMiddle {
        #[facet(flatten)]
        inner: DeepInner,
    }

    #[derive(facet::Facet)]
    struct ArgsWithDeepFlatten {
        #[facet(args::positional)]
        input: String,

        #[facet(flatten)]
        middle: DeepMiddle,
    }

    #[test]
    fn test_special_fields_deeply_flattened() {
        let schema = Schema::from_shape(ArgsWithDeepFlatten::SHAPE).unwrap();
        let special = schema.special();

        // With flatten, all fields bubble up to top level - path is just ["help"]
        assert!(
            special.help.is_some(),
            "help should be detected in deeply flattened struct"
        );
        assert_eq!(
            special.help.as_ref().unwrap(),
            &vec!["help".to_string()],
            "flattened fields appear at top level"
        );
    }
}
