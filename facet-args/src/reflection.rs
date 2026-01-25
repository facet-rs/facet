use crate::{
    Attr,
    config_value::{ConfigValue, Sourced},
};

/// Check if a field is marked with `args::counted`.
#[deprecated(note = "move to schema/from_schema.rs")]
pub(crate) fn is_counted_field(field: &facet_core::Field) -> bool {
    field.has_attr(Some("args"), "counted")
}

/// Check if a shape is a supported type for counted fields (integer types).
#[deprecated(note = "move to schema/from_schema.rs")]
pub(crate) const fn is_supported_counted_type(shape: &'static facet_core::Shape) -> bool {
    use facet_core::{NumericType, PrimitiveType, Type};
    matches!(
        shape.ty,
        Type::Primitive(PrimitiveType::Numeric(NumericType::Integer { .. }))
    )
}

/// Check if a field is marked with `args::config`.
#[deprecated(note = "move to schema/from_schema.rs")]
pub(crate) fn is_config_field(field: &facet_core::Field) -> bool {
    field.has_attr(Some("args"), "config")
}

/// Find the config field in a struct shape, if any.
#[deprecated(note = "move to schema/from_schema.rs")]
pub(crate) fn find_config_field(
    shape: &'static facet_core::Shape,
) -> Option<&'static facet_core::Field> {
    use facet_core::{Type, UserType};

    match &shape.ty {
        Type::User(UserType::Struct(s)) => s.fields.iter().find(|field| is_config_field(field)),
        _ => None,
    }
}

/// Get the env_prefix value from a field's attributes.
#[deprecated(note = "move to schema/from_schema.rs")]
pub(crate) fn get_env_prefix(field: &facet_core::Field) -> Option<&'static str> {
    let attr = field.get_attr(Some("args"), "env_prefix")?;
    let parsed = attr.get_as::<crate::Attr>()?;

    if let Attr::EnvPrefix(prefix_opt) = parsed {
        *prefix_opt
    } else {
        None
    }
}

/// Coerce ConfigValue types based on the target shape.
/// This is needed because environment variables always come in as strings,
/// but we want to display them with their proper types (int, bool, etc).
pub(crate) fn coerce_types_from_shape(
    value: &ConfigValue,
    shape: &'static facet_core::Shape,
) -> ConfigValue {
    match value {
        ConfigValue::Object(sourced) => {
            let mut new_map = sourced.value.clone();

            if let facet_core::Type::User(facet_core::UserType::Struct(s)) = &shape.ty {
                for field in s.fields {
                    if let Some(val) = new_map.get(field.name) {
                        let coerced = coerce_types_from_shape(val, field.shape.get());
                        new_map.insert(field.name.to_string(), coerced);
                    }
                }
            } else {
                // No struct info, just recurse on all values
                for (key, val) in sourced.value.iter() {
                    let coerced = coerce_types_from_shape(val, shape);
                    new_map.insert(key.clone(), coerced);
                }
            }

            ConfigValue::Object(Sourced {
                value: new_map,
                span: sourced.span,
                provenance: sourced.provenance.clone(),
            })
        }
        ConfigValue::Array(sourced) => {
            let element_shape = shape.inner.unwrap_or(shape);
            let new_items: Vec<ConfigValue> = sourced
                .value
                .iter()
                .map(|item| coerce_types_from_shape(item, element_shape))
                .collect();

            ConfigValue::Array(Sourced {
                value: new_items,
                span: sourced.span,
                provenance: sourced.provenance.clone(),
            })
        }
        ConfigValue::String(sourced) => {
            // Try to coerce string to the target type
            if let Some(scalar) = shape.scalar_type() {
                match scalar {
                    facet_core::ScalarType::I8
                    | facet_core::ScalarType::I16
                    | facet_core::ScalarType::I32
                    | facet_core::ScalarType::I64
                    | facet_core::ScalarType::I128 => {
                        if let Ok(num) = sourced.value.parse::<i64>() {
                            return ConfigValue::Integer(Sourced {
                                value: num,
                                span: sourced.span,
                                provenance: sourced.provenance.clone(),
                            });
                        }
                    }
                    facet_core::ScalarType::U8
                    | facet_core::ScalarType::U16
                    | facet_core::ScalarType::U32
                    | facet_core::ScalarType::U64
                    | facet_core::ScalarType::U128 => {
                        if let Ok(num) = sourced.value.parse::<i64>() {
                            return ConfigValue::Integer(Sourced {
                                value: num,
                                span: sourced.span,
                                provenance: sourced.provenance.clone(),
                            });
                        }
                    }
                    facet_core::ScalarType::F32 | facet_core::ScalarType::F64 => {
                        if let Ok(num) = sourced.value.parse::<f64>() {
                            return ConfigValue::Float(Sourced {
                                value: num,
                                span: sourced.span,
                                provenance: sourced.provenance.clone(),
                            });
                        }
                    }
                    facet_core::ScalarType::Bool => {
                        if let Ok(b) = sourced.value.parse::<bool>() {
                            return ConfigValue::Bool(Sourced {
                                value: b,
                                span: sourced.span,
                                provenance: sourced.provenance.clone(),
                            });
                        }
                    }
                    _ => {}
                }
            }
            // Keep as string if coercion fails or not needed
            value.clone()
        }
        // Other types don't need coercion
        _ => value.clone(),
    }
}
