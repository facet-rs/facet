#![warn(missing_docs)]
#![warn(clippy::std_instead_of_core)]
#![warn(clippy::std_instead_of_alloc)]
#![deny(unsafe_code)]
#![doc = include_str!("../README.md")]

extern crate alloc;

pub mod builder;
pub mod completions;
pub mod config_format;
pub mod config_value;
pub mod config_value_parser;
pub mod env;
mod format;
pub mod help;
pub mod merge;
pub mod provenance;

pub use builder::builder;

pub(crate) mod arg;
pub(crate) mod error;
pub(crate) mod span;

use error::ArgsError;
use facet_core::Facet;

pub use completions::{Shell, generate_completions, generate_completions_for_shape};
pub use error::{ArgsErrorKind, ArgsErrorWithInput};
pub use format::from_slice;
pub use format::from_slice_with_config;
pub use format::from_std_args;
pub use help::{HelpConfig, generate_help, generate_help_for_shape};

/// Parse command line arguments with automatic layered configuration support.
///
/// This function automatically detects fields marked with `#[facet(args::config)]`
/// and uses the layered configuration system to populate them from:
/// - Config files (via --{field_name} <path>)
/// - Environment variables (with optional prefix from args::env_prefix)
/// - CLI overrides (via --{field_name}.foo.bar syntax)
/// - Default values
///
/// If no config field is found, falls back to regular CLI-only parsing.
pub fn from_slice_layered<T: Facet<'static>>(args: &[&str]) -> Result<T, ArgsErrorWithInput> {
    use config_value_parser::from_config_value;
    use env::StdEnv;

    tracing::debug!(
        shape = T::SHAPE.type_identifier,
        "Checking for config field"
    );

    // Check if this type has a config field
    let config_field = find_config_field(T::SHAPE);

    if config_field.is_none() {
        tracing::debug!("No config field found, using regular parsing");
        return format::from_slice(args);
    }

    let config_field = config_field.unwrap();
    tracing::debug!(field = config_field.name, "Found config field");

    // Get env prefix if specified
    let env_prefix = get_env_prefix(config_field).unwrap_or("APP");
    tracing::debug!(env_prefix, "Using env prefix");

    // Build layered config from all sources (CLI args parsed into ConfigValue)
    let config_value = builder()
        .cli(|cli| cli.args(args.iter().map(|s| s.to_string())))
        .env(|env| env.prefix(env_prefix))
        .with_env_source(StdEnv)
        .build_value()
        .map_err(|e| ArgsErrorWithInput {
            inner: ArgsError::new(
                ArgsErrorKind::ReflectError(facet_reflect::ReflectError::OperationFailed {
                    shape: T::SHAPE,
                    operation: "Failed to build layered config",
                }),
                crate::span::Span::new(0, 0),
            ),
            flattened_args: args.join(" "),
        })?;

    tracing::debug!(?config_value, "Built merged ConfigValue");

    // Deserialize the merged ConfigValue into the target type
    from_config_value(&config_value).map_err(|e| {
        tracing::error!(?e, "Failed to deserialize config");
        ArgsErrorWithInput {
            inner: ArgsError::new(
                ArgsErrorKind::ReflectError(facet_reflect::ReflectError::OperationFailed {
                    shape: T::SHAPE,
                    operation: "Failed to deserialize config",
                }),
                crate::span::Span::new(0, 0),
            ),
            flattened_args: args.join(" "),
        }
    })
}

// Args extension attributes for use with #[facet(args::attr)] syntax.
//
// After importing `use facet_args as args;`, users can write:
//   #[facet(args::positional)]
//   #[facet(args::short = 'v')]
//   #[facet(args::named)]

// Generate args attribute grammar using the grammar DSL.
// This generates:
// - `Attr` enum with all args attribute variants
// - `__attr!` macro that dispatches to attribute handlers and returns ExtensionAttr
// - `__parse_attr!` macro for parsing (internal use)
facet::define_attr_grammar! {
    ns "args";
    crate_path ::facet_args;

    /// Args attribute types for field configuration.
    pub enum Attr {
        /// Marks a field as a positional argument.
        ///
        /// Usage: `#[facet(args::positional)]`
        Positional,
        /// Marks a field as a named argument.
        ///
        /// Usage: `#[facet(args::named)]`
        Named,
        /// Specifies a short flag character for the field.
        ///
        /// Usage: `#[facet(args::short = 'v')]` or just `#[facet(args::short)]`
        Short(Option<char>),
        /// Marks a field as a subcommand.
        ///
        /// The field type must be an enum where each variant represents a subcommand.
        /// Variant names are converted to kebab-case for matching.
        ///
        /// Usage: `#[facet(args::subcommand)]`
        Subcommand,
        /// Marks a field as a counted flag.
        ///
        /// Each occurrence of the flag increments the count. Works with both short
        /// flags (`-vvv` or `-v -v -v`) and long flags (`--verbose --verbose`).
        /// The field type must be an integer type (u8, u16, u32, u64, usize, i8, i16, i32, i64, isize).
        /// Uses saturating arithmetic to avoid overflow.
        ///
        /// Usage: `#[facet(args::named, args::short = 'v', args::counted)]`
        Counted,
        /// Marks a field as a layered configuration field.
        ///
        /// The field will be populated from merged configuration sources (CLI overrides,
        /// environment variables, config files) in priority order: CLI > env > file > default.
        ///
        /// This automatically generates:
        /// - `--{field_name} <PATH>` flag to specify config file path
        /// - `--{field_name}.foo.bar <VALUE>` style CLI overrides
        /// - Environment variable parsing
        /// - Config file loading with multiple format support
        ///
        /// Usage: `#[facet(args::config)]`
        Config,
        /// Specifies the environment variable prefix for a config field.
        ///
        /// Must be used together with `#[facet(args::config)]`.
        ///
        /// Usage: `#[facet(args::env_prefix = "MYAPP")]`
        ///
        /// Example: `env_prefix = "MYAPP"` results in `MYAPP__FIELD__NAME` env vars.
        EnvPrefix(Option<&'static str>),
    }
}

/// Check if a field is marked with `args::counted`.
pub fn is_counted_field(field: &facet_core::Field) -> bool {
    field.has_attr(Some("args"), "counted")
}

/// Check if a shape is a supported type for counted fields (integer types).
pub const fn is_supported_counted_type(shape: &'static facet_core::Shape) -> bool {
    use facet_core::{NumericType, PrimitiveType, Type};
    matches!(
        shape.ty,
        Type::Primitive(PrimitiveType::Numeric(NumericType::Integer { .. }))
    )
}

/// Check if a field is marked with `args::config`.
pub fn is_config_field(field: &facet_core::Field) -> bool {
    field.has_attr(Some("args"), "config")
}

/// Find the config field in a struct shape, if any.
pub fn find_config_field(shape: &'static facet_core::Shape) -> Option<&'static facet_core::Field> {
    use facet_core::{Type, UserType};

    match &shape.ty {
        Type::User(UserType::Struct(s)) => s.fields.iter().find(|field| is_config_field(field)),
        _ => None,
    }
}

/// Get the env_prefix value from a field's attributes.
pub fn get_env_prefix(field: &facet_core::Field) -> Option<&'static str> {
    let attr = field.get_attr(Some("args"), "env_prefix")?;
    let parsed = attr.get_as::<crate::Attr>()?;

    if let Attr::EnvPrefix(prefix_opt) = parsed {
        *prefix_opt
    } else {
        None
    }
}
