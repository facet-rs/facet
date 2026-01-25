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
use config_value::{ConfigValue, Sourced};
use owo_colors::OwoColorize;
use provenance::Provenance;
use unicode_width::UnicodeWidthStr;

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
        let value = format::from_slice(args)?;
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
    let mut builder = builder()
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

fn collect_missing_values(
    value: &ConfigValue,
    missing: &mut Vec<crate::config_value::MissingFieldInfo>,
) {
    match value {
        ConfigValue::Missing(info) => {
            missing.push(info.clone());
        }
        ConfigValue::Object(sourced) => {
            for (_key, val) in &sourced.value {
                collect_missing_values(val, missing);
            }
        }
        ConfigValue::Array(sourced) => {
            for val in &sourced.value {
                collect_missing_values(val, missing);
            }
        }
        _ => {}
    }
}

/// Dump config with special markers for missing required fields.
fn dump_config_with_missing_fields<T: Facet<'static>>(
    value: &ConfigValue,
    file_resolution: &provenance::FileResolution,
    _missing_fields: &[crate::config_value::MissingFieldInfo],
    _env_prefix: &str,
) {
    // Just show the normal dump - it already has header and sources
    dump_config_with_provenance::<T>(value, file_resolution);
}

/// A line to be printed in the config dump.
#[derive(Debug)]
struct DumpLine {
    indent: usize,
    key: String,
    value: String,
    provenance: String,
    is_header: bool,
}

/// Context for dumping configuration with provenance.
#[derive(Debug)]
struct DumpContext {
    config_field_name: &'static str,
    env_prefix: Option<&'static str>,
    max_string_length: usize,
    max_value_width: usize, // Max width for value column before wrapping
}

/// State tracking for the dump operation.
struct DumpState {
    had_truncation: bool,
}

impl DumpState {
    fn new() -> Self {
        Self {
            had_truncation: false,
        }
    }
}

impl DumpContext {
    /// Extract dump context from the shape.
    fn from_shape<T: Facet<'static>>() -> Self {
        let config_field = find_config_field(T::SHAPE);
        let (config_field_name, env_prefix) = if let Some(field) = config_field {
            (field.name, get_env_prefix(field))
        } else {
            ("settings", None)
        };

        // Check for FACET_ARGS_BLAST_IT env var to disable truncation
        let blast_it = std::env::var("FACET_ARGS_BLAST_IT")
            .map(|v| v == "1" || v.to_lowercase() == "true")
            .unwrap_or(false);

        Self {
            config_field_name,
            env_prefix,
            max_string_length: if blast_it { usize::MAX } else { 50 },
            max_value_width: 50, // Maximum width for value column
        }
    }
}

/// Dump the ConfigValue tree with provenance information from a ConfigResult.
///
/// This is useful when using the builder API directly and want to show
/// the config dump with file resolution info.
pub fn dump_config_from_result<T: Facet<'static>>(result: &provenance::ConfigResult<ConfigValue>) {
    dump_config_with_provenance::<T>(&result.value, &result.file_resolution);
}

/// Dump the ConfigValue tree with provenance information.
fn dump_config_with_provenance<T: Facet<'static>>(
    value: &ConfigValue,
    file_resolution: &provenance::FileResolution,
) {
    use provenance::FilePathStatus;
    use std::collections::HashMap;

    // Extract context from shape
    let ctx = DumpContext::from_shape::<T>();

    // Show sources
    println!("Sources:");

    // Config files - show resolution info with alignment
    if !file_resolution.paths.is_empty() {
        println!("  file:");

        // Find max path length for alignment
        let max_path_len = file_resolution
            .paths
            .iter()
            .map(|p| p.path.as_str().len())
            .max()
            .unwrap_or(0);

        for path_info in &file_resolution.paths {
            let status_label = match path_info.status {
                FilePathStatus::Picked => "  (picked)",
                FilePathStatus::NotTried => "(not tried)",
                FilePathStatus::Absent => "  (absent)",
            };

            // Calculate dots needed for alignment
            let path_str = path_info.path.as_str();
            let dots_needed = max_path_len.saturating_sub(path_str.len());
            let dots = ".".repeat(dots_needed);

            let suffix = if path_info.explicit {
                " (via --config)"
            } else {
                ""
            };

            // Color the path (purple/magenta for picked, dimmed for others)
            let colored_path = match path_info.status {
                FilePathStatus::Picked => path_str.magenta().to_string(),
                _ => path_str.dimmed().to_string(),
            };

            // Color the status
            let colored_status = match path_info.status {
                FilePathStatus::Picked => status_label.to_string(),
                _ => status_label.dimmed().to_string(),
            };

            println!("    {} {}{} {}", colored_status, colored_path, dots, suffix);
        }
    } else if file_resolution.had_explicit {
        println!("  file: (none - explicit --config not provided)");
    }

    // Environment variables - show actual prefix from shape
    if let Some(env_prefix) = ctx.env_prefix {
        println!("  env {}", format!("${}__*", env_prefix).yellow());
    }

    // CLI args - show pattern for config field overrides
    println!("  cli {}", format!("--{}.*", ctx.config_field_name).cyan());

    // Defaults
    println!("  defaults");

    println!();

    // Step 1: Coerce string values to their target types (for env vars)
    let coerced_value = coerce_types_from_shape(value, T::SHAPE);

    // Step 2: Collect all lines
    let mut lines = Vec::new();
    let mut state = DumpState::new();
    if let ConfigValue::Object(sourced) = &coerced_value
        && let facet_core::Type::User(facet_core::UserType::Struct(s)) = &T::SHAPE.ty
    {
        for field in s.fields {
            let key = field.name;
            if let Some(val) = sourced.value.get(key) {
                let is_sensitive = field.flags.contains(facet_core::FieldFlags::SENSITIVE);
                let field_shape = field.shape.get();
                collect_dump_lines(
                    val,
                    key,
                    0,
                    field_shape,
                    is_sensitive,
                    &mut lines,
                    &ctx,
                    &mut state,
                );
            }
        }
    }

    // Step 3: Calculate max widths per indent level
    let mut max_key_per_indent: HashMap<usize, usize> = HashMap::new();
    let mut max_val_per_indent: HashMap<usize, usize> = HashMap::new();

    for line in &lines {
        if !line.is_header {
            let key_width = visual_width(&line.key);
            let val_width = visual_width(&line.value);

            let key_max = max_key_per_indent.entry(line.indent).or_insert(0);
            *key_max = (*key_max).max(key_width);

            let val_max = max_val_per_indent.entry(line.indent).or_insert(0);
            *val_max = (*val_max).max(val_width);
        }
    }

    // Step 4: Print all lines with proper alignment
    for line in &lines {
        let indent_str = "  ".repeat(line.indent);

        if line.is_header {
            println!("{}{}", indent_str, line.key);
        } else {
            let key_width = visual_width(&line.key);
            let val_width = visual_width(&line.value);

            let max_key = max_key_per_indent.get(&line.indent).copied().unwrap_or(0);
            let max_val = max_val_per_indent.get(&line.indent).copied().unwrap_or(0);

            let key_padding = if key_width < max_key {
                ".".repeat(max_key - key_width)
            } else {
                String::new()
            };

            // Check if value needs wrapping
            if val_width > ctx.max_value_width {
                // Wrap value within its column
                let wrapped_lines = wrap_value(&line.value, ctx.max_value_width);

                for (i, wrapped_line) in wrapped_lines.iter().enumerate() {
                    if i == 0 {
                        // First line: show key, dots, value start, and provenance
                        let wrap_width = visual_width(wrapped_line);
                        let val_padding = if wrap_width < ctx.max_value_width {
                            ".".repeat(ctx.max_value_width - wrap_width)
                        } else {
                            String::new()
                        };
                        println!(
                            "{}{}{}  {}{} {}",
                            indent_str,
                            line.key,
                            key_padding.bright_black(),
                            wrapped_line,
                            val_padding.bright_black(),
                            line.provenance,
                        );
                    } else {
                        // Continuation lines: indent to value column
                        let continuation_indent = indent_str.len() + max_key + 2;
                        let spaces = " ".repeat(continuation_indent);
                        println!("{}{}", spaces, wrapped_line);
                    }
                }
            } else {
                // Normal single-line format with dot padding
                let val_padding = if val_width < max_val.min(ctx.max_value_width) {
                    ".".repeat(max_val.min(ctx.max_value_width) - val_width)
                } else {
                    String::new()
                };

                println!(
                    "{}{}{}  {}{} {}",
                    indent_str,
                    line.key,
                    key_padding.bright_black(),
                    line.value,
                    val_padding.bright_black(),
                    line.provenance,
                );
            }
        }
    }

    println!();

    // Show truncation notice if any values were truncated
    if state.had_truncation {
        println!();
        println!(
            "Some values were truncated. To show full values, rerun with {}=1",
            "FACET_ARGS_BLAST_IT".yellow()
        );
    }
}

/// Coerce ConfigValue types based on the target shape.
/// This is needed because environment variables always come in as strings,
/// but we want to display them with their proper types (int, bool, etc).
fn coerce_types_from_shape(value: &ConfigValue, shape: &'static facet_core::Shape) -> ConfigValue {
    match value {
        ConfigValue::Object(sourced) => {
            let mut new_map = sourced.value.clone();

            if let facet_core::Type::User(facet_core::UserType::Struct(s)) = &shape.ty {
                for field in s.fields {
                    if let Some(val) = new_map.get(field.name) {
                        let coerced = coerce_types_from_shape(val, field.shape.get());
                        new_map.insert(field.name.to_string(), coerced);
                    }
                }
            } else {
                // No struct info, just recurse on all values
                for (key, val) in sourced.value.iter() {
                    let coerced = coerce_types_from_shape(val, shape);
                    new_map.insert(key.clone(), coerced);
                }
            }

            ConfigValue::Object(Sourced {
                value: new_map,
                span: sourced.span,
                provenance: sourced.provenance.clone(),
            })
        }
        ConfigValue::Array(sourced) => {
            let element_shape = shape.inner.unwrap_or(shape);
            let new_items: Vec<ConfigValue> = sourced
                .value
                .iter()
                .map(|item| coerce_types_from_shape(item, element_shape))
                .collect();

            ConfigValue::Array(Sourced {
                value: new_items,
                span: sourced.span,
                provenance: sourced.provenance.clone(),
            })
        }
        ConfigValue::String(sourced) => {
            // Try to coerce string to the target type
            if let Some(scalar) = shape.scalar_type() {
                match scalar {
                    facet_core::ScalarType::I8
                    | facet_core::ScalarType::I16
                    | facet_core::ScalarType::I32
                    | facet_core::ScalarType::I64
                    | facet_core::ScalarType::I128 => {
                        if let Ok(num) = sourced.value.parse::<i64>() {
                            return ConfigValue::Integer(Sourced {
                                value: num,
                                span: sourced.span,
                                provenance: sourced.provenance.clone(),
                            });
                        }
                    }
                    facet_core::ScalarType::U8
                    | facet_core::ScalarType::U16
                    | facet_core::ScalarType::U32
                    | facet_core::ScalarType::U64
                    | facet_core::ScalarType::U128 => {
                        if let Ok(num) = sourced.value.parse::<i64>() {
                            return ConfigValue::Integer(Sourced {
                                value: num,
                                span: sourced.span,
                                provenance: sourced.provenance.clone(),
                            });
                        }
                    }
                    facet_core::ScalarType::F32 | facet_core::ScalarType::F64 => {
                        if let Ok(num) = sourced.value.parse::<f64>() {
                            return ConfigValue::Float(Sourced {
                                value: num,
                                span: sourced.span,
                                provenance: sourced.provenance.clone(),
                            });
                        }
                    }
                    facet_core::ScalarType::Bool => {
                        if let Ok(b) = sourced.value.parse::<bool>() {
                            return ConfigValue::Bool(Sourced {
                                value: b,
                                span: sourced.span,
                                provenance: sourced.provenance.clone(),
                            });
                        }
                    }
                    _ => {}
                }
            }
            // Keep as string if coercion fails or not needed
            value.clone()
        }
        // Other types don't need coercion
        _ => value.clone(),
    }
}

/// Recursively collect lines to be printed.
#[allow(clippy::too_many_arguments)]
fn collect_dump_lines(
    value: &ConfigValue,
    path: &str,
    indent: usize,
    shape: &'static facet_core::Shape,
    is_sensitive: bool,
    lines: &mut Vec<DumpLine>,
    ctx: &DumpContext,
    state: &mut DumpState,
) {
    match value {
        ConfigValue::Object(sourced) => {
            // Add header line for this object
            if !path.is_empty() {
                lines.push(DumpLine {
                    indent,
                    key: path.to_string(),
                    value: String::new(),
                    provenance: String::new(),
                    is_header: true,
                });
            }

            // Iterate in struct field order
            if let facet_core::Type::User(facet_core::UserType::Struct(s)) = &shape.ty {
                for field in s.fields {
                    let key = field.name;
                    if let Some(val) = sourced.value.get(key) {
                        let is_sensitive = field.flags.contains(facet_core::FieldFlags::SENSITIVE);
                        let field_shape = field.shape.get();
                        collect_dump_lines(
                            val,
                            key,
                            indent + 1,
                            field_shape,
                            is_sensitive,
                            lines,
                            ctx,
                            state,
                        );
                    }
                }
            } else {
                // Fallback: iterate in insertion order
                for (key, val) in sourced.value.iter() {
                    collect_dump_lines(val, key, indent + 1, shape, false, lines, ctx, state);
                }
            }
        }
        ConfigValue::Array(sourced) => {
            // Add header for array
            lines.push(DumpLine {
                indent,
                key: path.to_string(),
                value: String::new(),
                provenance: String::new(),
                is_header: true,
            });

            for (i, item) in sourced.value.iter().enumerate() {
                let element_shape = shape.inner.unwrap_or(shape);
                collect_dump_lines(
                    item,
                    &format!("[{}]", i),
                    indent + 1,
                    element_shape,
                    false,
                    lines,
                    ctx,
                    state,
                );
            }
        }
        ConfigValue::String(sourced) => {
            let colored_value = if is_sensitive {
                let len = sourced.value.len();
                format!("🔒 [REDACTED ({} bytes)]", len)
                    .bright_magenta()
                    .to_string()
            } else {
                // Replace newlines with visual indicator
                let escaped = sourced.value.replace('\n', "↵");
                let (truncated, was_truncated) = truncate_middle(&escaped, ctx.max_string_length);
                if was_truncated {
                    state.had_truncation = true;
                }
                format!("\"{}\"", truncated).green().to_string()
            };
            lines.push(DumpLine {
                indent,
                key: path.to_string(),
                value: colored_value,
                provenance: format_provenance(&sourced.provenance),
                is_header: false,
            });
        }
        ConfigValue::Integer(sourced) => {
            let colored_value = sourced.value.to_string().blue().to_string();
            lines.push(DumpLine {
                indent,
                key: path.to_string(),
                value: colored_value,
                provenance: format_provenance(&sourced.provenance),
                is_header: false,
            });
        }
        ConfigValue::Float(sourced) => {
            let colored_value = sourced.value.to_string().bright_blue().to_string();
            lines.push(DumpLine {
                indent,
                key: path.to_string(),
                value: colored_value,
                provenance: format_provenance(&sourced.provenance),
                is_header: false,
            });
        }
        ConfigValue::Bool(sourced) => {
            let colored_value = if sourced.value {
                sourced.value.to_string().green().to_string()
            } else {
                sourced.value.to_string().red().to_string()
            };
            lines.push(DumpLine {
                indent,
                key: path.to_string(),
                value: colored_value,
                provenance: format_provenance(&sourced.provenance),
                is_header: false,
            });
        }
        ConfigValue::Null(sourced) => {
            let colored_value = "null".bright_black().to_string();
            lines.push(DumpLine {
                indent,
                key: path.to_string(),
                value: colored_value,
                provenance: format_provenance(&sourced.provenance),
                is_header: false,
            });
        }
        ConfigValue::Missing(_info) => {
            // Show big red MISSING marker
            let colored_value = "❌ MISSING (required)".red().bold().to_string();
            lines.push(DumpLine {
                indent,
                key: path.to_string(),
                value: colored_value,
                provenance: String::new(),
                is_header: false,
            });

            // TODO: Add help text showing CLI/env/file options
            // For now, just mark it as missing - we can enhance this later
        }
    }
}

/// Calculate visual width of a string after stripping ANSI codes.
fn visual_width(s: &str) -> usize {
    let bytes = s.as_bytes();
    let stripped = strip_ansi_escapes::strip(bytes);
    let stripped_str = core::str::from_utf8(&stripped).unwrap_or(s);
    stripped_str.width()
}

/// Truncate a string in the middle if it exceeds max_length.
/// For example: "this is a very long string" -> "this is a...g string"
/// Returns (truncated_string, was_truncated)
fn truncate_middle(s: &str, max_length: usize) -> (String, bool) {
    if s.len() <= max_length {
        return (s.to_string(), false);
    }

    // Reserve 3 chars for "..."
    if max_length < 3 {
        return ("...".to_string(), true);
    }

    let available = max_length - 3;
    let start_len = available.div_ceil(2); // Round up for start
    let end_len = available / 2;

    let start = s.chars().take(start_len).collect::<String>();
    let end = s
        .chars()
        .rev()
        .take(end_len)
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();

    (format!("{}...{}", start, end), true)
}

/// Wrap a value string to fit within max_width, preserving ANSI color codes.
/// Returns a vector of lines with color codes reapplied to each line.
fn wrap_value(value: &str, max_width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current_line = String::new();
    let mut current_width = 0;
    let mut in_ansi = false;
    let mut ansi_buffer = String::new();
    let mut active_color = String::new(); // Track the last color code

    for ch in value.chars() {
        if ch == '\x1b' {
            // Start of ANSI escape sequence
            in_ansi = true;
            ansi_buffer.push(ch);
        } else if in_ansi {
            ansi_buffer.push(ch);
            if ch == 'm' {
                // End of ANSI escape sequence
                current_line.push_str(&ansi_buffer);
                active_color = ansi_buffer.clone(); // Save this color
                ansi_buffer.clear();
                in_ansi = false;
            }
        } else {
            // Regular character
            if current_width >= max_width {
                // Need to wrap - close current line and start new one with same color
                lines.push(current_line);
                current_line = String::new();
                if !active_color.is_empty() {
                    current_line.push_str(&active_color); // Reapply color to new line
                }
                current_width = 0;
            }
            current_line.push(ch);
            current_width += 1;
        }
    }

    // Push remaining content
    if !current_line.is_empty() || !ansi_buffer.is_empty() {
        current_line.push_str(&ansi_buffer);
        lines.push(current_line);
    }

    if lines.is_empty() {
        lines.push(String::new());
    }

    lines
}

/// Format provenance with colors.
fn format_provenance(prov: &Option<Provenance>) -> String {
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
