use std::collections::HashSet;
use std::hash::RandomState;

use facet::Facet;
use facet_core::{
    Def, EnumType, Field, ScalarType as FacetScalarType, Shape, StructKind, Type, UserType, Variant,
};
use facet_error as _;
use heck::ToKebabCase;
use indexmap::IndexMap;

use crate::{
    Attr,
    path::Path,
    reflection::{is_config_field, is_counted_field, is_supported_counted_type},
};

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
    args: IndexMap<String, ArgSchema, RandomState>,

    /// Any subcommands at this level
    subcommands: IndexMap<String, Subcommand, RandomState>,
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
    fields: IndexMap<String, ConfigFieldSchema, RandomState>,
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

fn has_any_args_attr(field: &Field) -> bool {
    field.has_attr(Some("args"), "positional")
        || field.has_attr(Some("args"), "named")
        || field.has_attr(Some("args"), "subcommand")
        || field.has_attr(Some("args"), "config")
        || field.has_attr(Some("args"), "short")
        || field.has_attr(Some("args"), "counted")
        || field.has_attr(Some("args"), "env_prefix")
}

fn docs_from_lines(lines: &'static [&'static str]) -> Docs {
    if lines.is_empty() {
        return Docs::default();
    }

    let summary = lines
        .first()
        .map(|line| line.trim().to_string())
        .filter(|s| !s.is_empty());

    let details = if lines.len() > 1 {
        let mut buf = String::new();
        for line in &lines[1..] {
            if !buf.is_empty() {
                buf.push('\n');
            }
            buf.push_str(line.trim());
        }
        if buf.is_empty() { None } else { Some(buf) }
    } else {
        None
    };

    Docs { summary, details }
}

fn scalar_kind_from_shape(shape: &'static Shape) -> Option<ScalarType> {
    match shape.scalar_type()? {
        FacetScalarType::Bool => Some(ScalarType::Bool),
        FacetScalarType::Str
        | FacetScalarType::String
        | FacetScalarType::CowStr
        | FacetScalarType::Char => Some(ScalarType::String),
        FacetScalarType::F32 | FacetScalarType::F64 => Some(ScalarType::Float),
        FacetScalarType::U8
        | FacetScalarType::U16
        | FacetScalarType::U32
        | FacetScalarType::U64
        | FacetScalarType::U128
        | FacetScalarType::USize
        | FacetScalarType::I8
        | FacetScalarType::I16
        | FacetScalarType::I32
        | FacetScalarType::I64
        | FacetScalarType::I128
        | FacetScalarType::ISize => Some(ScalarType::Integer),
        _ => None,
    }
}

fn enum_variants(enum_type: EnumType) -> Vec<String> {
    enum_type
        .variants
        .iter()
        .map(|variant| variant_cli_name(variant))
        .collect()
}

fn variant_cli_name(variant: &Variant) -> String {
    variant
        .get_builtin_attr("rename")
        .and_then(|attr| attr.get_as::<&str>())
        .map(|s| (*s).to_string())
        .unwrap_or_else(|| variant.name.to_kebab_case())
}

fn leaf_schema_from_shape(shape: &'static Shape) -> Result<LeafSchema, SchemaError> {
    if let Some(scalar) = scalar_kind_from_shape(shape) {
        return Ok(LeafSchema {
            kind: LeafKind::Scalar(scalar),
            shape,
        });
    }

    match &shape.ty {
        Type::User(UserType::Enum(enum_type)) => Ok(LeafSchema {
            kind: LeafKind::Enum {
                variants: enum_variants(*enum_type),
            },
            shape,
        }),
        _ => Err(SchemaError::BadSchema("unsupported leaf type")),
    }
}

fn value_schema_from_shape(shape: &'static Shape) -> Result<ValueSchema, SchemaError> {
    match shape.def {
        Def::Option(opt) => Ok(ValueSchema::Option {
            value: Box::new(value_schema_from_shape(opt.t)?),
            shape,
        }),
        Def::List(list) => Ok(ValueSchema::Vec {
            element: Box::new(value_schema_from_shape(list.t)?),
            shape,
        }),
        _ => match &shape.ty {
            Type::User(UserType::Struct(_)) => Ok(ValueSchema::Struct {
                fields: config_struct_schema_from_shape(shape)?,
                shape,
            }),
            _ => Ok(ValueSchema::Leaf(leaf_schema_from_shape(shape)?)),
        },
    }
}

fn config_value_schema_from_shape(shape: &'static Shape) -> Result<ConfigValueSchema, SchemaError> {
    match shape.def {
        Def::Option(opt) => Ok(ConfigValueSchema::Option {
            value: Box::new(config_value_schema_from_shape(opt.t)?),
            shape,
        }),
        Def::List(list) => Ok(ConfigValueSchema::Vec(ConfigVecSchema {
            element: Box::new(config_value_schema_from_shape(list.t)?),
            shape,
        })),
        _ => match &shape.ty {
            Type::User(UserType::Struct(_)) => Ok(ConfigValueSchema::Struct(
                config_struct_schema_from_shape(shape)?,
            )),
            _ => Ok(ConfigValueSchema::Leaf(leaf_schema_from_shape(shape)?)),
        },
    }
}

fn config_struct_schema_from_shape(
    shape: &'static Shape,
) -> Result<ConfigStructSchema, SchemaError> {
    let struct_type = match &shape.ty {
        Type::User(UserType::Struct(s)) => *s,
        _ => return Err(SchemaError::BadSchema("config field must be a struct")),
    };

    let mut fields_map: IndexMap<String, ConfigFieldSchema, RandomState> = IndexMap::default();
    for field in struct_type.fields {
        let docs = docs_from_lines(field.doc);
        let value = config_value_schema_from_shape(field.shape())?;
        fields_map.insert(field.name.to_string(), ConfigFieldSchema { docs, value });
    }

    Ok(ConfigStructSchema {
        shape,
        fields: fields_map,
    })
}

fn short_from_field(field: &Field) -> Option<char> {
    field
        .get_attr(Some("args"), "short")
        .and_then(|attr| attr.get_as::<Attr>())
        .and_then(|attr| {
            if let Attr::Short(c) = attr {
                c.or_else(|| field.effective_name().chars().next())
            } else {
                None
            }
        })
}

fn variant_fields_for_schema(variant: &Variant) -> &'static [Field] {
    let fields = variant.data.fields;
    if variant.data.kind == StructKind::TupleStruct && fields.len() == 1 {
        let inner_shape = fields[0].shape();
        if let Type::User(UserType::Struct(struct_type)) = inner_shape.ty {
            return struct_type.fields;
        }
    }
    fields
}

fn arg_level_from_fields(fields: &'static [Field]) -> Result<ArgLevelSchema, SchemaError> {
    let mut args: IndexMap<String, ArgSchema, RandomState> = IndexMap::default();
    let mut subcommands: IndexMap<String, Subcommand, RandomState> = IndexMap::default();

    let mut seen_long = HashSet::new();
    let mut seen_short = HashSet::new();

    let mut saw_subcommand = false;

    for field in fields {
        if is_config_field(field) {
            continue;
        }

        if !has_any_args_attr(field) {
            return Err(SchemaError::MissingArgsAnnotation { field: field.name });
        }

        if field.has_attr(Some("args"), "env_prefix") && !field.has_attr(Some("args"), "config") {
            return Err(SchemaError::EnvPrefixWithoutConfig { field: field.name });
        }

        let is_positional = field.has_attr(Some("args"), "positional");
        let is_subcommand = field.has_attr(Some("args"), "subcommand");

        if field.has_attr(Some("args"), "short") && is_positional {
            return Err(SchemaError::ShortOnPositional { field: field.name });
        }

        if is_counted_field(field) && !is_supported_counted_type(field.shape()) {
            return Err(SchemaError::CountedOnNonInteger { field: field.name });
        }

        if is_subcommand {
            if saw_subcommand {
                return Err(SchemaError::MultipleSubcommandFields);
            }
            saw_subcommand = true;

            let field_shape = field.shape();
            let (enum_shape, enum_type) = match field_shape.def {
                Def::Option(opt) => match opt.t.ty {
                    Type::User(UserType::Enum(enum_type)) => (opt.t, enum_type),
                    _ => return Err(SchemaError::SubcommandOnNonEnum { field: field.name }),
                },
                _ => match field_shape.ty {
                    Type::User(UserType::Enum(enum_type)) => (field_shape, enum_type),
                    _ => return Err(SchemaError::SubcommandOnNonEnum { field: field.name }),
                },
            };

            for variant in enum_type.variants {
                let name = variant_cli_name(variant);
                let docs = docs_from_lines(variant.doc);
                let variant_fields = variant_fields_for_schema(variant);
                let args_schema = arg_level_from_fields(variant_fields)?;

                let sub = Subcommand {
                    name: name.clone(),
                    docs,
                    args: args_schema,
                    shape: enum_shape,
                };

                if subcommands.insert(name.clone(), sub).is_some() {
                    return Err(SchemaError::ConflictingFlagNames { name });
                }
            }

            continue;
        }

        let short = if field.has_attr(Some("args"), "short") {
            short_from_field(field)
        } else {
            None
        };
        let counted = field.has_attr(Some("args"), "counted");

        let kind = if is_positional {
            ArgKind::Positional
        } else {
            ArgKind::Named { short, counted }
        };

        let value = value_schema_from_shape(field.shape())?;
        let required = {
            let shape = field.shape();
            !matches!(shape.def, Def::Option(_))
                && !field.has_default()
                && !shape.is_shape(bool::SHAPE)
                && !(counted && is_supported_counted_type(shape))
        };
        let multiple = counted || matches!(field.shape().def, Def::List(_));

        if !is_positional {
            let long = field.effective_name().to_kebab_case();
            if !seen_long.insert(long.clone()) {
                return Err(SchemaError::ConflictingFlagNames {
                    name: format!("--{long}"),
                });
            }
            if let Some(c) = short {
                if !seen_short.insert(c) {
                    return Err(SchemaError::ConflictingFlagNames {
                        name: format!("-{c}"),
                    });
                }
            }
        }

        let docs = docs_from_lines(field.doc);
        let arg = ArgSchema {
            name: field.effective_name().to_string(),
            docs,
            kind,
            value,
            required,
            multiple,
        };

        args.insert(field.effective_name().to_string(), arg);
    }

    Ok(ArgLevelSchema { args, subcommands })
}

impl Schema {
    /// Parse a schema from a given shape
    pub(crate) fn from_shape(shape: &'static Shape) -> Result<Self, SchemaError> {
        let struct_type = match &shape.ty {
            Type::User(UserType::Struct(s)) => *s,
            _ => return Err(SchemaError::TopLevelNotStruct),
        };

        let mut config_field: Option<&'static Field> = None;

        for field in struct_type.fields {
            if is_config_field(field) {
                if config_field.is_some() {
                    return Err(SchemaError::BadSchema("multiple config fields"));
                }
                config_field = Some(field);
            }

            if field.has_attr(Some("args"), "env_prefix") && !field.has_attr(Some("args"), "config")
            {
                return Err(SchemaError::EnvPrefixWithoutConfig { field: field.name });
            }
        }

        let args = arg_level_from_fields(struct_type.fields)?;

        let config = if let Some(field) = config_field {
            let shape = field.shape();
            let config_shape = match shape.def {
                Def::Option(opt) => opt.t,
                _ => shape,
            };
            Some(config_struct_schema_from_shape(config_shape)?)
        } else {
            None
        };

        Ok(Schema { args, config })
    }
}

impl core::fmt::Debug for Schema {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Schema")
            .field("args", &self.args)
            .field("config", &self.config)
            .finish()
    }
}

impl core::fmt::Debug for Docs {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Docs")
            .field("summary", &self.summary)
            .field("details", &self.details)
            .finish()
    }
}

impl core::fmt::Debug for ScalarType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ScalarType::Bool => f.write_str("Bool"),
            ScalarType::String => f.write_str("String"),
            ScalarType::Integer => f.write_str("Integer"),
            ScalarType::Float => f.write_str("Float"),
        }
    }
}

impl core::fmt::Debug for LeafKind {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            LeafKind::Scalar(s) => f.debug_tuple("Scalar").field(s).finish(),
            LeafKind::Enum { variants } => {
                f.debug_struct("Enum").field("variants", variants).finish()
            }
        }
    }
}

impl core::fmt::Debug for LeafSchema {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("LeafSchema")
            .field("kind", &self.kind)
            .finish()
    }
}

impl core::fmt::Debug for ValueSchema {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ValueSchema::Leaf(leaf) => f.debug_tuple("Leaf").field(leaf).finish(),
            ValueSchema::Option { value, .. } => {
                f.debug_struct("Option").field("value", value).finish()
            }
            ValueSchema::Vec { element, .. } => {
                f.debug_struct("Vec").field("element", element).finish()
            }
            ValueSchema::Struct { fields, .. } => {
                f.debug_struct("Struct").field("fields", fields).finish()
            }
        }
    }
}

impl core::fmt::Debug for ArgLevelSchema {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ArgLevelSchema")
            .field("args", &self.args)
            .field("subcommands", &self.subcommands)
            .finish()
    }
}

impl core::fmt::Debug for Subcommand {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Subcommand")
            .field("name", &self.name)
            .field("docs", &self.docs)
            .field("args", &self.args)
            .finish()
    }
}

impl core::fmt::Debug for ArgSchema {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ArgSchema")
            .field("name", &self.name)
            .field("docs", &self.docs)
            .field("kind", &self.kind)
            .field("value", &self.value)
            .field("required", &self.required)
            .field("multiple", &self.multiple)
            .finish()
    }
}

impl core::fmt::Debug for ArgKind {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ArgKind::Positional => f.write_str("Positional"),
            ArgKind::Named { short, counted } => f
                .debug_struct("Named")
                .field("short", short)
                .field("counted", counted)
                .finish(),
        }
    }
}

impl core::fmt::Debug for ConfigStructSchema {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ConfigStructSchema")
            .field("fields", &self.fields)
            .finish()
    }
}

impl core::fmt::Debug for ConfigFieldSchema {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ConfigFieldSchema")
            .field("docs", &self.docs)
            .field("value", &self.value)
            .finish()
    }
}

impl core::fmt::Debug for ConfigVecSchema {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ConfigVecSchema")
            .field("element", &self.element)
            .finish()
    }
}

impl core::fmt::Debug for ConfigValueSchema {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ConfigValueSchema::Struct(s) => f.debug_tuple("Struct").field(s).finish(),
            ConfigValueSchema::Vec(v) => f.debug_tuple("Vec").field(v).finish(),
            ConfigValueSchema::Option { value, .. } => {
                f.debug_struct("Option").field("value", value).finish()
            }
            ConfigValueSchema::Leaf(leaf) => f.debug_tuple("Leaf").field(leaf).finish(),
        }
    }
}

#[cfg(test)]
#[allow(dead_code)]
mod tests {
    use super::*;
    use crate as args;
    use facet::Facet;

    #[derive(Facet)]
    struct BasicArgs {
        /// Verbose output
        #[facet(args::named, args::short = 'v')]
        verbose: bool,
        /// Input file
        #[facet(args::positional)]
        input: String,
        /// Include list
        #[facet(args::named)]
        include: Vec<String>,
        /// Quiet count
        #[facet(args::named, args::short = 'q', args::counted)]
        quiet: u32,
        /// Subcommand
        #[facet(args::subcommand)]
        command: Option<Command>,
        /// Config
        #[facet(args::config, args::env_prefix = "APP")]
        config: Option<AppConfig>,
    }

    #[derive(Facet)]
    #[repr(u8)]
    enum Command {
        /// Build stuff
        Build(BuildArgs),
        /// Clean
        #[facet(rename = "clean-all")]
        Clean,
    }

    #[derive(Facet)]
    struct BuildArgs {
        /// Release build
        #[facet(args::named, args::short = 'r')]
        release: bool,
    }

    #[derive(Facet)]
    struct AppConfig {
        host: String,
        port: u16,
    }

    #[derive(Facet)]
    struct MissingArgsAnnotation {
        foo: String,
    }

    #[derive(Facet)]
    #[repr(u8)]
    enum SubA {
        A,
    }

    #[derive(Facet)]
    #[repr(u8)]
    enum SubB {
        B,
    }

    #[derive(Facet)]
    struct MultipleSubcommands {
        #[facet(args::subcommand)]
        a: SubA,
        #[facet(args::subcommand)]
        b: SubB,
    }

    #[derive(Facet)]
    struct SubcommandOnNonEnum {
        #[facet(args::subcommand)]
        value: String,
    }

    #[derive(Facet)]
    struct CountedOnNonInteger {
        #[facet(args::named, args::counted)]
        value: bool,
    }

    #[derive(Facet)]
    struct ShortOnPositional {
        #[facet(args::positional, args::short = 'p')]
        value: String,
    }

    #[derive(Facet)]
    struct EnvPrefixWithoutConfig {
        #[facet(args::env_prefix = "APP")]
        value: String,
    }

    #[derive(Facet)]
    struct ConflictingLongFlags {
        #[facet(args::named, rename = "dup")]
        a: bool,
        #[facet(args::named, rename = "dup")]
        b: bool,
    }

    #[derive(Facet)]
    struct ConflictingShortFlags {
        #[facet(args::named, args::short = 'v')]
        a: bool,
        #[facet(args::named, args::short = 'v')]
        b: bool,
    }

    #[derive(Facet)]
    struct BadConfigField {
        #[facet(args::config)]
        config: String,
    }

    #[derive(Facet)]
    #[repr(u8)]
    enum TopLevelEnum {
        Foo,
    }

    #[test]
    fn snapshot_schema_basic() {
        insta::assert_debug_snapshot!(Schema::from_shape(BasicArgs::SHAPE));
    }

    #[test]
    fn snapshot_schema_top_level_enum() {
        insta::assert_debug_snapshot!(Schema::from_shape(TopLevelEnum::SHAPE));
    }

    #[test]
    fn snapshot_schema_missing_args_annotation() {
        insta::assert_debug_snapshot!(Schema::from_shape(MissingArgsAnnotation::SHAPE));
    }

    #[test]
    fn snapshot_schema_multiple_subcommands() {
        insta::assert_debug_snapshot!(Schema::from_shape(MultipleSubcommands::SHAPE));
    }

    #[test]
    fn snapshot_schema_subcommand_on_non_enum() {
        insta::assert_debug_snapshot!(Schema::from_shape(SubcommandOnNonEnum::SHAPE));
    }

    #[test]
    fn snapshot_schema_counted_on_non_integer() {
        insta::assert_debug_snapshot!(Schema::from_shape(CountedOnNonInteger::SHAPE));
    }

    #[test]
    fn snapshot_schema_short_on_positional() {
        insta::assert_debug_snapshot!(Schema::from_shape(ShortOnPositional::SHAPE));
    }

    #[test]
    fn snapshot_schema_env_prefix_without_config() {
        insta::assert_debug_snapshot!(Schema::from_shape(EnvPrefixWithoutConfig::SHAPE));
    }

    #[test]
    fn snapshot_schema_conflicting_long_flags() {
        insta::assert_debug_snapshot!(Schema::from_shape(ConflictingLongFlags::SHAPE));
    }

    #[test]
    fn snapshot_schema_conflicting_short_flags() {
        insta::assert_debug_snapshot!(Schema::from_shape(ConflictingShortFlags::SHAPE));
    }

    #[test]
    fn snapshot_schema_bad_config_field() {
        insta::assert_debug_snapshot!(Schema::from_shape(BadConfigField::SHAPE));
    }
}
