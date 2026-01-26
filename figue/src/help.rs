//! Help text generation for command-line interfaces.
//!
//! This module provides utilities to generate help text from Schema,
//! including doc comments, field names, and attribute information.

use facet_core::Facet;
use owo_colors::OwoColorize;
use std::string::String;
use std::vec::Vec;

use crate::schema::{ArgLevelSchema, ArgSchema, Schema, Subcommand};

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
                .or_else(|| std::env::args().next())
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
    let program_name = config
        .program_name
        .clone()
        .or_else(|| std::env::args().next())
        .unwrap_or_else(|| "program".to_string());

    if subcommand_path.is_empty() {
        return generate_help_from_schema(schema, &program_name, config);
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
            return generate_help_from_schema(schema, &program_name, config);
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
fn generate_help_from_schema(schema: &Schema, program_name: &str, config: &HelpConfig) -> String {
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

    generate_arg_level_help(&mut out, schema.args(), program_name);

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

    generate_arg_level_help(&mut out, args, full_command);

    out
}

/// Generate help output for an argument level (args + subcommands).
fn generate_arg_level_help(out: &mut String, args: &ArgLevelSchema, program_name: &str) {
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

    if !flags.is_empty() {
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
            write_arg_help(out, arg);
        }
        out.push('\n');
    }

    // Options
    if !flags.is_empty() {
        out.push_str(&format!("{}:\n", "OPTIONS".yellow().bold()));
        for arg in &flags {
            write_arg_help(out, arg);
        }
        out.push('\n');
    }

    // Subcommands
    if args.has_subcommands() {
        out.push_str(&format!("{}:\n", "COMMANDS".yellow().bold()));
        for sub in args.subcommands().values() {
            write_subcommand_help(out, sub);
        }
        out.push('\n');
    }
}

/// Write help for a single argument.
fn write_arg_help(out: &mut String, arg: &ArgSchema) {
    out.push_str("    ");

    let is_positional = arg.kind().is_positional();

    // Short flag (or spacing for alignment)
    if let Some(c) = arg.kind().short() {
        out.push_str(&format!("{}, ", format!("-{c}").green()));
    } else {
        // Add spacing to align with flags that have short options
        out.push_str("    ");
    }

    // Long flag or positional name
    let name = arg.name();
    let is_counted = arg.kind().is_counted();

    if is_positional {
        out.push_str(&format!("{}", format!("<{}>", name.to_uppercase()).green()));
    } else {
        out.push_str(&format!("{}", format!("--{name}").green()));

        // Show value placeholder for non-bool, non-counted types
        if !is_counted && !arg.value().is_bool() {
            out.push_str(&format!(
                " <{}>",
                arg.value().type_identifier().to_uppercase()
            ));
        }
    }

    // Doc comment
    if let Some(summary) = arg.docs().summary() {
        out.push_str("\n            ");
        out.push_str(summary.trim());
    }

    if is_counted {
        out.push_str("\n            ");
        out.push_str("[can be repeated]");
    }

    out.push('\n');
}

/// Write help for a subcommand.
fn write_subcommand_help(out: &mut String, sub: &Subcommand) {
    out.push_str("    ");

    out.push_str(&format!("{}", sub.cli_name().green()));

    // Doc comment
    if let Some(summary) = sub.docs().summary() {
        out.push_str("\n            ");
        out.push_str(summary.trim());
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
            help.contains("--verbose"),
            "help should contain --verbose from flattened CommonArgs"
        );
        assert!(help.contains("-v"), "help should contain -v short flag");
        assert!(
            help.contains("--quiet"),
            "help should contain --quiet from flattened CommonArgs"
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
}
