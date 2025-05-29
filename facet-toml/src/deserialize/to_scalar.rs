//! Convert TOML values to it's scalar counterpart.

use alloc::string::{String, ToString};
use facet_core::Facet;
use facet_reflect::{Partial, ReflectError};
use num_traits::cast::NumCast;
use toml_edit::{Item, Value};

use super::error::{TomlDeError, TomlDeErrorKind};

/// Try to convert a TOML integer or float to a Rust number.
///
/// Applies to all Rust scalars supported by the `num` crate.
pub(crate) fn put_number<'input, 'a, 'shape, T>(
    toml: &'input str,
    wip: &mut Partial<'a, 'shape>,
    item: &Item,
) -> Result<(), TomlDeError<'input, 'shape>>
where
    T: Facet<'a> + NumCast + 'a,
{
    let v = item.as_value().ok_or_else(|| {
        TomlDeError::new(
            toml,
            TomlDeErrorKind::ExpectedType {
                expected: "value",
                got: item.type_name(),
            },
            item.span(),
            wip.path(),
        )
    })?;

    let value = match v {
        Value::Float(r) => Ok(T::from(*r.value()).ok_or_else(|| {
            TomlDeError::new(
                toml,
                TomlDeErrorKind::FailedTypeConversion {
                    toml_type_name: "float",
                    rust_type: T::SHAPE,
                    reason: None,
                },
                r.span(),
                wip.path(),
            )
        })?),
        Value::Integer(i) => Ok(T::from(*i.value()).ok_or_else(|| {
            TomlDeError::new(
                toml,
                TomlDeErrorKind::FailedTypeConversion {
                    toml_type_name: "integer",
                    rust_type: T::SHAPE,
                    reason: None,
                },
                i.span(),
                wip.path(),
            )
        })?),
        other => Err(TomlDeError::new(
            toml,
            TomlDeErrorKind::ExpectedType {
                expected: "number",
                got: other.type_name(),
            },
            other.span(),
            wip.path(),
        )),
    }?;

    // TODO: only generate if actually error
    let path = wip.path();
    wip.set(value).map_err(|e| {
        TomlDeError::new(toml, TomlDeErrorKind::GenericReflect(e), item.span(), path)
    })?;
    Ok(())
}

/// Try to convert a TOML boolean to a Rust boolean.
pub(crate) fn put_boolean<'input, 'a, 'shape>(
    toml: &'input str,
    wip: &mut Partial<'a, 'shape>,
    item: &Item,
) -> Result<(), TomlDeError<'input, 'shape>> {
    let v = item.as_value().ok_or_else(|| {
        TomlDeError::new(
            toml,
            TomlDeErrorKind::ExpectedType {
                expected: "value",
                got: item.type_name(),
            },
            item.span(),
            wip.path(),
        )
    })?;

    let value = if let Value::Boolean(boolean) = v {
        *boolean.value()
    } else {
        return Err(TomlDeError::new(
            toml,
            TomlDeErrorKind::ExpectedType {
                expected: "boolean",
                got: v.type_name(),
            },
            v.span(),
            wip.path(),
        ));
    };

    // TODO: only generate if actually error
    let path = wip.path();
    wip.set(value).map_err(|e| {
        TomlDeError::new(toml, TomlDeErrorKind::GenericReflect(e), item.span(), path)
    })?;
    Ok(())
}

/// Try to convert a TOML char to a Rust char.
pub(crate) fn put_char<'input, 'a, 'shape>(
    toml: &'input str,
    wip: &mut Partial<'a, 'shape>,
    item: &Item,
) -> Result<(), TomlDeError<'input, 'shape>> {
    let v = item.as_value().ok_or_else(|| {
        TomlDeError::new(
            toml,
            TomlDeErrorKind::ExpectedType {
                expected: "value",
                got: item.type_name(),
            },
            item.span(),
            wip.path(),
        )
    })?;

    let value = if let Value::String(string) = v {
        let value = string.value();
        if value.len() > 1 || value.is_empty() {
            return Err(TomlDeError::new(
                toml,
                TomlDeErrorKind::ExpectedType {
                    expected: "char",
                    got: v.type_name(),
                },
                v.span(),
                wip.path(),
            ));
        }

        value.chars().next().unwrap()
    } else {
        return Err(TomlDeError::new(
            toml,
            TomlDeErrorKind::ExpectedType {
                expected: "string",
                got: v.type_name(),
            },
            v.span(),
            wip.path(),
        ));
    };

    // TODO: only generate if actually error
    let path = wip.path();
    wip.set(value).map_err(|e| {
        TomlDeError::new(toml, TomlDeErrorKind::GenericReflect(e), item.span(), path)
    })?;
    Ok(())
}

/// Try to convert a TOML string to a Rust string.
pub(crate) fn put_string<'input, 'a, 'shape, T>(
    toml: &'input str,
    wip: &mut Partial<'a, 'shape>,
    item: &Item,
) -> Result<(), TomlDeError<'input, 'shape>>
where
    T: From<String> + Facet<'a> + 'a,
{
    let value: T = item
        .as_str()
        .ok_or_else(|| {
            TomlDeError::new(
                toml,
                TomlDeErrorKind::ExpectedType {
                    expected: "string",
                    got: item.type_name(),
                },
                item.span(),
                wip.path(),
            )
        })?
        // TODO: use reference
        .to_string()
        .into();

    // TODO: only generate if actually error
    let path = wip.path();
    wip.set(value).map_err(|e| {
        TomlDeError::new(toml, TomlDeErrorKind::GenericReflect(e), item.span(), path)
    })?;
    Ok(())
}

/// Try to convert a TOML string to a Rust type that implements `FromStr`.
pub(crate) fn put_from_str<'input, 'a, 'shape>(
    toml: &'input str,
    wip: &mut Partial<'a, 'shape>,
    item: &Item,
) -> Result<(), TomlDeError<'input, 'shape>> {
    let string = item.as_str().ok_or_else(|| {
        TomlDeError::new(
            toml,
            TomlDeErrorKind::ExpectedType {
                expected: "string",
                got: item.type_name(),
            },
            item.span(),
            wip.path(),
        )
    })?;

    // TODO: only generate if actually error
    let path = wip.path();
    // Simply set the string value - automatic conversion will handle parsing
    wip.set(string.to_string()).map_err(|e| match e {
        // Handle the specific parsing error with a custom error type
        ReflectError::OperationFailed {
            operation: "parsing",
            shape,
        } => TomlDeError::new(
            toml,
            TomlDeErrorKind::FailedTypeConversion {
                toml_type_name: item.type_name(),
                rust_type: shape,
                // TODO: use the from_str reason for failing here
                reason: None,
            },
            item.span(),
            path,
        ),
        e => TomlDeError::new(toml, TomlDeErrorKind::GenericReflect(e), item.span(), path),
    })?;
    Ok(())
}
