//! Convert TOML values to it's scalar counterpart.

use facet_core::Facet;
use facet_reflect::{ReflectError, Wip};
use num_traits::cast::NumCast;
use toml_edit::{Item, Value};

use crate::error::{TomlError, TomlErrorKind};

/// Try to convert a TOML integer or float to a Rust number.
///
/// Applies to all Rust scalars supported by the `num` crate.
pub(crate) fn put_number<'input, 'a, T>(
    toml: &'input str,
    wip: Wip<'a>,
    item: &Item,
) -> Result<Wip<'a>, TomlError<'input>>
where
    T: Facet + NumCast + 'a,
{
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

    let value = match v {
        Value::Float(r) => Ok(T::from(*r.value()).ok_or_else(|| {
            TomlError::new(
                toml,
                TomlErrorKind::FailedTypeConversion {
                    toml_type_name: "float",
                    rust_type: T::SHAPE,
                    reason: None,
                },
                r.span(),
            )
        })?),
        Value::Integer(i) => Ok(T::from(*i.value()).ok_or_else(|| {
            TomlError::new(
                toml,
                TomlErrorKind::FailedTypeConversion {
                    toml_type_name: "integer",
                    rust_type: T::SHAPE,
                    reason: None,
                },
                i.span(),
            )
        })?),
        other => Err(TomlError::new(
            toml,
            TomlErrorKind::ExpectedType {
                expected: "number",
                got: other.type_name(),
            },
            other.span(),
        )),
    }?;

    wip.put(value)
        .map_err(|e| TomlError::new(toml, TomlErrorKind::GenericReflect(e), item.span()))
}

/// Try to convert a TOML boolean to a Rust boolean.
pub(crate) fn put_boolean<'input, 'a>(
    toml: &'input str,
    wip: Wip<'a>,
    item: &Item,
) -> Result<Wip<'a>, TomlError<'input>> {
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

    let value = match v {
        Value::Boolean(boolean) => Ok(*boolean.value()),
        _ => Err(TomlError::new(
            toml,
            TomlErrorKind::ExpectedType {
                expected: "boolean",
                got: v.type_name(),
            },
            v.span(),
        )),
    }?;

    wip.put(value)
        .map_err(|e| TomlError::new(toml, TomlErrorKind::GenericReflect(e), item.span()))
}

/// Try to convert a TOML string to a Rust string.
pub(crate) fn put_string<'input, 'a, T>(
    toml: &'input str,
    wip: Wip<'a>,
    item: &Item,
) -> Result<Wip<'a>, TomlError<'input>>
where
    T: From<String> + Facet + 'a,
{
    let value: T = item
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
        // TODO: use reference
        .to_string()
        .into();

    wip.put(value)
        .map_err(|e| TomlError::new(toml, TomlErrorKind::GenericReflect(e), item.span()))
}

/// Try to convert a TOML string to a Rust type that implements `FromStr`.
pub(crate) fn put_from_str<'input, 'a>(
    toml: &'input str,
    wip: Wip<'a>,
    item: &Item,
) -> Result<Wip<'a>, TomlError<'input>> {
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

    wip.parse(string).map_err(|e| match e {
        // Handle the specific parsing error with a custom error type
        ReflectError::OperationFailed {
            operation: "parsing",
            shape,
        } => TomlError::new(
            toml,
            TomlErrorKind::FailedTypeConversion {
                toml_type_name: item.type_name(),
                rust_type: shape,
                // TODO: use the from_str reason for failing here
                reason: None,
            },
            item.span(),
        ),
        e => TomlError::new(toml, TomlErrorKind::GenericReflect(e), item.span()),
    })
}
