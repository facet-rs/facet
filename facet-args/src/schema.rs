//! Schema representation for CLI arguments and config.
//!
//! This module is under active development and not yet fully wired into the main API.
#![allow(dead_code)]

use std::hash::RandomState;

use facet::Facet;
use facet_core::Shape;
use facet_error as _;
use indexmap::IndexMap;

use crate::path::Path;

pub(crate) mod error;
pub(crate) mod from_schema;

/// A schema "parsed" from a struct. Simple applications will have only
/// top-level arguments, like `--verbose`, etc. and more involved ones will
/// have a `config` field, which contains anything that can be read from a
/// config file and environment variables.
///
/// ```rust,ignore
/// #[derive(Facet)]
/// struct Args {
///   verbose: bool,
///   config: SomeConfigStruct,
/// }
/// ```
#[derive(Facet)]
#[facet(skip_all_unless_truthy)]
pub struct Schema {
    /// Top-level arguments: `--verbose`, etc.
    args: ArgLevelSchema,

    /// Optional config, read from config file, environment
    config: Option<ConfigStructSchema>,
}

/// Schema for one "level" of arguments: top-level, a subcommand, a subcommand's subcommand etc.
#[derive(Facet)]
#[facet(skip_all_unless_truthy)]
pub struct ArgLevelSchema {
    /// Any valid arguments at this level, `--verbose` etc.
    args: IndexMap<String, ArgSchema, RandomState>,

    /// Any subcommands at this level
    subcommands: IndexMap<String, Subcommand, RandomState>,

    /// Name of the field that holds subcommands (e.g., "command" for `#[facet(args::subcommand)] command: Command`).
    /// None if there are no subcommands at this level.
    subcommand_field_name: Option<String>,
}

/// Schema for the `config` part of the schema
#[derive(Facet)]
#[facet(skip_all_unless_truthy)]
pub struct ConfigStructSchema {
    /// Name of the field in the parent struct (e.g., "config" for `#[facet(args::config)] config: ServerConfig`).
    /// None for nested structs within config.
    field_name: Option<String>,

    /// Shape of the config struct.
    #[facet(skip)]
    shape: &'static Shape,

    /// Fields from the struct
    fields: IndexMap<String, ConfigFieldSchema, RandomState>,
}

#[derive(Facet)]
#[facet(skip_all_unless_truthy)]
#[derive(Default)]
pub struct Docs {
    /// Short summary / first line.
    summary: Option<String>,
    /// Long-form doc string / details.
    details: Option<String>,
}

#[derive(Facet)]
#[repr(u8)]
pub enum ScalarType {
    Bool,
    String,
    Integer,
    Float,
}

#[derive(Facet)]
#[facet(skip_all_unless_truthy)]
#[repr(u8)]
pub enum LeafKind {
    /// Primitive scalar value (bool/string/number-like).
    Scalar(ScalarType),
    /// Enum value (variants represented as CLI strings).
    Enum { variants: Vec<String> },
}

#[derive(Facet)]
#[facet(skip_all_unless_truthy)]
pub struct LeafSchema {
    /// What kind of leaf value this is.
    kind: LeafKind,
    /// Underlying facet shape for defaults and parsing.
    #[facet(skip)]
    shape: &'static Shape,
}

#[derive(Facet)]
#[facet(skip_all_unless_truthy)]
#[repr(u8)]
pub enum ValueSchema {
    /// Leaf value (scalar or enum). Retains the original Shape.
    Leaf(LeafSchema),
    /// Optional value wrapper; Shape is `Option<T>`.
    Option {
        value: Box<ValueSchema>,
        #[facet(skip)]
        shape: &'static Shape,
    },
    /// Vector/list wrapper; Shape is `Vec<T>` / list.
    Vec {
        element: Box<ValueSchema>,
        #[facet(skip)]
        shape: &'static Shape,
    },
    /// Struct value; Shape is the struct itself.
    Struct {
        fields: ConfigStructSchema,
        #[facet(skip)]
        shape: &'static Shape,
    },
}

/// Schema for a subcommand
#[derive(Facet)]
#[facet(skip_all_unless_truthy)]
pub struct Subcommand {
    /// Subcommand name (kebab-case or rename).
    /// Derived from enum variant name, or `#[facet(rename = "...")]`.
    name: String,

    /// Documentation for this subcommand.
    docs: Docs,

    /// Arguments for this subcommand level.
    args: ArgLevelSchema,

    /// Underlying enum variant shape (kept for defaults / validation).
    #[facet(skip)]
    shape: &'static Shape,
}

/// Schema for a singular argument
#[derive(Facet)]
#[facet(skip_all_unless_truthy)]
pub struct ArgSchema {
    /// Argument name / effective name (rename or field name).
    name: String,

    /// Documentation for this argument.
    docs: Docs,

    /// How it appears on the CLI (driven by `#[facet(args::...)]`).
    kind: ArgKind,

    /// Value shape (including Option/Vec wrappers).
    value: ValueSchema,

    /// Whether the argument is required on the CLI.
    /// Set when the field is non-optional, has no default, is not a bool flag,
    /// and is not an optional subcommand.
    required: bool,

    /// Whether the argument can appear multiple times on the CLI.
    /// True for list-like values and counted flags.
    multiple: bool,
}

/// A kind of argument
#[derive(Facet)]
#[facet(skip_all_unless_truthy)]
#[repr(u8)]
pub enum ArgKind {
    /// Positional argument (`#[facet(args::positional)]`).
    Positional,

    /// Named flag (`#[facet(args::named)]`), with optional `short` and `counted`.
    /// `short` comes from `#[facet(args::short = 'x')]` (or defaulted when `args::short` is present).
    /// `counted` comes from `#[facet(args::counted)]`.
    Named { short: Option<char>, counted: bool },
}

/// Schema for the 'config' field of the top-level args struct
#[derive(Facet)]
#[facet(skip_all_unless_truthy)]
pub struct ConfigFieldSchema {
    /// Doc comments for a field
    docs: Docs,

    /// Value schema for a field
    pub value: ConfigValueSchema,
}

/// Schema for a vec in a config value
#[derive(Facet)]
#[facet(skip_all_unless_truthy)]
pub struct ConfigVecSchema {
    /// Shape of the vector/list.
    shape: &'static Shape,

    /// Schema for the vec element
    element: Box<ConfigValueSchema>,
}

/// Schema for a value in the config struct
#[derive(Facet)]
#[facet(skip_all_unless_truthy)]
#[repr(u8)]
pub enum ConfigValueSchema {
    Struct(ConfigStructSchema),
    Vec(ConfigVecSchema),
    Option {
        value: Box<ConfigValueSchema>,
        shape: &'static Shape,
    },
    Leaf(LeafSchema),
}

/// Visitor for walking schema structures.
pub trait SchemaVisitor {
    fn enter_schema(&mut self, _path: &Path, _schema: &Schema) {}
    fn enter_arg_level(&mut self, _path: &Path, _args: &ArgLevelSchema) {}
    fn enter_arg(&mut self, _path: &Path, _arg: &ArgSchema) {}
    fn enter_subcommand(&mut self, _path: &Path, _subcommand: &Subcommand) {}
    fn enter_value(&mut self, _path: &Path, _value: &ValueSchema) {}
    fn enter_config_struct(&mut self, _path: &Path, _config: &ConfigStructSchema) {}
    fn enter_config_value(&mut self, _path: &Path, _value: &ConfigValueSchema) {}
}

impl Schema {
    /// Visit all schema nodes in depth-first order.
    pub fn visit(&self, visitor: &mut impl SchemaVisitor) {
        let mut path: Path = Vec::new();
        visitor.enter_schema(&path, self);

        self.args.visit(visitor, &mut path);

        if let Some(config) = &self.config {
            path.push("config".to_string());
            config.visit(visitor, &mut path);
            path.pop();
        }
    }
}

impl ArgLevelSchema {
    fn visit(&self, visitor: &mut impl SchemaVisitor, path: &mut Path) {
        visitor.enter_arg_level(path, self);

        for (name, arg) in &self.args {
            path.push(name.clone());
            visitor.enter_arg(path, arg);
            arg.value.visit(visitor, path);
            path.pop();
        }

        for (name, sub) in &self.subcommands {
            path.push(name.clone());
            visitor.enter_subcommand(path, sub);
            sub.args.visit(visitor, path);
            path.pop();
        }
    }
}

impl ValueSchema {
    fn visit(&self, visitor: &mut impl SchemaVisitor, path: &mut Path) {
        visitor.enter_value(path, self);

        match self {
            ValueSchema::Leaf(_) => {}
            ValueSchema::Option { value, .. } => value.visit(visitor, path),
            ValueSchema::Vec { element, .. } => element.visit(visitor, path),
            ValueSchema::Struct { fields, .. } => fields.visit(visitor, path),
        }
    }
}

impl ConfigStructSchema {
    fn visit(&self, visitor: &mut impl SchemaVisitor, path: &mut Path) {
        visitor.enter_config_struct(path, self);

        for (name, field) in &self.fields {
            path.push(name.clone());
            field.value.visit(visitor, path);
            path.pop();
        }
    }

    /// Navigate to a config value schema by path.
    pub fn get_by_path(&self, path: &Path) -> Option<&ConfigValueSchema> {
        let mut iter = path.iter();
        let first = iter.next()?;
        let mut current = &self.fields.get(first)?.value;

        for segment in iter {
            current = match current {
                ConfigValueSchema::Struct(s) => &s.fields.get(segment)?.value,
                ConfigValueSchema::Vec(v) => {
                    segment.parse::<usize>().ok()?;
                    v.element.as_ref()
                }
                ConfigValueSchema::Option { value, .. } => value.as_ref(),
                ConfigValueSchema::Leaf(_) => return None,
            };
        }

        Some(current)
    }
}

impl ConfigValueSchema {
    fn visit(&self, visitor: &mut impl SchemaVisitor, path: &mut Path) {
        visitor.enter_config_value(path, self);

        match self {
            ConfigValueSchema::Struct(s) => s.visit(visitor, path),
            ConfigValueSchema::Vec(v) => v.element.visit(visitor, path),
            ConfigValueSchema::Option { value, .. } => value.visit(visitor, path),
            ConfigValueSchema::Leaf(_) => {}
        }
    }
}

// ============================================================================
// Accessor methods for parser2
// ============================================================================

impl Schema {
    /// Get the top-level arguments schema.
    pub fn args(&self) -> &ArgLevelSchema {
        &self.args
    }

    /// Get the config struct schema, if any.
    pub fn config(&self) -> Option<&ConfigStructSchema> {
        self.config.as_ref()
    }
}

impl ArgLevelSchema {
    /// Get the named/positional arguments at this level.
    pub fn args(&self) -> &IndexMap<String, ArgSchema, RandomState> {
        &self.args
    }

    /// Get the subcommands at this level.
    pub fn subcommands(&self) -> &IndexMap<String, Subcommand, RandomState> {
        &self.subcommands
    }

    /// Get the field name that holds subcommands at this level.
    /// Returns None if there are no subcommands.
    pub fn subcommand_field_name(&self) -> Option<&str> {
        self.subcommand_field_name.as_deref()
    }
}

impl ArgSchema {
    /// Get the argument name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the argument kind (positional or named).
    pub fn kind(&self) -> &ArgKind {
        &self.kind
    }

    /// Get the value schema.
    pub fn value(&self) -> &ValueSchema {
        &self.value
    }

    /// Check if this argument is required.
    pub fn required(&self) -> bool {
        self.required
    }

    /// Check if this argument can appear multiple times.
    pub fn multiple(&self) -> bool {
        self.multiple
    }
}

impl Subcommand {
    /// Get the subcommand name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the arguments schema for this subcommand.
    pub fn args(&self) -> &ArgLevelSchema {
        &self.args
    }
}

impl ConfigStructSchema {
    /// Get the field name in the parent struct (e.g., "config").
    pub fn field_name(&self) -> Option<&str> {
        self.field_name.as_deref()
    }

    /// Get the fields of this config struct.
    pub fn fields(&self) -> &IndexMap<String, ConfigFieldSchema, RandomState> {
        &self.fields
    }
}

impl ValueSchema {
    /// Check if this is a boolean type.
    pub fn is_bool(&self) -> bool {
        matches!(
            self,
            ValueSchema::Leaf(LeafSchema {
                kind: LeafKind::Scalar(ScalarType::Bool),
                ..
            })
        )
    }

    /// Unwrap Option wrapper if present, returning the inner schema.
    pub fn inner_if_option(&self) -> &ValueSchema {
        match self {
            ValueSchema::Option { value, .. } => value.as_ref(),
            other => other,
        }
    }
}

#[cfg(test)]
#[allow(dead_code)]
mod tests;
