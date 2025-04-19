//! Convert TOML values to it's scalar counterpart.

use std::{fmt::Display, str::FromStr};

use num_traits::cast::NumCast;
use toml_edit::{Item, Value};

use crate::error::{TomlError, TomlErrorKind};

/// Try to convert a TOML integer or float to a Rust number.
///
/// Applies to all Rust scalars supported by the `num` crate.
pub(crate) fn number<'input, T: NumCast>(
    toml: &'input str,
    item: &Item,
) -> Result<T, TomlError<'input>> {
    let v = item.as_value().ok_or_else(|| {
        TomlError::new(
            toml,
            TomlErrorKind::ExpectedType {
                expected: "value",
                got: item.type_name(),
            },
            item.span(),
        )
    })?;

    match v {
        Value::Float(r) => Ok(T::from(*r.value()).ok_or_else(|| {
            TomlError::new(
                toml,
                TomlErrorKind::TypeConversion {
                    toml_type: "float",
                    rust_type: std::any::type_name::<T>(),
                    reason: None,
                },
                item.span(),
            )
        })?),
        Value::Integer(i) => Ok(T::from(*i.value()).ok_or_else(|| {
            TomlError::new(
                toml,
                TomlErrorKind::TypeConversion {
                    toml_type: "integer",
                    rust_type: std::any::type_name::<T>(),
                    reason: None,
                },
                item.span(),
            )
        })?),
        _ => Err(TomlError::new(
            toml,
            TomlErrorKind::ExpectedType {
                expected: "number",
                got: v.type_name(),
            },
            item.span(),
        )),
    }
}

/// Try to convert a TOML boolean to a Rust boolean.
pub(crate) fn boolean<'input>(toml: &'input str, item: &Item) -> Result<bool, TomlError<'input>> {
    let v = item.as_value().ok_or_else(|| {
        TomlError::new(
            toml,
            TomlErrorKind::ExpectedType {
                expected: "value",
                got: item.type_name(),
            },
            item.span(),
        )
    })?;

    match v {
        Value::Boolean(boolean) => Ok(*boolean.value()),
        _ => Err(TomlError::new(
            toml,
            TomlErrorKind::ExpectedType {
                expected: "boolean",
                got: v.type_name(),
            },
            item.span(),
        )),
    }
}

/// Try to convert a TOML string to a Rust string.
pub(crate) fn string<'input>(toml: &'input str, item: &Item) -> Result<String, TomlError<'input>> {
    Ok(item
        .as_str()
        .ok_or_else(|| {
            TomlError::new(
                toml,
                TomlErrorKind::ExpectedType {
                    expected: "string",
                    got: item.type_name(),
                },
                item.span(),
            )
        })?
        .to_string())
}

/// Try to convert a TOML string to a Rust type that implements `FromStr`.
pub(crate) fn from_str<'input, T>(
    toml: &'input str,
    item: &Item,
    type_name: &'static str,
) -> Result<T, TomlError<'input>>
where
    T: FromStr,
    T::Err: Display,
{
    let string = item.as_str().ok_or_else(|| {
        TomlError::new(
            toml,
            TomlErrorKind::ExpectedType {
                expected: "string",
                got: item.type_name(),
            },
            item.span(),
        )
    })?;

    string.parse().map_err(|e: T::Err| {
        TomlError::new(
            toml,
            TomlErrorKind::TypeConversion {
                toml_type: type_name,
                rust_type: std::any::type_name::<T>(),
                reason: Some(e.to_string()),
            },
            item.span(),
        )
    })
}
