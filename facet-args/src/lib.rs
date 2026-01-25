#![warn(missing_docs)]
#![warn(clippy::std_instead_of_core)]
#![warn(clippy::std_instead_of_alloc)]
#![deny(unsafe_code)]
#![allow(macro_expanded_macro_exports_accessed_by_absolute_paths)]
// Allow deprecated during transition to new driver-based API
#![allow(deprecated)]
#![doc = include_str!("../README.md")]

extern crate alloc;
extern crate self as facet_args;

pub(crate) mod arg;
pub(crate) mod builder;
pub(crate) mod completions;
pub(crate) mod config_format;
pub(crate) mod config_value;
pub(crate) mod config_value_parser;
pub(crate) mod diagnostics;
pub(crate) mod driver;
pub(crate) mod dump;
pub(crate) mod env;
pub(crate) mod env2;
pub(crate) mod error;
pub(crate) mod help;
pub(crate) mod merge;
pub(crate) mod parser;
pub(crate) mod parser2;
pub(crate) mod path;
pub(crate) mod provenance;
pub(crate) mod reflection;
pub(crate) mod schema;
pub(crate) mod span;

use crate::{
    dump::{collect_missing_values, dump_config_with_missing_fields, dump_config_with_provenance},
    reflection::{find_config_field, get_env_prefix},
};
use config_value::{ConfigValue, Sourced};
use error::ArgsError;
use facet_core::Facet;
use owo_colors::OwoColorize;

// ==========================================
// PUBLIC INTERFACE
// ==========================================

pub use builder::builder;
pub use completions::{Shell, generate_completions, generate_completions_for_shape};
pub use error::{ArgsErrorKind, ArgsErrorWithInput};
pub use help::{HelpConfig, generate_help, generate_help_for_shape};
pub use parser::{from_slice_with_config, from_std_args};

/// Parse command-line arguments into a Facet type.
#[deprecated(note = "Use builder() instead; this entry point will be removed.")]
pub fn from_slice<T: Facet<'static>>(
    _args: &[&str],
) -> Result<T, crate::error::ArgsErrorWithInput> {
    panic!("from_slice is deprecated; use builder() instead")
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

/// Result of parsing with provenance and file resolution tracking.
pub struct ParseResult<T> {
    /// The parsed value.
    pub value: T,
    /// File resolution information (which paths were tried, which was picked).
    pub file_resolution: provenance::FileResolution,
    /// Configuration value tree (for dumping).
    config_value: ConfigValue,
}

impl<T: Facet<'static>> ParseResult<T> {
    /// Dump the configuration with provenance information.
    pub fn dump(&self) {
        dump_config_with_provenance::<T>(&self.config_value, &self.file_resolution);
    }
}

/// Parse command line arguments with automatic layered configuration support.
///
/// This function automatically detects fields marked with `#[facet(args::config)]`
/// and uses the layered configuration system to populate them from:
/// - Config files (via --{field_name} \<path\>)
/// - Environment variables (with optional prefix from args::env_prefix)
/// - CLI overrides (via --{field_name}.foo.bar syntax)
/// - Default values
///
/// Returns a `ParseResult` which includes the parsed value and methods for
/// dumping configuration with provenance tracking.
///
/// If no config field is found, falls back to regular CLI-only parsing.
pub fn from_slice_layered<T: Facet<'static>>(
    args: &[&str],
) -> Result<ParseResult<T>, ArgsErrorWithInput> {
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
        let value = parser::from_slice(args)?;
        return Ok(ParseResult {
            value,
            file_resolution: provenance::FileResolution::new(),
            config_value: ConfigValue::Object(Sourced::new(indexmap::IndexMap::default())),
        });
    }

    let config_field = config_field.unwrap();
    tracing::debug!(field = config_field.name, "Found config field");

    // Get env prefix if specified
    let env_prefix = get_env_prefix(config_field).unwrap_or("APP");
    tracing::debug!(env_prefix, "Using env prefix");

    // Extract config file path from CLI args if present (using field name)
    let config_flag = format!("--{}", config_field.name);
    let config_file_path = extract_config_file_path(args, &config_flag);
    if let Some(ref path) = config_file_path {
        tracing::debug!(path = %path, field = config_field.name, "Found config file path");
    }

    // Filter out the config file flag and its value from CLI args
    // These are handled separately via the file builder
    let config_flag = format!("--{}", config_field.name);
    let filtered_args: Vec<String> = args
        .iter()
        .enumerate()
        .filter_map(|(i, &arg)| {
            // Skip the --config flag
            if arg == config_flag {
                return None;
            }
            // Skip the value after --config flag
            if i > 0 && args[i - 1] == config_flag {
                return None;
            }
            // Skip --config=value format
            if arg.starts_with(&format!("{}=", config_flag)) {
                return None;
            }
            Some(arg.to_string())
        })
        .collect();

    // Build layered config from all sources (CLI args parsed into ConfigValue)
    let mut builder = builder::<T>()
        .unwrap()
        .cli(|cli| cli.args(filtered_args))
        .env(|env| env.prefix(env_prefix))
        .with_env_source(StdEnv);

    // Add file layer if specified
    if let Some(path) = config_file_path {
        builder = builder.file(|file| file.format(config_format::JsonFormat).path(path));
    }

    let config_result = builder.build_traced().map_err(|e| {
        // For FileNotFound errors, the error message already includes resolution info
        eprintln!("Error: {}", e);
        ArgsErrorWithInput {
            inner: ArgsError::new(
                ArgsErrorKind::ReflectError(facet_reflect::ReflectError::OperationFailed {
                    shape: T::SHAPE,
                    operation: "Failed to build layered config",
                }),
                crate::span::Span::new(0, 0),
            ),
            flattened_args: args.join(" "),
        }
    })?;

    let mut config_value = config_result.value;
    let file_resolution = config_result.file_resolution;

    tracing::debug!(?config_value, "Built merged ConfigValue");

    // Restructure the ConfigValue to match the Args shape:
    // - Extract top-level fields (like version, verbose) that aren't part of config
    // - Wrap the config-related fields under the config field name (e.g., settings)
    config_value = restructure_config_value(config_value, T::SHAPE, config_field);

    tracing::debug!(?config_value, "Restructured ConfigValue");

    // Fill in defaults before checking for missing fields
    let config_value_with_defaults =
        config_value_parser::fill_defaults_from_shape(&config_value, T::SHAPE);

    // Keep a copy for dumping
    let config_value_for_dump = config_value_with_defaults.clone();

    // Check for missing required fields after defaults are filled
    let missing_fields = find_missing_required_fields(&config_value_with_defaults, T::SHAPE);

    if !missing_fields.is_empty() {
        // 1. Show error first
        eprintln!();
        eprintln!(
            "❌ Missing {} required field(s)",
            missing_fields.len().to_string().red().bold()
        );
        eprintln!();

        // 2. Dump config with missing field markers
        dump_config_with_missing_fields::<T>(
            &config_value_with_defaults,
            &file_resolution,
            &missing_fields,
            env_prefix,
        );

        // 3. Show actionable info (how to set each missing field)
        for field_info in &missing_fields {
            // Show key with first line of doc comment on same line
            if let Some(doc) = &field_info.doc_comment {
                let first_line = doc.lines().next().unwrap_or("").trim();
                if !first_line.is_empty() {
                    eprintln!(
                        "  • {} {}",
                        field_info.field_path.bold().red(),
                        format!("/// {}", first_line).dimmed()
                    );
                } else {
                    eprintln!("  • {}", field_info.field_path.bold().red());
                }
            } else {
                eprintln!("  • {}", field_info.field_path.bold().red());
            }

            eprintln!(
                "    Set via CLI: {}=...",
                format!("--{}", field_info.field_path).cyan()
            );
            let env_var = format!(
                "{}__{}",
                env_prefix,
                field_info.field_path.replace('.', "__").to_uppercase()
            );
            eprintln!("    Or via environment: export {}=...", env_var.yellow());
            eprintln!();
        }

        // 4. Remind error
        eprintln!(
            "❌ Missing {} required field(s)",
            missing_fields.len().to_string().red().bold()
        );
        eprintln!();

        // 5. Exit
        // FIXME: never exit - return an error
        std::process::exit(1);
    }

    // Deserialize the merged ConfigValue into the target type
    let value = match from_config_value(&config_value_with_defaults) {
        Ok(v) => v,
        Err(e) => {
            // 1. Show error first
            eprintln!();
            eprintln!("❌ Failed to deserialize configuration");
            eprintln!();
            eprintln!("  {}", format!("{:?}", e).dimmed());
            eprintln!();

            // 2. Dump config
            dump_config_with_provenance::<T>(&config_value_with_defaults, &file_resolution);
            eprintln!();

            // 3. Remind error
            eprintln!("❌ Failed to deserialize configuration");
            eprintln!();

            // 4. Exit
            // FIXME: do NOT exit - return an error
            std::process::exit(1);
        }
    };

    Ok(ParseResult {
        value,
        file_resolution,
        config_value: config_value_for_dump,
    })
}

/// Restructure the ConfigValue to match the target shape.
///
/// The builder produces a flat ConfigValue with all fields at the root level.
/// This function:
/// 1. Separates top-level Args fields (like version, verbose) from config fields
/// 2. Wraps config-related fields under the config field name (e.g., settings)
///
///
#[deprecated(
    note = "FIXME: this shouldn't be needed, the parser should simply respect dotted paths to begin with."
)]
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

    let mut top_level_map = IndexMap::default();
    let mut config_map = IndexMap::default();

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
            value: IndexMap::default(),
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

/// Extract the config file path from CLI args if the config flag is present.
///
/// Looks for `--{field_name} <path>` or `--{field_name}=<path>` and returns the path.
/// Does not remove it from args (the builder will handle that).
///
/// FIXME:
#[deprecated(
    note = "this should be gotten from the ConfigValue object, not parsed from CLI args manually"
)]
fn extract_config_file_path(args: &[&str], flag: &str) -> Option<String> {
    let flag_with_eq = format!("{}=", flag);

    let mut i = 0;
    while i < args.len() {
        let arg = args[i];

        // Check for --flag=<path>
        if let Some(path) = arg.strip_prefix(&flag_with_eq) {
            return Some(path.to_string());
        }

        // Check for --flag <path>
        if arg == flag && i + 1 < args.len() {
            return Some(args[i + 1].to_string());
        }

        i += 1;
    }

    None
}

/// Find all required fields that are missing from the config value.
///
/// This is called AFTER defaults have been filled in by fill_defaults_from_shape.
/// So if a field is truly missing at this point, it means:
/// - No default was provided
/// - Not an Option type
/// - No value from CLI/env/file
fn find_missing_required_fields(
    value: &ConfigValue,
    _shape: &'static facet_core::Shape,
) -> Vec<crate::config_value::MissingFieldInfo> {
    let mut missing = Vec::new();
    collect_missing_values(value, &mut missing);
    missing
}
