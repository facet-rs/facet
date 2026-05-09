//! Help text generation for command-line interfaces.
//!
//! This module provides utilities to generate help text from Schema,
//! including doc comments, field names, and attribute information.

use crate::missing::normalize_program_name;
use crate::schema::{
    ArgLevelSchema, ArgSchema, ConfigFieldSchema, ConfigStructSchema, ConfigValueSchema, Schema,
    Subcommand,
};
use facet_core::Facet;
use heck::ToKebabCase;
use owo_colors::OwoColorize;
use owo_colors::Stream::Stdout;
use std::string::String;
use std::vec::Vec;

const DEFAULT_CONFIG_FILE_EXTENSIONS: &[&str] = &["json"];

/// Generate help text for a Facet type.
///
/// This is a convenience function that builds a Schema internally.
/// If you already have a Schema, use `generate_help_for_subcommand` instead.
pub fn generate_help<T: Facet<'static>>(config: &HelpConfig) -> String {
    generate_help_for_shape(T::SHAPE, config)
}

/// Generate help text from a Shape.
///
/// This is a convenience function that builds a Schema internally.
/// If you already have a Schema, use `generate_help_for_subcommand` instead.
pub fn generate_help_for_shape(shape: &'static facet_core::Shape, config: &HelpConfig) -> String {
    let schema = match Schema::from_shape(shape) {
        Ok(s) => s,
        Err(_) => {
            // Fall back to a minimal help message
            let program_name = config
                .program_name
                .clone()
                .or_else(|| {
                    std::env::args()
                        .next()
                        .map(|path| normalize_program_name(&path))
                })
                .unwrap_or_else(|| "program".to_string());
            return format!(
                "{}\n\n(Schema could not be built for this type)\n",
                program_name
            );
        }
    };

    generate_help_for_subcommand(&schema, &[], config)
}

/// Configuration for help text generation.
#[derive(Debug, Clone)]
pub struct HelpConfig {
    /// Program name (defaults to executable name)
    pub program_name: Option<String>,
    /// Program version
    pub version: Option<String>,
    /// Additional description to show after the auto-generated one
    pub description: Option<String>,
    /// Width for wrapping text (0 = no wrapping)
    pub width: usize,
}

impl Default for HelpConfig {
    fn default() -> Self {
        Self {
            program_name: None,
            version: None,
            description: None,
            width: 80,
        }
    }
}

/// Generate help text for a specific subcommand path from a Schema.
///
/// `subcommand_path` is a list of subcommand names (e.g., `["repo", "clone"]` for `myapp repo clone --help`).
/// This navigates through the schema to find the target subcommand and generates help for it.
pub fn generate_help_for_subcommand(
    schema: &Schema,
    subcommand_path: &[String],
    config: &HelpConfig,
) -> String {
    generate_help_for_subcommand_with_config_formats(
        schema,
        subcommand_path,
        config,
        DEFAULT_CONFIG_FILE_EXTENSIONS,
    )
}

pub(crate) fn generate_help_for_subcommand_with_config_formats(
    schema: &Schema,
    subcommand_path: &[String],
    config: &HelpConfig,
    config_file_extensions: &[&str],
) -> String {
    let program_name = config
        .program_name
        .clone()
        .or_else(|| {
            std::env::args()
                .next()
                .map(|path| normalize_program_name(&path))
        })
        .unwrap_or_else(|| "program".to_string());

    if subcommand_path.is_empty() {
        return generate_help_from_schema(schema, &program_name, config, config_file_extensions);
    }

    // Navigate to the subcommand
    let mut current_args = schema.args();
    let mut command_path = vec![program_name.clone()];

    for name in subcommand_path {
        // The path contains effective names (e.g., "Clone", "rm") from ConfigValue.
        // Look up by effective_name since that's what's stored in the path.
        let sub = current_args
            .subcommands()
            .values()
            .find(|s| s.effective_name() == name);

        if let Some(sub) = sub {
            command_path.push(sub.cli_name().to_string());
            current_args = sub.args();
        } else {
            // Subcommand not found, fall back to root help
            return generate_help_from_schema(
                schema,
                &program_name,
                config,
                config_file_extensions,
            );
        }
    }

    // Find the final subcommand to get its docs
    let mut final_sub: Option<&Subcommand> = None;
    let mut args = schema.args();

    for name in subcommand_path {
        let sub = args
            .subcommands()
            .values()
            .find(|s| s.effective_name() == name);
        if let Some(sub) = sub {
            final_sub = Some(sub);
            args = sub.args();
        }
    }

    generate_help_for_subcommand_level(current_args, final_sub, &command_path.join(" "), config)
}

/// Generate help from a built Schema.
fn generate_help_from_schema(
    schema: &Schema,
    program_name: &str,
    config: &HelpConfig,
    config_file_extensions: &[&str],
) -> String {
    let mut out = String::new();

    // Program name and version
    if let Some(version) = &config.version {
        out.push_str(&format!("{program_name} {version}\n"));
    } else {
        out.push_str(&format!("{program_name}\n"));
    }

    // Type doc comment from schema
    if let Some(summary) = schema.docs().summary() {
        out.push('\n');
        out.push_str(summary.trim());
        out.push('\n');
    }
    if let Some(details) = schema.docs().details() {
        for line in details.lines() {
            out.push_str(line.trim());
            out.push('\n');
        }
    }

    // Additional description
    if let Some(desc) = &config.description {
        out.push('\n');
        out.push_str(desc);
        out.push('\n');
    }

    out.push('\n');

    generate_arg_level_help(
        &mut out,
        schema.args(),
        schema.configs(),
        program_name,
        config,
        config_file_extensions,
    );

    out
}

/// Generate help for a subcommand level.
fn generate_help_for_subcommand_level(
    args: &ArgLevelSchema,
    subcommand: Option<&Subcommand>,
    full_command: &str,
    config: &HelpConfig,
) -> String {
    let mut out = String::new();

    // Header with full command
    out.push_str(&format!("{full_command}\n"));

    // Doc comment for the subcommand
    if let Some(sub) = subcommand {
        if let Some(summary) = sub.docs().summary() {
            out.push('\n');
            out.push_str(summary.trim());
            out.push('\n');
        }
        if let Some(details) = sub.docs().details() {
            for line in details.lines() {
                out.push_str(line.trim());
                out.push('\n');
            }
        }
    }

    // Additional description from config
    if let Some(desc) = &config.description {
        out.push('\n');
        out.push_str(desc);
        out.push('\n');
    }

    out.push('\n');

    generate_arg_level_help(
        &mut out,
        args,
        &[],
        full_command,
        config,
        DEFAULT_CONFIG_FILE_EXTENSIONS,
    );

    out
}

/// Wrap `text` into lines of at most `max_width - indent.len()` characters,
/// prefixing each line with `indent`. When `max_width` is 0 the text is
/// returned on a single line (no wrapping).
fn wrap_text(text: &str, indent: &str, max_width: usize) -> String {
    let available = if max_width == 0 || max_width <= indent.len() {
        // No wrapping or degenerate case – just one line.
        let mut s = indent.to_string();
        s.push_str(text);
        return s;
    } else {
        max_width - indent.len()
    };

    let mut result = String::new();
    let mut line = String::new();

    for word in text.split_whitespace() {
        if line.is_empty() {
            line.push_str(word);
        } else if line.len() + 1 + word.len() <= available {
            line.push(' ');
            line.push_str(word);
        } else {
            result.push_str(indent);
            result.push_str(&line);
            result.push('\n');
            line.clear();
            line.push_str(word);
        }
    }

    if !line.is_empty() {
        result.push_str(indent);
        result.push_str(&line);
    }

    result
}

/// Generate help output for an argument level (args + subcommands).
fn generate_arg_level_help(
    out: &mut String,
    args: &ArgLevelSchema,
    config_roots: &[ConfigStructSchema],
    program_name: &str,
    config: &HelpConfig,
    config_file_extensions: &[&str],
) {
    // Separate positionals and named flags
    let mut positionals: Vec<&ArgSchema> = Vec::new();
    let mut flags: Vec<&ArgSchema> = Vec::new();

    for (_name, arg) in args.args().iter() {
        if arg.kind().is_positional() {
            positionals.push(arg);
        } else {
            flags.push(arg);
        }
    }

    // Usage line
    out.push_str(&format!("{}:\n    ", "USAGE".yellow().bold()));
    out.push_str(program_name);

    if !flags.is_empty() || !config_roots.is_empty() {
        out.push_str(" [OPTIONS]");
    }

    for pos in &positionals {
        let name = pos.name().to_uppercase();
        if pos.required() {
            out.push_str(&format!(" <{name}>"));
        } else {
            out.push_str(&format!(" [{name}]"));
        }
    }

    if args.has_subcommands() {
        if args.subcommand_optional() {
            out.push_str(" [COMMAND]");
        } else {
            out.push_str(" <COMMAND>");
        }
    }

    out.push_str("\n\n");

    // Positional arguments
    if !positionals.is_empty() {
        out.push_str(&format!("{}:\n", "ARGUMENTS".yellow().bold()));
        for arg in &positionals {
            write_arg_help(out, arg, config);
        }
        out.push('\n');
    }

    // Options
    if !flags.is_empty() || !config_roots.is_empty() {
        out.push_str(&format!("{}:\n", "OPTIONS".yellow().bold()));
        for arg in &flags {
            write_arg_help(out, arg, config);
        }
        for config_root in config_roots {
            write_config_help(out, config_root, config, config_file_extensions);
        }
        out.push('\n');
    }

    // Subcommands
    if args.has_subcommands() {
        out.push_str(&format!("{}:\n", "COMMANDS".yellow().bold()));
        for sub in args.subcommands().values() {
            write_subcommand_help(out, sub, config);
        }
        out.push('\n');
    }
}

/// Write help for a config root.
fn write_config_help(
    out: &mut String,
    config_root: &ConfigStructSchema,
    config: &HelpConfig,
    config_file_extensions: &[&str],
) {
    let Some(name) = config_root.field_name() else {
        return;
    };
    let cli_name = name.to_kebab_case();
    let config_flag = format!("--{cli_name}");

    out.push_str("        ");
    out.push_str(&format!(
        "{} <FILE>",
        config_flag.if_supports_color(Stdout, |text| text.green())
    ));
    out.push('\n');
    let file_help = config_root
        .docs()
        .summary()
        .unwrap_or("Load configuration values from a file.");
    out.push_str(&wrap_text(file_help, "            ", config.width));
    out.push('\n');
    out.push_str(&wrap_text(
        &format_config_file_extensions(config_file_extensions),
        "            ",
        config.width,
    ));
    out.push('\n');

    for item in config_override_help_items(&config_flag, config_root) {
        write_config_override_help(out, &item, config);
    }
}

fn format_config_file_extensions(extensions: &[&str]) -> String {
    let mut unique = Vec::new();
    for extension in extensions {
        let extension = extension.trim_start_matches('.');
        if !extension.is_empty()
            && !unique
                .iter()
                .any(|existing: &&str| existing.eq_ignore_ascii_case(extension))
        {
            unique.push(extension);
        }
    }

    if unique.is_empty() {
        return "No config file formats are registered.".to_string();
    }

    let formatted = unique
        .iter()
        .map(|extension| format!(".{extension}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!("Supported file formats: {formatted}.")
}

struct ConfigOverrideHelpItem {
    flag: String,
    placeholder: String,
    help: Option<String>,
}

fn write_config_override_help(
    out: &mut String,
    item: &ConfigOverrideHelpItem,
    config: &HelpConfig,
) {
    out.push_str("        ");
    out.push_str(&format!(
        "{} <{}>",
        item.flag.if_supports_color(Stdout, |text| text.green()),
        item.placeholder
    ));
    out.push('\n');

    if let Some(help) = &item.help {
        out.push_str(&wrap_text(help, "            ", config.width));
        out.push('\n');
    }
}

fn config_override_help_items(
    config_flag: &str,
    config_root: &ConfigStructSchema,
) -> Vec<ConfigOverrideHelpItem> {
    let mut items = Vec::new();
    collect_config_struct_overrides(config_flag, config_root, Vec::new(), &mut items);
    items
}

fn collect_config_struct_overrides(
    config_flag: &str,
    config_struct: &ConfigStructSchema,
    path: Vec<String>,
    items: &mut Vec<ConfigOverrideHelpItem>,
) {
    for (field_name, field) in config_struct.fields() {
        let mut field_path = path.clone();
        field_path.push(field_name.clone());
        collect_config_field_override(config_flag, field_path, field, items);
    }
}

fn collect_config_field_override(
    config_flag: &str,
    path: Vec<String>,
    field: &ConfigFieldSchema,
    items: &mut Vec<ConfigOverrideHelpItem>,
) {
    let help = field.docs().summary().map(str::to_string);
    collect_config_value_overrides(config_flag, path, field.value(), help, items);
}

fn collect_config_value_overrides(
    config_flag: &str,
    path: Vec<String>,
    value: &ConfigValueSchema,
    help: Option<String>,
    items: &mut Vec<ConfigOverrideHelpItem>,
) {
    match value.inner_if_option() {
        ConfigValueSchema::Struct(config_struct) => {
            collect_config_struct_overrides(config_flag, config_struct, path, items);
        }
        ConfigValueSchema::Vec(vec_schema) => {
            let mut element_path = path;
            element_path.push("<INDEX>".to_string());
            collect_config_value_overrides(
                config_flag,
                element_path,
                vec_schema.element(),
                help,
                items,
            );
        }
        ConfigValueSchema::Enum(enum_schema) => {
            let variants: Vec<&str> = enum_schema.variants().keys().map(String::as_str).collect();
            if enum_schema
                .variants()
                .values()
                .any(|variant| variant.fields().is_empty())
            {
                items.push(ConfigOverrideHelpItem {
                    flag: config_override_flag(config_flag, &path),
                    placeholder: variants.join(","),
                    help: help.clone(),
                });
            }

            for (variant_name, variant_schema) in enum_schema.variants() {
                let mut variant_path = path.clone();
                variant_path.push(variant_name.clone());
                for (field_name, field) in variant_schema.fields() {
                    let mut field_path = variant_path.clone();
                    field_path.push(field_name.clone());
                    collect_config_field_override(config_flag, field_path, field, items);
                }
            }
        }
        ConfigValueSchema::Leaf(_) => {
            items.push(ConfigOverrideHelpItem {
                flag: config_override_flag(config_flag, &path),
                placeholder: value.type_identifier().to_uppercase(),
                help,
            });
        }
        ConfigValueSchema::Option { .. } => unreachable!("inner_if_option removes Option wrappers"),
    }
}

fn config_override_flag(config_flag: &str, path: &[String]) -> String {
    format!("{config_flag}.{}", path.join("."))
}

/// Write help for a single argument.
fn write_arg_help(out: &mut String, arg: &ArgSchema, config: &HelpConfig) {
    out.push_str("    ");

    let is_positional = arg.kind().is_positional();

    // Short flag (or spacing for alignment)
    if let Some(c) = arg.kind().short() {
        out.push_str(&format!(
            "{}, ",
            format!("-{c}").if_supports_color(Stdout, |text| text.green())
        ));
    } else {
        // Add spacing to align with flags that have short options
        out.push_str("    ");
    }

    // Long flag or positional name
    let name = arg.name();
    let is_counted = arg.kind().is_counted();

    if is_positional {
        out.push_str(&format!(
            "{}",
            format!("<{}>", name.to_uppercase()).if_supports_color(Stdout, |text| text.green())
        ));
    } else {
        let is_bool = arg.value().inner_if_option().is_bool();
        let flag_str = if is_bool {
            format!("--[no-]{}", name.to_kebab_case())
        } else {
            format!("--{}", name.to_kebab_case())
        };
        out.push_str(&format!(
            "{}",
            flag_str.if_supports_color(Stdout, |text| text.green())
        ));

        // Show value placeholder for non-bool, non-counted types
        if !is_counted && !arg.value().is_bool() {
            let placeholder = if let Some(desc) = arg.label() {
                desc.to_uppercase()
            } else if let Some(variants) = arg.value().inner_if_option().enum_variants() {
                variants.join(",")
            } else {
                arg.value().type_identifier().to_uppercase()
            };
            out.push_str(&format!(" <{}>", placeholder));
        }
    }

    // Doc comment
    const DOC_INDENT: &str = "            ";
    if let Some(summary) = arg.docs().summary() {
        out.push('\n');
        out.push_str(&wrap_text(summary.trim(), DOC_INDENT, config.width));
    }

    if is_counted {
        out.push('\n');
        out.push_str(&wrap_text("[can be repeated]", DOC_INDENT, config.width));
    }

    out.push('\n');
}

/// Write help for a subcommand.
fn write_subcommand_help(out: &mut String, sub: &Subcommand, config: &HelpConfig) {
    out.push_str("    ");

    out.push_str(&format!(
        "{}",
        sub.cli_name()
            .if_supports_color(Stdout, |text| text.green())
    ));

    // Doc comment
    if let Some(summary) = sub.docs().summary() {
        out.push('\n');
        out.push_str(&wrap_text(summary.trim(), "            ", config.width));
    }

    out.push('\n');
}

#[cfg(test)]
mod tests {
    use super::*;
    use facet::Facet;
    use figue_attrs as args;

    /// Common arguments that can be flattened into other structs
    #[derive(Facet)]
    struct CommonArgs {
        /// Enable verbose output
        #[facet(args::named, crate::short = 'v')]
        verbose: bool,

        /// Enable quiet mode
        #[facet(args::named, crate::short = 'q')]
        quiet: bool,
    }

    /// Args struct with flattened common args
    #[derive(Facet)]
    struct ArgsWithFlatten {
        /// Input file
        #[facet(args::positional)]
        input: String,

        /// Common options
        #[facet(flatten)]
        common: CommonArgs,
    }

    #[test]
    fn test_flatten_args_appear_in_help() {
        let schema = Schema::from_shape(ArgsWithFlatten::SHAPE).unwrap();
        let help = generate_help_for_subcommand(&schema, &[], &HelpConfig::default());

        // Flattened fields should appear at top level
        assert!(
            help.contains("--[no-]verbose"),
            "help should contain --[no-]verbose from flattened CommonArgs"
        );
        assert!(help.contains("-v"), "help should contain -v short flag");
        assert!(
            help.contains("--[no-]quiet"),
            "help should contain --[no-]quiet from flattened CommonArgs"
        );
        assert!(help.contains("-q"), "help should contain -q short flag");

        // The flattened field name 'common' should NOT appear as a flag
        assert!(
            !help.contains("--common"),
            "help should not show --common as a flag"
        );
    }

    #[test]
    fn test_flatten_docs_preserved() {
        let schema = Schema::from_shape(ArgsWithFlatten::SHAPE).unwrap();
        let help = generate_help_for_subcommand(&schema, &[], &HelpConfig::default());

        // Doc comments from flattened fields should be present
        assert!(
            help.contains("verbose output"),
            "help should contain verbose field doc"
        );
        assert!(
            help.contains("quiet mode"),
            "help should contain quiet field doc"
        );
    }

    /// Arguments for the serve subcommand
    #[derive(Facet)]
    struct ServeArgs {
        /// Port to serve on
        #[facet(args::named)]
        port: u16,

        /// Host to bind to
        #[facet(args::named)]
        host: String,
    }

    /// Top-level command with tuple variant subcommand
    #[derive(Facet)]
    struct TupleVariantArgs {
        /// Subcommand to run
        #[facet(args::subcommand)]
        command: Option<TupleVariantCommand>,
    }

    /// Command enum with tuple variant
    #[derive(Facet)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum TupleVariantCommand {
        /// Start the server
        Serve(ServeArgs),
    }

    #[test]
    fn test_label_overrides_placeholder() {
        #[derive(Facet)]
        struct TDArgs {
            /// Input path
            #[facet(args::named, args::label = "PATH")]
            input: std::path::PathBuf,
        }
        let schema = Schema::from_shape(TDArgs::SHAPE).unwrap();
        let help = generate_help_for_subcommand(&schema, &[], &HelpConfig::default());
        // Only assert on the placeholder to avoid issues with ANSI color codes around the flag name
        assert!(
            help.contains("<PATH>"),
            "help should use custom label placeholder"
        );
    }

    #[test]
    fn test_tuple_variant_fields_not_shown_as_option() {
        let schema = Schema::from_shape(TupleVariantArgs::SHAPE).unwrap();
        // Path contains effective names (e.g., "Serve" not "serve")
        let help =
            generate_help_for_subcommand(&schema, &["Serve".to_string()], &HelpConfig::default());

        // The inner struct's fields should appear
        assert!(
            help.contains("--port"),
            "help should contain --port from ServeArgs"
        );
        assert!(
            help.contains("--host"),
            "help should contain --host from ServeArgs"
        );

        // The tuple field "0" should NOT appear as --0
        assert!(
            !help.contains("--0"),
            "help should NOT show --0 for tuple variant wrapper field"
        );
        assert!(
            !help.contains("SERVEARGS"),
            "help should NOT show SERVEARGS as an option value"
        );
    }

    #[test]
    fn test_config_roots_appear_in_help() {
        #[derive(Facet)]
        struct Args {
            /// Session configuration
            #[facet(args::config, args::env_prefix = "BEE", rename = "cfg")]
            cfg: SessionConfig,

            /// Evaluation configuration
            #[facet(args::config, args::env_prefix = "BEE_EVAL", rename = "eval")]
            eval: EvalConfig,
        }

        #[derive(Facet)]
        struct SessionConfig {
            /// Session hostname
            #[facet(default = "localhost")]
            host: String,

            /// Labels attached to this session
            tags: Vec<String>,

            server: ServerConfig,
        }

        #[derive(Facet)]
        struct ServerConfig {
            /// Server port
            #[facet(default = 8080)]
            port: u16,
        }

        #[derive(Facet)]
        struct EvalConfig {
            /// Number of evaluation samples
            #[facet(default = 10)]
            samples: u32,
        }

        let schema = Schema::from_shape(Args::SHAPE).unwrap();
        let help = generate_help_for_subcommand(&schema, &[], &HelpConfig::default());
        let help = strip_ansi_escapes::strip_str(&help);

        assert!(help.contains("--cfg <FILE>"));
        assert!(help.contains("Session configuration"));
        assert!(help.contains("Supported file formats: .json."));
        assert!(help.contains("--cfg.host <STRING>"));
        assert!(help.contains("Session hostname"));
        assert!(help.contains("--cfg.tags.<INDEX> <STRING>"));
        assert!(help.contains("Labels attached to this session"));
        assert!(help.contains("--cfg.server.port <U16>"));
        assert!(help.contains("Server port"));
        assert!(help.contains("--eval <FILE>"));
        assert!(help.contains("Evaluation configuration"));
        assert!(help.contains("--eval.samples <U32>"));
        assert!(help.contains("Number of evaluation samples"));
        assert!(!help.contains("--cfg.<KEY>"));
        assert!(!help.contains("--eval.<KEY>"));
    }

    #[test]
    fn test_long_doc_comment_wraps() {
        #[derive(Facet)]
        struct LongDocArgs {
            /// This is a very long description that should definitely be wrapped because it exceeds the default width of eighty columns by quite a bit
            #[facet(args::named)]
            output: String,
        }

        let schema = Schema::from_shape(LongDocArgs::SHAPE).unwrap();
        let config = HelpConfig {
            width: 80,
            ..Default::default()
        };
        let help = generate_help_for_subcommand(&schema, &[], &config);
        eprintln!("{help}");

        // Every doc-comment line should fit within 80 columns
        for line in help.lines() {
            // Strip ANSI escape codes before measuring length
            let plain: String = line
                .chars()
                .fold((String::new(), false), |(mut s, in_esc), c| {
                    if in_esc {
                        if c == 'm' { (s, false) } else { (s, true) }
                    } else if c == '\x1b' {
                        (s, true)
                    } else {
                        s.push(c);
                        (s, false)
                    }
                })
                .0;
            assert!(
                plain.len() <= 80,
                "line exceeds 80 columns ({} chars): {:?}",
                plain.len(),
                plain
            );
        }
    }
}
