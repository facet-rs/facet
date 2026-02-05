//! Collect missing required fields by walking a Schema and ConfigValue together.

use crate::config_value::ConfigValue;
use crate::schema::{
    ArgKind, ArgLevelSchema, ConfigFieldSchema, ConfigStructSchema, ConfigValueSchema, Schema,
    ValueSchema,
};
use heck::ToKebabCase;
use heck::ToShoutySnakeCase;

/// Normalize a program name for display in error messages and help text.
///
/// This function:
/// 1. Extracts just the filename from a full path
/// 2. Strips Cargo test binary hash suffixes (e.g., "main-138217976bbdb088" -> "main")
///
/// This ensures stable, readable output in both tests and help messages.
pub fn normalize_program_name(path: &str) -> String {
    // Extract just the filename from the path
    let name = std::path::Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path);

    // Check if the name ends with a dash followed by exactly 16 hex characters
    if let Some(dash_pos) = name.rfind('-') {
        let suffix = &name[dash_pos + 1..];
        if suffix.len() == 16 && suffix.chars().all(|c| c.is_ascii_hexdigit()) {
            return name[..dash_pos].to_string();
        }
    }
    name.to_string()
}

/// The kind of field that is missing - determines how errors should be formatted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MissingFieldKind {
    /// A direct CLI argument (flag or positional) at the top level or in a subcommand
    CliArg,
    /// A field in a config struct (has layered sources: file, env, CLI overrides)
    ConfigField,
}

/// Information about a missing required field.
#[derive(Debug, Clone)]
pub struct MissingFieldInfo {
    /// Field name (e.g., "email" or "host")
    pub field_name: String,
    /// Full path (e.g., "config.server.host")
    pub field_path: String,
    /// Type name
    pub type_name: String,
    /// Documentation comment if available
    pub doc_comment: Option<String>,
    /// CLI flag to set this field (e.g., "--config.server-host" or "--settings.server-host")
    pub cli_flag: Option<String>,
    /// Environment variable to set this field (e.g., "APP__SERVER__HOST")
    pub env_var: Option<String>,
    /// Environment variable aliases (e.g., ["DATABASE_URL", "DB_URL"])
    pub env_aliases: Vec<String>,
    /// What kind of field this is - determines error formatting strategy
    pub kind: MissingFieldKind,
}

/// Information about a corrected command with missing arguments for display.
pub struct CorrectedCommandInfo {
    /// The corrected command text showing what should be typed
    pub corrected_source: String,
    /// Diagnostics with spans pointing to the corrected source
    pub diagnostics: Vec<crate::driver::Diagnostic>,
}

/// Build corrected command source and diagnostics for missing CLI arguments.
///
/// Returns the corrected command as a source and diagnostics that point to it.
pub fn build_corrected_command_diagnostics(
    missing: &[MissingFieldInfo],
    cli_args: Option<&str>,
) -> CorrectedCommandInfo {
    use crate::driver::{Diagnostic, Severity};
    use crate::span::Span;

    let cli_text = cli_args.unwrap_or("");

    // Handle the first missing field (most common case)
    // Multiple missing would need a different UI approach
    if let Some(field) = missing.first() {
        // Build the corrected command with the missing argument
        let missing_arg_text = if let Some(cli_flag) = &field.cli_flag {
            if cli_flag.starts_with('<') && cli_flag.ends_with('>') {
                // Positional argument like <name>
                cli_flag.clone()
            } else if cli_flag.starts_with("--") {
                // Named argument like --region
                let flag_name = cli_flag.trim_start_matches("--");
                format!("{} <{}>", cli_flag, flag_name)
            } else {
                cli_flag.clone()
            }
        } else {
            format!("<{}>", field.field_name)
        };

        let corrected_command = if cli_text.is_empty() {
            // Get program name
            let program_name = std::env::args()
                .next()
                .map(|path| normalize_program_name(&path))
                .unwrap_or_else(|| "program".to_string());
            format!("{} {}", program_name, missing_arg_text)
        } else {
            format!("{} {}", cli_text, missing_arg_text)
        };

        // Calculate the span of the missing argument in the corrected command
        let missing_arg_start = if cli_text.is_empty() {
            corrected_command.len() - missing_arg_text.len()
        } else {
            cli_text.len() + 1
        };

        // Use generic message for the error, field docs go in the label (rendered by Ariadne)
        let message = "missing required argument".to_string();

        // Use field documentation as the label message
        let label = field.doc_comment.clone();

        let diagnostic = Diagnostic {
            message,
            label,
            path: None,
            span: Some(Span::new(missing_arg_start, missing_arg_text.len())),
            severity: Severity::Error,
        };

        CorrectedCommandInfo {
            corrected_source: corrected_command,
            diagnostics: vec![diagnostic],
        }
    } else {
        // No missing fields - return empty
        CorrectedCommandInfo {
            corrected_source: cli_text.to_string(),
            diagnostics: vec![],
        }
    }
}

/// Collect missing required fields by walking the schema and checking against the ConfigValue.
/// A field is "missing" if it's not in the ConfigValue AND is not Option.
///
/// This walks the Schema (which already handles flattening), so no flattening logic is needed here.
pub fn collect_missing_fields(
    value: &ConfigValue,
    schema: &Schema,
    missing: &mut Vec<MissingFieldInfo>,
) {
    let obj_map = match value {
        ConfigValue::Object(sourced) => &sourced.value,
        _ => return,
    };

    // Check CLI args (top-level arguments like --verbose, --output, etc.)
    collect_missing_in_arg_level(obj_map, schema.args(), "", missing);

    // Check config section if present
    if let Some(config_schema) = schema.config() {
        let env_prefix = config_schema.env_prefix();

        // The config value is nested under the config field name
        if let Some(field_name) = config_schema.field_name() {
            if let Some(config_value) = obj_map.get(field_name) {
                collect_missing_in_config_struct(
                    config_value,
                    config_schema,
                    "",
                    env_prefix,
                    missing,
                );
            } else {
                // The entire config struct is missing - report it
                missing.push(MissingFieldInfo {
                    field_name: field_name.to_string(),
                    field_path: field_name.to_string(),
                    type_name: "Struct".to_string(),
                    doc_comment: None,
                    cli_flag: None,
                    env_var: None,
                    env_aliases: Vec::new(),
                    kind: MissingFieldKind::ConfigField,
                });
            }
        } else {
            // Config has no field name (shouldn't happen normally), check at root level
            collect_missing_in_config_struct(value, config_schema, "", env_prefix, missing);
        }
    }
}

/// Walk an arg level schema and check for missing required args.
fn collect_missing_in_arg_level(
    obj_map: &crate::config_value::ObjectMap,
    arg_level: &ArgLevelSchema,
    path_prefix: &str,
    missing: &mut Vec<MissingFieldInfo>,
) {
    for (name, arg_schema) in arg_level.args() {
        let field_path = if path_prefix.is_empty() {
            name.to_string()
        } else {
            format!("{}.{}", path_prefix, name)
        };

        if obj_map.get(name.as_str()).is_none() && arg_schema.required() {
            let type_name = get_value_type_name(arg_schema.value());

            // CLI args use kebab-case and are either positional or flags
            let cli_flag = match arg_schema.kind() {
                ArgKind::Positional => Some(format!("<{}>", name.to_kebab_case())),
                ArgKind::Named { .. } => Some(format!("--{}", name.to_kebab_case())),
            };

            missing.push(MissingFieldInfo {
                field_name: name.to_string(),
                field_path,
                type_name,
                doc_comment: arg_schema.docs().summary().map(|s| s.to_string()),
                cli_flag,
                env_var: None, // CLI args don't have env vars
                env_aliases: Vec::new(),
                kind: MissingFieldKind::CliArg,
            });
        }
    }

    // Check subcommands if present and required
    if let Some(subcommand_field) = arg_level.subcommand_field_name() {
        if !arg_level.subcommand_optional() && obj_map.get(subcommand_field).is_none() {
            // Subcommand field itself is missing
            let field_path = if path_prefix.is_empty() {
                subcommand_field.to_string()
            } else {
                format!("{}.{}", path_prefix, subcommand_field)
            };
            missing.push(MissingFieldInfo {
                field_name: subcommand_field.to_string(),
                field_path,
                type_name: "Subcommand".to_string(),
                doc_comment: None,
                cli_flag: Some(format!("<{}>", subcommand_field.to_kebab_case())),
                env_var: None,
                env_aliases: Vec::new(),
                kind: MissingFieldKind::CliArg,
            });
        } else if let Some(ConfigValue::Enum(sourced)) = obj_map.get(subcommand_field) {
            // Subcommand is present - recursively check its arguments
            let enum_value = &sourced.value;
            let variant_name = &enum_value.variant;

            // Find the matching subcommand schema
            if let Some(subcommand_schema) = arg_level
                .subcommands()
                .iter()
                .find(|(name, _)| name.as_str() == variant_name)
                .map(|(_, schema)| schema)
            {
                // Recursively check the subcommand's arguments
                let subcommand_path = if path_prefix.is_empty() {
                    format!("{}::{}", subcommand_field, variant_name)
                } else {
                    format!("{}.{}::{}", path_prefix, subcommand_field, variant_name)
                };

                collect_missing_in_arg_level(
                    &enum_value.fields,
                    subcommand_schema.args(),
                    &subcommand_path,
                    missing,
                );
            }
        }
    }
}

/// Get a human-readable type name from a ValueSchema.
fn get_value_type_name(schema: &ValueSchema) -> String {
    match schema {
        ValueSchema::Leaf(leaf) => leaf.shape.to_string(),
        ValueSchema::Option { value, .. } => format!("Option<{}>", get_value_type_name(value)),
        ValueSchema::Vec { element, .. } => format!("Vec<{}>", get_value_type_name(element)),
        ValueSchema::Struct { shape, .. } => shape.to_string(),
    }
}

/// Context for collecting missing config fields.
struct ConfigContext<'a> {
    /// The config field name (e.g., "config" or "settings")
    config_field_name: &'a str,
    /// Environment variable prefix if set
    env_prefix: Option<&'a str>,
}

/// Walk a config struct schema and its corresponding ConfigValue.
fn collect_missing_in_config_struct(
    value: &ConfigValue,
    struct_schema: &ConfigStructSchema,
    path_prefix: &str,
    env_prefix: Option<&str>,
    missing: &mut Vec<MissingFieldInfo>,
) {
    let obj_map = match value {
        ConfigValue::Object(sourced) => &sourced.value,
        _ => return, // Not an object, can't check struct fields
    };

    // Get the config field name from the struct schema
    let config_field_name = struct_schema.field_name().unwrap_or("config");
    let ctx = ConfigContext {
        config_field_name,
        env_prefix,
    };

    for (field_name, field_schema) in struct_schema.fields() {
        let field_path = if path_prefix.is_empty() {
            field_name.to_string()
        } else {
            format!("{}.{}", path_prefix, field_name)
        };

        if let Some(field_value) = obj_map.get(field_name.as_str()) {
            // Field exists - recurse into nested structs/vecs
            collect_missing_in_config_field(field_value, field_schema, &field_path, &ctx, missing);
        } else {
            // Field is missing - check if it's required
            check_missing_field(field_name, &field_path, field_schema, &ctx, missing);
        }
    }
}

/// Walk a config field schema and its corresponding ConfigValue.
fn collect_missing_in_config_field(
    value: &ConfigValue,
    field_schema: &ConfigFieldSchema,
    path_prefix: &str,
    ctx: &ConfigContext,
    missing: &mut Vec<MissingFieldInfo>,
) {
    collect_missing_in_config_value(value, field_schema.value(), path_prefix, ctx, missing);
}

/// Walk a config value schema and its corresponding ConfigValue.
fn collect_missing_in_config_value(
    value: &ConfigValue,
    value_schema: &ConfigValueSchema,
    path_prefix: &str,
    ctx: &ConfigContext,
    missing: &mut Vec<MissingFieldInfo>,
) {
    match value_schema {
        ConfigValueSchema::Struct(struct_schema) => {
            // For nested structs, we don't have a new env_prefix, pass the existing one
            collect_missing_in_config_struct_inner(value, struct_schema, path_prefix, ctx, missing);
        }
        ConfigValueSchema::Vec(vec_schema) => {
            // Recurse into array elements
            if let ConfigValue::Array(sourced) = value {
                for (i, item) in sourced.value.iter().enumerate() {
                    let item_path = format!("{}[{}]", path_prefix, i);
                    collect_missing_in_config_value(
                        item,
                        vec_schema.element(),
                        &item_path,
                        ctx,
                        missing,
                    );
                }
            }
        }
        ConfigValueSchema::Option { value: inner, .. } => {
            // Option fields are never required, but recurse to check nested structs
            collect_missing_in_config_value(value, inner, path_prefix, ctx, missing);
        }
        ConfigValueSchema::Enum(enum_schema) => {
            // For enums, check if the selected variant has missing fields
            if let ConfigValue::Enum(sourced) = value {
                let variant_name = &sourced.value.variant;
                if let Some(variant_schema) = enum_schema.get_variant(variant_name) {
                    for (field_name, field_schema) in variant_schema.fields() {
                        let field_path = format!("{}.{}", path_prefix, field_name);
                        if let Some(field_value) = sourced.value.fields.get(field_name.as_str()) {
                            collect_missing_in_config_value(
                                field_value,
                                field_schema.value(),
                                &field_path,
                                ctx,
                                missing,
                            );
                        } else {
                            let is_optional =
                                matches!(field_schema.value(), ConfigValueSchema::Option { .. });
                            if !is_optional {
                                missing.push(MissingFieldInfo {
                                    field_name: field_name.to_string(),
                                    field_path,
                                    type_name: type_name_from_config_value_schema(
                                        field_schema.value(),
                                    ),
                                    doc_comment: None,
                                    cli_flag: None,
                                    env_var: None,
                                    env_aliases: field_schema.env_aliases().to_vec(),
                                    kind: MissingFieldKind::ConfigField,
                                });
                            }
                        }
                    }
                }
            }
        }
        ConfigValueSchema::Leaf(_) => {
            // Leaf values have no nested fields to check
        }
    }
}

/// Inner function for nested structs (doesn't reset context).
fn collect_missing_in_config_struct_inner(
    value: &ConfigValue,
    struct_schema: &ConfigStructSchema,
    path_prefix: &str,
    ctx: &ConfigContext,
    missing: &mut Vec<MissingFieldInfo>,
) {
    let obj_map = match value {
        ConfigValue::Object(sourced) => &sourced.value,
        _ => return,
    };

    for (field_name, field_schema) in struct_schema.fields() {
        let field_path = if path_prefix.is_empty() {
            field_name.to_string()
        } else {
            format!("{}.{}", path_prefix, field_name)
        };

        if let Some(field_value) = obj_map.get(field_name.as_str()) {
            collect_missing_in_config_field(field_value, field_schema, &field_path, ctx, missing);
        } else {
            check_missing_field(field_name, &field_path, field_schema, ctx, missing);
        }
    }
}

/// Check if a missing field is required and add it to the missing list if so.
fn check_missing_field(
    field_name: &str,
    field_path: &str,
    field_schema: &ConfigFieldSchema,
    ctx: &ConfigContext,
    missing: &mut Vec<MissingFieldInfo>,
) {
    // A field is required if it's NOT wrapped in Option
    let is_optional = matches!(field_schema.value(), ConfigValueSchema::Option { .. });

    if !is_optional {
        let type_name = get_config_type_name(field_schema.value());

        // CLI flag: --config.field-path (kebab-case the path segments)
        let cli_path = field_path
            .split('.')
            .map(|s| s.to_kebab_case())
            .collect::<Vec<_>>()
            .join(".");
        let cli_flag = Some(format!("--{}.{}", ctx.config_field_name, cli_path));

        // Env var: PREFIX__FIELD__PATH (SCREAMING_SNAKE_CASE)
        let env_var = ctx.env_prefix.map(|prefix| {
            let env_path = field_path
                .split('.')
                .map(|s| s.to_shouty_snake_case())
                .collect::<Vec<_>>()
                .join("__");
            format!("{}__{}", prefix, env_path)
        });

        missing.push(MissingFieldInfo {
            field_name: field_name.to_string(),
            field_path: field_path.to_string(),
            type_name,
            doc_comment: None, // Schema doesn't currently expose docs on ConfigFieldSchema
            cli_flag,
            env_var,
            env_aliases: field_schema.env_aliases().to_vec(),
            kind: MissingFieldKind::ConfigField,
        });
    }
}

/// Get a human-readable type name from a ConfigValueSchema.
fn get_config_type_name(schema: &ConfigValueSchema) -> String {
    match schema {
        ConfigValueSchema::Struct(_) => "Struct".to_string(),
        ConfigValueSchema::Vec(v) => format!("Vec<{}>", get_config_type_name(v.element())),
        ConfigValueSchema::Option { value, .. } => {
            format!("Option<{}>", get_config_type_name(value))
        }
        ConfigValueSchema::Enum(e) => e.shape().to_string(),
        ConfigValueSchema::Leaf(leaf) => leaf.shape.to_string(),
    }
}

fn type_name_from_config_value_schema(schema: &ConfigValueSchema) -> String {
    get_config_type_name(schema)
}

/// Format a summary of missing fields for display after the config dump.
///
/// This produces a focused list of just the missing fields with their paths,
/// types, and documentation (if available), so users can quickly see what
/// needs to be provided without scrolling through the entire config dump.
///
/// For CLI-only missing fields (when all missing fields have cli_flag set),
/// prefer using `format_missing_cli_args_with_mockup` for a more user-friendly display.
pub fn format_missing_fields_summary(missing: &[MissingFieldInfo]) -> String {
    use owo_colors::OwoColorize;
    use std::fmt::Write;

    if missing.is_empty() {
        return String::new();
    }

    let mut output = String::new();

    for field in missing {
        // Field path and type
        write!(
            output,
            "  {} <{}>",
            field.field_path.bold(),
            field.type_name.cyan()
        )
        .unwrap();

        // Show how to set the field
        let mut hints = Vec::new();
        if let Some(cli) = &field.cli_flag {
            hints.push(cli.green().to_string());
        }
        if let Some(env) = &field.env_var {
            hints.push(format!("${}", env).yellow().to_string());
        }
        // Show env aliases
        for alias in &field.env_aliases {
            hints.push(
                format!("${} {}", alias, "(alias)".dimmed())
                    .yellow()
                    .to_string(),
            );
        }
        if !hints.is_empty() {
            write!(output, " ({})", hints.join(" or ")).unwrap();
        }

        // Add doc comment on a new line if available
        if let Some(doc) = &field.doc_comment {
            write!(output, "\n    {}", doc.dimmed()).unwrap();
        }

        output.push('\n');
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config_value::{ObjectMap, Sourced};
    use facet::Facet;
    use figue_attrs as args;

    // ========================================================================
    // Test helpers
    // ========================================================================

    fn cv_object(fields: impl IntoIterator<Item = (&'static str, ConfigValue)>) -> ConfigValue {
        let map: ObjectMap = fields
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        ConfigValue::Object(Sourced::new(map))
    }

    fn cv_array(items: impl IntoIterator<Item = ConfigValue>) -> ConfigValue {
        ConfigValue::Array(Sourced::new(items.into_iter().collect()))
    }

    fn cv_string(s: &str) -> ConfigValue {
        ConfigValue::String(Sourced::new(s.to_string()))
    }

    fn cv_int(i: i64) -> ConfigValue {
        ConfigValue::Integer(Sourced::new(i))
    }

    fn cv_bool(b: bool) -> ConfigValue {
        ConfigValue::Bool(Sourced::new(b))
    }

    // ========================================================================
    // Basic test structs - use different field names to avoid hardcoding
    // ========================================================================

    #[derive(Facet)]
    struct SimpleFields {
        host: String,
        port: u16,
    }

    // Uses "config" as the field name
    #[derive(Facet)]
    struct ArgsWithConfig {
        #[facet(args::config)]
        config: SimpleFields,
    }

    // Uses "settings" as the field name to ensure we don't hardcode "config"
    #[derive(Facet)]
    struct ArgsWithSettings {
        #[facet(args::config)]
        settings: SimpleFields,
    }

    // ========================================================================
    // Tests: All fields present
    // ========================================================================

    #[test]
    fn test_all_fields_present_with_config() {
        let schema = Schema::from_shape(ArgsWithConfig::SHAPE).unwrap();
        let value = cv_object([(
            "config",
            cv_object([("host", cv_string("localhost")), ("port", cv_int(8080))]),
        )]);

        let mut missing = Vec::new();
        collect_missing_fields(&value, &schema, &mut missing);

        assert!(
            missing.is_empty(),
            "Expected no missing fields, got: {:?}",
            missing
        );
    }

    #[test]
    fn test_all_fields_present_with_settings() {
        let schema = Schema::from_shape(ArgsWithSettings::SHAPE).unwrap();
        let value = cv_object([(
            "settings",
            cv_object([("host", cv_string("localhost")), ("port", cv_int(8080))]),
        )]);

        let mut missing = Vec::new();
        collect_missing_fields(&value, &schema, &mut missing);

        assert!(
            missing.is_empty(),
            "Expected no missing fields, got: {:?}",
            missing
        );
    }

    #[test]
    fn test_empty_root_reports_config_missing() {
        let schema = Schema::from_shape(ArgsWithConfig::SHAPE).unwrap();
        let value = cv_object([]);

        let mut missing = Vec::new();
        collect_missing_fields(&value, &schema, &mut missing);

        assert_eq!(
            missing.len(),
            1,
            "Expected 1 missing field (config), got: {:?}",
            missing
        );
        assert_eq!(missing[0].field_name, "config");
    }

    #[test]
    fn test_empty_root_reports_settings_missing() {
        let schema = Schema::from_shape(ArgsWithSettings::SHAPE).unwrap();
        let value = cv_object([]);

        let mut missing = Vec::new();
        collect_missing_fields(&value, &schema, &mut missing);

        assert_eq!(
            missing.len(),
            1,
            "Expected 1 missing field (settings), got: {:?}",
            missing
        );
        assert_eq!(missing[0].field_name, "settings");
    }

    #[test]
    fn test_empty_config_reports_inner_fields() {
        let schema = Schema::from_shape(ArgsWithConfig::SHAPE).unwrap();
        let value = cv_object([("config", cv_object([]))]);

        let mut missing = Vec::new();
        collect_missing_fields(&value, &schema, &mut missing);

        assert_eq!(
            missing.len(),
            2,
            "Expected 2 missing fields, got: {:?}",
            missing
        );
        let names: Vec<_> = missing.iter().map(|m| m.field_name.as_str()).collect();
        assert!(names.contains(&"host"), "Should report 'host' as missing");
        assert!(names.contains(&"port"), "Should report 'port' as missing");
    }

    // ========================================================================
    // Tests: Single missing field
    // ========================================================================

    #[test]
    fn test_missing_required_field() {
        let schema = Schema::from_shape(ArgsWithConfig::SHAPE).unwrap();
        let value = cv_object([("config", cv_object([("host", cv_string("localhost"))]))]);

        let mut missing = Vec::new();
        collect_missing_fields(&value, &schema, &mut missing);

        assert_eq!(
            missing.len(),
            1,
            "Expected 1 missing field, got: {:?}",
            missing
        );
        assert_eq!(missing[0].field_name, "port");
        assert_eq!(missing[0].field_path, "port");
    }

    #[test]
    fn test_missing_string_field_with_settings() {
        let schema = Schema::from_shape(ArgsWithSettings::SHAPE).unwrap();
        let value = cv_object([("settings", cv_object([("port", cv_int(8080))]))]);

        let mut missing = Vec::new();
        collect_missing_fields(&value, &schema, &mut missing);

        assert_eq!(
            missing.len(),
            1,
            "Expected 1 missing field, got: {:?}",
            missing
        );
        assert_eq!(missing[0].field_name, "host");
        assert_eq!(missing[0].field_path, "host");
    }

    // ========================================================================
    // Tests: Optional fields
    // ========================================================================

    #[derive(Facet)]
    struct FieldsWithOptional {
        host: String,
        port: Option<u16>,
    }

    #[derive(Facet)]
    struct ArgsWithOptionalConfig {
        #[facet(args::config)]
        config: FieldsWithOptional,
    }

    #[test]
    fn test_optional_field_not_reported_missing() {
        let schema = Schema::from_shape(ArgsWithOptionalConfig::SHAPE).unwrap();
        let value = cv_object([("config", cv_object([("host", cv_string("localhost"))]))]);

        let mut missing = Vec::new();
        collect_missing_fields(&value, &schema, &mut missing);

        assert!(
            missing.is_empty(),
            "Optional fields should not be reported as missing: {:?}",
            missing
        );
    }

    #[derive(Facet)]
    struct AllOptionalFields {
        host: Option<String>,
        port: Option<u16>,
        debug: Option<bool>,
    }

    #[derive(Facet)]
    struct ArgsWithAllOptionalSettings {
        #[facet(args::config)]
        settings: AllOptionalFields,
    }

    #[test]
    fn test_all_optional_fields_empty_config() {
        let schema = Schema::from_shape(ArgsWithAllOptionalSettings::SHAPE).unwrap();
        let value = cv_object([("settings", cv_object([]))]);

        let mut missing = Vec::new();
        collect_missing_fields(&value, &schema, &mut missing);

        assert!(
            missing.is_empty(),
            "All-optional config should have no missing fields: {:?}",
            missing
        );
    }

    // ========================================================================
    // Tests: Nested structs
    // ========================================================================

    #[derive(Facet)]
    struct NestedFields {
        server: SimpleFields,
    }

    #[derive(Facet)]
    struct ArgsWithNestedConfig {
        #[facet(args::config)]
        config: NestedFields,
    }

    #[derive(Facet)]
    struct ArgsWithNestedSettings {
        #[facet(args::config)]
        settings: NestedFields,
    }

    #[test]
    fn test_nested_missing_field() {
        let schema = Schema::from_shape(ArgsWithNestedConfig::SHAPE).unwrap();
        let value = cv_object([(
            "config",
            cv_object([("server", cv_object([("port", cv_int(8080))]))]),
        )]);

        let mut missing = Vec::new();
        collect_missing_fields(&value, &schema, &mut missing);

        assert_eq!(
            missing.len(),
            1,
            "Expected 1 missing field, got: {:?}",
            missing
        );
        assert_eq!(missing[0].field_name, "host");
        assert_eq!(missing[0].field_path, "server.host");
    }

    #[test]
    fn test_nested_all_fields_present() {
        let schema = Schema::from_shape(ArgsWithNestedSettings::SHAPE).unwrap();
        let value = cv_object([(
            "settings",
            cv_object([(
                "server",
                cv_object([("host", cv_string("localhost")), ("port", cv_int(8080))]),
            )]),
        )]);

        let mut missing = Vec::new();
        collect_missing_fields(&value, &schema, &mut missing);

        assert!(
            missing.is_empty(),
            "Expected no missing fields, got: {:?}",
            missing
        );
    }

    #[test]
    fn test_nested_struct_entirely_missing() {
        let schema = Schema::from_shape(ArgsWithNestedConfig::SHAPE).unwrap();
        let value = cv_object([("config", cv_object([]))]);

        let mut missing = Vec::new();
        collect_missing_fields(&value, &schema, &mut missing);

        assert_eq!(
            missing.len(),
            1,
            "Expected 1 missing field (the struct), got: {:?}",
            missing
        );
        assert_eq!(missing[0].field_name, "server");
        assert_eq!(missing[0].field_path, "server");
    }

    // ========================================================================
    // Tests: Deeply nested structs
    // ========================================================================

    #[derive(Facet)]
    struct Level2 {
        value: String,
    }

    #[derive(Facet)]
    struct Level1 {
        level2: Level2,
    }

    #[derive(Facet)]
    struct DeepFields {
        level1: Level1,
    }

    #[derive(Facet)]
    struct ArgsWithDeepConfig {
        #[facet(args::config)]
        config: DeepFields,
    }

    #[test]
    fn test_deeply_nested_missing_field() {
        let schema = Schema::from_shape(ArgsWithDeepConfig::SHAPE).unwrap();
        let value = cv_object([(
            "config",
            cv_object([("level1", cv_object([("level2", cv_object([]))]))]),
        )]);

        let mut missing = Vec::new();
        collect_missing_fields(&value, &schema, &mut missing);

        assert_eq!(
            missing.len(),
            1,
            "Expected 1 missing field, got: {:?}",
            missing
        );
        assert_eq!(missing[0].field_name, "value");
        assert_eq!(missing[0].field_path, "level1.level2.value");
    }

    #[test]
    fn test_deeply_nested_all_present() {
        let schema = Schema::from_shape(ArgsWithDeepConfig::SHAPE).unwrap();
        let value = cv_object([(
            "config",
            cv_object([(
                "level1",
                cv_object([("level2", cv_object([("value", cv_string("hello"))]))]),
            )]),
        )]);

        let mut missing = Vec::new();
        collect_missing_fields(&value, &schema, &mut missing);

        assert!(
            missing.is_empty(),
            "Expected no missing fields, got: {:?}",
            missing
        );
    }

    // ========================================================================
    // Tests: Flattened structs
    // ========================================================================

    #[derive(Facet)]
    struct CommonFields {
        timeout: u32,
    }

    #[derive(Facet)]
    struct FlattenedFields {
        host: String,
        #[facet(flatten)]
        common: CommonFields,
    }

    #[derive(Facet)]
    struct ArgsWithFlattenedConfig {
        #[facet(args::config)]
        config: FlattenedFields,
    }

    #[derive(Facet)]
    struct ArgsWithFlattenedSettings {
        #[facet(args::config)]
        settings: FlattenedFields,
    }

    #[test]
    fn test_flattened_all_fields_present() {
        let schema = Schema::from_shape(ArgsWithFlattenedConfig::SHAPE).unwrap();
        // Flattened fields appear at the same level
        let value = cv_object([(
            "config",
            cv_object([("host", cv_string("localhost")), ("timeout", cv_int(30))]),
        )]);

        let mut missing = Vec::new();
        collect_missing_fields(&value, &schema, &mut missing);

        assert!(
            missing.is_empty(),
            "Expected no missing fields, got: {:?}",
            missing
        );
    }

    #[test]
    fn test_flattened_missing_inner_field() {
        let schema = Schema::from_shape(ArgsWithFlattenedSettings::SHAPE).unwrap();
        // Missing 'timeout' which comes from flattened CommonFields
        let value = cv_object([("settings", cv_object([("host", cv_string("localhost"))]))]);

        let mut missing = Vec::new();
        collect_missing_fields(&value, &schema, &mut missing);

        assert_eq!(
            missing.len(),
            1,
            "Expected 1 missing field, got: {:?}",
            missing
        );
        assert_eq!(missing[0].field_name, "timeout");
    }

    #[test]
    fn test_flattened_missing_outer_field() {
        let schema = Schema::from_shape(ArgsWithFlattenedConfig::SHAPE).unwrap();
        // Missing 'host', have 'timeout' from flattened struct
        let value = cv_object([("config", cv_object([("timeout", cv_int(30))]))]);

        let mut missing = Vec::new();
        collect_missing_fields(&value, &schema, &mut missing);

        assert_eq!(
            missing.len(),
            1,
            "Expected 1 missing field, got: {:?}",
            missing
        );
        assert_eq!(missing[0].field_name, "host");
    }

    // ========================================================================
    // Tests: Vec fields
    // ========================================================================

    #[derive(Facet)]
    struct FieldsWithVec {
        name: String,
        items: Vec<String>,
    }

    #[derive(Facet)]
    struct ArgsWithVecConfig {
        #[facet(args::config)]
        config: FieldsWithVec,
    }

    #[test]
    fn test_vec_field_present_empty() {
        let schema = Schema::from_shape(ArgsWithVecConfig::SHAPE).unwrap();
        let value = cv_object([(
            "config",
            cv_object([("name", cv_string("test")), ("items", cv_array([]))]),
        )]);

        let mut missing = Vec::new();
        collect_missing_fields(&value, &schema, &mut missing);

        assert!(
            missing.is_empty(),
            "Empty vec should be valid: {:?}",
            missing
        );
    }

    #[test]
    fn test_vec_field_present_with_items() {
        let schema = Schema::from_shape(ArgsWithVecConfig::SHAPE).unwrap();
        let value = cv_object([(
            "config",
            cv_object([
                ("name", cv_string("test")),
                ("items", cv_array([cv_string("a"), cv_string("b")])),
            ]),
        )]);

        let mut missing = Vec::new();
        collect_missing_fields(&value, &schema, &mut missing);

        assert!(
            missing.is_empty(),
            "Vec with items should be valid: {:?}",
            missing
        );
    }

    #[test]
    fn test_vec_field_missing() {
        let schema = Schema::from_shape(ArgsWithVecConfig::SHAPE).unwrap();
        let value = cv_object([("config", cv_object([("name", cv_string("test"))]))]);

        let mut missing = Vec::new();
        collect_missing_fields(&value, &schema, &mut missing);

        assert_eq!(
            missing.len(),
            1,
            "Expected 1 missing field, got: {:?}",
            missing
        );
        assert_eq!(missing[0].field_name, "items");
    }

    // ========================================================================
    // Tests: Vec of structs
    // ========================================================================

    #[derive(Facet)]
    struct FieldsWithVecOfStructs {
        servers: Vec<SimpleFields>,
    }

    #[derive(Facet)]
    struct ArgsWithVecOfStructsSettings {
        #[facet(args::config)]
        settings: FieldsWithVecOfStructs,
    }

    #[test]
    fn test_vec_of_structs_all_valid() {
        let schema = Schema::from_shape(ArgsWithVecOfStructsSettings::SHAPE).unwrap();
        let value = cv_object([(
            "settings",
            cv_object([(
                "servers",
                cv_array([
                    cv_object([("host", cv_string("a.com")), ("port", cv_int(80))]),
                    cv_object([("host", cv_string("b.com")), ("port", cv_int(443))]),
                ]),
            )]),
        )]);

        let mut missing = Vec::new();
        collect_missing_fields(&value, &schema, &mut missing);

        assert!(
            missing.is_empty(),
            "Expected no missing fields, got: {:?}",
            missing
        );
    }

    #[test]
    fn test_vec_of_structs_missing_field_in_element() {
        let schema = Schema::from_shape(ArgsWithVecOfStructsSettings::SHAPE).unwrap();
        let value = cv_object([(
            "settings",
            cv_object([(
                "servers",
                cv_array([
                    cv_object([("host", cv_string("a.com")), ("port", cv_int(80))]),
                    cv_object([("host", cv_string("b.com"))]), // missing port
                ]),
            )]),
        )]);

        let mut missing = Vec::new();
        collect_missing_fields(&value, &schema, &mut missing);

        assert_eq!(
            missing.len(),
            1,
            "Expected 1 missing field, got: {:?}",
            missing
        );
        assert_eq!(missing[0].field_name, "port");
        assert_eq!(missing[0].field_path, "servers[1].port");
    }

    #[test]
    fn test_vec_of_structs_missing_in_multiple_elements() {
        let schema = Schema::from_shape(ArgsWithVecOfStructsSettings::SHAPE).unwrap();
        let value = cv_object([(
            "settings",
            cv_object([(
                "servers",
                cv_array([
                    cv_object([("port", cv_int(80))]),         // missing host
                    cv_object([("host", cv_string("b.com"))]), // missing port
                ]),
            )]),
        )]);

        let mut missing = Vec::new();
        collect_missing_fields(&value, &schema, &mut missing);

        assert_eq!(
            missing.len(),
            2,
            "Expected 2 missing fields, got: {:?}",
            missing
        );

        let paths: Vec<_> = missing.iter().map(|m| m.field_path.as_str()).collect();
        assert!(
            paths.contains(&"servers[0].host"),
            "Should report servers[0].host"
        );
        assert!(
            paths.contains(&"servers[1].port"),
            "Should report servers[1].port"
        );
    }

    // ========================================================================
    // Tests: Optional nested struct
    // ========================================================================

    #[derive(Facet)]
    struct FieldsWithOptionalNested {
        name: String,
        server: Option<SimpleFields>,
    }

    #[derive(Facet)]
    struct ArgsWithOptionalNestedConfig {
        #[facet(args::config)]
        config: FieldsWithOptionalNested,
    }

    #[test]
    fn test_optional_nested_struct_absent() {
        let schema = Schema::from_shape(ArgsWithOptionalNestedConfig::SHAPE).unwrap();
        let value = cv_object([("config", cv_object([("name", cv_string("test"))]))]);

        let mut missing = Vec::new();
        collect_missing_fields(&value, &schema, &mut missing);

        assert!(
            missing.is_empty(),
            "Optional nested struct should not be required: {:?}",
            missing
        );
    }

    #[test]
    fn test_optional_nested_struct_present_but_incomplete() {
        let schema = Schema::from_shape(ArgsWithOptionalNestedConfig::SHAPE).unwrap();
        // server is present but missing 'port'
        let value = cv_object([(
            "config",
            cv_object([
                ("name", cv_string("test")),
                ("server", cv_object([("host", cv_string("localhost"))])),
            ]),
        )]);

        let mut missing = Vec::new();
        collect_missing_fields(&value, &schema, &mut missing);

        // When the optional struct IS provided, its required fields must be present
        assert_eq!(
            missing.len(),
            1,
            "Expected 1 missing field, got: {:?}",
            missing
        );
        assert_eq!(missing[0].field_name, "port");
        assert_eq!(missing[0].field_path, "server.port");
    }

    // ========================================================================
    // Tests: Mixed required and optional
    // ========================================================================

    #[derive(Facet)]
    struct MixedFields {
        required_str: String,
        optional_str: Option<String>,
        required_int: u32,
        optional_int: Option<u32>,
    }

    #[derive(Facet)]
    struct ArgsWithMixedSettings {
        #[facet(args::config)]
        settings: MixedFields,
    }

    #[test]
    fn test_mixed_only_required_missing() {
        let schema = Schema::from_shape(ArgsWithMixedSettings::SHAPE).unwrap();
        // Only optional fields provided
        let value = cv_object([(
            "settings",
            cv_object([
                ("optional_str", cv_string("opt")),
                ("optional_int", cv_int(42)),
            ]),
        )]);

        let mut missing = Vec::new();
        collect_missing_fields(&value, &schema, &mut missing);

        assert_eq!(
            missing.len(),
            2,
            "Expected 2 missing required fields, got: {:?}",
            missing
        );
        let names: Vec<_> = missing.iter().map(|m| m.field_name.as_str()).collect();
        assert!(names.contains(&"required_str"));
        assert!(names.contains(&"required_int"));
    }

    #[test]
    fn test_mixed_all_required_present() {
        let schema = Schema::from_shape(ArgsWithMixedSettings::SHAPE).unwrap();
        // Only required fields provided (optional ones absent)
        let value = cv_object([(
            "settings",
            cv_object([
                ("required_str", cv_string("req")),
                ("required_int", cv_int(123)),
            ]),
        )]);

        let mut missing = Vec::new();
        collect_missing_fields(&value, &schema, &mut missing);

        assert!(
            missing.is_empty(),
            "All required fields present, should have no missing: {:?}",
            missing
        );
    }

    // ========================================================================
    // Tests: No config section (CLI args only)
    // ========================================================================

    #[derive(Facet)]
    struct ArgsWithoutConfig {
        #[facet(args::named)]
        verbose: bool,
    }

    #[test]
    fn test_no_config_section() {
        let schema = Schema::from_shape(ArgsWithoutConfig::SHAPE).unwrap();
        let value = cv_object([("verbose", cv_bool(true))]);

        let mut missing = Vec::new();
        collect_missing_fields(&value, &schema, &mut missing);

        assert!(
            missing.is_empty(),
            "No config section means no missing config fields: {:?}",
            missing
        );
    }

    // ========================================================================
    // Tests: Required CLI args
    // ========================================================================

    #[derive(Facet)]
    struct ArgsWithRequiredCliArg {
        #[facet(args::named)]
        required_field: String,
    }

    #[test]
    fn test_missing_required_cli_arg() {
        let schema = Schema::from_shape(ArgsWithRequiredCliArg::SHAPE).unwrap();
        let value = cv_object([]);

        let mut missing = Vec::new();
        collect_missing_fields(&value, &schema, &mut missing);

        assert_eq!(
            missing.len(),
            1,
            "Expected 1 missing field, got: {:?}",
            missing
        );
        assert_eq!(missing[0].field_name, "required_field");
    }

    #[test]
    fn test_required_cli_arg_present() {
        let schema = Schema::from_shape(ArgsWithRequiredCliArg::SHAPE).unwrap();
        let value = cv_object([("required_field", cv_string("value"))]);

        let mut missing = Vec::new();
        collect_missing_fields(&value, &schema, &mut missing);

        assert!(
            missing.is_empty(),
            "Required CLI arg present, should have no missing: {:?}",
            missing
        );
    }

    // ========================================================================
    // Tests: Type name reporting
    // ========================================================================

    #[test]
    fn test_type_name_for_string() {
        let schema = Schema::from_shape(ArgsWithConfig::SHAPE).unwrap();
        let value = cv_object([("config", cv_object([("port", cv_int(8080))]))]);

        let mut missing = Vec::new();
        collect_missing_fields(&value, &schema, &mut missing);

        assert_eq!(missing.len(), 1);
        assert!(!missing[0].type_name.is_empty(), "Type name should be set");
    }

    #[test]
    fn test_type_name_for_integer() {
        let schema = Schema::from_shape(ArgsWithSettings::SHAPE).unwrap();
        let value = cv_object([("settings", cv_object([("host", cv_string("localhost"))]))]);

        let mut missing = Vec::new();
        collect_missing_fields(&value, &schema, &mut missing);

        assert_eq!(missing.len(), 1);
        assert!(!missing[0].type_name.is_empty(), "Type name should be set");
    }

    #[test]
    fn test_normalize_program_name() {
        // Test with cargo test binary pattern and full path
        assert_eq!(
            normalize_program_name(
                "/Users/amos/bearcove/figue/target/debug/deps/main-138217976bbdb088"
            ),
            "main"
        );
        assert_eq!(
            normalize_program_name(
                "/home/runner/work/figue/figue/target/debug/deps/main-b36e7ccd11ac5f87"
            ),
            "main"
        );

        // Test with just binary name and hash
        assert_eq!(normalize_program_name("main-138217976bbdb088"), "main");
        assert_eq!(normalize_program_name("main-b36e7ccd11ac5f87"), "main");

        // Test with regular binary names (no hash)
        assert_eq!(normalize_program_name("myapp"), "myapp");
        assert_eq!(normalize_program_name("my-app"), "my-app");
        assert_eq!(normalize_program_name("/usr/local/bin/myapp"), "myapp");

        // Test with names that have dashes but not the hash pattern
        assert_eq!(normalize_program_name("my-app-test"), "my-app-test");
        assert_eq!(normalize_program_name("app-v1-final"), "app-v1-final");

        // Test with hash that's too short
        assert_eq!(normalize_program_name("app-123abc"), "app-123abc");

        // Test with hash that's too long
        assert_eq!(
            normalize_program_name("app-123456789abcdef01"),
            "app-123456789abcdef01"
        );

        // Test with non-hex characters
        assert_eq!(
            normalize_program_name("app-123456789abcdexg"),
            "app-123456789abcdexg"
        );
    }
}
