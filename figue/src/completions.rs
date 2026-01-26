//! Shell completion script generation for command-line interfaces.
//!
//! This module generates completion scripts for various shells (bash, zsh, fish)
//! based on Schema metadata built from Facet types.

use facet_core::Facet;
use heck::ToKebabCase;
use std::string::String;
use std::vec::Vec;

use crate::schema::{ArgLevelSchema, ArgSchema, Schema, Subcommand};

/// Supported shells for completion generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, facet::Facet)]
#[repr(u8)]
pub enum Shell {
    /// Bash shell
    Bash,
    /// Zsh shell
    Zsh,
    /// Fish shell
    Fish,
}

/// Generate shell completion script for a Facet type.
///
/// This is a convenience function that builds a Schema internally.
/// If you already have a Schema, use `generate_completions_for_schema` instead.
pub fn generate_completions<T: Facet<'static>>(shell: Shell, program_name: &str) -> String {
    generate_completions_for_shape(T::SHAPE, shell, program_name)
}

/// Generate shell completion script for a shape.
///
/// This is a convenience function that builds a Schema internally.
/// If you already have a Schema, use `generate_completions_for_schema` instead.
pub fn generate_completions_for_shape(
    shape: &'static facet_core::Shape,
    shell: Shell,
    program_name: &str,
) -> String {
    let schema = match Schema::from_shape(shape) {
        Ok(s) => s,
        Err(_) => {
            // Fall back to a minimal completion script
            return format!("# Could not generate completions for {program_name}\n");
        }
    };

    generate_completions_for_schema(&schema, shell, program_name)
}

/// Generate shell completion script from a Schema.
pub fn generate_completions_for_schema(
    schema: &Schema,
    shell: Shell,
    program_name: &str,
) -> String {
    match shell {
        Shell::Bash => generate_bash(schema.args(), program_name),
        Shell::Zsh => generate_zsh(schema.args(), program_name),
        Shell::Fish => generate_fish(schema.args(), program_name),
    }
}

// === Bash Completion ===

fn generate_bash(args: &ArgLevelSchema, program_name: &str) -> String {
    let mut out = String::new();

    out.push_str(&format!(
        r#"_{program_name}() {{
    local cur prev words cword
    _init_completion || return

    local commands=""
    local flags=""

"#
    ));

    // Collect flags and subcommands
    let (flags, subcommands) = collect_options(args);

    // Add flags
    if !flags.is_empty() {
        out.push_str("    flags=\"");
        for (i, flag) in flags.iter().enumerate() {
            if i > 0 {
                out.push(' ');
            }
            out.push_str(&format!("--{}", flag.long));
            if let Some(short) = flag.short {
                out.push_str(&format!(" -{short}"));
            }
        }
        out.push_str("\"\n");
    }

    // Add subcommands
    if !subcommands.is_empty() {
        out.push_str("    commands=\"");
        for (i, cmd) in subcommands.iter().enumerate() {
            if i > 0 {
                out.push(' ');
            }
            out.push_str(&cmd.name);
        }
        out.push_str("\"\n");
    }

    out.push_str(
        r#"
    case "$prev" in
        # Add cases for flags that take values
        *)
            ;;
    esac

    if [[ "$cur" == -* ]]; then
        COMPREPLY=($(compgen -W "$flags" -- "$cur"))
    elif [[ -n "$commands" ]]; then
        COMPREPLY=($(compgen -W "$commands" -- "$cur"))
    fi
}

"#,
    );

    out.push_str(&format!("complete -F _{program_name} {program_name}\n"));

    out
}

// === Zsh Completion ===

fn generate_zsh(args: &ArgLevelSchema, program_name: &str) -> String {
    let mut out = String::new();

    out.push_str(&format!(
        r#"#compdef {program_name}

_{program_name}() {{
    local -a commands
    local -a options

"#
    ));

    let (flags, subcommands) = collect_options(args);

    // Add options
    out.push_str("    options=(\n");
    for flag in &flags {
        let desc = flag.doc.as_deref().unwrap_or("");
        let escaped_desc = desc.replace('\'', "'\\''");
        if let Some(short) = flag.short {
            out.push_str(&format!("        '-{short}[{escaped_desc}]'\n"));
        }
        out.push_str(&format!("        '--{}[{escaped_desc}]'\n", flag.long));
    }
    out.push_str("    )\n\n");

    // Add subcommands if any
    if !subcommands.is_empty() {
        out.push_str("    commands=(\n");
        for cmd in &subcommands {
            let desc = cmd.doc.as_deref().unwrap_or("");
            let escaped_desc = desc.replace('\'', "'\\''");
            out.push_str(&format!("        '{}:{}'\n", cmd.name, escaped_desc));
        }
        out.push_str("    )\n\n");

        out.push_str(
            r#"    _arguments -C \
        $options \
        "1: :->command" \
        "*::arg:->args"

    case $state in
        command)
            _describe -t commands 'commands' commands
            ;;
        args)
            case $words[1] in
"#,
        );

        // Add cases for each subcommand
        for cmd in &subcommands {
            out.push_str(&format!(
                "                {})\n                    ;;\n",
                cmd.name
            ));
        }

        out.push_str(
            r#"            esac
            ;;
    esac
"#,
        );
    } else {
        out.push_str("    _arguments $options\n");
    }

    out.push_str("}\n\n");
    out.push_str(&format!("_{program_name} \"$@\"\n"));

    out
}

// === Fish Completion ===

fn generate_fish(args: &ArgLevelSchema, program_name: &str) -> String {
    let mut out = String::new();

    out.push_str(&format!("# Fish completion for {program_name}\n\n"));

    let (flags, subcommands) = collect_options(args);

    // Add flag completions
    for flag in &flags {
        let desc = flag.doc.as_deref().unwrap_or("");
        out.push_str(&format!("complete -c {program_name}"));
        if let Some(short) = flag.short {
            out.push_str(&format!(" -s {short}"));
        }
        out.push_str(&format!(" -l {}", flag.long));
        if !desc.is_empty() {
            let escaped_desc = desc.replace('\'', "'\\''");
            out.push_str(&format!(" -d '{escaped_desc}'"));
        }
        out.push('\n');
    }

    // Add subcommand completions
    if !subcommands.is_empty() {
        out.push('\n');
        out.push_str("# Subcommands\n");

        // Disable file completion when expecting a subcommand
        out.push_str(&format!("complete -c {program_name} -f\n"));

        for cmd in &subcommands {
            let desc = cmd.doc.as_deref().unwrap_or("");
            out.push_str(&format!(
                "complete -c {program_name} -n '__fish_use_subcommand' -a {}",
                cmd.name
            ));
            if !desc.is_empty() {
                let escaped_desc = desc.replace('\'', "'\\''");
                out.push_str(&format!(" -d '{escaped_desc}'"));
            }
            out.push('\n');
        }
    }

    out
}

// === Helper types and functions ===

struct FlagInfo {
    long: String,
    short: Option<char>,
    doc: Option<String>,
}

struct SubcommandInfo {
    name: String,
    doc: Option<String>,
}

/// Collect flags and subcommands from an ArgLevelSchema.
///
/// This uses the Schema which already has:
/// - Flattened fields at the correct level
/// - Renames applied (effective names)
fn collect_options(args: &ArgLevelSchema) -> (Vec<FlagInfo>, Vec<SubcommandInfo>) {
    let mut flags = Vec::new();
    let mut subcommands = Vec::new();

    // Collect flags from args (Schema already handles flatten)
    for (name, arg) in args.args() {
        if !arg.kind().is_positional() {
            flags.push(arg_to_flag(name, arg));
        }
    }

    // Collect subcommands (Schema already handles renames via cli_name)
    for sub in args.subcommands().values() {
        subcommands.push(subcommand_to_info(sub));
    }

    (flags, subcommands)
}

/// Convert an ArgSchema to FlagInfo.
fn arg_to_flag(name: &str, arg: &ArgSchema) -> FlagInfo {
    FlagInfo {
        // Use kebab-case for the CLI flag name
        long: name.to_kebab_case(),
        short: arg.kind().short(),
        doc: arg.docs().summary().map(|s| s.trim().to_string()),
    }
}

/// Convert a Subcommand to SubcommandInfo.
fn subcommand_to_info(sub: &Subcommand) -> SubcommandInfo {
    SubcommandInfo {
        // cli_name is already kebab-case and respects renames
        name: sub.cli_name().to_string(),
        doc: sub.docs().summary().map(|s| s.trim().to_string()),
    }
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
    fn test_flatten_args_appear_in_completions() {
        let schema = Schema::from_shape(ArgsWithFlatten::SHAPE).unwrap();
        let completions = generate_completions_for_schema(&schema, Shell::Bash, "myapp");

        // Flattened fields should appear at top level
        assert!(
            completions.contains("--verbose"),
            "completions should contain --verbose from flattened CommonArgs"
        );
        assert!(
            completions.contains("-v"),
            "completions should contain -v short flag"
        );
        assert!(
            completions.contains("--quiet"),
            "completions should contain --quiet from flattened CommonArgs"
        );
        assert!(
            completions.contains("-q"),
            "completions should contain -q short flag"
        );

        // The flattened field name 'common' should NOT appear as a flag
        assert!(
            !completions.contains("--common"),
            "completions should not show --common as a flag"
        );
    }

    /// Test struct with a renamed field
    #[derive(Facet)]
    struct ArgsWithRename {
        /// Enable debug mode
        #[facet(args::named, rename = "debug-mode")]
        debug: bool,

        /// Set output file
        #[facet(args::named, rename = "out")]
        output_file: String,
    }

    #[test]
    fn test_rename_respected_in_completions() {
        let schema = Schema::from_shape(ArgsWithRename::SHAPE).unwrap();
        let completions = generate_completions_for_schema(&schema, Shell::Bash, "myapp");

        // Renamed flags should use the renamed name
        assert!(
            completions.contains("--debug-mode"),
            "completions should contain --debug-mode (renamed from debug)"
        );
        assert!(
            completions.contains("--out"),
            "completions should contain --out (renamed from output_file)"
        );

        // Original names should NOT appear
        assert!(
            !completions.contains("--debug ") && !completions.contains("--debug\n"),
            "completions should not show --debug (was renamed to --debug-mode)"
        );
        assert!(
            !completions.contains("--output-file"),
            "completions should not show --output-file (was renamed to --out)"
        );
    }

    /// Subcommand enum with renamed variant
    #[derive(Facet)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum CommandWithRename {
        /// List all items
        List,
        /// Remove an item
        #[facet(rename = "rm")]
        Remove,
    }

    /// Args with subcommand that has a renamed variant
    #[derive(Facet)]
    struct ArgsWithRenamedSubcommand {
        #[facet(args::subcommand)]
        command: Option<CommandWithRename>,
    }

    #[test]
    fn test_subcommand_rename_respected_in_completions() {
        let schema = Schema::from_shape(ArgsWithRenamedSubcommand::SHAPE).unwrap();
        let completions = generate_completions_for_schema(&schema, Shell::Bash, "myapp");

        // Should use the CLI name (kebab-case of effective name)
        // Bash format is: commands="list rm"
        assert!(
            completions.contains("list"),
            "completions should contain 'list' subcommand"
        );
        assert!(
            completions.contains("rm"),
            "completions should contain 'rm' subcommand (renamed from Remove)"
        );

        // Original name 'remove' should NOT appear (was renamed to 'rm')
        // We need to be careful: "remove" should not appear as a standalone subcommand
        assert!(
            !completions.contains("remove"),
            "completions should not show 'remove' (was renamed to 'rm')"
        );
    }

    #[test]
    fn test_zsh_completions_with_docs() {
        let schema = Schema::from_shape(ArgsWithFlatten::SHAPE).unwrap();
        let completions = generate_completions_for_schema(&schema, Shell::Zsh, "myapp");

        // Doc comments should appear in zsh completions
        assert!(
            completions.contains("verbose output"),
            "zsh completions should include doc for --verbose"
        );
        assert!(
            completions.contains("quiet mode"),
            "zsh completions should include doc for --quiet"
        );
    }

    #[test]
    fn test_fish_completions_with_docs() {
        let schema = Schema::from_shape(ArgsWithFlatten::SHAPE).unwrap();
        let completions = generate_completions_for_schema(&schema, Shell::Fish, "myapp");

        // Doc comments should appear in fish completions
        assert!(
            completions.contains("verbose output"),
            "fish completions should include doc for --verbose"
        );
        assert!(
            completions.contains("quiet mode"),
            "fish completions should include doc for --quiet"
        );
    }
}
