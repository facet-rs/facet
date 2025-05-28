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
    let wip = Partial::alloc::<T>()?;
    let wip = from_str_value(wip, yaml)?;
    let heap_value = wip.build().map_err(|e| AnyErr(e.to_string()))?;
    heap_value
        .materialize::<T>()
        .map_err(|e| AnyErr(e.to_string()))
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
    wip: Partial<'facet, 'shape>,
    yaml: &str,
) -> Result<Partial<'facet, 'shape>, AnyErr> {
    let docs = YamlLoader::load_from_str(yaml).map_err(|e| e.to_string())?;
    if docs.len() != 1 {
        return Err("Expected exactly one YAML document".into());
    }
    deserialize_value(wip, &docs[0])
}

fn deserialize_value<'facet, 'shape>(
    mut wip: Partial<'facet, 'shape>,
    value: &Yaml,
) -> Result<Partial<'facet, 'shape>, AnyErr> {
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
            wip = wip.put(s.to_string()).map_err(|e| AnyErr(e.to_string()))?;
            return Ok(wip);
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

                wip = wip
                    .field(field_index)
                    .map_err(|e| AnyErr(format!("Field '{}' error: {}", k, e)))?;
                wip = deserialize_value(wip, v)?;
                wip = wip.end().map_err(|e| AnyErr(e.to_string()))?;
            }

            // Process any unset fields with defaults
            let mut has_unset = false;
            for (index, field) in sd.fields.iter().enumerate() {
                let is_set = wip.is_field_set(index).map_err(|e| AnyErr(e.to_string()))?;
                if !is_set {
                    has_unset = true;

                    // If field has default attribute, apply it
                    if field.flags.contains(FieldFlags::DEFAULT) {
                        #[cfg(feature = "log")]
                        log::debug!("Setting default for field: {}", field.name);

                        wip = wip.field(index).map_err(|e| AnyErr(e.to_string()))?;

                        // Check for custom default function
                        if let Some(default_in_place_fn) = field.vtable.default_fn {
                            #[cfg(feature = "log")]
                            log::debug!("Using custom default function for field: {}", field.name);

                            wip = wip
                                .put_from_fn(default_in_place_fn)
                                .map_err(|e| AnyErr(e.to_string()))?;
                        } else {
                            // Use regular Default implementation
                            if !field.shape().is(Characteristic::Default) {
                                return Err(AnyErr(format!(
                                    "Field '{}' has default attribute but its type doesn't implement Default",
                                    field.name
                                )));
                            }

                            #[cfg(feature = "log")]
                            log::debug!("Using Default impl for field: {}", field.name);

                            // Create a default instance but use the field_shape instead of just wip.put_default()
                            // This is needed to correctly process fields with default values in nested structs
                            let field_shape = field.shape();

                            // Check if this is a struct type that might have fields with default values
                            if let Type::User(UserType::Struct(_)) = &field_shape.ty {
                                // Process the nested struct with defaults
                                #[cfg(feature = "log")]
                                log::debug!(
                                    "Processing nested struct with defaults: {}",
                                    field.name
                                );

                                // For handling nested structs, we need to be careful with our approach.
                                // We'll just use the standard Default implementation and let the system
                                // handle the custom field defaults during creation. The facet_core system will
                                // handle the custom field defaults during the Default::default() call.

                                #[cfg(feature = "log")]
                                log::debug!(
                                    "Using Default impl for nested struct field: {}",
                                    field.name
                                );

                                wip = wip.put_default().map_err(|e| AnyErr(e.to_string()))?;
                            } else {
                                // Simple default for non-struct types
                                wip = wip.put_default().map_err(|e| AnyErr(e.to_string()))?;
                            }
                        }

                        wip = wip.end().map_err(|e| AnyErr(e.to_string()))?;
                    }
                }
            }

            // If there are still unset fields and the struct has a default attribute,
            // create a default instance and copy any remaining unset fields from it
            if has_unset && shape.has_default_attr() {
                #[cfg(feature = "log")]
                log::debug!("Using struct-level default");

                // Create default instance
                let default_val = Partial::alloc_shape(shape)
                    .map_err(|e| AnyErr(e.to_string()))?
                    .put_default()
                    .map_err(|e| AnyErr(e.to_string()))?
                    .build()
                    .map_err(|e| AnyErr(e.to_string()))?;

                let peek = default_val.peek().into_struct().unwrap();

                // Copy unset fields from default instance
                for (index, field) in sd.fields.iter().enumerate() {
                    let is_set = wip.is_field_set(index).map_err(|e| AnyErr(e.to_string()))?;
                    if !is_set {
                        #[cfg(feature = "log")]
                        log::debug!("Copying default for field: {}", field.name);

                        let address_of_field_from_default = peek.field(index).unwrap().data();
                        wip = wip.field(index).map_err(|e| AnyErr(e.to_string()))?;
                        wip = wip
                            .put_shape(address_of_field_from_default, field.shape())
                            .map_err(|e| AnyErr(e.to_string()))?;
                        wip = wip.end().map_err(|e| AnyErr(e.to_string()))?;
                    }
                }
            }
        } else {
            return Err(AnyErr(format!("Expected a YAML hash, got: {:?}", value)));
        }
        return Ok(wip);
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
                wip = wip.put(u).map_err(|e| AnyErr(e.to_string()))?;
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
                wip = wip.put(i).map_err(|e| AnyErr(e.to_string()))?;
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
                wip = wip.put(f).map_err(|e| AnyErr(e.to_string()))?;
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
                wip = wip.put(b).map_err(|e| AnyErr(e.to_string()))?;
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
                wip = wip.put(s).map_err(|e| AnyErr(e.to_string()))?;
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

            wip = deserialize_as_list(wip, value)?;
        }
        Def::Map(_) => {
            #[cfg(feature = "log")]
            log::debug!("Processing map type");

            wip = deserialize_as_map(wip, value)?;
        }
        // Enum has been moved to Type system
        _ => return Err(AnyErr(format!("Unsupported type: {:?}", shape))),
    }
    Ok(wip)
}

fn deserialize_as_list<'facet, 'shape>(
    mut wip: Partial<'facet, 'shape>,
    value: &Yaml,
) -> Result<Partial<'facet, 'shape>, AnyErr> {
    #[cfg(feature = "log")]
    log::debug!("deserialize_as_list: shape={}", wip.shape());

    if let Yaml::Array(array) = value {
        // Handle empty list
        if array.is_empty() {
            return wip.put_empty_list().map_err(|e| AnyErr(e.to_string()));
        }

        // Start the list
        wip = wip.begin_pushback().map_err(|e| AnyErr(e.to_string()))?;

        // Process each element
        for element in array.iter() {
            #[cfg(feature = "log")]
            log::debug!("Processing list element: {:?}", element);

            // Push element
            wip = wip
                .begin_list_item().ma
                p_err(|e| AnyErr(e.to_string()))?;
            wip = deserialize_value(wip, element)?;
            wip = wip.end().map_err(|e| AnyErr(e.to_string()))?;
        }

        Ok(wip)
    } else {
        Err(AnyErr(format!(
            "Expected a YAML array, got: {}",
            yaml_type(value)
        )))
    }
}

fn deserialize_as_map<'facet, 'shape>(
    mut wip: Partial<'facet, 'shape>,
    value: &Yaml,
) -> Result<Partial<'facet, 'shape>, AnyErr> {
    if let Yaml::Hash(hash) = value {
        // Handle empty map
        if hash.is_empty() {
            return wip.put_empty_map().map_err(|e| AnyErr(e.to_string()));
        }

        // Start the map
        wip = wip.begin_map_insert().map_err(|e| AnyErr(e.to_string()))?;

        // Process each key-value pair
        for (k, v) in hash {
            // Get the key as a string
            let key_str = k
                .as_str()
                .ok_or_else(|| AnyErr(format!("Expected string key, got: {}", yaml_type(k))))?;

            // Push map key
            wip = wip.push_map_key().map_err(|e| AnyErr(e.to_string()))?;
            wip = wip
                .put(key_str.to_string())
                .map_err(|e| AnyErr(e.to_string()))?;

            // Push map value
            wip = wip.push_map_value().map_err(|e| AnyErr(e.to_string()))?;
            wip = deserialize_value(wip, v)?;
            wip = wip.end().map_err(|e| AnyErr(e.to_string()))?;
        }

        Ok(wip)
    } else {
        Err(AnyErr(format!(
            "Expected a YAML hash/map, got: {}",
            yaml_type(value)
        )))
    }
}
