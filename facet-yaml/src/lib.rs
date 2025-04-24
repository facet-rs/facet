#![warn(missing_docs)]
#![doc = include_str!("../README.md")]

use facet_core::{Def, Facet};
use facet_reflect::Wip;
use yaml_rust2::{Yaml, YamlLoader};

/// Deserializes a YAML string into a value of type `T` that implements `Facet`.
pub fn from_str<'input: 'facet, 'facet, T: Facet<'facet>>(yaml: &'input str) -> Result<T, AnyErr> {
    let wip = Wip::alloc::<T>();
    let wip = from_str_value(wip, yaml)?;
    let heap_value = wip.build().map_err(|e| AnyErr(e.to_string()))?;
    heap_value
        .materialize::<T>()
        .map_err(|e| AnyErr(e.to_string()))
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

fn from_str_value<'a>(wip: Wip<'a>, yaml: &str) -> Result<Wip<'a>, AnyErr> {
    let docs = YamlLoader::load_from_str(yaml).map_err(|e| e.to_string())?;
    if docs.len() != 1 {
        return Err("Expected exactly one YAML document".into());
    }
    deserialize_value(wip, &docs[0])
}

fn deserialize_value<'a>(mut wip: Wip<'a>, value: &Yaml) -> Result<Wip<'a>, AnyErr> {
    let shape = wip.shape();
    match shape.def {
        Def::Scalar(_) => {
            if shape.is_type::<u64>() {
                let u = yaml_to_u64(value)?;
                wip = wip.put(u).map_err(|e| AnyErr(e.to_string()))?;
            } else if shape.is_type::<String>() {
                let s = value
                    .as_str()
                    .ok_or_else(|| AnyErr(format!("Expected string, got: {}", yaml_type(value))))?
                    .to_string();
                wip = wip.put(s).map_err(|e| AnyErr(e.to_string()))?;
            } else {
                return Err(AnyErr(format!("Unsupported scalar type: {}", shape)));
            }
        }
        Def::List(_) => todo!(),
        Def::Map(_) => todo!(),
        Def::Struct(_) => {
            if let Yaml::Hash(hash) = value {
                for (k, v) in hash {
                    let k = k.as_str().ok_or_else(|| {
                        AnyErr(format!("Expected string key, got: {}", yaml_type(k)))
                    })?;
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
        }
        Def::Enum(_) => todo!(),
        _ => return Err(AnyErr(format!("Unsupported type: {:?}", shape))),
    }
    Ok(wip)
}
