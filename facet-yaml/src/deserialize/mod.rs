//! Parse YAML strings into Rust values.

#[cfg(not(feature = "alloc"))]
compile_error!("feature `alloc` is required");

mod error;

use alloc::{
    format,
    string::{String, ToString},
};
use error::AnyErr;
use facet_core::{Characteristic, Def, Facet, FieldFlags, ScalarAffinity, Type, UserType};
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
    // Get both the direct shape and innermost shape (for transparent types)
    let shape = wip.shape();
    let innermost_shape = wip.innermost_shape();
    let is_transparent = shape != innermost_shape;

    #[cfg(feature = "log")]
    {
        log::debug!(
            "deserialize_value: shape={}, innermost_shape={}, transparent={}",
            shape,
            innermost_shape,
            is_transparent
        );
        log::debug!("YAML value: {:?}", value);
    }

    // Handle transparent types that wrap String
    if is_transparent && innermost_shape.is_type::<String>() {
        #[cfg(feature = "log")]
        log::debug!("Handling transparent String wrapper");

        if let Yaml::String(s) = value {
            wip.set(s.to_string()).map_err(|e| AnyErr(e.to_string()))?;
            return Ok(());
        }
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

                        // Use set_default which internally handles custom default functions safely
                        if field.shape().is(Characteristic::Default)
                            || field.vtable.default_fn.is_some()
                        {
                            #[cfg(feature = "log")]
                            log::debug!("Using Default for field: {}", field.name);

                            wip.set_default().map_err(|e| AnyErr(e.to_string()))?;
                        } else {
                            return Err(AnyErr(format!(
                                "Field '{}' has default attribute but its type doesn't implement Default",
                                field.name
                            )));
                        }

                        wip.end().map_err(|e| AnyErr(e.to_string()))?;
                    }
                }
            }

            // Note: Struct-level defaults would require unsafe code to copy fields,
            // which is not allowed in facet-yaml due to deny(unsafe_code).
            // Individual field defaults are handled above.
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

            // For type conversions like String → OffsetDateTime, simply put the string value.
            // The Wip system will use the target type's try_from vtable function to handle
            // the conversion automatically. This works for time types, UUIDs, paths, etc.
            if innermost_shape.is_type::<u64>()
                || innermost_shape.is_type::<u32>()
                || innermost_shape.is_type::<u16>()
                || innermost_shape.is_type::<u8>()
                || innermost_shape.is_type::<usize>()
            {
                let u = yaml_to_u64(value)?;
                wip.set(u).map_err(|e| AnyErr(e.to_string()))?;
            } else if innermost_shape.is_type::<i64>()
                || innermost_shape.is_type::<i32>()
                || innermost_shape.is_type::<i16>()
                || innermost_shape.is_type::<i8>()
                || innermost_shape.is_type::<isize>()
            {
                // Handle signed integers
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
                wip.set(i).map_err(|e| AnyErr(e.to_string()))?;
            } else if innermost_shape.is_type::<f64>() || innermost_shape.is_type::<f32>() {
                // Handle floating point numbers
                let f = match value {
                    Yaml::Real(r) => r
                        .parse::<f64>()
                        .map_err(|_| AnyErr("Failed to parse real as f64".into()))?,
                    Yaml::Integer(i) => *i as f64,
                    Yaml::String(s) => s
                        .parse::<f64>()
                        .map_err(|_| AnyErr("Failed to parse string as f64".into()))?,
                    Yaml::Boolean(b) => {
                        if *b {
                            1.0
                        } else {
                            0.0
                        }
                    }
                    _ => {
                        return Err(AnyErr(format!(
                            "Cannot convert {} to f64",
                            yaml_type(value)
                        )));
                    }
                };
                wip.set(f).map_err(|e| AnyErr(e.to_string()))?;
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
            } else if innermost_shape.is_type::<String>()
                || matches!(scalar_def.affinity, ScalarAffinity::Time(_))
                || matches!(scalar_def.affinity, ScalarAffinity::UUID(_))
                || matches!(scalar_def.affinity, ScalarAffinity::ULID(_))
                || matches!(scalar_def.affinity, ScalarAffinity::Path(_))
            {
                // For strings and types with special affinity, parse from string
                let s = value
                    .as_str()
                    .ok_or_else(|| AnyErr(format!("Expected string, got: {}", yaml_type(value))))?
                    .to_string();
                wip.set(s).map_err(|e| AnyErr(e.to_string()))?;
            } else {
                return Err(AnyErr(format!(
                    "facet-yaml: unsupported scalar type: {}",
                    shape
                )));
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
