use std::hash::DefaultHasher;

use facet::Facet;
use facet_error as _;
use indexmap::IndexMap;

/// The struct passed into facet_args::builder has some problems: some fields are not
/// annotated, etc.
#[derive(Facet, Debug)]
#[facet(derive(Error))]
#[repr(u8)]
pub enum SchemaError {
    /// The
    BadSchema(&'static str),
}

/// A schema "parsed" from
pub struct Schema {
    /// Top-level arguments: `--verbose`, etc.
    args: ArgLevelSchema,

    /// Optional config, read from config file, environment
    config: Option<ConfigStructSchema>,
}

pub struct ArgLevelSchema {
    /// Any valid arguments at this level, `--verbose` etc.
    args: Vec<ArgSchema>,

    /// Any subcommands at this level
    subcommands: Vec<Subcommand>,
}

pub struct Subcommand {}

/// Schema for a singular argument
pub struct ArgSchema {
    kind: ArgKind,
}

// A kind of argument
pub enum ArgKind {
    Positional,
    Named { short: Option<char> },
}

pub struct ConfigStructSchema {
    fields: IndexMap<String, ConfigValueSchema, DefaultHasher>,
}

pub struct ConfigVecSchema {
    element: Box<ConfigValueSchema>,
}

enum ConfigValueSchema {
    Struct(ConfigStructSchema),
    Vec(ConfigVecSchema),
}

impl Schema {
    /// Parse a schema from a given shape
    pub(crate) fn from_shape() -> Result<Self, SchemaError> {
        todo!("walk shape to fill in schema")
    }
}
