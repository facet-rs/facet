//! Sweet helpers for facet deserialization.
//!
//! This crate provides common setter functions for handling string, bytes, and scalar values
//! when deserializing into facet types. It's used by both `facet-format` and `facet-dom`.

extern crate alloc;

use std::borrow::Cow;

use facet_core::{Def, KnownPointer};
use facet_reflect::{Partial, ReflectError, Span};

/// Error type for dessert operations.
#[derive(Debug)]
pub enum DessertError {
    /// A reflection error occurred.
    Reflect {
        /// The underlying reflection error.
        error: ReflectError,
        /// Optional span where the error occurred.
        span: Option<Span>,
    },
    /// Cannot borrow from input.
    CannotBorrow {
        /// Message explaining why borrowing failed.
        message: Cow<'static, str>,
    },
}

impl std::fmt::Display for DessertError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DessertError::Reflect { error, span } => {
                if let Some(span) = span {
                    write!(f, "{} at {:?}", error, span)
                } else {
                    write!(f, "{}", error)
                }
            }
            DessertError::CannotBorrow { message } => write!(f, "{}", message),
        }
    }
}

impl std::error::Error for DessertError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            DessertError::Reflect { error, .. } => Some(error),
            DessertError::CannotBorrow { .. } => None,
        }
    }
}

impl From<ReflectError> for DessertError {
    fn from(error: ReflectError) -> Self {
        DessertError::Reflect { error, span: None }
    }
}

/// Set a string value, handling `Option<T>`, parseable types, and string types.
///
/// This function handles:
/// 1. `Option<T>` - unwraps to Some and recurses
/// 2. Types with `parse_from_str` (numbers, bools, etc.)
/// 3. String types (`&str`, `Cow<str>`, `String`)
pub fn set_string_value<'input, const BORROW: bool>(
    mut wip: Partial<'input, BORROW>,
    s: Cow<'input, str>,
    span: Option<Span>,
) -> Result<Partial<'input, BORROW>, DessertError> {
    let shape = wip.shape();

    if matches!(&shape.def, Def::Option(_)) {
        wip = wip.begin_some()?;
        wip = set_string_value(wip, s, span)?;
        wip = wip.end()?;
        return Ok(wip);
    }

    if shape.vtable.has_parse() {
        wip = wip.parse_from_str(s.as_ref())?;
        return Ok(wip);
    }

    set_string_value_inner(wip, s, span)
}

fn set_string_value_inner<'input, const BORROW: bool>(
    mut wip: Partial<'input, BORROW>,
    s: Cow<'input, str>,
    span: Option<Span>,
) -> Result<Partial<'input, BORROW>, DessertError> {
    let shape = wip.shape();

    let reflect_err = |e: ReflectError| DessertError::Reflect { error: e, span };

    if let Def::Pointer(ptr_def) = shape.def
        && matches!(ptr_def.known, Some(KnownPointer::SharedReference))
        && ptr_def
            .pointee()
            .is_some_and(|p| p.type_identifier == "str")
    {
        if !BORROW {
            return Err(DessertError::CannotBorrow {
                message: "cannot deserialize into &str when borrowing is disabled - use String or Cow<str> instead".into(),
            });
        }
        match s {
            Cow::Borrowed(borrowed) => {
                wip = wip.set(borrowed).map_err(&reflect_err)?;
                return Ok(wip);
            }
            Cow::Owned(_) => {
                return Err(DessertError::CannotBorrow {
                    message: "cannot borrow &str from string containing escape sequences - use String or Cow<str> instead".into(),
                });
            }
        }
    }

    if let Def::Pointer(ptr_def) = shape.def
        && matches!(ptr_def.known, Some(KnownPointer::Cow))
        && ptr_def
            .pointee()
            .is_some_and(|p| p.type_identifier == "str")
    {
        wip = wip.set(s).map_err(&reflect_err)?;
        return Ok(wip);
    }

    wip = wip.set(s.into_owned()).map_err(&reflect_err)?;
    Ok(wip)
}

/// Set a bytes value with proper handling for borrowed vs owned data.
///
/// This handles `&[u8]`, `Cow<[u8]>`, and `Vec<u8>` appropriately based on
/// whether borrowing is enabled and whether the data is borrowed or owned.
pub fn set_bytes_value<'input, const BORROW: bool>(
    mut wip: Partial<'input, BORROW>,
    b: Cow<'input, [u8]>,
    span: Option<Span>,
) -> Result<Partial<'input, BORROW>, DessertError> {
    let shape = wip.shape();

    let reflect_err = |e: ReflectError| DessertError::Reflect { error: e, span };

    let is_byte_slice = |pointee: &facet_core::Shape| matches!(pointee.def, Def::Slice(slice_def) if slice_def.t.type_identifier == "u8");

    if let Def::Pointer(ptr_def) = shape.def
        && matches!(ptr_def.known, Some(KnownPointer::SharedReference))
        && ptr_def.pointee().is_some_and(is_byte_slice)
    {
        if !BORROW {
            return Err(DessertError::CannotBorrow {
                message: "cannot deserialize into &[u8] when borrowing is disabled - use Vec<u8> or Cow<[u8]> instead".into(),
            });
        }
        match b {
            Cow::Borrowed(borrowed) => {
                wip = wip.set(borrowed).map_err(&reflect_err)?;
                return Ok(wip);
            }
            Cow::Owned(_) => {
                return Err(DessertError::CannotBorrow {
                    message:
                        "cannot borrow &[u8] from owned bytes - use Vec<u8> or Cow<[u8]> instead"
                            .into(),
                });
            }
        }
    }

    if let Def::Pointer(ptr_def) = shape.def
        && matches!(ptr_def.known, Some(KnownPointer::Cow))
        && ptr_def.pointee().is_some_and(is_byte_slice)
    {
        wip = wip.set(b).map_err(&reflect_err)?;
        return Ok(wip);
    }

    wip = wip.set(b.into_owned()).map_err(&reflect_err)?;
    Ok(wip)
}
