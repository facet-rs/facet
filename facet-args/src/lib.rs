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
use config_value::ConfigValue;

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
    let mut config_value = builder()
        .cli(|cli| cli.args(args.iter().map(|s| s.to_string())))
        .env(|env| env.prefix(env_prefix))
        .with_env_source(StdEnv)
        .build_value()
        .map_err(|_e| ArgsErrorWithInput {
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

    // Restructure the ConfigValue to match the Args shape:
    // - Extract top-level fields (like version, verbose) that aren't part of config
    // - Wrap the config-related fields under the config field name (e.g., settings)
    config_value = restructure_config_value(config_value, T::SHAPE, config_field);

    tracing::debug!(?config_value, "Restructured ConfigValue");

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

/// Restructure the ConfigValue to match the target shape.
///
/// The builder produces a flat ConfigValue with all fields at the root level.
/// This function:
/// 1. Separates top-level Args fields (like version, verbose) from config fields
/// 2. Wraps config-related fields under the config field name (e.g., settings)
fn restructure_config_value(
    value: ConfigValue,
    target_shape: &'static facet_core::Shape,
    config_field: &'static facet_core::Field,
) -> ConfigValue {
    use crate::config_value::Sourced;
    use crate::merge::merge;
    use facet_core::{Type, UserType};
    use indexmap::IndexMap;

    // Get all field names from the target shape
    let struct_def = match &target_shape.ty {
        Type::User(UserType::Struct(s)) => s,
        _ => return value, // Not a struct, return as-is
    };

    // Extract the ConfigValue's map
    let source_map = match value {
        ConfigValue::Object(ref sourced) => &sourced.value,
        _ => return value, // Not an object, return as-is
    };

    let mut top_level_map = IndexMap::new();
    let mut config_map = IndexMap::new();

    // Separate top-level fields from config fields, and extract existing config field if present
    let mut existing_config_value: Option<ConfigValue> = None;

    for (key, val) in source_map.iter() {
        if key == config_field.name {
            // The config field is already present at top level
            existing_config_value = Some(val.clone());
        } else {
            // Check if this key corresponds to a top-level field (not the config field)
            let is_top_level_field = struct_def
                .fields
                .iter()
                .any(|f| f.name == key && f.name != config_field.name);

            if is_top_level_field {
                // Top-level field like version, verbose
                top_level_map.insert(key.clone(), val.clone());
            } else {
                // Config-related field - goes under the config field
                config_map.insert(key.clone(), val.clone());
            }
        }
    }

    // Create the config field value, merging if necessary
    let config_value = if !config_map.is_empty() {
        let new_config = ConfigValue::Object(Sourced {
            value: config_map,
            span: None,
            provenance: None,
        });

        // If we already had a config field, deep merge them
        if let Some(existing) = existing_config_value {
            merge(new_config, existing, "").value
        } else {
            new_config
        }
    } else if let Some(existing) = existing_config_value {
        // No new config fields, but we have an existing config field
        existing
    } else {
        // No config fields at all - create empty object
        ConfigValue::Object(Sourced {
            value: IndexMap::new(),
            span: None,
            provenance: None,
        })
    };

    // Insert the final config field
    top_level_map.insert(config_field.name.to_string(), config_value);

    ConfigValue::Object(Sourced {
        value: top_level_map,
        span: None,
        provenance: None,
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
