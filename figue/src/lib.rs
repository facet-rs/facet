#![warn(missing_docs)]
#![deny(unsafe_code)]
// Allow deprecated during transition to new driver-based API
#![doc = include_str!("../README.md")]

extern crate self as figue;

// Re-export attribute macros from figue-attrs.
// This allows users to write `#[facet(figue::named)]` or `use figue as args; #[facet(args::named)]`
pub use figue_attrs::*;

// Alias for internal use - allows `#[facet(args::named)]` syntax
use figue_attrs as args;

#[macro_use]
mod macros;

pub(crate) mod builder;
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

pub use builder::builder;
pub use completions::{Shell, generate_completions, generate_completions_for_shape};
#[cfg(feature = "styx")]
pub use config_format::StyxFormat;
pub use config_format::{ConfigFormat, ConfigFormatError, JsonFormat};
pub use config_value::ConfigValue;
pub use driver::{Driver, DriverError, DriverOutcome, DriverOutput, DriverReport};
pub use error::{ArgsErrorKind, ArgsErrorWithInput};
pub use help::{HelpConfig, generate_help, generate_help_for_shape};
pub use layers::env::MockEnv;
pub use layers::file::FormatRegistry;

/// Parse command-line arguments from `std::env::args()`.
///
/// This is a convenience function for CLI-only parsing (no env vars, no config files).
/// For layered configuration, use `builder()` instead.
///
/// # Example
///
/// ```ignore
/// fn main() {
///     let args = figue::from_std_args::<Args>().unwrap();
///     // use args...
/// }
/// ```
pub fn from_std_args<T: Facet<'static>>() -> driver::DriverOutcome<T> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    from_slice(&args_ref)
}

/// Parse command-line arguments from a slice.
///
/// This is a convenience function for CLI-only parsing (no env vars, no config files).
/// For layered configuration, use `builder()` instead.
///
/// # Example
///
/// ```ignore
/// fn main() {
///     let args = figue::from_slice::<Args>(&["--verbose", "input.txt"]).unwrap();
///     // use args...
/// }
/// ```
pub fn from_slice<T: Facet<'static>>(args: &[&str]) -> driver::DriverOutcome<T> {
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
/// ```rust,ignore
/// use figue::{self as args, FigueBuiltins};
/// use facet::Facet;
///
/// #[derive(Facet)]
/// struct Args {
///     /// Your actual arguments
///     #[facet(args::positional)]
///     input: String,
///
///     /// Standard CLI options
///     #[facet(flatten)]
///     builtins: FigueBuiltins,
/// }
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
/// ```rust,ignore
/// fn main() {
///     let args = figue::builder::<Args>()
///         .unwrap()
///         .cli(|cli| cli.args(std::env::args().skip(1)))
///         .help(|h| h
///             .program_name(env!("CARGO_PKG_NAME"))
///             .version(env!("CARGO_PKG_VERSION")))
///         .build();
///
///     let args = figue::Driver::new(args).run().unwrap();
///     // use args...
/// }
/// ```
///
/// The `env!("CARGO_PKG_VERSION")` macro is evaluated at *your* crate's compile time,
/// capturing the correct version from your `Cargo.toml`.
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
        let help = generate_help::<ArgsWithBuiltins>(&crate::help::HelpConfig::default());
        assert!(help.contains("--help"), "help should contain --help");
        assert!(help.contains("-h"), "help should contain -h");
        assert!(help.contains("--version"), "help should contain --version");
        assert!(help.contains("-V"), "help should contain -V");
        assert!(
            help.contains("--completions"),
            "help should contain --completions"
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
