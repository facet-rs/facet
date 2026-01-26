use crate::config_value::{ConfigValue, Sourced};

/// Coerce ConfigValue types based on the target shape.
/// This is needed because environment variables always come in as strings,
/// but we want to display them with their proper types (int, bool, etc).
pub(crate) fn coerce_types_from_shape(
    value: &ConfigValue,
    shape: &'static facet_core::Shape,
) -> ConfigValue {
    tracing::trace!(
        shape = shape.type_identifier,
        ?value,
        "coerce_types_from_shape: entering"
    );

    // If target is a Vec/List but value is not an array, wrap it
    // (but not for Option<T> which also has shape.inner)
    if matches!(shape.def, facet_core::Def::List(_)) && !matches!(value, ConfigValue::Array(_)) {
        let element_shape = shape.inner.unwrap();
        let coerced_element = coerce_types_from_shape(value, element_shape);
        return ConfigValue::Array(Sourced {
            value: vec![coerced_element],
            span: None,
            provenance: None,
        });
    }

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
            tracing::trace!(
                scalar_type = ?shape.scalar_type(),
                string_value = %sourced.value,
                "coerce_types_from_shape: trying to coerce string"
            );
            if let Some(scalar) = shape.scalar_type() {
                match scalar {
                    facet_core::ScalarType::I8 => {
                        if let Ok(num) = sourced.value.parse::<i64>()
                            && num >= i8::MIN as i64
                            && num <= i8::MAX as i64
                        {
                            tracing::trace!(num, "coerce_types_from_shape: coerced to i8");
                            return ConfigValue::Integer(Sourced {
                                value: num,
                                span: sourced.span,
                                provenance: sourced.provenance.clone(),
                            });
                        }
                        // Value out of range - leave as string to get proper error
                    }
                    facet_core::ScalarType::I16 => {
                        if let Ok(num) = sourced.value.parse::<i64>()
                            && num >= i16::MIN as i64
                            && num <= i16::MAX as i64
                        {
                            tracing::trace!(num, "coerce_types_from_shape: coerced to i16");
                            return ConfigValue::Integer(Sourced {
                                value: num,
                                span: sourced.span,
                                provenance: sourced.provenance.clone(),
                            });
                        }
                    }
                    facet_core::ScalarType::I32 => {
                        if let Ok(num) = sourced.value.parse::<i64>()
                            && num >= i32::MIN as i64
                            && num <= i32::MAX as i64
                        {
                            tracing::trace!(num, "coerce_types_from_shape: coerced to i32");
                            return ConfigValue::Integer(Sourced {
                                value: num,
                                span: sourced.span,
                                provenance: sourced.provenance.clone(),
                            });
                        }
                    }
                    facet_core::ScalarType::I64 | facet_core::ScalarType::I128 => {
                        if let Ok(num) = sourced.value.parse::<i64>() {
                            tracing::trace!(num, "coerce_types_from_shape: coerced to i64");
                            return ConfigValue::Integer(Sourced {
                                value: num,
                                span: sourced.span,
                                provenance: sourced.provenance.clone(),
                            });
                        }
                    }
                    facet_core::ScalarType::U8 => {
                        if let Ok(num) = sourced.value.parse::<i64>()
                            && num >= 0
                            && num <= u8::MAX as i64
                        {
                            tracing::trace!(num, "coerce_types_from_shape: coerced to u8");
                            return ConfigValue::Integer(Sourced {
                                value: num,
                                span: sourced.span,
                                provenance: sourced.provenance.clone(),
                            });
                        }
                    }
                    facet_core::ScalarType::U16 => {
                        if let Ok(num) = sourced.value.parse::<i64>()
                            && num >= 0
                            && num <= u16::MAX as i64
                        {
                            tracing::trace!(num, "coerce_types_from_shape: coerced to u16");
                            return ConfigValue::Integer(Sourced {
                                value: num,
                                span: sourced.span,
                                provenance: sourced.provenance.clone(),
                            });
                        }
                    }
                    facet_core::ScalarType::U32 => {
                        if let Ok(num) = sourced.value.parse::<i64>()
                            && num >= 0
                            && num <= u32::MAX as i64
                        {
                            tracing::trace!(num, "coerce_types_from_shape: coerced to u32");
                            return ConfigValue::Integer(Sourced {
                                value: num,
                                span: sourced.span,
                                provenance: sourced.provenance.clone(),
                            });
                        }
                    }
                    facet_core::ScalarType::U64 | facet_core::ScalarType::U128 => {
                        if let Ok(num) = sourced.value.parse::<i64>()
                            && num >= 0
                        {
                            tracing::trace!(num, "coerce_types_from_shape: coerced to u64");
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
