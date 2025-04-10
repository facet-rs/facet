#![warn(missing_docs)]
#![doc = include_str!("../README.md")]

use facet_core::{Facet, Opaque, OpaqueConst, ShapeExt};
use facet_poke::Poke;
use toml_edit::{DocumentMut, Item, TomlError, Value};

#[cfg(test)]
mod tests;

/// Deserializes a TOML string into a value of type `T` that implements `Facet`.
pub fn from_str<T: Facet>(toml: &str) -> Result<T, AnyErr> {
    let (poke, _guard) = Poke::alloc::<T>();
    let opaque = from_str_opaque(poke, toml)?;
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

fn toml_to_u64(ty: &Value) -> Result<u64, AnyErr> {
    match ty {
        Value::Float(r) => Ok(*r.value() as u64),
        Value::Integer(i) => Ok(*i.value() as u64),
        Value::String(s) => s
            .value()
            .parse::<u64>()
            .map_err(|_| AnyErr("Failed to parse string as u64".into())),
        Value::Boolean(b) => Ok(if *b.value() { 1 } else { 0 }),
        _ => Err(AnyErr(format!("Cannot convert {} to u64", ty.type_name()))),
    }
}

fn from_str_opaque<'mem>(poke: Poke<'mem>, toml: &str) -> Result<Opaque<'mem>, AnyErr> {
    let docs: DocumentMut = toml.parse().map_err(|e| TomlError::to_string(&e))?;
    deserialize_item(poke, docs.as_item())
}

fn deserialize_item<'mem>(poke: Poke<'mem>, value: &Item) -> Result<Opaque<'mem>, AnyErr> {
    let opaque = match poke {
        Poke::Scalar(ps) => {
            if ps.shape().is_type::<u64>() {
                let v = value
                    .as_value()
                    .ok_or_else(|| format!("Expected value, got: {}", value.type_name()))?;
                let u = toml_to_u64(v)?;
                let opaque = OpaqueConst::new(&u);
                unsafe { ps.write(opaque) }
            } else if ps.shape().is_type::<String>() {
                let s = value
                    .as_str()
                    .ok_or_else(|| AnyErr(format!("Expected string, got: {}", value.type_name())))?
                    .to_string();
                let opaque = OpaqueConst::new(&s);
                let res = unsafe { ps.write(opaque) };
                core::mem::forget(s);
                res
            } else {
                return Err(format!("Unsupported scalar type: {}", ps.shape()).into());
            }
        }
        Poke::List(_) => todo!(),
        Poke::Map(_) => todo!(),
        Poke::Struct(mut ps) => {
            let table = value.as_table_like().ok_or_else(|| {
                format!("Expected table like structure, got {}", value.type_name())
            })?;

            for (k, v) in table.iter() {
                let (index, field_poke) = ps
                    .field_by_name(k)
                    .map_err(|e| format!("Field '{}' error: {}", k, e))?;
                let _v = deserialize_item(field_poke, v)
                    .map_err(|e| format!("Error deserializing field '{}': {}", k, e))?;
                unsafe {
                    ps.mark_initialized(index);
                }
            }
            ps.build_in_place()
        }
        Poke::Enum(_) => todo!(),
        _ => todo!("unsupported poke type"),
    };
    Ok(opaque)
}
