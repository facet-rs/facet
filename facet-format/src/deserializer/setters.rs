extern crate alloc;

use std::borrow::Cow;

use facet_core::ScalarType;
use facet_reflect::{Partial, ReflectError, Span};

use crate::{
    DeserializeError, FormatDeserializer, FormatParser, InnerDeserializeError, ScalarValue,
};

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
    let reflect_err = |e: ReflectError| InnerDeserializeError::Reflect {
        error: e,
        span,
        path: None,
    };

    match scalar {
        ScalarValue::Null => {
            wip = wip.set_default().map_err(&reflect_err)?;
        }
        ScalarValue::Bool(b) => {
            wip = wip.set(b).map_err(&reflect_err)?;
        }
        ScalarValue::Char(c) => {
            wip = wip.set(c).map_err(&reflect_err)?;
        }
        ScalarValue::I64(n) => {
            match scalar_type {
                // Handle signed types
                Some(ScalarType::I8) => wip = wip.set(n as i8).map_err(&reflect_err)?,
                Some(ScalarType::I16) => wip = wip.set(n as i16).map_err(&reflect_err)?,
                Some(ScalarType::I32) => wip = wip.set(n as i32).map_err(&reflect_err)?,
                Some(ScalarType::I64) => wip = wip.set(n).map_err(&reflect_err)?,
                Some(ScalarType::I128) => wip = wip.set(n as i128).map_err(&reflect_err)?,
                Some(ScalarType::ISize) => wip = wip.set(n as isize).map_err(&reflect_err)?,
                // Handle unsigned types (I64 can fit in unsigned if non-negative)
                Some(ScalarType::U8) => wip = wip.set(n as u8).map_err(&reflect_err)?,
                Some(ScalarType::U16) => wip = wip.set(n as u16).map_err(&reflect_err)?,
                Some(ScalarType::U32) => wip = wip.set(n as u32).map_err(&reflect_err)?,
                Some(ScalarType::U64) => wip = wip.set(n as u64).map_err(&reflect_err)?,
                Some(ScalarType::U128) => wip = wip.set(n as u128).map_err(&reflect_err)?,
                Some(ScalarType::USize) => wip = wip.set(n as usize).map_err(&reflect_err)?,
                // Handle floats
                Some(ScalarType::F32) => wip = wip.set(n as f32).map_err(&reflect_err)?,
                Some(ScalarType::F64) => wip = wip.set(n as f64).map_err(&reflect_err)?,
                // Handle String - stringify the number
                Some(ScalarType::String) => {
                    wip = wip
                        .set(alloc::string::ToString::to_string(&n))
                        .map_err(&reflect_err)?
                }
                _ => wip = wip.set(n).map_err(&reflect_err)?,
            }
        }
        ScalarValue::U64(n) => {
            match scalar_type {
                // Handle unsigned types
                Some(ScalarType::U8) => wip = wip.set(n as u8).map_err(&reflect_err)?,
                Some(ScalarType::U16) => wip = wip.set(n as u16).map_err(&reflect_err)?,
                Some(ScalarType::U32) => wip = wip.set(n as u32).map_err(&reflect_err)?,
                Some(ScalarType::U64) => wip = wip.set(n).map_err(&reflect_err)?,
                Some(ScalarType::U128) => wip = wip.set(n as u128).map_err(&reflect_err)?,
                Some(ScalarType::USize) => wip = wip.set(n as usize).map_err(&reflect_err)?,
                // Handle signed types (U64 can fit in signed if small enough)
                Some(ScalarType::I8) => wip = wip.set(n as i8).map_err(&reflect_err)?,
                Some(ScalarType::I16) => wip = wip.set(n as i16).map_err(&reflect_err)?,
                Some(ScalarType::I32) => wip = wip.set(n as i32).map_err(&reflect_err)?,
                Some(ScalarType::I64) => wip = wip.set(n as i64).map_err(&reflect_err)?,
                Some(ScalarType::I128) => wip = wip.set(n as i128).map_err(&reflect_err)?,
                Some(ScalarType::ISize) => wip = wip.set(n as isize).map_err(&reflect_err)?,
                // Handle floats
                Some(ScalarType::F32) => wip = wip.set(n as f32).map_err(&reflect_err)?,
                Some(ScalarType::F64) => wip = wip.set(n as f64).map_err(&reflect_err)?,
                // Handle String - stringify the number
                Some(ScalarType::String) => {
                    wip = wip
                        .set(alloc::string::ToString::to_string(&n))
                        .map_err(&reflect_err)?
                }
                _ => wip = wip.set(n).map_err(&reflect_err)?,
            }
        }
        ScalarValue::U128(n) => {
            match scalar_type {
                Some(ScalarType::U128) => wip = wip.set(n).map_err(&reflect_err)?,
                Some(ScalarType::I128) => wip = wip.set(n as i128).map_err(&reflect_err)?,
                // For smaller types, truncate (caller should have used correct hint)
                _ => wip = wip.set(n as u64).map_err(&reflect_err)?,
            }
        }
        ScalarValue::I128(n) => {
            match scalar_type {
                Some(ScalarType::I128) => wip = wip.set(n).map_err(&reflect_err)?,
                Some(ScalarType::U128) => wip = wip.set(n as u128).map_err(&reflect_err)?,
                // For smaller types, truncate (caller should have used correct hint)
                _ => wip = wip.set(n as i64).map_err(&reflect_err)?,
            }
        }
        ScalarValue::F64(n) => {
            match scalar_type {
                Some(ScalarType::F32) => wip = wip.set(n as f32).map_err(&reflect_err)?,
                Some(ScalarType::F64) => wip = wip.set(n).map_err(&reflect_err)?,
                _ if shape.vtable.has_try_from() && shape.inner.is_some() => {
                    // For opaque types with try_from (like NotNan, OrderedFloat), use
                    // begin_inner() + set + end() to trigger conversion
                    let inner_shape = shape.inner.unwrap();
                    wip = wip.begin_inner().map_err(&reflect_err)?;
                    if inner_shape.is_type::<f32>() {
                        wip = wip.set(n as f32).map_err(&reflect_err)?;
                    } else {
                        wip = wip.set(n).map_err(&reflect_err)?;
                    }
                    wip = wip.end().map_err(&reflect_err)?;
                }
                _ if shape.vtable.has_parse() => {
                    // For types that support parsing (like Decimal), convert to string
                    // and use parse_from_str to preserve their parsing semantics
                    wip = wip
                        .parse_from_str(&alloc::string::ToString::to_string(&n))
                        .map_err(&reflect_err)?;
                }
                _ => wip = wip.set(n).map_err(&reflect_err)?,
            }
        }
        ScalarValue::Str(s) => {
            // Try parse_from_str first if the type supports it
            if shape.vtable.has_parse() {
                wip = wip.parse_from_str(s.as_ref()).map_err(&reflect_err)?;
            } else {
                // Delegate to set_string_value - this requires the caller to handle it
                return Err(SetScalarResult::NeedsStringValue { wip, s });
            }
        }
        ScalarValue::Bytes(b) => {
            // First try parse_from_bytes if the type supports it (e.g., UUID from 16 bytes)
            if shape.vtable.has_parse_bytes() {
                wip = wip.parse_from_bytes(b.as_ref()).map_err(&reflect_err)?;
            } else {
                // Delegate to set_bytes_value - this requires the caller to handle it
                return Err(SetScalarResult::NeedsBytesValue { wip, b });
            }
        }
        ScalarValue::Unit => {
            // Unit value - set to default/unit value
            wip = wip.set_default().map_err(&reflect_err)?;
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
    Error(InnerDeserializeError),
}

impl<'input, const BORROW: bool> From<InnerDeserializeError> for SetScalarResult<'input, BORROW> {
    fn from(e: InnerDeserializeError) -> Self {
        SetScalarResult::Error(e)
    }
}

impl<'input, const BORROW: bool, P> FormatDeserializer<'input, BORROW, P>
where
    P: FormatParser<'input>,
{
    /// Set a scalar value into a `Partial`, handling type coercion.
    ///
    /// This is a thin wrapper around `set_scalar_inner` that handles the
    /// string/bytes delegation cases and converts error types.
    pub(crate) fn set_scalar(
        &mut self,
        wip: Partial<'input, BORROW>,
        scalar: ScalarValue<'input>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>> {
        match set_scalar_inner(wip, scalar, self.last_span) {
            Ok(wip) => Ok(wip),
            Err(SetScalarResult::NeedsStringValue { wip, s }) => self.set_string_value(wip, s),
            Err(SetScalarResult::NeedsBytesValue { wip, b }) => self.set_bytes_value(wip, b),
            Err(SetScalarResult::Error(e)) => Err(e.into_deserialize_error()),
        }
    }

    /// Set a string value, handling `&str`, `Cow<str>`, and `String` appropriately.
    pub(crate) fn set_string_value(
        &mut self,
        wip: Partial<'input, BORROW>,
        s: Cow<'input, str>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>> {
        facet_dessert::set_string_value(wip, s, self.last_span).map_err(|e| match e {
            facet_dessert::DessertError::Reflect { error, span } => DeserializeError::Reflect {
                error,
                span,
                path: None,
            },
            facet_dessert::DessertError::CannotBorrow { message } => {
                DeserializeError::CannotBorrow {
                    message: message.into_owned(),
                }
            }
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
    ) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>> {
        facet_dessert::set_bytes_value(wip, b, self.last_span).map_err(|e| match e {
            facet_dessert::DessertError::Reflect { error, span } => DeserializeError::Reflect {
                error,
                span,
                path: None,
            },
            facet_dessert::DessertError::CannotBorrow { message } => {
                DeserializeError::CannotBorrow {
                    message: message.into_owned(),
                }
            }
        })
    }
}
