//! Parse YAML strings into Rust values.

#[cfg(not(feature = "alloc"))]
compile_error!("feature `alloc` is required");

mod error;

use alloc::{
    format,
    string::{String, ToString},
};
use error::AnyErr;
use facet_core::{Def, Facet, ScalarAffinity, Type, UserType};
use facet_reflect::Wip;
use yaml_rust2::{Yaml, YamlLoader};

/// Deserializes a YAML string into a value of type `T` that implements `Facet`.
pub fn from_str<'input: 'facet, 'facet, T: Facet<'facet>>(yaml: &'input str) -> Result<T, AnyErr> {
    let wip = Wip::alloc::<T>()?;
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

fn from_str_value<'a>(wip: Wip<'a>, yaml: &str) -> Result<Wip<'a>, AnyErr> {
    let docs = YamlLoader::load_from_str(yaml).map_err(|e| e.to_string())?;
    if docs.len() != 1 {
        return Err("Expected exactly one YAML document".into());
    }
    deserialize_value(wip, &docs[0])
}

fn deserialize_value<'a>(mut wip: Wip<'a>, value: &Yaml) -> Result<Wip<'a>, AnyErr> {
    let shape = wip.shape();

    // First check the type system (Type)
    if let Type::User(UserType::Struct(_)) = &shape.ty {
        if let Yaml::Hash(hash) = value {
            for (k, v) in hash {
                let k = k
                    .as_str()
                    .ok_or_else(|| AnyErr(format!("Expected string key, got: {}", yaml_type(k))))?;
                let field_index = wip
                    .field_index(k)
                    .ok_or_else(|| AnyErr(format!("Field '{}' not found", k)))?;
                wip = wip
                    .field(field_index)
                    .map_err(|e| AnyErr(format!("Field '{}' error: {}", k, e)))?;
                wip = deserialize_value(wip, v)?;
                wip = wip.pop().map_err(|e| AnyErr(e.to_string()))?;
            }
        } else {
            return Err(AnyErr(format!("Expected a YAML hash, got: {:?}", value)));
        }
        return Ok(wip);
    }

    // Then check the def system (Def)
    match shape.def {
        Def::Scalar(scalar_def) => {
            // For type conversions like String → OffsetDateTime, simply put the string value.
            // The Wip system will use the target type's try_from vtable function to handle
            // the conversion automatically. This works for time types, UUIDs, paths, etc.
            if shape.is_type::<u64>() {
                let u = yaml_to_u64(value)?;
                wip = wip.put(u).map_err(|e| AnyErr(e.to_string()))?;
            } else if shape.is_type::<String>()
                || matches!(scalar_def.affinity, ScalarAffinity::Time(_))
            {
                // For strings and types with time affinity (like OffsetDateTime), parse from string
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
            wip = deserialize_as_list(wip, value)?;
        }
        Def::Map(_) => {
            wip = deserialize_as_map(wip, value)?;
        }
        // Enum has been moved to Type system
        _ => return Err(AnyErr(format!("Unsupported type: {:?}", shape))),
    }
    Ok(wip)
}

fn deserialize_as_list<'a>(mut wip: Wip<'a>, value: &Yaml) -> Result<Wip<'a>, AnyErr> {
    if let Yaml::Array(array) = value {
        // Handle empty list
        if array.is_empty() {
            return wip.put_empty_list().map_err(|e| AnyErr(e.to_string()));
        }

        // Start the list
        wip = wip.begin_pushback().map_err(|e| AnyErr(e.to_string()))?;

        // Process each element
        for element in array {
            // Push element
            wip = wip.push().map_err(|e| AnyErr(e.to_string()))?;
            wip = deserialize_value(wip, element)?;
            wip = wip.pop().map_err(|e| AnyErr(e.to_string()))?;
        }

        Ok(wip)
    } else {
        Err(AnyErr(format!(
            "Expected a YAML array, got: {}",
            yaml_type(value)
        )))
    }
}

fn deserialize_as_map<'a>(mut wip: Wip<'a>, value: &Yaml) -> Result<Wip<'a>, AnyErr> {
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
            wip = wip.pop().map_err(|e| AnyErr(e.to_string()))?;
        }

        Ok(wip)
    } else {
        Err(AnyErr(format!(
            "Expected a YAML hash/map, got: {}",
            yaml_type(value)
        )))
    }
}
