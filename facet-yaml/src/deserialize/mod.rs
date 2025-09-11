//! Parse YAML strings into Rust values.

#[cfg(not(feature = "alloc"))]
compile_error!("feature `alloc` is required");

mod error;

use alloc::{
    format,
    string::{String, ToString},
};
use error::AnyErr;
use facet_core::{
    Def, Facet, FieldFlags, NumericType, PrimitiveType, SequenceType, Type, UserType,
};
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

fn from_str_value<'facet>(wip: &mut Partial<'facet>, yaml: &str) -> Result<(), AnyErr> {
    let docs = YamlLoader::load_from_str(yaml).map_err(|e| e.to_string())?;
    if docs.len() != 1 {
        return Err("Expected exactly one YAML document".into());
    }
    deserialize_value(wip, &docs[0])?;
    Ok(())
}

fn deserialize_value<'facet>(wip: &mut Partial<'facet>, value: &Yaml) -> Result<(), AnyErr> {
    // Get the shape
    let shape = wip.shape();

    #[cfg(feature = "log")]
    {
        log::debug!("deserialize_value: shape={shape}");
        log::debug!("Shape type: {:?}", shape.ty);
        log::debug!("Shape attributes: {:?}", shape.attributes);
        log::debug!("YAML value: {value:?}");
    }

    // Handle transparent types - check if shape has the transparent attribute
    if shape
        .attributes
        .contains(&facet_core::ShapeAttribute::Transparent)
    {
        #[cfg(feature = "log")]
        log::debug!("Handling facet(transparent) type");

        // For transparent types, push inner and deserialize as inner type
        wip.begin_inner().map_err(|e| AnyErr(e.to_string()))?;
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
                    .ok_or_else(|| AnyErr(format!("Field '{k}' not found")))?;

                #[cfg(feature = "log")]
                log::debug!("Processing struct field '{k}' (index: {field_index})");

                wip.begin_nth_field(field_index)
                    .map_err(|e| AnyErr(format!("Field '{k}' error: {e}")))?;
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

                        wip.set_nth_field_to_default(index)
                            .map_err(|e| AnyErr(e.to_string()))?;
                    }
                }
            }

            for (index, _field) in sd.fields.iter().enumerate() {
                let is_set = wip.is_field_set(index).map_err(|e| AnyErr(e.to_string()))?;
                if !is_set {
                    todo!(
                        "should fill unset fields from struct's Default, but not implemented yet. the previous implementation was unsound."
                    )
                }
            }
        } else {
            return Err(AnyErr(format!("Expected a YAML hash, got: {value:?}")));
        }
        return Ok(());
    }

    match shape.def {
        Def::Scalar => {
            #[cfg(feature = "log")]
            {
                log::debug!("Processing scalar type");
                log::debug!("  shape: {shape}");
                log::debug!("  shape.ty: {:?}", shape.ty);
            }

            // Check if it's a numeric type
            if let Type::Primitive(PrimitiveType::Numeric(numeric_type)) = shape.ty {
                let size = shape.layout.sized_layout().unwrap().size();
                match numeric_type {
                    NumericType::Integer { signed: false } => {
                        let u = yaml_to_u64(value)?;
                        match size {
                            1 => {
                                let val = u8::try_from(u).map_err(|_| {
                                    AnyErr(format!("Value {u} out of range for u8"))
                                })?;
                                wip.set(val).map_err(|e| AnyErr(e.to_string()))?;
                            }
                            2 => {
                                let val = u16::try_from(u).map_err(|_| {
                                    AnyErr(format!("Value {u} out of range for u16"))
                                })?;
                                wip.set(val).map_err(|e| AnyErr(e.to_string()))?;
                            }
                            4 => {
                                let val = u32::try_from(u).map_err(|_| {
                                    AnyErr(format!("Value {u} out of range for u32"))
                                })?;
                                wip.set(val).map_err(|e| AnyErr(e.to_string()))?;
                            }
                            8 => {
                                // Check if it's usize or u64
                                if shape.is_type::<usize>() {
                                    let val = usize::try_from(u).map_err(|_| {
                                        AnyErr(format!("Value {u} out of range for usize"))
                                    })?;
                                    wip.set(val).map_err(|e| AnyErr(e.to_string()))?;
                                } else {
                                    wip.set(u).map_err(|e| AnyErr(e.to_string()))?;
                                }
                            }
                            16 => {
                                let val = u128::from(u);
                                wip.set(val).map_err(|e| AnyErr(e.to_string()))?;
                            }
                            _ => {
                                // Handle usize
                                let val = usize::try_from(u).map_err(|_| {
                                    AnyErr(format!("Value {u} out of range for usize"))
                                })?;
                                wip.set(val).map_err(|e| AnyErr(e.to_string()))?;
                            }
                        }
                    }
                    NumericType::Integer { signed: true } => {
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
                        match size {
                            1 => {
                                let val = i8::try_from(i).map_err(|_| {
                                    AnyErr(format!("Value {i} out of range for i8"))
                                })?;
                                wip.set(val).map_err(|e| AnyErr(e.to_string()))?;
                            }
                            2 => {
                                let val = i16::try_from(i).map_err(|_| {
                                    AnyErr(format!("Value {i} out of range for i16"))
                                })?;
                                wip.set(val).map_err(|e| AnyErr(e.to_string()))?;
                            }
                            4 => {
                                let val = i32::try_from(i).map_err(|_| {
                                    AnyErr(format!("Value {i} out of range for i32"))
                                })?;
                                wip.set(val).map_err(|e| AnyErr(e.to_string()))?;
                            }
                            8 => {
                                // Check if it's isize or i64
                                if shape.is_type::<isize>() {
                                    let val = isize::try_from(i).map_err(|_| {
                                        AnyErr(format!("Value {i} out of range for isize"))
                                    })?;
                                    wip.set(val).map_err(|e| AnyErr(e.to_string()))?;
                                } else {
                                    wip.set(i).map_err(|e| AnyErr(e.to_string()))?;
                                }
                            }
                            16 => {
                                let val = i128::from(i);
                                wip.set(val).map_err(|e| AnyErr(e.to_string()))?;
                            }
                            _ => {
                                // Handle isize
                                let val = isize::try_from(i).map_err(|_| {
                                    AnyErr(format!("Value {i} out of range for isize"))
                                })?;
                                wip.set(val).map_err(|e| AnyErr(e.to_string()))?;
                            }
                        }
                    }
                    NumericType::Float => {
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
                        // Determine float type based on size (f32 is 4 bytes, f64 is 8 bytes)
                        if size == 4 {
                            let val = f as f32;
                            wip.set(val).map_err(|e| AnyErr(e.to_string()))?;
                        } else {
                            wip.set(f).map_err(|e| AnyErr(e.to_string()))?;
                        }
                    }
                }
            } else if shape.is_type::<bool>() {
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
            } else if shape.is_type::<String>() {
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
                wip.begin_some().map_err(|e| AnyErr(e.to_string()))?;
                deserialize_value(wip, value)?;
                wip.end().map_err(|e| AnyErr(e.to_string()))?;
            }
        }

        Def::Pointer(smart_ptr_def) => {
            #[cfg(feature = "log")]
            log::debug!("Processing smart pointer type");

            // Check the pointee type before calling begin_smart_ptr
            let pointee_shape = smart_ptr_def
                .pointee()
                .ok_or_else(|| AnyErr("SmartPointer must have a pointee shape".to_string()))?;

            #[cfg(feature = "log")]
            log::debug!("Smart pointer pointee shape: {pointee_shape}");

            // Begin smart pointer
            wip.begin_smart_ptr().map_err(|e| AnyErr(e.to_string()))?;

            // For smart pointers to slices, the shape doesn't change after begin_smart_ptr
            // but the internal state changes to use a slice builder
            match pointee_shape.ty {
                Type::Sequence(SequenceType::Slice(_)) => {
                    #[cfg(feature = "log")]
                    log::debug!("Smart pointer pointee is a slice, deserializing as list");
                    // Slices are handled like lists
                    deserialize_as_list(wip, value)?;
                }
                _ => {
                    #[cfg(feature = "log")]
                    log::debug!("Smart pointer pointee is not a slice, deserializing normally");
                    // For other types, deserialize normally
                    deserialize_value(wip, value)?;
                }
            }

            // End smart pointer
            wip.end().map_err(|e| AnyErr(e.to_string()))?;
        }
        Def::Slice(_) => {
            #[cfg(feature = "log")]
            log::debug!("Processing slice type");

            // Slices are deserialized like lists
            deserialize_as_list(wip, value)?;
        }
        // Enum has been moved to Type system
        _ => return Err(AnyErr(format!("Unsupported type: {shape:?}"))),
    }
    Ok(())
}

fn deserialize_as_list<'facet>(wip: &mut Partial<'facet>, value: &Yaml) -> Result<(), AnyErr> {
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
            log::debug!("Processing list element: {element:?}");

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

fn deserialize_as_map<'facet>(wip: &mut Partial<'facet>, value: &Yaml) -> Result<(), AnyErr> {
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
