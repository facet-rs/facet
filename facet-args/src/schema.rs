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
pub struct ArgLevelSchema {
    /// Any valid arguments at this level, `--verbose` etc.
    args: IndexMap<String, ArgSchema, RandomState>,

    /// Any subcommands at this level
    subcommands: IndexMap<String, Subcommand, RandomState>,
}

/// Schema for the `config` part of the schema
#[derive(Facet)]
#[facet(skip_all_unless_truthy)]
pub struct ConfigStructSchema {
    /// Shape of the config struct.
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
        shape: &'static Shape,
    },
    /// Vector/list wrapper; Shape is `Vec<T>` / list.
    Vec {
        element: Box<ValueSchema>,
        shape: &'static Shape,
    },
    /// Struct value; Shape is the struct itself.
    Struct {
        fields: ConfigStructSchema,
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
    value: ConfigValueSchema,
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

#[cfg(test)]
#[allow(dead_code)]
mod tests;
