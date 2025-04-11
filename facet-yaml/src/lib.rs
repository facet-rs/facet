#![warn(missing_docs)]
#![doc = include_str!("../README.md")]

use facet::PokeUninit;
use facet::{Facet, Opaque};
use yaml_rust2::{Yaml, YamlLoader};

#[cfg(test)]
mod tests;

/// Deserializes a YAML string into a value of type `T` that implements `Facet`.
pub fn from_str<T: Facet>(yaml: &str) -> Result<T, AnyErr> {
    let (poke, _guard) = PokeUninit::alloc::<T>();
    let opaque = from_str_opaque(poke, yaml)?;
    Ok(unsafe { opaque.read::<T>() })
}

/// Any error
#[derive(Debug, Clone)]
pub struct AnyErr(String);

impl core::fmt::Display for AnyErr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for AnyErr {}

impl From<String> for AnyErr {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for AnyErr {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
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

fn from_str_opaque<'mem>(poke: PokeUninit<'mem>, yaml: &str) -> Result<Opaque<'mem>, AnyErr> {
    let docs = YamlLoader::load_from_str(yaml).map_err(|e| e.to_string())?;
    if docs.len() != 1 {
        return Err("Expected exactly one YAML document".into());
    }
    deserialize_value(poke, &docs[0])
}

fn deserialize_value<'mem>(poke: PokeUninit<'mem>, value: &Yaml) -> Result<Opaque<'mem>, AnyErr> {
    let opaque = match poke {
        PokeUninit::Scalar(ps) => {
            if ps.shape().is_type::<u64>() {
                let u = yaml_to_u64(value)?;
                ps.put(u)
            } else if ps.shape().is_type::<String>() {
                let s = value
                    .as_str()
                    .ok_or_else(|| AnyErr(format!("Expected string, got: {}", yaml_type(value))))?
                    .to_string();
                ps.put(s)
            } else {
                return Err(format!("Unsupported scalar type: {}", ps.shape()).into());
            }
        }
        PokeUninit::List(_) => todo!(),
        PokeUninit::Map(_) => todo!(),
        PokeUninit::Struct(mut ps) => match value {
            Yaml::Hash(hash) => {
                for (k, v) in hash {
                    let k = k
                        .as_str()
                        .ok_or_else(|| format!("Expected string key, got: {}", yaml_type(k)))?;
                    let (index, field_poke) = ps
                        .field_by_name(k)
                        .map_err(|e| format!("Field '{}' error: {}", k, e))?;
                    let _v = deserialize_value(field_poke, v)
                        .map_err(|e| format!("Error deserializing field '{}': {}", k, e))?;
                    unsafe {
                        ps.mark_initialized(index);
                    }
                }
                ps.build_in_place()
            }
            _ => {
                return Err(format!("Expected a YAML hash, got: {:?}", value).into());
            }
        },
        PokeUninit::Enum(_) => todo!(),
        _ => todo!("unsupported poke type"),
    };
    Ok(opaque)
}
