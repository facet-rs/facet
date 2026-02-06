use crate::config_value::{ConfigValue, Sourced};

/// Coerce ConfigValue types based on the target shape.
/// This is needed because environment variables always come in as strings,
/// but we want to display them with their proper types (int, bool, etc.).
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
        ConfigValue::Enum(sourced) => {
            // For enum variants, recurse into the variant's fields
            let enum_type = match &shape.ty {
                facet_core::Type::User(facet_core::UserType::Enum(e)) => *e,
                _ => return value.clone(),
            };

            // Find the variant by effective_name
            let variant = enum_type
                .variants
                .iter()
                .find(|v| v.effective_name() == sourced.value.variant);

            let Some(variant) = variant else {
                return value.clone();
            };

            let variant_fields = variant.data.fields;

            // For tuple variants like Run(RunArgs) with a single struct field,
            // the fields in EnumValue are the inner struct's fields (flattened).
            let effective_fields: &[facet_core::Field] = if variant.data.kind
                == facet_core::StructKind::TupleStruct
                && variant_fields.len() == 1
            {
                let inner_shape = variant_fields[0].shape.get();
                if let facet_core::Type::User(facet_core::UserType::Struct(s)) = &inner_shape.ty {
                    s.fields
                } else {
                    variant_fields
                }
            } else {
                variant_fields
            };

            // Coerce each field value using its field shape
            let mut new_fields = sourced.value.fields.clone();
            for field in effective_fields {
                if let Some(val) = new_fields.get(field.name) {
                    let coerced = coerce_types_from_shape(val, field.shape.get());
                    new_fields.insert(field.name.to_string(), coerced);
                }
            }

            ConfigValue::Enum(Sourced {
                value: crate::config_value::EnumValue {
                    variant: sourced.value.variant.clone(),
                    fields: new_fields,
                },
                span: sourced.span,
                provenance: sourced.provenance.clone(),
            })
        }
        // Other types don't need coercion
        _ => value.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use facet::Facet;

    // Helper to create a string ConfigValue
    fn string_value(s: &str) -> ConfigValue {
        ConfigValue::String(Sourced {
            value: s.to_string(),
            span: None,
            provenance: None,
        })
    }

    // Helper to extract integer from ConfigValue
    fn get_integer(v: &ConfigValue) -> Option<i64> {
        match v {
            ConfigValue::Integer(sourced) => Some(sourced.value),
            _ => None,
        }
    }

    // Helper to extract bool from ConfigValue
    fn get_bool(v: &ConfigValue) -> Option<bool> {
        match v {
            ConfigValue::Bool(sourced) => Some(sourced.value),
            _ => None,
        }
    }

    // Helper to extract float from ConfigValue
    fn get_float(v: &ConfigValue) -> Option<f64> {
        match v {
            ConfigValue::Float(sourced) => Some(sourced.value),
            _ => None,
        }
    }

    // Helper to check if value is still a string
    fn is_string(v: &ConfigValue) -> bool {
        matches!(v, ConfigValue::String(_))
    }

    // Helper to check if value is an array
    fn is_array(v: &ConfigValue) -> bool {
        matches!(v, ConfigValue::Array(_))
    }

    // Helper to get array length
    fn array_len(v: &ConfigValue) -> Option<usize> {
        match v {
            ConfigValue::Array(sourced) => Some(sourced.value.len()),
            _ => None,
        }
    }

    // ========================================================================
    // String to integer coercion tests
    // ========================================================================

    #[test]
    fn test_coerce_string_to_i64() {
        #[derive(Facet)]
        struct Test {
            count: i64,
        }

        let value = string_value("42");
        let coerced = coerce_types_from_shape(&value, i64::SHAPE);
        assert_eq!(get_integer(&coerced), Some(42));
    }

    #[test]
    fn test_coerce_string_to_i32() {
        let value = string_value("1000");
        let coerced = coerce_types_from_shape(&value, i32::SHAPE);
        assert_eq!(get_integer(&coerced), Some(1000));
    }

    #[test]
    fn test_coerce_string_to_i16() {
        let value = string_value("255");
        let coerced = coerce_types_from_shape(&value, i16::SHAPE);
        assert_eq!(get_integer(&coerced), Some(255));
    }

    #[test]
    fn test_coerce_string_to_i8() {
        let value = string_value("100");
        let coerced = coerce_types_from_shape(&value, i8::SHAPE);
        assert_eq!(get_integer(&coerced), Some(100));
    }

    #[test]
    fn test_coerce_string_to_u64() {
        let value = string_value("999");
        let coerced = coerce_types_from_shape(&value, u64::SHAPE);
        assert_eq!(get_integer(&coerced), Some(999));
    }

    #[test]
    fn test_coerce_string_to_u32() {
        let value = string_value("65535");
        let coerced = coerce_types_from_shape(&value, u32::SHAPE);
        assert_eq!(get_integer(&coerced), Some(65535));
    }

    #[test]
    fn test_coerce_string_to_u16() {
        let value = string_value("8080");
        let coerced = coerce_types_from_shape(&value, u16::SHAPE);
        assert_eq!(get_integer(&coerced), Some(8080));
    }

    #[test]
    fn test_coerce_string_to_u8() {
        let value = string_value("200");
        let coerced = coerce_types_from_shape(&value, u8::SHAPE);
        assert_eq!(get_integer(&coerced), Some(200));
    }

    #[test]
    fn test_coerce_negative_string_to_signed() {
        let value = string_value("-42");
        let coerced = coerce_types_from_shape(&value, i64::SHAPE);
        assert_eq!(get_integer(&coerced), Some(-42));
    }

    #[test]
    fn test_coerce_negative_string_to_unsigned_fails() {
        // Negative numbers should stay as strings for unsigned types
        let value = string_value("-1");
        let coerced = coerce_types_from_shape(&value, u64::SHAPE);
        assert!(
            is_string(&coerced),
            "negative should stay as string for unsigned"
        );
    }

    #[test]
    fn test_coerce_out_of_range_i8_stays_string() {
        // Value too large for i8 should stay as string
        let value = string_value("999");
        let coerced = coerce_types_from_shape(&value, i8::SHAPE);
        assert!(is_string(&coerced), "out of range should stay as string");
    }

    #[test]
    fn test_coerce_invalid_integer_stays_string() {
        let value = string_value("not_a_number");
        let coerced = coerce_types_from_shape(&value, i64::SHAPE);
        assert!(is_string(&coerced));
    }

    // ========================================================================
    // String to bool coercion tests
    // ========================================================================

    #[test]
    fn test_coerce_string_true_to_bool() {
        let value = string_value("true");
        let coerced = coerce_types_from_shape(&value, bool::SHAPE);
        assert_eq!(get_bool(&coerced), Some(true));
    }

    #[test]
    fn test_coerce_string_false_to_bool() {
        let value = string_value("false");
        let coerced = coerce_types_from_shape(&value, bool::SHAPE);
        assert_eq!(get_bool(&coerced), Some(false));
    }

    #[test]
    fn test_coerce_invalid_bool_stays_string() {
        let value = string_value("yes");
        let coerced = coerce_types_from_shape(&value, bool::SHAPE);
        // "yes" is not valid for Rust's bool::parse, stays as string
        assert!(is_string(&coerced));
    }

    // ========================================================================
    // String to float coercion tests
    // ========================================================================

    #[test]
    fn test_coerce_string_to_f64() {
        let value = string_value("1.5");
        let coerced = coerce_types_from_shape(&value, f64::SHAPE);
        let float_val = get_float(&coerced);
        assert!(float_val.is_some());
        assert!((float_val.unwrap() - 1.5).abs() < 0.0001);
    }

    #[test]
    fn test_coerce_string_to_f32() {
        let value = string_value("2.5");
        let coerced = coerce_types_from_shape(&value, f32::SHAPE);
        let float_val = get_float(&coerced);
        assert!(float_val.is_some());
        assert!((float_val.unwrap() - 2.5).abs() < 0.0001);
    }

    #[test]
    fn test_coerce_integer_string_to_float() {
        let value = string_value("42");
        let coerced = coerce_types_from_shape(&value, f64::SHAPE);
        let float_val = get_float(&coerced);
        assert!(float_val.is_some());
        assert!((float_val.unwrap() - 42.0).abs() < 0.0001);
    }

    // ========================================================================
    // List wrapping tests
    // ========================================================================

    #[test]
    fn test_coerce_scalar_to_vec() {
        let value = string_value("42");
        let coerced = coerce_types_from_shape(&value, <Vec<i64>>::SHAPE);

        // Should wrap in array
        assert!(is_array(&coerced));
        assert_eq!(array_len(&coerced), Some(1));

        // Element should be coerced to integer
        if let ConfigValue::Array(arr) = &coerced {
            assert_eq!(get_integer(&arr.value[0]), Some(42));
        }
    }

    #[test]
    fn test_coerce_array_stays_array() {
        let arr = ConfigValue::Array(Sourced {
            value: vec![string_value("1"), string_value("2"), string_value("3")],
            span: None,
            provenance: None,
        });

        let coerced = coerce_types_from_shape(&arr, <Vec<i64>>::SHAPE);

        // Should stay as array
        assert!(is_array(&coerced));
        assert_eq!(array_len(&coerced), Some(3));

        // Elements should be coerced to integers
        if let ConfigValue::Array(arr) = &coerced {
            assert_eq!(get_integer(&arr.value[0]), Some(1));
            assert_eq!(get_integer(&arr.value[1]), Some(2));
            assert_eq!(get_integer(&arr.value[2]), Some(3));
        }
    }

    // ========================================================================
    // Struct coercion tests
    // ========================================================================

    #[test]
    fn test_coerce_struct_fields() {
        use indexmap::IndexMap;

        #[derive(Facet)]
        struct Config {
            port: u16,
            debug: bool,
        }

        let mut map = IndexMap::default();
        map.insert("port".to_string(), string_value("8080"));
        map.insert("debug".to_string(), string_value("true"));

        let obj = ConfigValue::Object(Sourced {
            value: map,
            span: None,
            provenance: None,
        });

        let coerced = coerce_types_from_shape(&obj, Config::SHAPE);

        if let ConfigValue::Object(sourced) = coerced {
            assert_eq!(get_integer(sourced.value.get("port").unwrap()), Some(8080));
            assert_eq!(get_bool(sourced.value.get("debug").unwrap()), Some(true));
        } else {
            panic!("expected object");
        }
    }

    // ========================================================================
    // Edge cases
    // ========================================================================

    #[test]
    fn test_coerce_already_correct_type_unchanged() {
        let value = ConfigValue::Integer(Sourced {
            value: 42,
            span: None,
            provenance: None,
        });
        let coerced = coerce_types_from_shape(&value, i64::SHAPE);
        assert_eq!(get_integer(&coerced), Some(42));
    }

    #[test]
    fn test_coerce_bool_value_unchanged() {
        let value = ConfigValue::Bool(Sourced {
            value: true,
            span: None,
            provenance: None,
        });
        let coerced = coerce_types_from_shape(&value, bool::SHAPE);
        assert_eq!(get_bool(&coerced), Some(true));
    }

    #[test]
    fn test_coerce_empty_string_stays_string() {
        let value = string_value("");
        let coerced = coerce_types_from_shape(&value, i64::SHAPE);
        // Empty string doesn't parse as integer, stays as string
        assert!(is_string(&coerced));
    }
}
