//! Parse YAML strings into Rust values.

#[cfg(not(feature = "alloc"))]
compile_error!("feature `alloc` is required");

mod error;

use alloc::{
    format,
    string::{String, ToString},
};
use error::AnyErr;
use facet_core::{Characteristic, Def, Facet, FieldFlags, Type, UserType};
use facet_reflect::Partial;
use yaml_rust2::{Yaml, YamlLoader};

/// Deserializes a YAML string into a value of type `T` that implements `Facet`.
pub fn from_str<'input: 'facet, 'facet, T: Facet<'facet>>(yaml: &'input str) -> Result<T, AnyErr> {
    let mut typed_partial = Partial::alloc::<T>()?;
    {
        let wip = typed_partial.inner_mut();
        from_str_value(wip, yaml)?;
    }
    let boxed_value = typed_partial.build().map_err(|e| AnyErr(e.to_string()))?;
    Ok(*boxed_value)
}

fn yaml_type(ty: &Yaml) -> &'static str {
    match ty {
        Yaml::Real(_) => "real number",
        Yaml::Integer(_) => "integer",
        Yaml::String(_) => "string",
        Yaml::Boolean(_) => "boolean",
        Yaml::Array(_) => "array",
        Yaml::Hash(_) => "hash/map",
        Yaml::Alias(_) => "alias",
        Yaml::Null => "null",
        Yaml::BadValue => "bad value",
    }
}

fn yaml_to_u64(ty: &Yaml) -> Result<u64, AnyErr> {
    match ty {
        Yaml::Real(r) => r
            .parse::<u64>()
            .map_err(|_| AnyErr("Failed to parse real as u64".into())),
        Yaml::Integer(i) => Ok(*i as u64),
        Yaml::String(s) => s
            .parse::<u64>()
            .map_err(|_| AnyErr("Failed to parse string as u64".into())),
        Yaml::Boolean(b) => Ok(if *b { 1 } else { 0 }),
        _ => Err(AnyErr(format!("Cannot convert {} to u64", yaml_type(ty)))),
    }
}

fn from_str_value<'facet, 'shape>(
    wip: &mut Partial<'facet, 'shape>,
    yaml: &str,
) -> Result<(), AnyErr> {
    let docs = YamlLoader::load_from_str(yaml).map_err(|e| e.to_string())?;
    if docs.len() != 1 {
        return Err("Expected exactly one YAML document".into());
    }
    deserialize_value(wip, &docs[0])?;
    Ok(())
}

fn deserialize_value<'facet, 'shape>(
    wip: &mut Partial<'facet, 'shape>,
    value: &Yaml,
) -> Result<(), AnyErr> {
    // Get the shape
    let shape = wip.shape();
    let innermost_shape = wip.innermost_shape();

    #[cfg(feature = "log")]
    {
        log::debug!(
            "deserialize_value: shape={}, innermost_shape={}",
            shape,
            innermost_shape
        );
        log::debug!("Shape type: {:?}", shape.ty);
        log::debug!("Shape attributes: {:?}", shape.attributes);
        log::debug!("YAML value: {:?}", value);
    }

    // Handle transparent types - check if shape has the transparent attribute
    if shape
        .attributes
        .contains(&facet_core::ShapeAttribute::Transparent)
    {
        #[cfg(feature = "log")]
        log::debug!("Handling facet(transparent) type");

        // For transparent types, push inner and deserialize as inner type
        wip.push_inner().map_err(|e| AnyErr(e.to_string()))?;
        deserialize_value(wip, value)?;
        wip.end().map_err(|e| AnyErr(e.to_string()))?;
        return Ok(());
    }

    // First check the type system (Type)
    if let Type::User(UserType::Struct(sd)) = &shape.ty {
        if let Yaml::Hash(hash) = value {
            // Process all fields in the YAML map
            for (k, v) in hash {
                let k = k
                    .as_str()
                    .ok_or_else(|| AnyErr(format!("Expected string key, got: {}", yaml_type(k))))?;
                let field_index = wip
                    .field_index(k)
                    .ok_or_else(|| AnyErr(format!("Field '{}' not found", k)))?;

                #[cfg(feature = "log")]
                log::debug!("Processing struct field '{}' (index: {})", k, field_index);

                wip.begin_nth_field(field_index)
                    .map_err(|e| AnyErr(format!("Field '{}' error: {}", k, e)))?;
                deserialize_value(wip, v)?;
                wip.end().map_err(|e| AnyErr(e.to_string()))?;
            }

            // Process any unset fields with defaults
            for (index, field) in sd.fields.iter().enumerate() {
                let is_set = wip.is_field_set(index).map_err(|e| AnyErr(e.to_string()))?;
                if !is_set {
                    // If field has default attribute, apply it
                    if field.flags.contains(FieldFlags::DEFAULT) {
                        #[cfg(feature = "log")]
                        log::debug!("Setting default for field: {}", field.name);

                        wip.begin_nth_field(index)
                            .map_err(|e| AnyErr(e.to_string()))?;

                        // Check for field-level default function first, then type-level default
                        if let Some(field_default_fn) = field.vtable.default_fn {
                            #[cfg(feature = "log")]
                            log::debug!("Using field default function for field: {}", field.name);

                            wip.set_field_default(field_default_fn)
                                .map_err(|e| AnyErr(e.to_string()))?;
                        } else if field.shape().is(Characteristic::Default) {
                            #[cfg(feature = "log")]
                            log::debug!("Using type Default for field: {}", field.name);

                            wip.set_default().map_err(|e| AnyErr(e.to_string()))?;
                        } else {
                            return Err(AnyErr(format!(
                                "Field '{}' has default attribute but its type doesn't implement Default and no default function provided",
                                field.name
                            )));
                        }

                        wip.end().map_err(|e| AnyErr(e.to_string()))?;
                    }
                }
            }

            // Handle struct-level defaults using the safe API from facet-reflect
            wip.fill_unset_fields_from_default()
                .map_err(|e| AnyErr(e.to_string()))?;
        } else {
            return Err(AnyErr(format!("Expected a YAML hash, got: {:?}", value)));
        }
        return Ok(());
    }

    // Then check the def system (Def) using innermost_shape instead of shape
    // This handles transparent types automatically by using the wrapped type
    match innermost_shape.def {
        Def::Scalar(scalar_def) => {
            #[cfg(feature = "log")]
            log::debug!(
                "Processing scalar type with affinity: {:?}",
                scalar_def.affinity
            );

            // Handle numeric types with proper conversion
            use facet_core::{IntegerSize, NumberBits, ScalarAffinity, Signedness};
            if let ScalarAffinity::Number(num_affinity) = scalar_def.affinity {
                match num_affinity.bits {
                    NumberBits::Integer { size, sign } => match (size, sign) {
                        (IntegerSize::Fixed(bits), Signedness::Unsigned) => {
                            let u = yaml_to_u64(value)?;
                            match bits {
                                8 => {
                                    let val = u8::try_from(u).map_err(|_| {
                                        AnyErr(format!("Value {} out of range for u8", u))
                                    })?;
                                    wip.set(val).map_err(|e| AnyErr(e.to_string()))?;
                                }
                                16 => {
                                    let val = u16::try_from(u).map_err(|_| {
                                        AnyErr(format!("Value {} out of range for u16", u))
                                    })?;
                                    wip.set(val).map_err(|e| AnyErr(e.to_string()))?;
                                }
                                32 => {
                                    let val = u32::try_from(u).map_err(|_| {
                                        AnyErr(format!("Value {} out of range for u32", u))
                                    })?;
                                    wip.set(val).map_err(|e| AnyErr(e.to_string()))?;
                                }
                                64 => {
                                    wip.set(u).map_err(|e| AnyErr(e.to_string()))?;
                                }
                                128 => {
                                    let val = u128::from(u);
                                    wip.set(val).map_err(|e| AnyErr(e.to_string()))?;
                                }
                                _ => {
                                    return Err(AnyErr(format!(
                                        "Unsupported fixed unsigned integer size: {}",
                                        bits
                                    )));
                                }
                            }
                        }
                        (IntegerSize::PointerSized, Signedness::Unsigned) => {
                            let u = yaml_to_u64(value)?;
                            let val = usize::try_from(u).map_err(|_| {
                                AnyErr(format!("Value {} out of range for usize", u))
                            })?;
                            wip.set(val).map_err(|e| AnyErr(e.to_string()))?;
                        }
                        (IntegerSize::Fixed(bits), Signedness::Signed) => {
                            let i = match value {
                                Yaml::Integer(i) => *i,
                                Yaml::Real(r) => r
                                    .parse::<i64>()
                                    .map_err(|_| AnyErr("Failed to parse real as i64".into()))?,
                                Yaml::String(s) => s
                                    .parse::<i64>()
                                    .map_err(|_| AnyErr("Failed to parse string as i64".into()))?,
                                Yaml::Boolean(b) => {
                                    if *b {
                                        1
                                    } else {
                                        0
                                    }
                                }
                                _ => {
                                    return Err(AnyErr(format!(
                                        "Cannot convert {} to i64",
                                        yaml_type(value)
                                    )));
                                }
                            };
                            match bits {
                                8 => {
                                    let val = i8::try_from(i).map_err(|_| {
                                        AnyErr(format!("Value {} out of range for i8", i))
                                    })?;
                                    wip.set(val).map_err(|e| AnyErr(e.to_string()))?;
                                }
                                16 => {
                                    let val = i16::try_from(i).map_err(|_| {
                                        AnyErr(format!("Value {} out of range for i16", i))
                                    })?;
                                    wip.set(val).map_err(|e| AnyErr(e.to_string()))?;
                                }
                                32 => {
                                    let val = i32::try_from(i).map_err(|_| {
                                        AnyErr(format!("Value {} out of range for i32", i))
                                    })?;
                                    wip.set(val).map_err(|e| AnyErr(e.to_string()))?;
                                }
                                64 => {
                                    wip.set(i).map_err(|e| AnyErr(e.to_string()))?;
                                }
                                128 => {
                                    let val = i128::from(i);
                                    wip.set(val).map_err(|e| AnyErr(e.to_string()))?;
                                }
                                _ => {
                                    return Err(AnyErr(format!(
                                        "Unsupported fixed signed integer size: {}",
                                        bits
                                    )));
                                }
                            }
                        }
                        (IntegerSize::PointerSized, Signedness::Signed) => {
                            let i = match value {
                                Yaml::Integer(i) => *i,
                                Yaml::Real(r) => r
                                    .parse::<i64>()
                                    .map_err(|_| AnyErr("Failed to parse real as i64".into()))?,
                                Yaml::String(s) => s
                                    .parse::<i64>()
                                    .map_err(|_| AnyErr("Failed to parse string as i64".into()))?,
                                Yaml::Boolean(b) => {
                                    if *b {
                                        1
                                    } else {
                                        0
                                    }
                                }
                                _ => {
                                    return Err(AnyErr(format!(
                                        "Cannot convert {} to i64",
                                        yaml_type(value)
                                    )));
                                }
                            };
                            let val = isize::try_from(i).map_err(|_| {
                                AnyErr(format!("Value {} out of range for isize", i))
                            })?;
                            wip.set(val).map_err(|e| AnyErr(e.to_string()))?;
                        }
                    },
                    NumberBits::Float {
                        sign_bits: _,
                        exponent_bits: _,
                        mantissa_bits,
                        has_explicit_first_mantissa_bit: _,
                    } => {
                        // Handle floating point numbers
                        let f = match value {
                            Yaml::Real(r) => r
                                .parse::<f64>()
                                .map_err(|_| AnyErr("Failed to parse real as f64".into()))?,
                            Yaml::Integer(i) => *i as f64,
                            Yaml::String(s) => s
                                .parse::<f64>()
                                .map_err(|_| AnyErr("Failed to parse string as f64".into()))?,
                            _ => {
                                return Err(AnyErr(format!(
                                    "Cannot convert {} to f64",
                                    yaml_type(value)
                                )));
                            }
                        };
                        // Determine float type based on mantissa bits (f32 has 23, f64 has 52)
                        if mantissa_bits <= 23 {
                            let val = f as f32;
                            wip.set(val).map_err(|e| AnyErr(e.to_string()))?;
                        } else {
                            wip.set(f).map_err(|e| AnyErr(e.to_string()))?;
                        }
                    }
                    NumberBits::Fixed { .. } | NumberBits::Decimal { .. } => {
                        return Err(AnyErr(
                            "Fixed and decimal number types not supported in YAML deserializer"
                                .into(),
                        ));
                    }
                    _ => {
                        return Err(AnyErr(
                            "Unsupported number type in YAML deserializer".into(),
                        ));
                    }
                }
            } else if innermost_shape.is_type::<bool>() {
                // Handle boolean values
                let b = match value {
                    Yaml::Boolean(b) => *b,
                    Yaml::Integer(i) => *i != 0,
                    Yaml::String(s) => {
                        let s = s.to_lowercase();
                        s == "true" || s == "yes" || s == "1"
                    }
                    _ => {
                        return Err(AnyErr(format!(
                            "Cannot convert {} to bool",
                            yaml_type(value)
                        )));
                    }
                };
                wip.set(b).map_err(|e| AnyErr(e.to_string()))?;
            } else if innermost_shape.is_type::<String>() {
                // For strings, set directly
                let s = value
                    .as_str()
                    .ok_or_else(|| AnyErr(format!("Expected string, got: {}", yaml_type(value))))?
                    .to_string();
                wip.set(s).map_err(|e| AnyErr(e.to_string()))?;
            } else {
                // Try parse_from_str first for any scalar type that supports it
                let s = value
                    .as_str()
                    .ok_or_else(|| AnyErr(format!("Expected string, got: {}", yaml_type(value))))?;
                if wip.parse_from_str(s).is_err() {
                    // If parsing fails, fall back to setting as String
                    wip.set(s.to_string()).map_err(|e| AnyErr(e.to_string()))?;
                }
            }
        }
        Def::List(_) => {
            #[cfg(feature = "log")]
            log::debug!("Processing list type");

            deserialize_as_list(wip, value)?;
        }
        Def::Map(_) => {
            #[cfg(feature = "log")]
            log::debug!("Processing map type");

            deserialize_as_map(wip, value)?;
        }
        Def::Option(_) => {
            #[cfg(feature = "log")]
            log::debug!("Processing option type");

            // Handle Option<T>
            if let Yaml::Null = value {
                // Null maps to None - already handled by default
            } else {
                // Non-null maps to Some(value)
                wip.push_some().map_err(|e| AnyErr(e.to_string()))?;
                deserialize_value(wip, value)?;
                wip.end().map_err(|e| AnyErr(e.to_string()))?;
            }
        }
        // Enum has been moved to Type system
        _ => return Err(AnyErr(format!("Unsupported type: {:?}", shape))),
    }
    Ok(())
}

fn deserialize_as_list<'facet, 'shape>(
    wip: &mut Partial<'facet, 'shape>,
    value: &Yaml,
) -> Result<(), AnyErr> {
    #[cfg(feature = "log")]
    log::debug!("deserialize_as_list: shape={}", wip.shape());

    if let Yaml::Array(array) = value {
        // Start the list
        wip.begin_list().map_err(|e| AnyErr(e.to_string()))?;

        // Handle empty list - just return without adding items
        if array.is_empty() {
            return Ok(());
        }

        // Process each element
        for element in array.iter() {
            #[cfg(feature = "log")]
            log::debug!("Processing list element: {:?}", element);

            // Push element
            wip.begin_list_item().map_err(|e| AnyErr(e.to_string()))?;
            deserialize_value(wip, element)?;
            wip.end().map_err(|e| AnyErr(e.to_string()))?;
        }

        Ok(())
    } else {
        Err(AnyErr(format!(
            "Expected a YAML array, got: {}",
            yaml_type(value)
        )))
    }
}

fn deserialize_as_map<'facet, 'shape>(
    wip: &mut Partial<'facet, 'shape>,
    value: &Yaml,
) -> Result<(), AnyErr> {
    if let Yaml::Hash(hash) = value {
        // Start the map
        wip.begin_map().map_err(|e| AnyErr(e.to_string()))?;

        // Handle empty map
        if hash.is_empty() {
            return Ok(());
        }

        // Process each key-value pair
        for (k, v) in hash {
            // Get the key as a string
            let key_str = k
                .as_str()
                .ok_or_else(|| AnyErr(format!("Expected string key, got: {}", yaml_type(k))))?;

            // Push map key
            wip.begin_key().map_err(|e| AnyErr(e.to_string()))?;
            wip.set(key_str.to_string())
                .map_err(|e| AnyErr(e.to_string()))?;
            wip.end().map_err(|e| AnyErr(e.to_string()))?;

            // Push map value
            wip.begin_value().map_err(|e| AnyErr(e.to_string()))?;
            deserialize_value(wip, v)?;
            wip.end().map_err(|e| AnyErr(e.to_string()))?;
        }

        Ok(())
    } else {
        Err(AnyErr(format!(
            "Expected a YAML hash/map, got: {}",
            yaml_type(value)
        )))
    }
}
