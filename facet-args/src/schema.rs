use std::hash::DefaultHasher;

use facet::Facet;
use facet_core::Shape;
use facet_error as _;
use indexmap::IndexMap;

/// The struct passed into facet_args::builder has some problems: some fields are not
/// annotated, etc.
#[derive(Facet, Debug)]
#[facet(derive(Error))]
#[repr(u8)]
pub enum SchemaError {
    /// Top-level shape must be a struct.
    TopLevelNotStruct,
    /// A field was not annotated with any args attribute (positional/named/subcommand/config).
    MissingArgsAnnotation { field: &'static str },
    /// More than one field marked as `#[facet(args::subcommand)]` at the same level.
    MultipleSubcommandFields,
    /// `#[facet(args::subcommand)]` used on a non-enum field.
    SubcommandOnNonEnum { field: &'static str },
    /// `#[facet(args::counted)]` used on a non-integer type.
    CountedOnNonInteger { field: &'static str },
    /// `#[facet(args::short)]` used on a positional-only argument.
    ShortOnPositional { field: &'static str },
    /// `#[facet(args::env_prefix)]` used without `#[facet(args::config)]`.
    EnvPrefixWithoutConfig { field: &'static str },
    /// Duplicate CLI flag name or short at the same level.
    ConflictingFlagNames { name: String },
    /// Generic schema validation failure.
    BadSchema(&'static str),
}

/// A schema "parsed" from
pub struct Schema {
    /// Top-level arguments: `--verbose`, etc.
    args: ArgLevelSchema,

    /// Optional config, read from config file, environment
    config: Option<ConfigStructSchema>,
}

#[derive(Default)]
pub struct Docs {
    /// Short summary / first line.
    summary: Option<String>,
    /// Long-form doc string / details.
    details: Option<String>,
}

pub enum ScalarType {
    Bool,
    String,
    Integer,
    Float,
}

pub enum LeafKind {
    /// Primitive scalar value (bool/string/number-like).
    Scalar(ScalarType),
    /// Enum value (variants represented as CLI strings).
    Enum { variants: Vec<String> },
}

pub struct LeafSchema {
    /// What kind of leaf value this is.
    kind: LeafKind,
    /// Underlying facet shape for defaults and parsing.
    shape: &'static Shape,
}

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

pub struct ArgLevelSchema {
    /// Any valid arguments at this level, `--verbose` etc.
    args: IndexMap<String, ArgSchema, DefaultHasher>,

    /// Any subcommands at this level
    subcommands: IndexMap<String, Subcommand, DefaultHasher>,
}

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

// A kind of argument
pub enum ArgKind {
    /// Positional argument (`#[facet(args::positional)]`).
    Positional,
    /// Named flag (`#[facet(args::named)]`), with optional `short` and `counted`.
    /// `short` comes from `#[facet(args::short = 'x')]` (or defaulted when `args::short` is present).
    /// `counted` comes from `#[facet(args::counted)]`.
    Named { short: Option<char>, counted: bool },
}

pub struct ConfigStructSchema {
    /// Shape of the struct.
    shape: &'static Shape,
    fields: IndexMap<String, ConfigFieldSchema, DefaultHasher>,
}

pub struct ConfigFieldSchema {
    docs: Docs,
    value: ConfigValueSchema,
}

pub struct ConfigVecSchema {
    element: Box<ConfigValueSchema>,
    /// Shape of the vector/list.
    shape: &'static Shape,
}

pub enum ConfigValueSchema {
    Struct(ConfigStructSchema),
    Vec(ConfigVecSchema),
    Option {
        value: Box<ConfigValueSchema>,
        shape: &'static Shape,
    },
    Leaf(LeafSchema),
}

impl Schema {
    /// Parse a schema from a given shape
    pub(crate) fn from_shape(_shape: &'static Shape) -> Result<Self, SchemaError> {
        todo!("walk shape to fill in schema")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use facet_pretty::{PathSegment, format_shape_with_spans};

    #[test]
    fn pretty_shape_spans_prototype() {
        #[derive(Facet)]
        struct App {
            #[facet(args::named)]
            verbose: bool,
            #[facet(args::subcommand)]
            cmd: Cmd,
        }

        #[derive(Facet)]
        enum Cmd {
            #[facet(rename = "do-thing")]
            DoThing {
                #[facet(args::positional)]
                path: String,
            },
        }

        let formatted = format_shape_with_spans(App::SHAPE);

        let verbose_path = vec![PathSegment::Field("verbose".into())];
        assert!(formatted.spans.contains_key(&verbose_path));
        assert!(formatted.type_name_span.is_some());
    }
}
