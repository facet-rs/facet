//! Help text generation for command-line interfaces.
//!
//! This module provides utilities to generate help text from Facet type metadata,
//! including doc comments, field names, and attribute information.

use alloc::string::String;
use alloc::vec::Vec;
use facet_core::{Def, Facet, Field, Shape, StructKind, Type, UserType, Variant};
use heck::ToKebabCase;
use owo_colors::OwoColorize;

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

/// Generate help text for a Facet type.
pub fn generate_help<T: facet_core::Facet<'static>>(config: &HelpConfig) -> String {
    generate_help_for_shape(T::SHAPE, config)
}

/// Generate help text for a shape.
pub fn generate_help_for_shape(shape: &'static Shape, config: &HelpConfig) -> String {
    let mut out = String::new();

    // Program name and version
    let program_name = config
        .program_name
        .clone()
        .or_else(|| std::env::args().next())
        .unwrap_or_else(|| "program".to_string());

    if let Some(version) = &config.version {
        out.push_str(&format!("{program_name} {version}\n"));
    } else {
        out.push_str(&format!("{program_name}\n"));
    }

    // Type doc comment
    if !shape.doc.is_empty() {
        out.push('\n');
        for line in shape.doc {
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

    // Generate based on type
    match &shape.ty {
        Type::User(UserType::Struct(struct_type)) => {
            generate_struct_help(&mut out, &program_name, struct_type.fields);
        }
        Type::User(UserType::Enum(enum_type)) => {
            generate_enum_help(&mut out, &program_name, enum_type.variants);
        }
        _ => {
            out.push_str("(No help available for this type)\n");
        }
    }

    out
}

fn generate_struct_help(out: &mut String, program_name: &str, fields: &'static [Field]) {
    // Collect flags, positionals, and subcommand
    let mut flags: Vec<&Field> = Vec::new();
    let mut positionals: Vec<&Field> = Vec::new();
    let mut subcommand: Option<&Field> = None;

    for field in fields {
        if field.has_attr(Some("args"), "subcommand") {
            subcommand = Some(field);
        } else if field.has_attr(Some("args"), "positional") {
            positionals.push(field);
        } else {
            flags.push(field);
        }
    }

    // Usage line
    out.push_str(&format!("{}:\n    ", "USAGE".yellow().bold()));
    out.push_str(program_name);

    if !flags.is_empty() {
        out.push_str(" [OPTIONS]");
    }

    for pos in &positionals {
        let name = pos.name.to_kebab_case().to_uppercase();
        let is_optional = matches!(pos.shape().def, Def::Option(_)) || pos.has_default();
        if is_optional {
            out.push_str(&format!(" [{name}]"));
        } else {
            out.push_str(&format!(" <{name}>"));
        }
    }

    if let Some(sub) = subcommand {
        let is_optional = matches!(sub.shape().def, Def::Option(_));
        if is_optional {
            out.push_str(" [COMMAND]");
        } else {
            out.push_str(" <COMMAND>");
        }
    }

    out.push_str("\n\n");

    // Positional arguments
    if !positionals.is_empty() {
        out.push_str(&format!("{}:\n", "ARGUMENTS".yellow().bold()));
        for field in &positionals {
            write_field_help(out, field, true);
        }
        out.push('\n');
    }

    // Options
    if !flags.is_empty() {
        out.push_str(&format!("{}:\n", "OPTIONS".yellow().bold()));
        for field in &flags {
            write_field_help(out, field, false);
        }
        out.push('\n');
    }

    // Subcommands
    if let Some(sub_field) = subcommand {
        let sub_shape = sub_field.shape();
        // Handle Option<Enum> or direct Enum
        let enum_shape = if let Def::Option(opt) = sub_shape.def {
            opt.t
        } else {
            sub_shape
        };

        if let Type::User(UserType::Enum(enum_type)) = enum_shape.ty {
            out.push_str(&format!("{}:\n", "COMMANDS".yellow().bold()));
            for variant in enum_type.variants {
                write_variant_help(out, variant);
            }
            out.push('\n');
        }
    }
}

fn generate_enum_help(out: &mut String, program_name: &str, variants: &'static [Variant]) {
    // For top-level enum, show subcommands
    out.push_str(&format!("{}:\n    ", "USAGE".yellow().bold()));
    out.push_str(program_name);
    out.push_str(" <COMMAND>\n\n");

    out.push_str(&format!("{}:\n", "COMMANDS".yellow().bold()));
    for variant in variants {
        write_variant_help(out, variant);
    }
    out.push('\n');
}

fn write_field_help(out: &mut String, field: &Field, is_positional: bool) {
    out.push_str("    ");

    // Short flag
    let short = get_short_flag(field);
    if let Some(c) = short {
        out.push_str(&format!("{}, ", format!("-{c}").green()));
    } else {
        out.push_str("    ");
    }

    // Long flag or positional name
    let kebab_name = field.name.to_kebab_case();
    if is_positional {
        out.push_str(&format!(
            "{}",
            format!("<{}>", kebab_name.to_uppercase()).green()
        ));
    } else {
        out.push_str(&format!("{}", format!("--{kebab_name}").green()));

        // Show value placeholder for non-bool types
        let shape = field.shape();
        if !shape.is_shape(bool::SHAPE) {
            out.push_str(&format!(" <{}>", shape.type_identifier.to_uppercase()));
        }
    }

    // Doc comment
    if let Some(doc) = field.doc.first() {
        out.push_str("\n            ");
        out.push_str(doc.trim());
    }

    out.push('\n');
}

fn write_variant_help(out: &mut String, variant: &Variant) {
    out.push_str("    ");

    // Variant name (check for rename)
    let name = variant
        .get_builtin_attr("rename")
        .and_then(|attr| attr.get_as::<&str>())
        .map(|s| (*s).to_string())
        .unwrap_or_else(|| variant.name.to_kebab_case());

    out.push_str(&format!("{}", name.green()));

    // Doc comment
    if let Some(doc) = variant.doc.first() {
        out.push_str("\n            ");
        out.push_str(doc.trim());
    }

    out.push('\n');
}

/// Get the short flag character for a field, if any
fn get_short_flag(field: &Field) -> Option<char> {
    field
        .get_attr(Some("args"), "short")
        .and_then(|attr| attr.get_as::<crate::Attr>())
        .and_then(|attr| {
            if let crate::Attr::Short(c) = attr {
                // If explicit char provided, use it; otherwise use first char of field name
                c.or_else(|| field.name.chars().next())
            } else {
                None
            }
        })
}

/// Generate help for a specific subcommand variant.
pub fn generate_subcommand_help(
    variant: &'static Variant,
    parent_program: &str,
    config: &HelpConfig,
) -> String {
    let mut out = String::new();

    let variant_name = variant
        .get_builtin_attr("rename")
        .and_then(|attr| attr.get_as::<&str>())
        .map(|s| (*s).to_string())
        .unwrap_or_else(|| variant.name.to_kebab_case());

    let full_name = format!("{parent_program} {variant_name}");

    // Header
    if let Some(version) = &config.version {
        out.push_str(&format!("{full_name} {version}\n"));
    } else {
        out.push_str(&format!("{full_name}\n"));
    }

    // Variant doc comment
    if !variant.doc.is_empty() {
        out.push('\n');
        for line in variant.doc {
            out.push_str(line.trim());
            out.push('\n');
        }
    }

    out.push('\n');

    // Generate help for variant fields
    // Handle tuple variant with single struct field (newtype pattern)
    // e.g., `Build(BuildArgs)` should flatten BuildArgs fields
    // This matches clap's behavior: "automatically flattened with a tuple-variant"
    let fields = variant.data.fields;
    if variant.data.kind == StructKind::TupleStruct && fields.len() == 1 {
        let inner_shape = fields[0].shape();
        if let Type::User(UserType::Struct(struct_type)) = inner_shape.ty {
            // Use the inner struct's fields instead of the tuple field
            generate_struct_help(&mut out, &full_name, struct_type.fields);
            return out;
        }
    }

    generate_struct_help(&mut out, &full_name, fields);

    out
}
