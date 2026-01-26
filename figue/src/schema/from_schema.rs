use std::{collections::HashMap, hash::RandomState};

use crate::{
    Attr,
    schema::{
        ArgKind, ArgLevelSchema, ArgSchema, ConfigEnumSchema, ConfigEnumVariantSchema,
        ConfigFieldSchema, ConfigStructSchema, ConfigValueSchema, ConfigVecSchema, Docs, LeafKind,
        LeafSchema, ScalarType, Schema, SpecialFields, Subcommand, ValueSchema,
        error::{SchemaError, SchemaErrorContext},
    },
};
use facet::{
    Def, EnumType, Facet, Field, ScalarType as FacetScalarType, Shape, StructKind, Type, UserType,
    Variant,
};
use heck::ToKebabCase;
use indexmap::IndexMap;

impl Schema {
    /// Parse a schema from a given shape
    pub(crate) fn from_shape(shape: &'static Shape) -> Result<Self, SchemaError> {
        let struct_type = match &shape.ty {
            Type::User(UserType::Struct(s)) => *s,
            _ => {
                return Err(SchemaError::new(
                    SchemaErrorContext::root(shape),
                    "top-level shape must be a struct",
                ));
            }
        };

        let ctx_root = SchemaErrorContext::root(shape);
        let mut config_field: Option<(&'static Field, SchemaErrorContext)> = None;

        for field in struct_type.fields {
            let field_ctx = ctx_root.with_field(field.name);

            if is_config_field(field) {
                if let Some((_, first_ctx)) = &config_field {
                    return Err(SchemaError::new(
                        first_ctx.clone(),
                        "only one field may be marked with #[facet(args::config)]",
                    )
                    .with_primary_label("first marked here")
                    .with_label(field_ctx, "also marked here"));
                }
                config_field = Some((field, field_ctx.clone()));
            }

            if field.has_attr(Some("args"), "env_prefix") && !field.has_attr(Some("args"), "config")
            {
                return Err(SchemaError::new(
                    field_ctx,
                    format!(
                        "field `{}` uses args::env_prefix without args::config",
                        field.name
                    ),
                ));
            }
        }

        let (args, special) = arg_level_from_fields_with_special(struct_type.fields, &ctx_root)?;

        let config = if let Some((field, field_ctx)) = config_field {
            let shape = field.shape();
            let config_shape = match shape.def {
                Def::Option(opt) => opt.t,
                _ => shape,
            };
            // Extract env_prefix from the config field's attributes
            let env_prefix = extract_env_prefix(field);
            Some(config_struct_schema_from_shape(
                config_shape,
                &field_ctx,
                Some(field.name.to_string()),
                env_prefix,
            )?)
        } else {
            None
        };

        // Extract docs from the top-level shape
        let docs = docs_from_lines(shape.doc);

        Ok(Schema {
            docs,
            args,
            config,
            special,
        })
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

/// Extract the env_prefix value from a field's `#[facet(args::env_prefix = "...")]` attribute.
fn extract_env_prefix(field: &Field) -> Option<String> {
    let attr = field.get_attr(Some("args"), "env_prefix")?;
    let parsed = attr.get_as::<crate::Attr>()?;

    if let crate::Attr::EnvPrefix(prefix_opt) = parsed {
        prefix_opt.map(|s| s.to_string())
    } else {
        None
    }
}

/// Extract all env_alias values from a field's `#[facet(args::env_alias = "...")]` attributes.
/// Multiple aliases can be specified by using the attribute multiple times:
/// `#[facet(args::env_alias = "A", args::env_alias = "B")]`
fn extract_env_aliases(field: &Field) -> Vec<String> {
    let mut aliases = Vec::new();
    // Iterate through all attributes to find all env_alias entries
    for field_attr in field.attributes {
        if field_attr.ns == Some("args") && field_attr.key == "env_alias" {
            // The attribute data is stored as &str directly
            if let Some(s) = field_attr.get_as::<&str>() {
                aliases.push(s.to_string());
            }
        }
    }
    aliases
}

/// Check if a field has `#[facet(args::env_subst)]` attribute.
fn has_env_subst(field: &Field) -> bool {
    field.has_attr(Some("args"), "env_subst")
}

/// Check if a shape (struct) has `#[facet(args::env_subst_all)]` attribute.
fn has_env_subst_all(shape: &'static Shape) -> bool {
    shape
        .attributes
        .iter()
        .any(|attr| attr.ns == Some("args") && attr.key == "env_subst_all")
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
    enum_type.variants.iter().map(variant_cli_name).collect()
}

fn variant_cli_name(variant: &Variant) -> String {
    variant.effective_name().to_kebab_case()
}

fn leaf_schema_from_shape(
    shape: &'static Shape,
    _ctx: &SchemaErrorContext,
) -> Result<LeafSchema, SchemaError> {
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
        // Fallback: treat as Other and let deserialization handle it
        _ => Ok(LeafSchema {
            kind: LeafKind::Scalar(ScalarType::Other),
            shape,
        }),
    }
}

fn value_schema_from_shape(
    shape: &'static Shape,
    ctx: &SchemaErrorContext,
) -> Result<ValueSchema, SchemaError> {
    match shape.def {
        Def::Option(opt) => Ok(ValueSchema::Option {
            value: Box::new(value_schema_from_shape(opt.t, ctx)?),
            shape,
        }),
        Def::List(list) => Ok(ValueSchema::Vec {
            element: Box::new(value_schema_from_shape(list.t, ctx)?),
            shape,
        }),
        _ => match &shape.ty {
            Type::User(UserType::Struct(_)) => Ok(ValueSchema::Struct {
                fields: config_struct_schema_from_shape(shape, ctx, None, None)?,
                shape,
            }),
            _ => Ok(ValueSchema::Leaf(leaf_schema_from_shape(shape, ctx)?)),
        },
    }
}

fn config_value_schema_from_shape(
    shape: &'static Shape,
    ctx: &SchemaErrorContext,
) -> Result<ConfigValueSchema, SchemaError> {
    match shape.def {
        Def::Option(opt) => Ok(ConfigValueSchema::Option {
            value: Box::new(config_value_schema_from_shape(opt.t, ctx)?),
            shape,
        }),
        Def::List(list) => Ok(ConfigValueSchema::Vec(ConfigVecSchema {
            element: Box::new(config_value_schema_from_shape(list.t, ctx)?),
            shape,
        })),
        _ => match &shape.ty {
            Type::User(UserType::Struct(_)) => Ok(ConfigValueSchema::Struct(
                config_struct_schema_from_shape(shape, ctx, None, None)?,
            )),
            Type::User(UserType::Enum(enum_type)) => Ok(ConfigValueSchema::Enum(
                config_enum_schema_from_shape(shape, *enum_type, ctx)?,
            )),
            _ => Ok(ConfigValueSchema::Leaf(leaf_schema_from_shape(shape, ctx)?)),
        },
    }
}

fn config_enum_schema_from_shape(
    shape: &'static Shape,
    enum_type: facet::EnumType,
    ctx: &SchemaErrorContext,
) -> Result<ConfigEnumSchema, SchemaError> {
    let mut variants: IndexMap<String, ConfigEnumVariantSchema, RandomState> = IndexMap::default();

    for variant in enum_type.variants {
        let variant_ctx = ctx.with_variant(variant.name.to_string());
        let docs = docs_from_lines(variant.doc);

        let mut fields: IndexMap<String, ConfigFieldSchema, RandomState> = IndexMap::default();

        // Handle struct variants
        for field in variant.data.fields {
            let field_ctx = variant_ctx.with_field(field.name);
            let field_docs = docs_from_lines(field.doc);
            let sensitive = field.flags.contains(facet_core::FieldFlags::SENSITIVE);
            let env_aliases = extract_env_aliases(field);
            let env_subst = has_env_subst(field);
            let value = config_value_schema_from_shape(field.shape(), &field_ctx)?;

            fields.insert(
                field.effective_name().to_string(),
                ConfigFieldSchema {
                    docs: field_docs,
                    sensitive,
                    env_aliases,
                    env_subst,
                    value,
                },
            );
        }

        variants.insert(
            variant.effective_name().to_string(),
            ConfigEnumVariantSchema { docs, fields },
        );
    }

    Ok(ConfigEnumSchema { shape, variants })
}

fn config_struct_schema_from_shape(
    shape: &'static Shape,
    ctx: &SchemaErrorContext,
    field_name: Option<String>,
    env_prefix: Option<String>,
) -> Result<ConfigStructSchema, SchemaError> {
    config_struct_schema_from_shape_inner(shape, ctx, field_name, env_prefix, Vec::new(), false)
}

fn config_struct_schema_from_shape_inner(
    shape: &'static Shape,
    ctx: &SchemaErrorContext,
    field_name: Option<String>,
    env_prefix: Option<String>,
    path_prefix: Vec<String>,
    parent_env_subst_all: bool,
) -> Result<ConfigStructSchema, SchemaError> {
    let struct_type = match &shape.ty {
        Type::User(UserType::Struct(s)) => *s,
        _ => {
            return Err(SchemaError::new(
                ctx.clone(),
                "config field must be a struct",
            ));
        }
    };

    // Check if this struct has env_subst_all - applies to direct children only
    let this_env_subst_all = has_env_subst_all(shape);
    // For direct children, env_subst is enabled if either:
    // - The parent passed down env_subst_all (for flattened fields)
    // - This struct has env_subst_all
    let apply_env_subst_to_children = parent_env_subst_all || this_env_subst_all;

    let mut fields_map: IndexMap<String, ConfigFieldSchema, RandomState> = IndexMap::default();

    for field in struct_type.fields {
        let field_ctx = ctx.with_field(field.name);

        // Handle flattened fields - recurse into the inner struct and merge fields
        if field.is_flattened() {
            let inner_shape = field.shape();
            let _inner_struct = match &inner_shape.ty {
                Type::User(UserType::Struct(s)) => *s,
                _ => {
                    return Err(SchemaError::new(
                        field_ctx,
                        format!("flattened config field `{}` must be a struct", field.name),
                    ));
                }
            };

            // Build the new path prefix including this field's effective name
            let mut new_prefix = path_prefix.clone();
            new_prefix.push(field.effective_name().to_string());

            // Recursively process the inner struct's fields
            // Nested flattened structs don't have their own env_prefix
            // Pass down env_subst_all so flattened fields inherit it (they become direct children)
            let inner = config_struct_schema_from_shape_inner(
                inner_shape,
                &field_ctx,
                None,
                None,
                new_prefix,
                apply_env_subst_to_children,
            )?;

            // Merge the inner fields into our fields (checking for conflicts)
            for (name, field_schema) in inner.fields {
                if fields_map.contains_key(&name) {
                    return Err(SchemaError::new(
                        field_ctx.clone(),
                        format!(
                            "duplicate config field `{}` (from flattened field `{}`)",
                            name, field.name
                        ),
                    ));
                }
                fields_map.insert(name, field_schema);
            }

            continue;
        }

        // Non-flattened field
        let docs = docs_from_lines(field.doc);
        let sensitive = field.flags.contains(facet_core::FieldFlags::SENSITIVE);
        let env_aliases = extract_env_aliases(field);
        let value = config_value_schema_from_shape(field.shape(), &field_ctx)?;

        // env_subst is enabled if:
        // - The field has #[facet(args::env_subst)] directly, OR
        // - The parent struct has env_subst_all (applies to direct children)
        let env_subst = has_env_subst(field) || apply_env_subst_to_children;

        // Use the effective (serialized) name as the key
        let effective_name = field.effective_name().to_string();

        fields_map.insert(
            effective_name,
            ConfigFieldSchema {
                docs,
                sensitive,
                env_aliases,
                env_subst,
                value,
            },
        );
    }

    // Check for conflicting env aliases across all fields
    check_env_alias_conflicts(&fields_map, ctx)?;

    Ok(ConfigStructSchema {
        field_name,
        env_prefix,
        shape,
        fields: fields_map,
    })
}

/// Check that no two fields share the same env alias.
fn check_env_alias_conflicts(
    fields: &IndexMap<String, ConfigFieldSchema, RandomState>,
    ctx: &SchemaErrorContext,
) -> Result<(), SchemaError> {
    use std::collections::HashMap;

    // Map from alias to the field name that uses it
    let mut alias_to_field: HashMap<&str, &str> = HashMap::new();

    for (field_name, field_schema) in fields.iter() {
        for alias in field_schema.env_aliases() {
            if let Some(existing_field) = alias_to_field.get(alias.as_str()) {
                return Err(SchemaError::new(
                    ctx.clone(),
                    format!(
                        "env alias `{}` is used by both `{}` and `{}`",
                        alias, existing_field, field_name
                    ),
                ));
            }
            alias_to_field.insert(alias.as_str(), field_name.as_str());
        }
    }

    Ok(())
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
    if is_flattened_tuple_variant(variant) {
        let inner_shape = fields[0].shape();
        if let Type::User(UserType::Struct(struct_type)) = inner_shape.ty {
            return struct_type.fields;
        }
    }
    fields
}

/// Check if this variant is a tuple variant with a single struct field that gets flattened.
/// E.g., `Bench(BenchArgs)` where `BenchArgs` is a struct.
fn is_flattened_tuple_variant(variant: &Variant) -> bool {
    let fields = variant.data.fields;
    if variant.data.kind == StructKind::TupleStruct && fields.len() == 1 {
        let inner_shape = fields[0].shape();
        matches!(inner_shape.ty, Type::User(UserType::Struct(_)))
    } else {
        false
    }
}

fn arg_level_from_fields(
    fields: &'static [Field],
    ctx: &SchemaErrorContext,
) -> Result<ArgLevelSchema, SchemaError> {
    let (args, _special) = arg_level_from_fields_with_prefix(fields, ctx, Vec::new())?;
    Ok(args)
}

fn arg_level_from_fields_with_special(
    fields: &'static [Field],
    ctx: &SchemaErrorContext,
) -> Result<(ArgLevelSchema, SpecialFields), SchemaError> {
    arg_level_from_fields_with_prefix(fields, ctx, Vec::new())
}

fn arg_level_from_fields_with_prefix(
    fields: &'static [Field],
    ctx: &SchemaErrorContext,
    path_prefix: Vec<String>,
) -> Result<(ArgLevelSchema, SpecialFields), SchemaError> {
    let mut args: IndexMap<String, ArgSchema, RandomState> = IndexMap::default();
    let mut subcommands: IndexMap<String, Subcommand, RandomState> = IndexMap::default();
    let mut subcommand_field_name: Option<String> = None;
    let mut subcommand_optional: bool = false;
    let mut special = SpecialFields::default();

    let mut seen_long: HashMap<String, SchemaErrorContext> = HashMap::new();
    let mut seen_short: HashMap<char, SchemaErrorContext> = HashMap::new();
    let mut seen_subcommands: HashMap<String, SchemaErrorContext> = HashMap::new();

    let mut first_subcommand_field: Option<SchemaErrorContext> = None;

    for field in fields {
        if is_config_field(field) {
            continue;
        }

        let field_ctx = ctx.with_field(field.name);

        // Handle flattened fields - recurse into the inner struct
        if field.is_flattened() {
            let inner_shape = field.shape();
            let struct_type = match &inner_shape.ty {
                Type::User(UserType::Struct(s)) => *s,
                _ => {
                    return Err(SchemaError::new(
                        field_ctx,
                        format!("flattened field `{}` must be a struct", field.name),
                    ));
                }
            };

            // For flatten, fields appear at the CURRENT level in ConfigValue,
            // so we pass the same path_prefix (don't add the field name)
            let (inner, inner_special) = arg_level_from_fields_with_prefix(
                struct_type.fields,
                &field_ctx,
                path_prefix.clone(),
            )?;

            // Merge special fields from the inner struct
            if inner_special.help.is_some() {
                special.help = inner_special.help;
            }
            if inner_special.version.is_some() {
                special.version = inner_special.version;
            }
            if inner_special.completions.is_some() {
                special.completions = inner_special.completions;
            }

            // Merge the inner args into our args (checking for conflicts)
            for (name, arg) in inner.args {
                if let Some(existing_ctx) = seen_long.get(&name) {
                    return Err(SchemaError::new(
                        existing_ctx.clone(),
                        format!("duplicate flag `--{}` (from flattened field)", name),
                    )
                    .with_primary_label("first defined here")
                    .with_label(field_ctx.clone(), "flattened here"));
                }
                seen_long.insert(name.clone(), field_ctx.clone());

                if let ArgKind::Named { short: Some(c), .. } = &arg.kind {
                    if let Some(existing_ctx) = seen_short.get(c) {
                        return Err(SchemaError::new(
                            existing_ctx.clone(),
                            format!("duplicate flag `-{}` (from flattened field)", c),
                        )
                        .with_primary_label("first defined here")
                        .with_label(field_ctx.clone(), "flattened here"));
                    }
                    seen_short.insert(*c, field_ctx.clone());
                }

                args.insert(name, arg);
            }

            // Merge subcommands too
            for (name, sub) in inner.subcommands {
                if let Some(existing_ctx) = seen_subcommands.get(&name) {
                    return Err(SchemaError::new(
                        existing_ctx.clone(),
                        format!("duplicate subcommand `{}` (from flattened field)", name),
                    )
                    .with_primary_label("first defined here")
                    .with_label(field_ctx.clone(), "flattened here"));
                }
                seen_subcommands.insert(name.clone(), field_ctx.clone());
                subcommands.insert(name, sub);
            }

            // If the inner level had a subcommand field, propagate it
            if inner.subcommand_field_name.is_some() {
                if first_subcommand_field.is_some() {
                    return Err(SchemaError::new(
                        field_ctx,
                        "multiple subcommand fields via flatten",
                    ));
                }
                first_subcommand_field = Some(field_ctx.clone());
                subcommand_field_name = inner.subcommand_field_name;
                subcommand_optional = inner.subcommand_optional;
            }

            continue;
        }

        if !has_any_args_attr(field) {
            return Err(SchemaError::new(
                field_ctx,
                format!(
                    "field `{}` is missing a #[facet(args::...)] annotation",
                    field.name
                ),
            ));
        }

        if field.has_attr(Some("args"), "env_prefix") && !field.has_attr(Some("args"), "config") {
            return Err(SchemaError::new(
                field_ctx,
                format!(
                    "field `{}` uses args::env_prefix without args::config",
                    field.name
                ),
            ));
        }

        let is_positional = field.has_attr(Some("args"), "positional");
        let is_subcommand = field.has_attr(Some("args"), "subcommand");

        if field.has_attr(Some("args"), "short") && is_positional {
            return Err(SchemaError::new(
                field_ctx,
                "#[facet(args::positional)] is not compatible with #[facet(args::short)]",
            )
            .with_primary_label("has both attributes"));
        }

        if is_counted_field(field) && !is_supported_counted_type(field.shape()) {
            return Err(SchemaError::new(
                field_ctx,
                format!(
                    "field `{}` marked as counted must be an integer",
                    field.name
                ),
            ));
        }

        if is_subcommand {
            if let Some(first_ctx) = &first_subcommand_field {
                return Err(SchemaError::new(
                    first_ctx.clone(),
                    "only one field may be marked with #[facet(args::subcommand)] at this level",
                )
                .with_primary_label("first marked here")
                .with_label(field_ctx, "also marked here"));
            }
            first_subcommand_field = Some(field_ctx.clone());
            subcommand_field_name = Some(field.name.to_string());

            let field_shape = field.shape();
            let (enum_shape, enum_type, is_optional) = match field_shape.def {
                Def::Option(opt) => match opt.t.ty {
                    Type::User(UserType::Enum(enum_type)) => (opt.t, enum_type, true),
                    _ => {
                        return Err(SchemaError::new(
                            field_ctx,
                            format!(
                                "field `{}` marked as subcommand must be an enum",
                                field.name
                            ),
                        ));
                    }
                },
                _ => match field_shape.ty {
                    Type::User(UserType::Enum(enum_type)) => (field_shape, enum_type, false),
                    _ => {
                        return Err(SchemaError::new(
                            field_ctx,
                            format!(
                                "field `{}` marked as subcommand must be an enum",
                                field.name
                            ),
                        ));
                    }
                },
            };
            subcommand_optional = is_optional;

            for variant in enum_type.variants {
                let cli_name = variant_cli_name(variant);
                // effective_name respects #[facet(rename = "...")], used for deserialization
                let effective_name = variant.effective_name().to_string();
                let docs = docs_from_lines(variant.doc);
                let variant_fields = variant_fields_for_schema(variant);
                let variant_ctx =
                    SchemaErrorContext::root(enum_shape).with_variant(cli_name.clone());
                let args_schema = arg_level_from_fields(variant_fields, &variant_ctx)?;
                let is_flattened_tuple = is_flattened_tuple_variant(variant);

                let sub = Subcommand {
                    name: cli_name.clone(),
                    effective_name,
                    docs,
                    args: args_schema,
                    is_flattened_tuple,
                    shape: enum_shape,
                };

                if let Some(existing_ctx) = seen_subcommands.get(&cli_name) {
                    return Err(SchemaError::new(
                        existing_ctx.clone(),
                        format!("duplicate subcommand name `{cli_name}`"),
                    )
                    .with_primary_label("first defined here")
                    .with_label(variant_ctx, "defined again here"));
                }
                seen_subcommands.insert(cli_name.clone(), variant_ctx.clone());
                subcommands.insert(cli_name.clone(), sub);
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

        let value = value_schema_from_shape(field.shape(), &field_ctx)?;

        // Struct types in args must be flattened - CLI can't represent nested structs
        // without dotted path syntax (which is only for args::config fields)
        if matches!(value, ValueSchema::Struct { .. }) {
            return Err(SchemaError::new(
                field_ctx.clone(),
                "struct fields in args must use #[facet(flatten)]",
            )
            .with_primary_label("this field is a struct type")
            .with_label(
                field_ctx.clone(),
                "add #[facet(flatten)] to include its fields at this level",
            ));
        }

        #[allow(clippy::nonminimal_bool)]
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
            if let Some(existing_ctx) = seen_long.get(&long) {
                return Err(SchemaError::new(
                    existing_ctx.clone(),
                    format!("duplicate flag `--{long}`"),
                )
                .with_primary_label(format!("`--{long}` first defined here"))
                .with_label(field_ctx.clone(), "defined again here"));
            }
            seen_long.insert(long.clone(), field_ctx.clone());

            if let Some(c) = short {
                if let Some(existing_ctx) = seen_short.get(&c) {
                    return Err(SchemaError::new(
                        existing_ctx.clone(),
                        format!("duplicate flag `-{c}`"),
                    )
                    .with_primary_label(format!("`-{c}` first defined here"))
                    .with_label(field_ctx.clone(), "defined again here"));
                }
                seen_short.insert(c, field_ctx.clone());
            }
        }

        let docs = docs_from_lines(field.doc);
        let effective_name = field.effective_name().to_string();

        // Build the full path for this field (for special field detection)
        // The path uses the EFFECTIVE name (what appears in ConfigValue)
        let mut field_path = path_prefix.clone();
        field_path.push(effective_name.clone());

        // Detect special fields by ATTRIBUTE, not field name
        if field.has_attr(Some("args"), "help") {
            special.help = Some(field_path.clone());
        }
        if field.has_attr(Some("args"), "version") {
            special.version = Some(field_path.clone());
        }
        if field.has_attr(Some("args"), "completions") {
            special.completions = Some(field_path.clone());
        }

        let arg = ArgSchema {
            name: effective_name.clone(),
            docs,
            kind,
            value,
            required,
            multiple,
        };

        args.insert(effective_name, arg);
    }

    Ok((
        ArgLevelSchema {
            args,
            subcommands,
            subcommand_field_name,
            subcommand_optional,
        },
        special,
    ))
}

/// Check if a field is marked with `args::counted`.
fn is_counted_field(field: &facet_core::Field) -> bool {
    field.has_attr(Some("args"), "counted")
}

/// Check if a shape is a supported type for counted fields (integer types).
const fn is_supported_counted_type(shape: &'static facet_core::Shape) -> bool {
    use facet_core::{NumericType, PrimitiveType, Type};
    matches!(
        shape.ty,
        Type::Primitive(PrimitiveType::Numeric(NumericType::Integer { .. }))
    )
}

/// Check if a field is marked with `args::config`.
fn is_config_field(field: &facet_core::Field) -> bool {
    field.has_attr(Some("args"), "config")
}
