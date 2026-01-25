use std::hash::RandomState;

use facet::Facet;
use facet_core::Shape;
use facet_error as _;
use indexmap::IndexMap;

use crate::path::Path;

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

impl Schema {
    /// Parse a schema from a given shape
    pub(crate) fn from_shape(_shape: &'static Shape) -> Result<Self, SchemaError> {
        todo!("walk shape to fill in schema")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate as args;
    use facet_pretty::{PathSegment, format_shape_with_spans};

    #[test]
    fn pretty_shape_spans_prototype() {
        use ariadne::{Color, Label, Report, ReportKind, Source};
        #[derive(Facet)]
        struct App {
            #[facet(args::named)]
            verbose: bool,
            config_path: String,
            #[facet(args::subcommand)]
            cmd: Cmd,
        }

        #[derive(Facet)]
        #[repr(u8)]
        enum Cmd {
            #[facet(rename = "do-thing")]
            DoThing {
                #[facet(args::positional)]
                path: String,
            },
        }

        let formatted = format_shape_with_spans(App::SHAPE);

        let verbose_path = vec![PathSegment::Field("verbose".into())];
        let missing_path = vec![PathSegment::Field("config_path".into())];
        assert!(formatted.spans.contains_key(&verbose_path));
        assert!(formatted.spans.contains_key(&missing_path));

        let type_name_span = formatted.type_name_span.expect("type name span");
        let type_label_span = type_name_span.0..type_name_span.1;

        let def_end = formatted.text[type_name_span.1..]
            .find('}')
            .map(|offset| type_name_span.1 + offset)
            .unwrap_or_else(|| formatted.text.len().saturating_sub(1));
        let def_end_end = (def_end + 1).min(formatted.text.len());
        let def_end_span = def_end..def_end_end;

        let source_label = App::SHAPE
            .source_file
            .zip(App::SHAPE.source_line)
            .map(|(file, line)| format!("defined at {file}:{line}"))
            .unwrap_or_else(|| "definition location unavailable (enable facet/doc)".to_string());

        let field_span = &formatted.spans[&missing_path];
        let span = field_span.key.0..field_span.value.1;

        let report = Report::build(ReportKind::Error, span.clone())
            .with_message("missing facet(args::...) annotation")
            .with_label(
                Label::new(type_label_span)
                    .with_message(source_label)
                    .with_color(Color::Blue),
            )
            .with_label(
                Label::new(span)
                    .with_message("THIS IS WHERE YOU FORGOT A facet(args::) annotation")
                    .with_color(Color::Red),
            )
            .with_label(
                Label::new(def_end_span)
                    .with_message("end of definition")
                    .with_color(Color::Blue),
            )
            .finish();

        let mut out = Vec::new();
        report
            .write(Source::from(&formatted.text), &mut out)
            .expect("write ariadne report");
        println!(
            "{}",
            String::from_utf8(out).expect("ariadne output is UTF-8")
        );
    }
}
