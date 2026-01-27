extern crate alloc;

use std::borrow::Cow;

use facet_core::{NumericType, PrimitiveType, ScalarType, Type, UserType};
use facet_reflect::{Partial, Span};

use crate::{DeserializeError, DeserializeErrorKind, FormatDeserializer, ScalarValue};

/// Set a scalar value into a `Partial`, handling type coercion.
///
/// This is a non-generic inner function that handles the core logic of `set_scalar`.
/// It's extracted to reduce monomorphization bloat - each parser type only needs
/// a thin wrapper that converts the error type.
///
/// Note: `ScalarValue::Str` and `ScalarValue::Bytes` cases delegate to `facet_dessert`
/// for string/bytes handling.
pub(crate) fn set_scalar_inner<'input, const BORROW: bool>(
    mut wip: Partial<'input, BORROW>,
    scalar: ScalarValue<'input>,
    span: Option<Span>,
) -> Result<Partial<'input, BORROW>, SetScalarResult<'input, BORROW>> {
    let shape = wip.shape();
    let scalar_type = shape.scalar_type();

    match scalar {
        ScalarValue::Null => {
            wip = wip.set_default()?;
        }
        ScalarValue::Bool(b) => {
            wip = wip.set(b)?;
        }
        ScalarValue::Char(c) => {
            wip = wip.set(c)?;
        }
        ScalarValue::I64(n) => {
            match scalar_type {
                // Handle signed types
                Some(ScalarType::I8) => wip = wip.set(n as i8)?,
                Some(ScalarType::I16) => wip = wip.set(n as i16)?,
                Some(ScalarType::I32) => wip = wip.set(n as i32)?,
                Some(ScalarType::I64) => wip = wip.set(n)?,
                Some(ScalarType::I128) => wip = wip.set(n as i128)?,
                Some(ScalarType::ISize) => wip = wip.set(n as isize)?,
                // Handle unsigned types (I64 can fit in unsigned if non-negative)
                Some(ScalarType::U8) => wip = wip.set(n as u8)?,
                Some(ScalarType::U16) => wip = wip.set(n as u16)?,
                Some(ScalarType::U32) => wip = wip.set(n as u32)?,
                Some(ScalarType::U64) => wip = wip.set(n as u64)?,
                Some(ScalarType::U128) => wip = wip.set(n as u128)?,
                Some(ScalarType::USize) => wip = wip.set(n as usize)?,
                // Handle floats
                Some(ScalarType::F32) => wip = wip.set(n as f32)?,
                Some(ScalarType::F64) => wip = wip.set(n as f64)?,
                // Handle String - stringify the number
                Some(ScalarType::String) => {
                    wip = wip.set(alloc::string::ToString::to_string(&n))?
                }
                _ => wip = wip.set(n)?,
            }
        }
        ScalarValue::U64(n) => {
            match scalar_type {
                // Handle unsigned types
                Some(ScalarType::U8) => wip = wip.set(n as u8)?,
                Some(ScalarType::U16) => wip = wip.set(n as u16)?,
                Some(ScalarType::U32) => wip = wip.set(n as u32)?,
                Some(ScalarType::U64) => wip = wip.set(n)?,
                Some(ScalarType::U128) => wip = wip.set(n as u128)?,
                Some(ScalarType::USize) => wip = wip.set(n as usize)?,
                // Handle signed types (U64 can fit in signed if small enough)
                Some(ScalarType::I8) => wip = wip.set(n as i8)?,
                Some(ScalarType::I16) => wip = wip.set(n as i16)?,
                Some(ScalarType::I32) => wip = wip.set(n as i32)?,
                Some(ScalarType::I64) => wip = wip.set(n as i64)?,
                Some(ScalarType::I128) => wip = wip.set(n as i128)?,
                Some(ScalarType::ISize) => wip = wip.set(n as isize)?,
                // Handle floats
                Some(ScalarType::F32) => wip = wip.set(n as f32)?,
                Some(ScalarType::F64) => wip = wip.set(n as f64)?,
                // Handle String - stringify the number
                Some(ScalarType::String) => {
                    wip = wip.set(alloc::string::ToString::to_string(&n))?
                }
                _ => wip = wip.set(n)?,
            }
        }
        ScalarValue::U128(n) => {
            match scalar_type {
                Some(ScalarType::U128) => wip = wip.set(n)?,
                Some(ScalarType::I128) => wip = wip.set(n as i128)?,
                // For smaller types, truncate (caller should have used correct hint)
                _ => wip = wip.set(n as u64)?,
            }
        }
        ScalarValue::I128(n) => {
            match scalar_type {
                Some(ScalarType::I128) => wip = wip.set(n)?,
                Some(ScalarType::U128) => wip = wip.set(n as u128)?,
                // For smaller types, truncate (caller should have used correct hint)
                _ => wip = wip.set(n as i64)?,
            }
        }
        ScalarValue::F64(n) => {
            match scalar_type {
                Some(ScalarType::F32) => wip = wip.set(n as f32)?,
                Some(ScalarType::F64) => wip = wip.set(n)?,
                _ if shape.vtable.has_try_from() && shape.inner.is_some() => {
                    // For opaque types with try_from (like NotNan, OrderedFloat), use
                    // begin_inner() + set + end() to trigger conversion
                    let inner_shape = shape.inner.unwrap();
                    wip = wip.begin_inner()?;
                    if inner_shape.is_type::<f32>() {
                        wip = wip.set(n as f32)?;
                    } else {
                        wip = wip.set(n)?;
                    }
                    wip = wip.end()?;
                }
                _ if shape.vtable.has_parse() => {
                    // For types that support parsing (like Decimal), convert to string
                    // and use parse_from_str to preserve their parsing semantics
                    wip = wip.parse_from_str(&alloc::string::ToString::to_string(&n))?;
                }
                _ => wip = wip.set(n)?,
            }
        }
        ScalarValue::Str(s) => {
            // Try parse_from_str first if the type supports it
            if shape.vtable.has_parse() {
                wip = wip.parse_from_str(s.as_ref())?;
            } else {
                // Delegate to set_string_value - this requires the caller to handle it
                return Err(SetScalarResult::NeedsStringValue { wip, s });
            }
        }
        ScalarValue::Bytes(b) => {
            // First try parse_from_bytes if the type supports it (e.g., UUID from 16 bytes)
            if shape.vtable.has_parse_bytes() {
                wip = wip.parse_from_bytes(b.as_ref())?;
            } else {
                // Delegate to set_bytes_value - this requires the caller to handle it
                return Err(SetScalarResult::NeedsBytesValue { wip, b });
            }
        }
        ScalarValue::Unit => {
            // Unit value - set to default/unit value
            wip = wip.set_default()?;
        }
    }

    Ok(wip)
}

/// Result of `set_scalar_inner` - either success, an error, or delegation to string/bytes handling.
pub(crate) enum SetScalarResult<'input, const BORROW: bool> {
    /// Need to call `set_string_value` with these parameters.
    NeedsStringValue {
        wip: Partial<'input, BORROW>,
        s: Cow<'input, str>,
    },
    /// Need to call `set_bytes_value` with these parameters.
    NeedsBytesValue {
        wip: Partial<'input, BORROW>,
        b: Cow<'input, [u8]>,
    },
    /// An error occurred.
    Error(DeserializeError),
}

impl<'input, const BORROW: bool> From<DeserializeError> for SetScalarResult<'input, BORROW> {
    fn from(e: DeserializeError) -> Self {
        SetScalarResult::Error(e)
    }
}

impl<'input, const BORROW: bool> From<facet_reflect::ReflectError>
    for SetScalarResult<'input, BORROW>
{
    fn from(e: facet_reflect::ReflectError) -> Self {
        SetScalarResult::Error(e.into())
    }
}

/// Result of `deserialize_map_key_terminal_inner` - either success or delegation.
pub(crate) enum MapKeyTerminalResult<'input, const BORROW: bool> {
    /// Need to call `set_string_value` with these parameters.
    NeedsSetString {
        wip: Partial<'input, BORROW>,
        s: Cow<'input, str>,
    },
    /// An error occurred.
    Error(DeserializeError),
}

impl<'input, const BORROW: bool> From<DeserializeError> for MapKeyTerminalResult<'input, BORROW> {
    fn from(e: DeserializeError) -> Self {
        MapKeyTerminalResult::Error(e)
    }
}

impl<'input, const BORROW: bool> From<facet_reflect::ReflectError>
    for MapKeyTerminalResult<'input, BORROW>
{
    fn from(e: facet_reflect::ReflectError) -> Self {
        MapKeyTerminalResult::Error(e.into())
    }
}

/// Handle terminal cases of map key deserialization (enum, numeric, string).
///
/// This is a non-generic inner function that handles the final step of `deserialize_map_key`
/// when recursion is not needed. It's extracted to reduce monomorphization bloat.
///
/// The function handles:
/// - Enum types: use `select_variant_named`
/// - Numeric types: parse the string key as a number
/// - String types: delegate to `set_string_value` (returns `NeedsSetString`)
pub(crate) fn deserialize_map_key_terminal_inner<'input, const BORROW: bool>(
    mut wip: Partial<'input, BORROW>,
    key: Cow<'input, str>,
    span: Option<Span>,
) -> Result<Partial<'input, BORROW>, MapKeyTerminalResult<'input, BORROW>> {
    let shape = wip.shape();

    // Check if target is an enum - use select_variant_named for unit variants
    if let Type::User(UserType::Enum(_)) = &shape.ty {
        wip = wip.select_variant_named(&key)?;
        return Ok(wip);
    }

    // Check if target is a numeric type - parse the string key as a number
    if let Type::Primitive(PrimitiveType::Numeric(num_ty)) = &shape.ty {
        match num_ty {
            NumericType::Integer { signed } => {
                if *signed {
                    let n: i64 = key.parse().map_err(|_| DeserializeError {
                        span,
                        path: None,
                        kind: DeserializeErrorKind::UnexpectedToken {
                            expected: "valid integer for map key",
                            got: alloc::format!("string '{}'", key).into(),
                        },
                    })?;
                    // Use set for each size - the Partial handles type conversion
                    wip = wip.set(n)?;
                } else {
                    let n: u64 = key.parse().map_err(|_| DeserializeError {
                        span,
                        path: None,
                        kind: DeserializeErrorKind::UnexpectedToken {
                            expected: "valid unsigned integer for map key",
                            got: alloc::format!("string '{}'", key).into(),
                        },
                    })?;
                    wip = wip.set(n)?;
                }
                return Ok(wip);
            }
            NumericType::Float => {
                let n: f64 = key.parse().map_err(|_| DeserializeError {
                    span,
                    path: None,
                    kind: DeserializeErrorKind::UnexpectedToken {
                        expected: "valid float for map key",
                        got: alloc::format!("string '{}'", key).into(),
                    },
                })?;
                wip = wip.set(n)?;
                return Ok(wip);
            }
        }
    }

    // Default: treat as string - delegate to set_string_value
    Err(MapKeyTerminalResult::NeedsSetString { wip, s: key })
}

impl<'input, const BORROW: bool> FormatDeserializer<'input, BORROW> {
    /// Set a scalar value into a `Partial`, handling type coercion.
    ///
    /// This is a thin wrapper around `set_scalar_inner` that handles the
    /// string/bytes delegation cases and converts error types.
    pub(crate) fn set_scalar(
        &mut self,
        wip: Partial<'input, BORROW>,
        scalar: ScalarValue<'input>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError> {
        match set_scalar_inner(wip, scalar, self.last_span) {
            Ok(wip) => Ok(wip),
            Err(SetScalarResult::NeedsStringValue { wip, s }) => self.set_string_value(wip, s),
            Err(SetScalarResult::NeedsBytesValue { wip, b }) => self.set_bytes_value(wip, b),
            Err(SetScalarResult::Error(e)) => Err(e),
        }
    }

    /// Set a string value, handling `&str`, `Cow<str>`, and `String` appropriately.
    pub(crate) fn set_string_value(
        &mut self,
        wip: Partial<'input, BORROW>,
        s: Cow<'input, str>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError> {
        facet_dessert::set_string_value(wip, s, self.last_span).map_err(|e| match e {
            facet_dessert::DessertError::Reflect { error, span } => DeserializeError {
                span,
                path: None,
                kind: DeserializeErrorKind::Reflect(error),
            },
            facet_dessert::DessertError::CannotBorrow { message } => DeserializeError {
                span: None,
                path: None,
                kind: DeserializeErrorKind::CannotBorrow {
                    reason: message.into_owned().leak(),
                },
            },
        })
    }

    /// Set a bytes value with proper handling for borrowed vs owned data.
    ///
    /// This handles `&[u8]`, `Cow<[u8]>`, and `Vec<u8>` appropriately based on
    /// whether borrowing is enabled and whether the data is borrowed or owned.
    pub(crate) fn set_bytes_value(
        &mut self,
        wip: Partial<'input, BORROW>,
        b: Cow<'input, [u8]>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError> {
        facet_dessert::set_bytes_value(wip, b, self.last_span).map_err(|e| match e {
            facet_dessert::DessertError::Reflect { error, span } => DeserializeError {
                span,
                path: None,
                kind: DeserializeErrorKind::Reflect(error),
            },
            facet_dessert::DessertError::CannotBorrow { message } => DeserializeError {
                span: None,
                path: None,
                kind: DeserializeErrorKind::CannotBorrow {
                    reason: message.into_owned().leak(),
                },
            },
        })
    }
}
