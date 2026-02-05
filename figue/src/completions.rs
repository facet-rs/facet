//! Shell completion script generation for command-line interfaces.
//!
//! This module generates completion scripts for various shells (bash, zsh, fish)
//! based on Schema metadata built from Facet types.

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

/// Generate shell completion script for a shape.
///
/// This is a convenience function that builds a Schema internally.
/// If you already have a Schema, use [generate_completions_for_schema] instead.
pub(crate) fn generate_completions_for_shape(
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

/// Generate shell completion script from a [Schema].
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

    // Generate the main completion function
    generate_bash_function(&mut out, args, program_name, &[]);

    // Generate subcommand functions recursively
    generate_bash_subcommand_functions(&mut out, args, program_name, &[]);

    // Register the completion
    out.push_str(&format!("complete -F _{program_name} {program_name}\n"));

    out
}

fn generate_bash_function(
    out: &mut String,
    args: &ArgLevelSchema,
    program_name: &str,
    path: &[&str],
) {
    let func_name = if path.is_empty() {
        program_name.to_string()
    } else {
        format!("{}_{}", program_name, path.join("_").replace('-', "_"))
    };

    let (flags, subcommands) = collect_options(args);

    out.push_str(&format!(
        r#"_{func_name}() {{
    local cur prev words cword
    _init_completion || return

"#
    ));

    // Build flags string
    if !flags.is_empty() {
        out.push_str("    local flags=\"");
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
    } else {
        out.push_str("    local flags=\"\"\n");
    }

    // Build commands string
    if !subcommands.is_empty() {
        out.push_str("    local commands=\"");
        for (i, cmd) in subcommands.iter().enumerate() {
            if i > 0 {
                out.push(' ');
            }
            out.push_str(&cmd.name);
        }
        out.push_str("\"\n");
    } else {
        out.push_str("    local commands=\"\"\n");
    }

    // Handle flags that take values
    let value_flags: Vec<_> = flags.iter().filter(|f| f.takes_value).collect();
    if !value_flags.is_empty() {
        out.push_str("\n    case \"$prev\" in\n");
        for flag in &value_flags {
            let mut cases = vec![format!("--{}", flag.long)];
            if let Some(short) = flag.short {
                cases.push(format!("-{short}"));
            }
            out.push_str(&format!(
                "        {})\n            # Value expected, provide default file completion\n            return\n            ;;\n",
                cases.join("|")
            ));
        }
        out.push_str("    esac\n");
    }

    if !subcommands.is_empty() {
        // Find the subcommand position and dispatch
        out.push_str(&format!(
            r#"
    # Find the subcommand
    local cmd_idx={}
    local cmd=""
    for ((i=1; i < cword; i++)); do
        case "${{words[i]}}" in
            -*)
                # Skip flags
"#,
            path.len() + 1
        ));

        // Skip flags that take values (they consume the next argument)
        if !value_flags.is_empty() {
            out.push_str("                case \"${words[i]}\" in\n");
            for flag in &value_flags {
                let mut cases = vec![format!("--{}", flag.long)];
                if let Some(short) = flag.short {
                    cases.push(format!("-{short}"));
                }
                out.push_str(&format!(
                    "                    {}) ((i++)) ;;\n",
                    cases.join("|")
                ));
            }
            out.push_str("                esac\n");
        }

        out.push_str(
            r#"                ;;
            *)
                cmd="${words[i]}"
                cmd_idx=$i
                break
                ;;
        esac
    done

    # If we're past the subcommand, dispatch to subcommand completer
    if [[ -n "$cmd" && $cword -gt $cmd_idx ]]; then
        case "$cmd" in
"#,
        );

        for cmd in &subcommands {
            let sub_func = if path.is_empty() {
                format!("{}_{}", program_name, cmd.name.replace('-', "_"))
            } else {
                format!("{}_{}", func_name, cmd.name.replace('-', "_"))
            };
            out.push_str(&format!(
                "            {})\n                _{sub_func}\n                return\n                ;;\n",
                cmd.name
            ));
        }

        out.push_str(
            r#"        esac
    fi

    # Complete flags or subcommands
    if [[ "$cur" == -* ]]; then
        COMPREPLY=($(compgen -W "$flags" -- "$cur"))
    else
        COMPREPLY=($(compgen -W "$commands" -- "$cur"))
    fi
}

"#,
        );
    } else {
        // No subcommands, just complete flags
        out.push_str(
            r#"
    if [[ "$cur" == -* ]]; then
        COMPREPLY=($(compgen -W "$flags" -- "$cur"))
    fi
}

"#,
        );
    }
}

fn generate_bash_subcommand_functions(
    out: &mut String,
    args: &ArgLevelSchema,
    program_name: &str,
    path: &[&str],
) {
    for (_, sub) in args.subcommands() {
        let mut new_path = path.to_vec();
        new_path.push(sub.cli_name());
        generate_bash_function(out, sub.args(), program_name, &new_path);
        generate_bash_subcommand_functions(out, sub.args(), program_name, &new_path);
    }
}

// === Zsh Completion ===

fn generate_zsh(args: &ArgLevelSchema, program_name: &str) -> String {
    let mut out = String::new();

    // Header with compdef directive
    out.push_str(&format!("#compdef {program_name}\n\n"));

    // Generate the main completion function
    generate_zsh_function(&mut out, args, program_name, program_name);

    // Generate subcommand helper functions
    generate_zsh_subcommand_helpers(&mut out, args, program_name);

    // Footer: works for both autoload (fpath) and inline sourcing (eval/source)
    out.push_str(&format!(
        r#"
if [ "$funcstack[1]" = "_{program_name}" ]; then
    _{program_name} "$@"
else
    compdef _{program_name} {program_name}
fi
"#
    ));

    out
}

fn generate_zsh_function(
    out: &mut String,
    args: &ArgLevelSchema,
    func_name: &str,
    _program_name: &str,
) {
    out.push_str(&format!(
        r#"_{func_name}() {{
    local -a options
    local -a commands
    local ret=1

"#
    ));

    let (flags, subcommands) = collect_options(args);

    // Build options array with proper zsh _arguments format
    out.push_str("    options=(\n");
    for flag in &flags {
        let desc = flag.doc.as_deref().unwrap_or("");
        let escaped_desc = escape_zsh_description(desc);

        // For flags that take values, add the value placeholder
        let value_spec = if flag.takes_value {
            ":value:_default"
        } else {
            ""
        };

        if let Some(short) = flag.short {
            // Short and long are mutually exclusive in completion
            out.push_str(&format!(
                "        '(-{short} --{long})'-{short}'[{escaped_desc}]{value_spec}'\n",
                long = flag.long,
            ));
            out.push_str(&format!(
                "        '(-{short} --{long})'--{long}'[{escaped_desc}]{value_spec}'\n",
                long = flag.long,
            ));
        } else {
            out.push_str(&format!(
                "        '--{long}[{escaped_desc}]{value_spec}'\n",
                long = flag.long,
            ));
        }
    }
    out.push_str("    )\n\n");

    if !subcommands.is_empty() {
        // Build commands array
        out.push_str("    commands=(\n");
        for cmd in &subcommands {
            let desc = cmd.doc.as_deref().unwrap_or("");
            let escaped_desc = escape_zsh_description(desc);
            out.push_str(&format!(
                "        '{name}:{escaped_desc}'\n",
                name = cmd.name
            ));
        }
        out.push_str("    )\n\n");

        // Use _arguments with state machine for subcommands
        out.push_str(&format!(
            r#"    _arguments -C \
        $options \
        "1: :->command" \
        "*::arg:->args" \
        && ret=0

    case $state in
        command)
            _describe -t commands '{func_name} commands' commands && ret=0
            ;;
        args)
            case $words[1] in
"#
        ));

        // Add case for each subcommand
        for cmd in &subcommands {
            let sub_func = format!("{}_{}", func_name, cmd.name.replace('-', "_"));
            out.push_str(&format!(
                "                {name})\n                    _{sub_func} && ret=0\n                    ;;\n",
                name = cmd.name,
            ));
        }

        out.push_str(
            r#"            esac
            ;;
    esac

    return ret
}

"#,
        );
    } else {
        // No subcommands, just complete options
        out.push_str(
            r#"    _arguments $options && ret=0
    return ret
}

"#,
        );
    }
}

fn generate_zsh_subcommand_helpers(out: &mut String, args: &ArgLevelSchema, parent_func: &str) {
    for (_, sub) in args.subcommands() {
        let func_name = format!("{}_{}", parent_func, sub.cli_name().replace('-', "_"));
        generate_zsh_function(out, sub.args(), &func_name, parent_func);
        // Recurse for nested subcommands
        generate_zsh_subcommand_helpers(out, sub.args(), &func_name);
    }
}

fn escape_zsh_description(s: &str) -> String {
    s.replace('\'', "'\\''")
        .replace('[', "\\[")
        .replace(']', "\\]")
        .replace(':', "\\:")
}

// === Fish Completion ===

fn generate_fish(args: &ArgLevelSchema, program_name: &str) -> String {
    let mut out = String::new();

    out.push_str(&format!("# Fish completion for {program_name}\n\n"));

    // Generate completions for top level
    generate_fish_level(&mut out, args, program_name, &[]);

    // Generate completions for subcommands recursively
    generate_fish_subcommands(&mut out, args, program_name, &[]);

    out
}

fn generate_fish_level(out: &mut String, args: &ArgLevelSchema, program_name: &str, path: &[&str]) {
    let (flags, subcommands) = collect_options(args);

    // Build the condition for this level
    let condition = if path.is_empty() {
        "__fish_use_subcommand".to_string()
    } else {
        // Check that we're in this subcommand context
        let seen_checks: Vec<String> = path
            .iter()
            .map(|cmd| format!("__fish_seen_subcommand_from {cmd}"))
            .collect();
        seen_checks.join("; and ")
    };

    // Add comment for this level
    if !path.is_empty() {
        out.push_str(&format!("\n# {} subcommand\n", path.join(" ")));
    }

    // Add flag completions for this level
    for flag in &flags {
        let desc = flag.doc.as_deref().unwrap_or("");
        out.push_str(&format!("complete -c {program_name}"));

        // Add condition if we're in a subcommand
        if !path.is_empty() {
            out.push_str(&format!(" -n '{condition}'"));
        }

        if let Some(short) = flag.short {
            out.push_str(&format!(" -s {short}"));
        }
        out.push_str(&format!(" -l {}", flag.long));

        // If flag takes a value, require an argument
        if flag.takes_value {
            out.push_str(" -r");
        }

        if !desc.is_empty() {
            let escaped_desc = desc.replace('\'', "'\\''");
            out.push_str(&format!(" -d '{escaped_desc}'"));
        }
        out.push('\n');
    }

    // Add subcommand completions
    if !subcommands.is_empty() {
        out.push_str(&format!(
            "\n# {prefix}subcommands\n",
            prefix = if path.is_empty() { "" } else { "Nested " }
        ));

        // Build condition that no subcommand of THIS level has been seen yet
        let sub_names: Vec<&str> = subcommands.iter().map(|s| s.name.as_str()).collect();
        let no_sub_condition = if path.is_empty() {
            "__fish_use_subcommand".to_string()
        } else {
            format!(
                "{}; and not __fish_seen_subcommand_from {}",
                condition,
                sub_names.join(" ")
            )
        };

        for cmd in &subcommands {
            let desc = cmd.doc.as_deref().unwrap_or("");
            out.push_str(&format!(
                "complete -c {program_name} -n '{no_sub_condition}' -f -a {name}",
                name = cmd.name
            ));
            if !desc.is_empty() {
                let escaped_desc = desc.replace('\'', "'\\''");
                out.push_str(&format!(" -d '{escaped_desc}'"));
            }
            out.push('\n');
        }
    }
}

fn generate_fish_subcommands(
    out: &mut String,
    args: &ArgLevelSchema,
    program_name: &str,
    path: &[&str],
) {
    for (_, sub) in args.subcommands() {
        let mut new_path = path.to_vec();
        new_path.push(sub.cli_name());
        generate_fish_level(out, sub.args(), program_name, &new_path);
        generate_fish_subcommands(out, sub.args(), program_name, &new_path);
    }
}

// === Helper types and functions ===

struct FlagInfo {
    long: String,
    short: Option<char>,
    doc: Option<String>,
    takes_value: bool,
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
    // Determine if this flag takes a value (not a boolean flag)
    let takes_value = !arg.value().inner_if_option().is_bool();

    FlagInfo {
        // Use kebab-case for the CLI flag name
        long: name.to_kebab_case(),
        short: arg.kind().short(),
        doc: arg.docs().summary().map(|s| s.trim().to_string()),
        takes_value,
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

    /// Git-like CLI with nested subcommands for testing
    #[derive(Facet)]
    #[allow(dead_code)]
    struct GitLikeArgs {
        /// Show version information
        #[facet(args::named)]
        version: bool,

        /// Git command to run
        #[facet(args::subcommand)]
        command: Option<GitCommand>,
    }

    #[derive(Facet)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum GitCommand {
        /// Clone a repository
        Clone {
            /// The repository URL to clone
            #[facet(args::positional)]
            url: String,

            /// Clone only the specified branch
            #[facet(default, args::named, args::short = 'b')]
            branch: Option<String>,
        },
        /// Manage remotes
        Remote {
            /// Remote action to perform
            #[facet(args::subcommand)]
            action: RemoteAction,
        },
    }

    #[derive(Facet)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum RemoteAction {
        /// Add a remote
        Add {
            /// Name of the remote
            #[facet(args::positional)]
            name: String,
        },
        /// Remove a remote
        #[facet(rename = "rm")]
        Remove {
            /// Name of the remote
            #[facet(args::positional)]
            name: String,
        },
    }

    #[test]
    fn test_zsh_inline_sourcing_format() {
        let schema = Schema::from_shape(ArgsWithFlatten::SHAPE).unwrap();
        let completions = generate_completions_for_schema(&schema, Shell::Zsh, "myapp");

        // Should have the inline-compatible footer
        assert!(
            completions.contains("compdef _myapp myapp"),
            "zsh completions should have compdef for inline sourcing"
        );
        assert!(
            completions.contains(r#"if [ "$funcstack[1]" = "_myapp" ]"#),
            "zsh completions should detect autoload vs inline sourcing"
        );
    }

    #[test]
    fn test_nested_subcommands_zsh() {
        let schema = Schema::from_shape(GitLikeArgs::SHAPE).unwrap();
        let completions = generate_completions_for_schema(&schema, Shell::Zsh, "git");

        // Should have top-level subcommands
        assert!(
            completions.contains("'clone:"),
            "zsh completions should have clone subcommand"
        );
        assert!(
            completions.contains("'remote:"),
            "zsh completions should have remote subcommand"
        );

        // Should have nested subcommand function for 'remote'
        assert!(
            completions.contains("_git_remote()"),
            "zsh completions should generate function for remote subcommand"
        );

        // Should have nested subcommand 'rm' (renamed from Remove)
        assert!(
            completions.contains("'rm:"),
            "zsh completions should have 'rm' subcommand (renamed from Remove)"
        );
    }

    #[test]
    fn test_nested_subcommands_bash() {
        let schema = Schema::from_shape(GitLikeArgs::SHAPE).unwrap();
        let completions = generate_completions_for_schema(&schema, Shell::Bash, "git");

        // Should have top-level subcommand handling
        assert!(
            completions.contains("clone"),
            "bash completions should have clone subcommand"
        );
        assert!(
            completions.contains("remote"),
            "bash completions should have remote subcommand"
        );

        // Should generate subcommand completion functions
        assert!(
            completions.contains("_git_remote()"),
            "bash completions should generate function for remote subcommand"
        );

        // Nested subcommand 'rm' should appear
        assert!(
            completions.contains("rm"),
            "bash completions should have 'rm' subcommand"
        );
    }

    #[test]
    fn test_nested_subcommands_fish() {
        let schema = Schema::from_shape(GitLikeArgs::SHAPE).unwrap();
        let completions = generate_completions_for_schema(&schema, Shell::Fish, "git");

        // Should have top-level subcommands
        assert!(
            completions.contains("-a clone"),
            "fish completions should have clone subcommand"
        );
        assert!(
            completions.contains("-a remote"),
            "fish completions should have remote subcommand"
        );

        // Should have nested subcommand handling
        assert!(
            completions.contains("__fish_seen_subcommand_from remote"),
            "fish completions should handle remote subcommand context"
        );

        // Nested 'rm' subcommand
        assert!(
            completions.contains("-a rm"),
            "fish completions should have 'rm' subcommand"
        );
    }

    #[test]
    fn test_value_flags_distinguished_from_bool_flags() {
        let schema = Schema::from_shape(ArgsWithRename::SHAPE).unwrap();
        let completions = generate_completions_for_schema(&schema, Shell::Zsh, "myapp");

        // Bool flag (debug-mode) should NOT have :value:_default
        // The debug field is bool
        assert!(
            !completions.contains("--debug-mode[Enable debug mode]:value"),
            "bool flag should not require value"
        );

        // String flag (out) SHOULD have :value:_default
        assert!(
            completions.contains("--out[Set output file]:value:_default"),
            "string flag should require value"
        );
    }
}
