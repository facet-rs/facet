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
use owo_colors::OwoColorize;
use provenance::Provenance;

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

    // Extract --config file path from CLI args if present
    let config_file_path = extract_config_file_path(args);
    if let Some(ref path) = config_file_path {
        tracing::debug!(path, "Found --config file");
    }

    // Build layered config from all sources (CLI args parsed into ConfigValue)
    let mut builder = builder()
        .cli(|cli| cli.args(args.iter().map(|s| s.to_string())))
        .env(|env| env.prefix(env_prefix))
        .with_env_source(StdEnv);

    // Add file layer if specified
    if let Some(path) = config_file_path {
        builder = builder.file(|file| file.path(path));
    }

    let mut config_value = builder.build_value().map_err(|_e| ArgsErrorWithInput {
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

    // Check if --dump-config was requested before deserializing
    if should_dump_config(args) {
        dump_config_with_provenance(&config_value);
        std::process::exit(0);
    }

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

/// Extract the config file path from CLI args if --config is present.
///
/// Looks for `--config <path>` or `--config=<path>` and returns the path.
/// Does not remove it from args (the builder will handle that).
fn extract_config_file_path(args: &[&str]) -> Option<String> {
    let mut i = 0;
    while i < args.len() {
        let arg = args[i];

        // Check for --config=<path>
        if let Some(path) = arg.strip_prefix("--config=") {
            return Some(path.to_string());
        }

        // Check for --config <path>
        if arg == "--config" {
            if i + 1 < args.len() {
                return Some(args[i + 1].to_string());
            }
        }

        i += 1;
    }

    None
}

/// Check if --dump-config flag is present in CLI args.
fn should_dump_config(args: &[&str]) -> bool {
    args.iter()
        .any(|arg| *arg == "--dump-config" || *arg == "--dump_config")
}

/// Dump the ConfigValue tree with provenance information.
fn dump_config_with_provenance(value: &ConfigValue) {
    use std::collections::{HashMap, HashSet};

    // Collect all config file sources
    let mut config_files = HashSet::new();
    collect_config_files(value, &mut config_files);

    // Calculate max widths for alignment at each indent level
    let mut widths: HashMap<usize, (usize, usize)> = HashMap::new();
    calculate_widths(value, "", 0, &mut widths);

    println!("Final Merged Configuration (with provenance)");
    println!("==============================================");
    println!();

    // Show config file sources if any
    if !config_files.is_empty() {
        println!("Config files:");
        for file in &config_files {
            println!("  {}", file);
        }
        println!();
    }

    dump_value_recursive(value, "", 0, &config_files, &widths);

    println!();
    println!("Legend:");
    println!("  {}        Command-line argument", "--flag".cyan());
    println!("  {}          Environment variable", "$VAR".yellow());
    println!("  {}     Config file (line number)", "file:line".magenta());
    println!("  {}       Default value", "DEFAULT".bright_black());
    println!();
}

/// Recursively collect all config file paths from the tree.
fn collect_config_files(value: &ConfigValue, files: &mut std::collections::HashSet<String>) {
    match value {
        ConfigValue::Object(sourced) => {
            if let Some(Provenance::File { file, .. }) = &sourced.provenance {
                files.insert(file.path.to_string());
            }
            for val in sourced.value.values() {
                collect_config_files(val, files);
            }
        }
        ConfigValue::Array(sourced) => {
            if let Some(Provenance::File { file, .. }) = &sourced.provenance {
                files.insert(file.path.to_string());
            }
            for item in &sourced.value {
                collect_config_files(item, files);
            }
        }
        ConfigValue::String(sourced) => {
            if let Some(Provenance::File { file, .. }) = &sourced.provenance {
                files.insert(file.path.to_string());
            }
        }
        ConfigValue::Integer(sourced) => {
            if let Some(Provenance::File { file, .. }) = &sourced.provenance {
                files.insert(file.path.to_string());
            }
        }
        ConfigValue::Float(sourced) => {
            if let Some(Provenance::File { file, .. }) = &sourced.provenance {
                files.insert(file.path.to_string());
            }
        }
        ConfigValue::Bool(sourced) => {
            if let Some(Provenance::File { file, .. }) = &sourced.provenance {
                files.insert(file.path.to_string());
            }
        }
        ConfigValue::Null(sourced) => {
            if let Some(Provenance::File { file, .. }) = &sourced.provenance {
                files.insert(file.path.to_string());
            }
        }
    }
}

/// Calculate maximum key and value widths at each indent level.
fn calculate_widths(
    value: &ConfigValue,
    path: &str,
    indent: usize,
    widths: &mut std::collections::HashMap<usize, (usize, usize)>,
) {
    match value {
        ConfigValue::Object(sourced) => {
            for (key, val) in sourced.value.iter() {
                calculate_widths(val, key, indent + 1, widths);
            }
        }
        ConfigValue::Array(sourced) => {
            for (i, item) in sourced.value.iter().enumerate() {
                calculate_widths(item, &format!("[{}]", i), indent + 1, widths);
            }
        }
        ConfigValue::String(sourced) => {
            let key_len = path.len();
            let val_len = format!("\"{}\"", sourced.value).len();
            let entry = widths.entry(indent).or_insert((0, 0));
            entry.0 = entry.0.max(key_len);
            entry.1 = entry.1.max(val_len);
        }
        ConfigValue::Integer(sourced) => {
            let key_len = path.len();
            let val_len = sourced.value.to_string().len();
            let entry = widths.entry(indent).or_insert((0, 0));
            entry.0 = entry.0.max(key_len);
            entry.1 = entry.1.max(val_len);
        }
        ConfigValue::Float(sourced) => {
            let key_len = path.len();
            let val_len = sourced.value.to_string().len();
            let entry = widths.entry(indent).or_insert((0, 0));
            entry.0 = entry.0.max(key_len);
            entry.1 = entry.1.max(val_len);
        }
        ConfigValue::Bool(sourced) => {
            let key_len = path.len();
            let val_len = sourced.value.to_string().len();
            let entry = widths.entry(indent).or_insert((0, 0));
            entry.0 = entry.0.max(key_len);
            entry.1 = entry.1.max(val_len);
        }
        ConfigValue::Null(_) => {
            let key_len = path.len();
            let val_len = 4; // "null"
            let entry = widths.entry(indent).or_insert((0, 0));
            entry.0 = entry.0.max(key_len);
            entry.1 = entry.1.max(val_len);
        }
    }
}

/// Recursively dump a ConfigValue showing provenance.
fn dump_value_recursive(
    value: &ConfigValue,
    path: &str,
    indent: usize,
    config_files: &std::collections::HashSet<String>,
    widths: &std::collections::HashMap<usize, (usize, usize)>,
) {
    let indent_str = "  ".repeat(indent);
    let (max_key, max_val) = widths.get(&indent).copied().unwrap_or((0, 0));

    match value {
        ConfigValue::Object(sourced) => {
            if !path.is_empty() {
                println!("{}{}", indent_str, path.white());
            }

            for (key, val) in sourced.value.iter() {
                dump_value_recursive(val, key, indent + 1, config_files, widths);
            }
        }
        ConfigValue::Array(sourced) => {
            println!("{}{}", indent_str, path.white());
            for (i, item) in sourced.value.iter().enumerate() {
                dump_value_recursive(item, &format!("[{}]", i), indent + 1, config_files, widths);
            }
        }
        ConfigValue::String(sourced) => {
            let prov = format_provenance(&sourced.provenance, config_files);
            let value_str = format!("\"{}\"", sourced.value);
            println!(
                "{}{:>key_width$}  {:<val_width$}  {}",
                indent_str,
                path.white(),
                value_str.green(),
                prov,
                key_width = max_key,
                val_width = max_val
            );
        }
        ConfigValue::Integer(sourced) => {
            let prov = format_provenance(&sourced.provenance, config_files);
            let value_str = sourced.value.to_string();
            println!(
                "{}{:>key_width$}  {:<val_width$}  {}",
                indent_str,
                path.white(),
                value_str.green(),
                prov,
                key_width = max_key,
                val_width = max_val
            );
        }
        ConfigValue::Float(sourced) => {
            let prov = format_provenance(&sourced.provenance, config_files);
            let value_str = sourced.value.to_string();
            println!(
                "{}{:>key_width$}  {:<val_width$}  {}",
                indent_str,
                path.white(),
                value_str.green(),
                prov,
                key_width = max_key,
                val_width = max_val
            );
        }
        ConfigValue::Bool(sourced) => {
            let prov = format_provenance(&sourced.provenance, config_files);
            let value_str = sourced.value.to_string();
            println!(
                "{}{:>key_width$}  {:<val_width$}  {}",
                indent_str,
                path.white(),
                value_str.green(),
                prov,
                key_width = max_key,
                val_width = max_val
            );
        }
        ConfigValue::Null(sourced) => {
            let prov = format_provenance(&sourced.provenance, config_files);
            println!(
                "{}{:>key_width$}  {:<val_width$}  {}",
                indent_str,
                path.white(),
                "null".green(),
                prov,
                key_width = max_key,
                val_width = max_val
            );
        }
    }
}

/// Format provenance with colors.
fn format_provenance(
    prov: &Option<Provenance>,
    _config_files: &std::collections::HashSet<String>,
) -> String {
    match prov {
        Some(Provenance::Cli { arg, .. }) => format!("{}", arg.cyan()),
        Some(Provenance::Env { var, .. }) => format!("{}", format!("${}", var).yellow()),
        Some(Provenance::File { file, offset, .. }) => {
            // Calculate line number from byte offset
            let line_num = calculate_line_number(&file.contents, *offset);
            // Extract just filename if full path was shown at start
            let filename = std::path::Path::new(file.path.as_str())
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(file.path.as_str());
            format!("{}:{}", filename, line_num).magenta().to_string()
        }
        Some(Provenance::Default) => "DEFAULT".bright_black().to_string(),
        None => "".to_string(),
    }
}

/// Calculate line number (1-based) from byte offset in file contents.
fn calculate_line_number(contents: &str, offset: usize) -> usize {
    if offset == 0 {
        return 1;
    }

    // Count newlines before the offset
    let line_count = contents[..offset.min(contents.len())]
        .chars()
        .filter(|&c| c == '\n')
        .count();

    line_count + 1
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
