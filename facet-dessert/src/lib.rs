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

/// Set a string value, handling `&str`, `Cow<str>`, and `String` appropriately.
///
/// # Type Parameters
/// - `'input`: The lifetime of the input data
/// - `BORROW`: Whether borrowing from input is allowed
///
/// # Arguments
/// - `wip`: The partial value being constructed
/// - `s`: The string value to set
/// - `span`: Optional span for error reporting
pub fn set_string_value<'input, const BORROW: bool>(
    mut wip: Partial<'input, BORROW>,
    s: Cow<'input, str>,
    span: Option<Span>,
) -> Result<Partial<'input, BORROW>, DessertError> {
    let shape = wip.shape();

    let reflect_err = |e: ReflectError| DessertError::Reflect { error: e, span };

    // Check if target is &str (shared reference to str)
    if let Def::Pointer(ptr_def) = shape.def
        && matches!(ptr_def.known, Some(KnownPointer::SharedReference))
        && ptr_def
            .pointee()
            .is_some_and(|p| p.type_identifier == "str")
    {
        // In owned mode, we cannot borrow from input at all
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

    // Check if target is Cow<str>
    if let Def::Pointer(ptr_def) = shape.def
        && matches!(ptr_def.known, Some(KnownPointer::Cow))
        && ptr_def
            .pointee()
            .is_some_and(|p| p.type_identifier == "str")
    {
        wip = wip.set(s).map_err(&reflect_err)?;
        return Ok(wip);
    }

    // Default: convert to owned String
    wip = wip.set(s.into_owned()).map_err(&reflect_err)?;
    Ok(wip)
}

/// Set a bytes value with proper handling for borrowed vs owned data.
///
/// This handles `&[u8]`, `Cow<[u8]>`, and `Vec<u8>` appropriately based on
/// whether borrowing is enabled and whether the data is borrowed or owned.
///
/// # Type Parameters
/// - `'input`: The lifetime of the input data
/// - `BORROW`: Whether borrowing from input is allowed
///
/// # Arguments
/// - `wip`: The partial value being constructed
/// - `b`: The bytes value to set
/// - `span`: Optional span for error reporting
pub fn set_bytes_value<'input, const BORROW: bool>(
    mut wip: Partial<'input, BORROW>,
    b: Cow<'input, [u8]>,
    span: Option<Span>,
) -> Result<Partial<'input, BORROW>, DessertError> {
    let shape = wip.shape();

    let reflect_err = |e: ReflectError| DessertError::Reflect { error: e, span };

    // Helper to check if a shape is a byte slice ([u8])
    let is_byte_slice = |pointee: &facet_core::Shape| matches!(pointee.def, Def::Slice(slice_def) if slice_def.t.type_identifier == "u8");

    // Check if target is &[u8] (shared reference to byte slice)
    if let Def::Pointer(ptr_def) = shape.def
        && matches!(ptr_def.known, Some(KnownPointer::SharedReference))
        && ptr_def.pointee().is_some_and(is_byte_slice)
    {
        // In owned mode, we cannot borrow from input at all
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

    // Check if target is Cow<[u8]>
    if let Def::Pointer(ptr_def) = shape.def
        && matches!(ptr_def.known, Some(KnownPointer::Cow))
        && ptr_def.pointee().is_some_and(is_byte_slice)
    {
        wip = wip.set(b).map_err(&reflect_err)?;
        return Ok(wip);
    }

    // Default: convert to owned Vec<u8>
    wip = wip.set(b.into_owned()).map_err(&reflect_err)?;
    Ok(wip)
}
