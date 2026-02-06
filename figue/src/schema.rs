//! Schema representation for CLI arguments and config.
//!
//! This module is under active development and not yet fully wired into the main API.

use std::hash::RandomState;

use facet::Facet;
use facet_core::Shape;
use facet_error as _;
use heck::ToKebabCase;
use indexmap::IndexMap;

use crate::path::Path;

/// Wrapper around args IndexMap that provides kebab-case lookup.
///
/// Schema stores field names as effective names (snake_case or renamed).
/// CLI flags come in as kebab-case. This wrapper converts schema keys to
/// kebab-case during lookup so `--deep-flag` matches `deep_flag` in schema.
pub struct Args<'a> {
    inner: &'a IndexMap<String, ArgSchema, RandomState>,
}

impl<'a> Args<'a> {
    /// Look up an argument by CLI flag name (kebab-case).
    /// Returns the (effective_name, schema) pair if found.
    pub fn get(&self, flag_name: &str) -> Option<(&'a String, &'a ArgSchema)> {
        self.inner
            .iter()
            .find(|(key, _)| key.to_kebab_case() == flag_name)
    }

    /// Iterate over all args with their effective names.
    pub fn iter(&self) -> impl Iterator<Item = (&'a String, &'a ArgSchema)> {
        self.inner.iter()
    }

    /// Get the effective names of all args.
    pub fn keys(&self) -> impl Iterator<Item = &'a String> {
        self.inner.keys()
    }
}

impl<'a> IntoIterator for Args<'a> {
    type Item = (&'a String, &'a ArgSchema);
    type IntoIter = indexmap::map::Iter<'a, String, ArgSchema>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.iter()
    }
}

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
#[derive(Facet, Debug, Clone)]
#[facet(skip_all_unless_truthy)]
pub struct Schema {
    /// Documentation for the top-level type.
    docs: Docs,

    /// Top-level arguments: `--verbose`, etc.
    args: ArgLevelSchema,

    /// Optional config, read from config file, environment
    config: Option<ConfigStructSchema>,

    /// Special fields that trigger early exit behavior.
    special: SpecialFields,
}

/// Fields marked with special attributes that trigger early exit behavior.
///
/// These fields are detected during schema building by looking for well-known
/// field names: `help`, `version`, `completions`. These are typically provided
/// by flattening `FigueBuiltins` into your Args struct.
///
/// After CLI parsing, the driver checks these fields to short-circuit before
/// full deserialization.
#[derive(Facet, Default, Debug, Clone)]
#[facet(skip_all_unless_truthy)]
pub struct SpecialFields {
    /// Path to the `help` field - when true, show help and exit 0.
    /// The field should be a `bool`.
    pub help: Option<Path>,

    /// Path to the `completions` field - when set, generate completions and exit 0.
    /// The field should be `Option<Shell>`.
    pub completions: Option<Path>,

    /// Path to the `version` field - when true, show version and exit 0.
    /// The field should be a `bool`.
    pub version: Option<Path>,
}

/// Schema for one "level" of arguments: top-level, a subcommand, a subcommand's subcommand etc.
#[derive(Facet, Debug, Clone)]
#[facet(skip_all_unless_truthy)]
pub struct ArgLevelSchema {
    /// Any valid arguments at this level, `--verbose` etc.
    args: IndexMap<String, ArgSchema, RandomState>,

    /// Any subcommands at this level
    subcommands: IndexMap<String, Subcommand, RandomState>,

    /// Name of the field that holds subcommands (e.g., "command" for `#[facet(args::subcommand)] command: Command`).
    /// None if there are no subcommands at this level.
    subcommand_field_name: Option<String>,

    /// Whether the subcommand is optional (field type is `Option<Enum>`).
    subcommand_optional: bool,
}

/// Schema for the `config` part of the schema
#[derive(Facet, Debug, Clone)]
#[facet(skip_all_unless_truthy)]
pub struct ConfigStructSchema {
    /// Name of the field in the parent struct (e.g., "config" for `#[facet(args::config)] config: ServerConfig`).
    /// None for nested structs within config.
    field_name: Option<String>,

    /// Environment variable prefix from `#[facet(args::env_prefix = "...")]`.
    /// Only present on the top-level config struct, not nested structs.
    env_prefix: Option<String>,

    /// Shape of the config struct.
    #[facet(skip)]
    shape: &'static Shape,

    /// Fields from the struct
    fields: IndexMap<String, ConfigFieldSchema, RandomState>,
}

#[derive(Facet, Debug, Default, Clone)]
#[facet(skip_all_unless_truthy)]
pub struct Docs {
    /// Short summary / first line.
    summary: Option<String>,
    /// Long-form doc string / details.
    details: Option<String>,
}

#[derive(Facet, Debug, Clone)]
#[repr(u8)]
pub enum ScalarType {
    Bool,
    String,
    Integer,
    Float,
    /// Catch-all for types that can be deserialized from a string (e.g., IpAddr, PathBuf).
    Other,
}

#[derive(Facet, Debug, Clone)]
#[facet(skip_all_unless_truthy)]
#[repr(u8)]
pub enum LeafKind {
    /// Primitive scalar value (bool/string/number-like).
    Scalar(ScalarType),
    /// Enum value (variants represented as CLI strings).
    #[allow(dead_code)]
    Enum { variants: Vec<String> },
}

#[derive(Facet, Debug, Clone)]
#[facet(skip_all_unless_truthy)]
pub struct LeafSchema {
    /// What kind of leaf value this is.
    kind: LeafKind,
    /// Underlying facet shape for defaults and parsing.
    #[facet(skip)]
    pub(crate) shape: &'static Shape,
}

#[derive(Facet, Debug, Clone)]
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

/// Schema for a subcommand.
///
/// # Naming conventions
///
/// There are three different "names" for any facet item (field, variant, etc.):
///
/// 1. **Rust name**: The actual Rust identifier (e.g., `Remove`, `log_file`).
///    Available via `facet_core::Variant::name` or `facet_core::Field::name`.
///
/// 2. **Effective name**: The serialization name, respecting `#[facet(rename = "...")]`.
///    For `#[facet(rename = "rm")] Remove`, the effective name is `"rm"`.
///    Without rename, it's the same as the Rust name.
///    Available via `facet_core::Variant::effective_name()`.
///
/// 3. **CLI name**: The command-line name, derived from effective_name converted to kebab-case.
///    For `Remove` (no rename), the CLI name is `"remove"`.
///    For `#[facet(rename = "rm")] Remove`, the CLI name is `"rm"` (already kebab-case).
///    This is what users type on the command line.
///
/// The schema stores both `effective_name` (for deserialization) and `name` (CLI name, for matching).
#[derive(Facet, Debug, Clone)]
#[facet(skip_all_unless_truthy)]
pub struct Subcommand {
    /// CLI name (kebab-case, used for command-line matching).
    /// Derived from effective_name converted to kebab-case.
    name: String,

    /// Effective name (respects `#[facet(rename = "...")]`).
    /// Used for deserialization with facet-format.
    effective_name: String,

    /// Documentation for this subcommand.
    docs: Docs,

    /// Arguments for this subcommand level.
    args: ArgLevelSchema,

    /// Whether this is a tuple variant with a flattened struct (e.g., `Bench(BenchArgs)`).
    /// When true, the parsed fields need to be wrapped in a "0" field for facet deserialization.
    is_flattened_tuple: bool,

    /// Underlying enum variant shape (kept for defaults / validation).
    #[facet(skip)]
    shape: &'static Shape,
}

/// Schema for a singular argument
#[derive(Facet, Debug, Clone)]
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

    /// Type description for help messages.
    label: Option<String>,

    /// Whether the argument is required on the CLI.
    /// Set when the field is non-optional, has no default, is not a bool flag,
    /// and is not an optional subcommand.
    required: bool,

    /// Whether the argument can appear multiple times on the CLI.
    /// True for list-like values and counted flags.
    multiple: bool,

    /// The field's default value, if any (from `#[facet(default)]` or `#[facet(default = ...)]`).
    /// Pre-serialized to ConfigValue during schema building.
    default: Option<crate::config_value::ConfigValue>,
}

/// A kind of argument
#[derive(Facet, Debug, Clone)]
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
#[derive(Facet, Debug, Clone)]
#[facet(skip_all_unless_truthy)]
pub struct ConfigFieldSchema {
    /// Doc comments for a field
    docs: Docs,

    /// Whether this field contains sensitive data (passwords, tokens, etc.)
    sensitive: bool,

    /// Environment variable aliases for this field.
    ///
    /// These are absolute env var names (like "DATABASE_URL") that can be used
    /// to set this field's value, in addition to the standard prefixed name.
    /// The prefixed env var takes priority over aliases.
    env_aliases: Vec<String>,

    /// Whether to perform environment variable substitution on this field's value.
    ///
    /// When true, `${VAR}` patterns in the field's string value will be replaced
    /// with the corresponding environment variable. Supports default values with
    /// `${VAR:-default}` syntax. Use `$$` to escape a literal `$`.
    env_subst: bool,

    /// Value schema for a field
    pub value: ConfigValueSchema,

    /// The field's default value, if any (from `#[facet(default)]` or `#[facet(default = ...)]`).
    /// Pre-serialized to ConfigValue during schema building.
    default: Option<crate::config_value::ConfigValue>,
}

/// Schema for a vec in a config value
#[derive(Facet, Debug, Clone)]
#[facet(skip_all_unless_truthy)]
pub struct ConfigVecSchema {
    /// Shape of the vector/list.
    shape: &'static Shape,

    /// Schema for the vec element
    element: Box<ConfigValueSchema>,
}

/// Schema for a value in the config struct
#[derive(Facet, Debug, Clone)]
#[facet(skip_all_unless_truthy)]
#[repr(u8)]
pub enum ConfigValueSchema {
    Struct(ConfigStructSchema),
    Vec(ConfigVecSchema),
    Option {
        value: Box<ConfigValueSchema>,
        shape: &'static Shape,
    },
    Enum(ConfigEnumSchema),
    Leaf(LeafSchema),
}

/// Schema for an enum in a config value (with struct variants)
#[derive(Facet, Debug, Clone)]
#[facet(skip_all_unless_truthy)]
pub struct ConfigEnumSchema {
    /// Shape of the enum.
    #[facet(skip)]
    shape: &'static Shape,

    /// Variants of the enum, keyed by effective name.
    variants: IndexMap<String, ConfigEnumVariantSchema, RandomState>,
}

/// Schema for an enum variant
#[derive(Facet, Debug, Clone)]
#[facet(skip_all_unless_truthy)]
pub struct ConfigEnumVariantSchema {
    /// Documentation for this variant.
    docs: Docs,

    /// Fields of this variant (empty for unit variants).
    fields: IndexMap<String, ConfigFieldSchema, RandomState>,
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
                ConfigValueSchema::Enum(e) => {
                    // For enums, the segment could be a variant name or a field within a variant
                    // Try to find the field in any variant
                    e.variants
                        .values()
                        .find_map(|v| v.fields.get(segment))
                        .map(|f| &f.value)?
                }
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
            ConfigValueSchema::Enum(e) => {
                for (name, variant) in &e.variants {
                    path.push(name.clone());
                    for (field_name, field) in &variant.fields {
                        path.push(field_name.clone());
                        field.value.visit(visitor, path);
                        path.pop();
                    }
                    path.pop();
                }
            }
            ConfigValueSchema::Leaf(_) => {}
        }
    }
}

// ============================================================================
// Accessor methods for parser2
// ============================================================================

impl Schema {
    /// Get the documentation for the top-level type.
    pub fn docs(&self) -> &Docs {
        &self.docs
    }

    /// Get the top-level arguments schema.
    pub fn args(&self) -> &ArgLevelSchema {
        &self.args
    }

    /// Get the config struct schema, if any.
    pub fn config(&self) -> Option<&ConfigStructSchema> {
        self.config.as_ref()
    }

    /// Get the special fields (help, version, completions) if detected.
    pub fn special(&self) -> &SpecialFields {
        &self.special
    }
}

impl ArgLevelSchema {
    /// Get the named/positional arguments at this level.
    /// Returns an `Args` wrapper that provides kebab-case lookup for CLI flags.
    pub fn args(&self) -> Args<'_> {
        Args { inner: &self.args }
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

    /// Check if the subcommand is optional (field type is `Option<Enum>`).
    pub fn subcommand_optional(&self) -> bool {
        self.subcommand_optional
    }

    /// Check if this level has any subcommands.
    pub fn has_subcommands(&self) -> bool {
        !self.subcommands.is_empty()
    }
}

impl Docs {
    /// Get the summary (first line of doc comment).
    pub fn summary(&self) -> Option<&str> {
        self.summary.as_deref()
    }

    /// Get the details (full doc comment after summary).
    pub fn details(&self) -> Option<&str> {
        self.details.as_deref()
    }
}

impl ArgKind {
    /// Get the short flag character if this is a named argument with a short flag.
    pub fn short(&self) -> Option<char> {
        match self {
            ArgKind::Named { short, .. } => *short,
            ArgKind::Positional => None,
        }
    }

    /// Check if this is a counted flag.
    pub fn is_counted(&self) -> bool {
        matches!(self, ArgKind::Named { counted: true, .. })
    }

    /// Check if this is a positional argument.
    pub fn is_positional(&self) -> bool {
        matches!(self, ArgKind::Positional)
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

    /// Get the documentation for this argument.
    pub fn docs(&self) -> &Docs {
        &self.docs
    }

    /// Get the custom type description for this argument, if any.
    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    /// Get the default value for this argument, if any.
    ///
    /// This returns the field's default from `#[facet(default)]` or `#[facet(default = ...)]`,
    /// pre-serialized to ConfigValue during schema building.
    pub fn default(&self) -> Option<&crate::config_value::ConfigValue> {
        self.default.as_ref()
    }
}

impl Subcommand {
    /// Get the CLI name (kebab-case, used for command-line matching).
    pub fn cli_name(&self) -> &str {
        &self.name
    }

    /// Get the effective name (respects `#[facet(rename = "...")]`).
    /// This is what facet-format expects for deserialization.
    pub fn effective_name(&self) -> &str {
        &self.effective_name
    }

    /// Get the arguments schema for this subcommand.
    pub fn args(&self) -> &ArgLevelSchema {
        &self.args
    }

    /// Check if this is a tuple variant with a flattened struct.
    /// When true, parsed fields need to be wrapped in a "0" field for deserialization.
    pub fn is_flattened_tuple(&self) -> bool {
        self.is_flattened_tuple
    }

    /// Get the documentation for this subcommand.
    pub fn docs(&self) -> &Docs {
        &self.docs
    }
}

impl ConfigStructSchema {
    /// Get the field name in the parent struct (e.g., "config").
    pub fn field_name(&self) -> Option<&str> {
        self.field_name.as_deref()
    }

    /// Get the environment variable prefix (e.g., "MYAPP").
    pub fn env_prefix(&self) -> Option<&str> {
        self.env_prefix.as_deref()
    }

    /// Get the fields of this config struct.
    pub fn fields(&self) -> &IndexMap<String, ConfigFieldSchema, RandomState> {
        &self.fields
    }

    /// Get the shape of this config struct.
    pub fn shape(&self) -> &'static Shape {
        self.shape
    }
}

impl ConfigFieldSchema {
    /// Get the value schema for this field.
    pub fn value(&self) -> &ConfigValueSchema {
        &self.value
    }

    /// Get the documentation for this field.
    pub fn docs(&self) -> &Docs {
        &self.docs
    }

    /// Check if this field contains sensitive data.
    pub fn is_sensitive(&self) -> bool {
        self.sensitive
    }

    /// Get environment variable aliases for this field.
    ///
    /// These are absolute env var names (like "DATABASE_URL") that can be used
    /// to set this field's value, in addition to the standard prefixed name.
    pub fn env_aliases(&self) -> &[String] {
        &self.env_aliases
    }

    /// Check if environment variable substitution is enabled for this field.
    ///
    /// When true, `${VAR}` patterns in the field's string value will be replaced
    /// with the corresponding environment variable.
    pub fn env_subst(&self) -> bool {
        self.env_subst
    }

    /// Get the default value for this field, if any.
    ///
    /// This returns the field's default from `#[facet(default)]` or `#[facet(default = ...)]`,
    /// pre-serialized to ConfigValue during schema building.
    pub fn default(&self) -> Option<&crate::config_value::ConfigValue> {
        self.default.as_ref()
    }
}

impl ConfigVecSchema {
    /// Get the element schema for this vec.
    pub fn element(&self) -> &ConfigValueSchema {
        &self.element
    }

    /// Get the shape of this vec.
    pub fn shape(&self) -> &'static Shape {
        self.shape
    }
}

impl ConfigEnumSchema {
    /// Get the variants of this enum.
    pub fn variants(&self) -> &IndexMap<String, ConfigEnumVariantSchema, RandomState> {
        &self.variants
    }

    /// Get a variant by name.
    pub fn get_variant(&self, name: &str) -> Option<&ConfigEnumVariantSchema> {
        self.variants.get(name)
    }

    /// Get the shape of this enum.
    pub fn shape(&self) -> &'static Shape {
        self.shape
    }
}

impl ConfigEnumVariantSchema {
    /// Get the fields of this variant.
    pub fn fields(&self) -> &IndexMap<String, ConfigFieldSchema, RandomState> {
        &self.fields
    }

    /// Get the documentation for this variant.
    pub fn docs(&self) -> &Docs {
        &self.docs
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

    /// Check if this is a boolean type or a Vec<bool>.
    /// Used for determining if a flag can appear multiple times as a bool accumulator.
    pub fn is_bool_or_vec_of_bool(&self) -> bool {
        match self {
            ValueSchema::Leaf(LeafSchema {
                kind: LeafKind::Scalar(ScalarType::Bool),
                ..
            }) => true,
            ValueSchema::Vec { element, .. } => element.is_bool(),
            _ => false,
        }
    }

    /// Unwrap [Option] wrapper if present, returning the inner schema.
    pub fn inner_if_option(&self) -> &ValueSchema {
        match self {
            ValueSchema::Option { value, .. } => value.as_ref(),
            other => other,
        }
    }

    /// Get the type identifier for display purposes (e.g., "STRING", "U16").
    /// Returns the innermost type's identifier, unwrapping [Option]/[Vec].
    pub fn type_identifier(&self) -> &'static str {
        match self {
            ValueSchema::Leaf(leaf) => leaf.shape.type_identifier,
            ValueSchema::Option { value, .. } => value.type_identifier(),
            ValueSchema::Vec { element, .. } => element.type_identifier(),
            ValueSchema::Struct { shape, .. } => shape.type_identifier,
        }
    }

    /// Check if this is an Option type.
    pub fn is_option(&self) -> bool {
        matches!(self, ValueSchema::Option { .. })
    }

    /// Get enum variants if this is an enum type.
    /// Returns [None] for non-enum types.
    pub fn enum_variants(&self) -> Option<&[String]> {
        match self {
            ValueSchema::Leaf(LeafSchema {
                kind: LeafKind::Enum { variants },
                ..
            }) => Some(variants.as_slice()),
            _ => None,
        }
    }
}

#[cfg(test)]
#[allow(dead_code)]
mod tests;
