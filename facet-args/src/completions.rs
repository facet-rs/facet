//! Shell completion script generation for command-line interfaces.
//!
//! This module generates completion scripts for various shells (bash, zsh, fish)
//! based on Facet type metadata.

use alloc::string::String;
use alloc::vec::Vec;
use facet_core::{Def, Facet, Field, Shape, Type, UserType, Variant};
use heck::ToKebabCase;

/// Supported shells for completion generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shell {
    /// Bash shell
    Bash,
    /// Zsh shell
    Zsh,
    /// Fish shell
    Fish,
}

/// Generate shell completion script for a Facet type.
pub fn generate_completions<T: Facet<'static>>(shell: Shell, program_name: &str) -> String {
    generate_completions_for_shape(T::SHAPE, shell, program_name)
}

/// Generate shell completion script for a shape.
pub fn generate_completions_for_shape(
    shape: &'static Shape,
    shell: Shell,
    program_name: &str,
) -> String {
    match shell {
        Shell::Bash => generate_bash(shape, program_name),
        Shell::Zsh => generate_zsh(shape, program_name),
        Shell::Fish => generate_fish(shape, program_name),
    }
}

// === Bash Completion ===

fn generate_bash(shape: &'static Shape, program_name: &str) -> String {
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
    let (flags, subcommands) = collect_options(shape);

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

fn generate_zsh(shape: &'static Shape, program_name: &str) -> String {
    let mut out = String::new();

    out.push_str(&format!(
        r#"#compdef {program_name}

_{program_name}() {{
    local -a commands
    local -a options

"#
    ));

    let (flags, subcommands) = collect_options(shape);

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

fn generate_fish(shape: &'static Shape, program_name: &str) -> String {
    let mut out = String::new();

    out.push_str(&format!("# Fish completion for {program_name}\n\n"));

    let (flags, subcommands) = collect_options(shape);

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

fn collect_options(shape: &'static Shape) -> (Vec<FlagInfo>, Vec<SubcommandInfo>) {
    let mut flags = Vec::new();
    let mut subcommands = Vec::new();

    match &shape.ty {
        Type::User(UserType::Struct(struct_type)) => {
            for field in struct_type.fields {
                if field.has_attr(Some("args"), "subcommand") {
                    // Collect subcommands from the enum
                    let field_shape = field.shape();
                    let enum_shape = if let Def::Option(opt) = field_shape.def {
                        opt.t
                    } else {
                        field_shape
                    };

                    if let Type::User(UserType::Enum(enum_type)) = enum_shape.ty {
                        for variant in enum_type.variants {
                            subcommands.push(variant_to_subcommand(variant));
                        }
                    }
                } else if !field.has_attr(Some("args"), "positional") {
                    flags.push(field_to_flag(field));
                }
            }
        }
        Type::User(UserType::Enum(enum_type)) => {
            // Top-level enum = subcommands
            for variant in enum_type.variants {
                subcommands.push(variant_to_subcommand(variant));
            }
        }
        _ => {}
    }

    (flags, subcommands)
}

fn field_to_flag(field: &Field) -> FlagInfo {
    let short = field
        .get_attr(Some("args"), "short")
        .and_then(|attr| attr.get_as::<crate::Attr>())
        .and_then(|attr| {
            if let crate::Attr::Short(c) = attr {
                c.or_else(|| field.name.chars().next())
            } else {
                None
            }
        });

    FlagInfo {
        long: field.name.to_kebab_case(),
        short,
        doc: field.doc.first().map(|s| s.trim().to_string()),
    }
}

fn variant_to_subcommand(variant: &Variant) -> SubcommandInfo {
    let name = variant
        .get_builtin_attr("rename")
        .and_then(|attr| attr.get_as::<&str>())
        .map(|s| (*s).to_string())
        .unwrap_or_else(|| variant.name.to_kebab_case());

    SubcommandInfo {
        name,
        doc: variant.doc.first().map(|s| s.trim().to_string()),
    }
}
