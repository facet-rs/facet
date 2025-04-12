//! Convert TOML values to it's scalar counterpart.

use std::{fmt::Display, str::FromStr};

use num_traits::cast::NumCast;
use toml_edit::{Item, Value};

use crate::AnyErr;

/// Try to convert a TOML integer or float to a Rust number.
///
/// Applies to all Rust scalars supported by the `num` crate.
pub(crate) fn number<T: NumCast>(item: &Item) -> Result<T, AnyErr> {
    let v = item
        .as_value()
        .ok_or_else(|| format!("Expected value, got: {}", item.type_name()))?;

    match v {
        Value::Float(r) => Ok(T::from(*r.value())
            .ok_or_else(|| format!("Cannot convert float to {}", std::any::type_name::<T>()))?),
        Value::Integer(i) => Ok(T::from(*i.value())
            .ok_or_else(|| format!("Cannot convert integer to {}", std::any::type_name::<T>()))?),
        _ => Err(format!("Cannot convert {} to u64", v.type_name()).into()),
    }
}

/// Try to convert a TOML boolean to a Rust boolean.
pub(crate) fn boolean(item: &Item) -> Result<bool, AnyErr> {
    let v = item
        .as_value()
        .ok_or_else(|| format!("Expected value, got: {}", item.type_name()))?;

    match v {
        Value::Boolean(boolean) => Ok(*boolean.value()),
        _ => Err(format!("Cannot convert {} to u64", v.type_name()).into()),
    }
}

/// Try to convert a TOML string to a Rust string.
pub(crate) fn string(item: &Item) -> Result<String, AnyErr> {
    Ok(item
        .as_str()
        .ok_or_else(|| format!("Expected string, got: {}", item.type_name()))?
        .to_string())
}

/// Try to convert a TOML string to a Rust type that implements `FromStr`.
pub(crate) fn from_str<T>(item: &Item, type_name: &'static str) -> Result<T, AnyErr>
where
    T: FromStr,
    T::Err: Display,
{
    let string = item
        .as_str()
        .ok_or_else(|| format!("Expected string, got: {}", item.type_name()))?;

    Ok(string
        .parse()
        .map_err(|e: T::Err| format!("Cannot convert string to {}: {e}", type_name))?)
}
