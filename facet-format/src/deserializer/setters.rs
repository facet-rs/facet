extern crate alloc;

use std::borrow::Cow;

use facet_core::{Def, KnownPointer};
use facet_reflect::{Partial, ReflectError};

use crate::{DeserializeError, FormatDeserializer, FormatParser, ScalarValue};

impl<'input, const BORROW: bool, P> FormatDeserializer<'input, BORROW, P>
where
    P: FormatParser<'input>,
{
    pub(crate) fn set_scalar(
        &mut self,
        mut wip: Partial<'input, BORROW>,
        scalar: ScalarValue<'input>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>> {
        panic!(
            "this is using type_identifier for type identification which is... well I know what the field is named but it's VERBOTEN"
        );

        let shape = wip.shape();
        // Capture the span for error reporting - this is where the scalar value was parsed
        let span = self.last_span;
        let reflect_err = |e: ReflectError| DeserializeError::Reflect {
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
                // Handle signed types
                if shape.type_identifier == "i8" {
                    wip = wip.set(n as i8).map_err(&reflect_err)?;
                } else if shape.type_identifier == "i16" {
                    wip = wip.set(n as i16).map_err(&reflect_err)?;
                } else if shape.type_identifier == "i32" {
                    wip = wip.set(n as i32).map_err(&reflect_err)?;
                } else if shape.type_identifier == "i64" {
                    wip = wip.set(n).map_err(&reflect_err)?;
                } else if shape.type_identifier == "i128" {
                    wip = wip.set(n as i128).map_err(&reflect_err)?;
                } else if shape.type_identifier == "isize" {
                    wip = wip.set(n as isize).map_err(&reflect_err)?;
                // Handle unsigned types (I64 can fit in unsigned if non-negative)
                } else if shape.type_identifier == "u8" {
                    wip = wip.set(n as u8).map_err(&reflect_err)?;
                } else if shape.type_identifier == "u16" {
                    wip = wip.set(n as u16).map_err(&reflect_err)?;
                } else if shape.type_identifier == "u32" {
                    wip = wip.set(n as u32).map_err(&reflect_err)?;
                } else if shape.type_identifier == "u64" {
                    wip = wip.set(n as u64).map_err(&reflect_err)?;
                } else if shape.type_identifier == "u128" {
                    wip = wip.set(n as u128).map_err(&reflect_err)?;
                } else if shape.type_identifier == "usize" {
                    wip = wip.set(n as usize).map_err(&reflect_err)?;
                // Handle floats
                } else if shape.type_identifier == "f32" {
                    wip = wip.set(n as f32).map_err(&reflect_err)?;
                } else if shape.type_identifier == "f64" {
                    wip = wip.set(n as f64).map_err(&reflect_err)?;
                // Handle String - stringify the number
                } else if shape.type_identifier == "String" {
                    wip = wip
                        .set(alloc::string::ToString::to_string(&n))
                        .map_err(&reflect_err)?;
                } else {
                    wip = wip.set(n).map_err(&reflect_err)?;
                }
            }
            ScalarValue::U64(n) => {
                // Handle unsigned types
                if shape.type_identifier == "u8" {
                    wip = wip.set(n as u8).map_err(&reflect_err)?;
                } else if shape.type_identifier == "u16" {
                    wip = wip.set(n as u16).map_err(&reflect_err)?;
                } else if shape.type_identifier == "u32" {
                    wip = wip.set(n as u32).map_err(&reflect_err)?;
                } else if shape.type_identifier == "u64" {
                    wip = wip.set(n).map_err(&reflect_err)?;
                } else if shape.type_identifier == "u128" {
                    wip = wip.set(n as u128).map_err(&reflect_err)?;
                } else if shape.type_identifier == "usize" {
                    wip = wip.set(n as usize).map_err(&reflect_err)?;
                // Handle signed types (U64 can fit in signed if small enough)
                } else if shape.type_identifier == "i8" {
                    wip = wip.set(n as i8).map_err(&reflect_err)?;
                } else if shape.type_identifier == "i16" {
                    wip = wip.set(n as i16).map_err(&reflect_err)?;
                } else if shape.type_identifier == "i32" {
                    wip = wip.set(n as i32).map_err(&reflect_err)?;
                } else if shape.type_identifier == "i64" {
                    wip = wip.set(n as i64).map_err(&reflect_err)?;
                } else if shape.type_identifier == "i128" {
                    wip = wip.set(n as i128).map_err(&reflect_err)?;
                } else if shape.type_identifier == "isize" {
                    wip = wip.set(n as isize).map_err(&reflect_err)?;
                // Handle floats
                } else if shape.type_identifier == "f32" {
                    wip = wip.set(n as f32).map_err(&reflect_err)?;
                } else if shape.type_identifier == "f64" {
                    wip = wip.set(n as f64).map_err(&reflect_err)?;
                // Handle String - stringify the number
                } else if shape.type_identifier == "String" {
                    wip = wip
                        .set(alloc::string::ToString::to_string(&n))
                        .map_err(&reflect_err)?;
                } else {
                    wip = wip.set(n).map_err(&reflect_err)?;
                }
            }
            ScalarValue::U128(n) => {
                // Handle u128 scalar
                if shape.type_identifier == "u128" {
                    wip = wip.set(n).map_err(&reflect_err)?;
                } else if shape.type_identifier == "i128" {
                    wip = wip.set(n as i128).map_err(&reflect_err)?;
                } else {
                    // For smaller types, truncate (caller should have used correct hint)
                    wip = wip.set(n as u64).map_err(&reflect_err)?;
                }
            }
            ScalarValue::I128(n) => {
                // Handle i128 scalar
                if shape.type_identifier == "i128" {
                    wip = wip.set(n).map_err(&reflect_err)?;
                } else if shape.type_identifier == "u128" {
                    wip = wip.set(n as u128).map_err(&reflect_err)?;
                } else {
                    // For smaller types, truncate (caller should have used correct hint)
                    wip = wip.set(n as i64).map_err(&reflect_err)?;
                }
            }
            ScalarValue::F64(n) => {
                if shape.type_identifier == "f32" {
                    wip = wip.set(n as f32).map_err(&reflect_err)?;
                } else if shape.type_identifier == "f64" {
                    wip = wip.set(n).map_err(&reflect_err)?;
                } else if shape.vtable.has_try_from() && shape.inner.is_some() {
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
                } else if shape.vtable.has_parse() {
                    // For types that support parsing (like Decimal), convert to string
                    // and use parse_from_str to preserve their parsing semantics
                    wip = wip
                        .parse_from_str(&alloc::string::ToString::to_string(&n))
                        .map_err(&reflect_err)?;
                } else {
                    wip = wip.set(n).map_err(&reflect_err)?;
                }
            }
            ScalarValue::Str(s) => {
                // Try parse_from_str first if the type supports it
                if shape.vtable.has_parse() {
                    wip = wip.parse_from_str(s.as_ref()).map_err(&reflect_err)?;
                } else {
                    wip = self.set_string_value(wip, s)?;
                }
            }
            ScalarValue::Bytes(b) => {
                // First try parse_from_bytes if the type supports it (e.g., UUID from 16 bytes)
                if shape.vtable.has_parse_bytes() {
                    wip = wip.parse_from_bytes(b.as_ref()).map_err(&reflect_err)?;
                } else {
                    // Fall back to setting as Vec<u8>
                    wip = wip.set(b.into_owned()).map_err(&reflect_err)?;
                }
            }
            ScalarValue::StringlyTyped(s) => {
                // Stringly-typed values from XML need to be parsed based on target type.
                //
                // For DynamicValue (like facet_value::Value), we need to detect the type
                // by trying to parse as null, bool, number, then falling back to string.
                //
                // For concrete types with has_parse(), use parse_from_str.
                // For string types, use set_string_value.
                if matches!(shape.def, facet_core::Def::DynamicValue(_)) {
                    // Try to detect the type for DynamicValue
                    let text = s.as_ref();
                    if text.eq_ignore_ascii_case("null") {
                        wip = wip.set_default().map_err(&reflect_err)?;
                    } else if let Ok(b) = text.parse::<bool>() {
                        wip = wip.set(b).map_err(&reflect_err)?;
                    } else if let Ok(n) = text.parse::<i64>() {
                        wip = wip.set(n).map_err(&reflect_err)?;
                    } else if let Ok(n) = text.parse::<u64>() {
                        wip = wip.set(n).map_err(&reflect_err)?;
                    } else if let Ok(n) = text.parse::<f64>() {
                        wip = wip.set(n).map_err(&reflect_err)?;
                    } else {
                        // Fall back to string
                        wip = self.set_string_value(wip, s)?;
                    }
                } else if shape.vtable.has_parse() {
                    wip = wip.parse_from_str(s.as_ref()).map_err(&reflect_err)?;
                } else {
                    wip = self.set_string_value(wip, s)?;
                }
            }
            ScalarValue::Unit => {
                // Unit value - set to default/unit value
                wip = wip.set_default().map_err(&reflect_err)?;
            }
        }

        Ok(wip)
    }

    /// Set a string value, handling `&str`, `Cow<str>`, and `String` appropriately.
    pub(crate) fn set_string_value(
        &mut self,
        mut wip: Partial<'input, BORROW>,
        s: Cow<'input, str>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>> {
        let shape = wip.shape();

        // Check if target is &str (shared reference to str)
        if let Def::Pointer(ptr_def) = shape.def
            && matches!(ptr_def.known, Some(KnownPointer::SharedReference))
            && ptr_def
                .pointee()
                .is_some_and(|p| p.type_identifier == "str")
        {
            // In owned mode, we cannot borrow from input at all
            if !BORROW {
                return Err(DeserializeError::CannotBorrow {
                message: "cannot deserialize into &str when borrowing is disabled - use String or Cow<str> instead".into(),
            });
            }
            match s {
                Cow::Borrowed(borrowed) => {
                    wip = wip.set(borrowed).map_err(DeserializeError::reflect)?;
                    return Ok(wip);
                }
                Cow::Owned(_) => {
                    return Err(DeserializeError::CannotBorrow {
                    message: "cannot borrow &str from string containing escape sequences - use String or Cow<str> instead".into(),
                });
                }
            }
        }

        // Check if target is Cow<str>
        if let Def::Pointer(ptr_def) = shape.def
            && matches!(ptr_def.known, Some(KnownPointer::Cow))
            && ptr_def
                .pointee()
                .is_some_and(|p| p.type_identifier == "str")
        {
            wip = wip.set(s).map_err(DeserializeError::reflect)?;
            return Ok(wip);
        }

        // Default: convert to owned String
        wip = wip.set(s.into_owned()).map_err(DeserializeError::reflect)?;
        Ok(wip)
    }

    /// Set a bytes value with proper handling for borrowed vs owned data.
    ///
    /// This handles `&[u8]`, `Cow<[u8]>`, and `Vec<u8>` appropriately based on
    /// whether borrowing is enabled and whether the data is borrowed or owned.
    pub(crate) fn set_bytes_value(
        &mut self,
        mut wip: Partial<'input, BORROW>,
        b: Cow<'input, [u8]>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>> {
        let shape = wip.shape();

        // Helper to check if a shape is a byte slice ([u8])
        let is_byte_slice = |pointee: &facet_core::Shape| matches!(pointee.def, Def::Slice(slice_def) if slice_def.t.type_identifier == "u8");

        // Check if target is &[u8] (shared reference to byte slice)
        if let Def::Pointer(ptr_def) = shape.def
            && matches!(ptr_def.known, Some(KnownPointer::SharedReference))
            && ptr_def.pointee().is_some_and(is_byte_slice)
        {
            // In owned mode, we cannot borrow from input at all
            if !BORROW {
                return Err(DeserializeError::CannotBorrow {
                message: "cannot deserialize into &[u8] when borrowing is disabled - use Vec<u8> or Cow<[u8]> instead".into(),
            });
            }
            match b {
                Cow::Borrowed(borrowed) => {
                    wip = wip.set(borrowed).map_err(DeserializeError::reflect)?;
                    return Ok(wip);
                }
                Cow::Owned(_) => {
                    return Err(DeserializeError::CannotBorrow {
                    message: "cannot borrow &[u8] from owned bytes - use Vec<u8> or Cow<[u8]> instead".into(),
                });
                }
            }
        }

        // Check if target is Cow<[u8]>
        if let Def::Pointer(ptr_def) = shape.def
            && matches!(ptr_def.known, Some(KnownPointer::Cow))
            && ptr_def.pointee().is_some_and(is_byte_slice)
        {
            wip = wip.set(b).map_err(DeserializeError::reflect)?;
            return Ok(wip);
        }

        // Default: convert to owned Vec<u8>
        wip = wip.set(b.into_owned()).map_err(DeserializeError::reflect)?;
        Ok(wip)
    }
}
