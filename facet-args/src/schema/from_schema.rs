use std::{collections::HashSet, hash::RandomState};

use crate::{
    Attr,
    reflection::{is_config_field, is_counted_field, is_supported_counted_type},
    schema::{
        ArgKind, ArgLevelSchema, ArgSchema, ConfigFieldSchema, ConfigStructSchema,
        ConfigValueSchema, ConfigVecSchema, Docs, LeafKind, LeafSchema, ScalarType, Schema,
        Subcommand, ValueSchema,
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
                return Err(SchemaError::TopLevelNotStruct {
                    ctx: SchemaErrorContext::root(shape),
                });
            }
        };

        let ctx_root = SchemaErrorContext::root(shape);
        let mut config_field: Option<&'static Field> = None;

        for field in struct_type.fields {
            let field_ctx = ctx_root.with_field(field.name);

            if is_config_field(field) {
                if config_field.is_some() {
                    return Err(SchemaError::MultipleConfigFields {
                        ctx: field_ctx,
                        field: field.name,
                    });
                }
                config_field = Some(field);
            }

            if field.has_attr(Some("args"), "env_prefix") && !field.has_attr(Some("args"), "config")
            {
                return Err(SchemaError::EnvPrefixWithoutConfig {
                    ctx: field_ctx,
                    field: field.name,
                });
            }
        }

        let args = arg_level_from_fields(struct_type.fields, &ctx_root)?;

        let config = if let Some(field) = config_field {
            let field_ctx = ctx_root.with_field(field.name);
            let shape = field.shape();
            let config_shape = match shape.def {
                Def::Option(opt) => opt.t,
                _ => shape,
            };
            Some(config_struct_schema_from_shape(config_shape, &field_ctx)?)
        } else {
            None
        };

        Ok(Schema { args, config })
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

fn leaf_schema_from_shape(
    shape: &'static Shape,
    ctx: &SchemaErrorContext,
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
        _ => Err(SchemaError::UnsupportedLeafType { ctx: ctx.clone() }),
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
                fields: config_struct_schema_from_shape(shape, ctx)?,
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
                config_struct_schema_from_shape(shape, ctx)?,
            )),
            _ => Ok(ConfigValueSchema::Leaf(leaf_schema_from_shape(shape, ctx)?)),
        },
    }
}

fn config_struct_schema_from_shape(
    shape: &'static Shape,
    ctx: &SchemaErrorContext,
) -> Result<ConfigStructSchema, SchemaError> {
    let struct_type = match &shape.ty {
        Type::User(UserType::Struct(s)) => *s,
        _ => return Err(SchemaError::ConfigFieldMustBeStruct { ctx: ctx.clone() }),
    };

    let mut fields_map: IndexMap<String, ConfigFieldSchema, RandomState> = IndexMap::default();
    for field in struct_type.fields {
        let docs = docs_from_lines(field.doc);
        let field_ctx = ctx.with_field(field.name);
        let value = config_value_schema_from_shape(field.shape(), &field_ctx)?;
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

fn arg_level_from_fields(
    fields: &'static [Field],
    ctx: &SchemaErrorContext,
) -> Result<ArgLevelSchema, SchemaError> {
    let mut args: IndexMap<String, ArgSchema, RandomState> = IndexMap::default();
    let mut subcommands: IndexMap<String, Subcommand, RandomState> = IndexMap::default();

    let mut seen_long = HashSet::new();
    let mut seen_short = HashSet::new();

    let mut saw_subcommand = false;

    for field in fields {
        if is_config_field(field) {
            continue;
        }

        let field_ctx = ctx.with_field(field.name);

        if !has_any_args_attr(field) {
            return Err(SchemaError::MissingArgsAnnotation {
                ctx: field_ctx,
                field: field.name,
            });
        }

        if field.has_attr(Some("args"), "env_prefix") && !field.has_attr(Some("args"), "config") {
            return Err(SchemaError::EnvPrefixWithoutConfig {
                ctx: field_ctx,
                field: field.name,
            });
        }

        let is_positional = field.has_attr(Some("args"), "positional");
        let is_subcommand = field.has_attr(Some("args"), "subcommand");

        if field.has_attr(Some("args"), "short") && is_positional {
            return Err(SchemaError::ShortOnPositional {
                ctx: field_ctx,
                field: field.name,
            });
        }

        if is_counted_field(field) && !is_supported_counted_type(field.shape()) {
            return Err(SchemaError::CountedOnNonInteger {
                ctx: field_ctx,
                field: field.name,
            });
        }

        if is_subcommand {
            if saw_subcommand {
                return Err(SchemaError::MultipleSubcommandFields {
                    ctx: field_ctx,
                    field: field.name,
                });
            }
            saw_subcommand = true;

            let field_shape = field.shape();
            let (enum_shape, enum_type) = match field_shape.def {
                Def::Option(opt) => match opt.t.ty {
                    Type::User(UserType::Enum(enum_type)) => (opt.t, enum_type),
                    _ => {
                        return Err(SchemaError::SubcommandOnNonEnum {
                            ctx: field_ctx,
                            field: field.name,
                        });
                    }
                },
                _ => match field_shape.ty {
                    Type::User(UserType::Enum(enum_type)) => (field_shape, enum_type),
                    _ => {
                        return Err(SchemaError::SubcommandOnNonEnum {
                            ctx: field_ctx,
                            field: field.name,
                        });
                    }
                },
            };

            for variant in enum_type.variants {
                let name = variant_cli_name(variant);
                let docs = docs_from_lines(variant.doc);
                let variant_fields = variant_fields_for_schema(variant);
                let variant_ctx = SchemaErrorContext::root(enum_shape).with_variant(name.clone());
                let args_schema = arg_level_from_fields(variant_fields, &variant_ctx)?;

                let sub = Subcommand {
                    name: name.clone(),
                    docs,
                    args: args_schema,
                    shape: enum_shape,
                };

                if subcommands.insert(name.clone(), sub).is_some() {
                    return Err(SchemaError::ConflictingFlagNames {
                        ctx: variant_ctx,
                        name,
                    });
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

        let value = value_schema_from_shape(field.shape(), &field_ctx)?;
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
                    ctx: field_ctx.clone(),
                    name: format!("--{long}"),
                });
            }
            if let Some(c) = short {
                if !seen_short.insert(c) {
                    return Err(SchemaError::ConflictingFlagNames {
                        ctx: field_ctx.clone(),
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
