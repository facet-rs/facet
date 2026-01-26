//! Sweet helpers for facet deserialization.
//!
//! This crate provides common setter functions for handling string, bytes, and scalar values
//! when deserializing into facet types. It's used by both `facet-format` and `facet-dom`.
//!
//! By extracting these functions into a non-generic crate, we reduce monomorphization bloat
//! in format deserializers. See <https://github.com/bearcove/facet/issues/1924> for details.

extern crate alloc;

use std::borrow::Cow;

use facet_core::{Def, KnownPointer, Type, UserType};
use facet_reflect::{Partial, ReflectError, Span};

/// Result of checking if a pointer type needs special handling.
pub enum PointerAction {
    /// Pointer to str (`Cow<str>`, `&str`, `Arc<str>`, `Box<str>`, `Rc<str>`) - should be handled as a scalar/string.
    /// `set_string_value` already handles these via `begin_smart_ptr` internally.
    HandleAsScalar,
    /// Smart pointer with slice builder (`Arc<[T]>`, `Box<[T]>`) - deserialize as list, then end().
    SliceBuilder,
    /// Smart pointer with sized pointee (`Arc<T>`, `Box<T>`) - deserialize inner, then end().
    SizedPointee,
}

/// Prepare a smart pointer for deserialization.
///
/// This handles the common logic of:
/// 1. Detecting string pointers (`Cow<str>`, `&str`, `Arc<str>`, `Box<str>`, `Rc<str>`) which should be handled as scalars
/// 2. Calling `begin_smart_ptr()` for other pointer types
/// 3. Detecting whether it's a slice builder or sized pointee
///
/// Returns the prepared `Partial` and an action indicating what the caller should do next.
///
/// # Usage
/// ```ignore
/// match begin_pointer(wip)? {
///     (wip, PointerAction::HandleAsScalar) => {
///         // Handle as scalar/string - set_string_value handles all str pointers
///         deserialize_scalar(wip)
///     }
///     (wip, PointerAction::SliceBuilder) => {
///         // Arc<[T]>, Box<[T]>, etc - deserialize list items
///         let wip = deserialize_list(wip)?;
///         wip.end()
///     }
///     (wip, PointerAction::SizedPointee) => {
///         // Arc<T>, Box<T> - deserialize inner
///         let wip = deserialize_into(wip)?;
///         wip.end()
///     }
/// }
/// ```
pub fn begin_pointer<'input, const BORROW: bool>(
    mut wip: Partial<'input, BORROW>,
) -> Result<(Partial<'input, BORROW>, PointerAction), DessertError> {
    let shape = wip.shape();
    let ptr_def = match &shape.def {
        Def::Pointer(ptr_def) => ptr_def,
        _ => {
            return Err(DessertError::Reflect {
                error: ReflectError::OperationFailed {
                    shape,
                    operation: "begin_pointer requires a pointer type",
                },
                span: None,
            });
        }
    };

    // All string pointers (Cow<str>, &str, Arc<str>, Box<str>, Rc<str>) - handle as scalar
    // set_string_value handles begin_smart_ptr internally for Arc/Box/Rc
    if ptr_def
        .pointee()
        .is_some_and(|p| p.type_identifier == "str")
    {
        return Ok((wip, PointerAction::HandleAsScalar));
    }

    // Regular smart pointer (Box, Arc, Rc) with non-str pointee
    wip = wip.begin_smart_ptr()?;

    // Check if begin_smart_ptr set up a slice builder (for Arc<[T]>, Rc<[T]>, Box<[T]>)
    let action = if wip.is_building_smart_ptr_slice() {
        PointerAction::SliceBuilder
    } else {
        PointerAction::SizedPointee
    };

    Ok((wip, action))
}

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

/// Set a string value, handling `Option<T>`, parseable types, enums, and string types.
///
/// This function handles:
/// 1. `Option<T>` - unwraps to Some and recurses
/// 2. Types with `parse_from_str` (numbers, bools, etc.)
/// 3. Enums - selects variant by name (externally tagged) or by discriminant (numeric)
/// 4. Transparent structs (newtypes) - recurses into inner type
/// 5. String types (`&str`, `Cow<str>`, `String`)
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

    // Handle enums by selecting a variant by name (or by discriminant for numeric enums)
    if let Type::User(UserType::Enum(enum_def)) = &shape.ty {
        // For numeric enums (e.g., #[repr(u8)]), try parsing the string as a discriminant
        if shape.is_numeric()
            && let Ok(discriminant) = s.parse::<i64>()
        {
            wip = wip.select_variant(discriminant)?;
            return Ok(wip);
        }
        // Fall through to try name-based lookup

        // Try to find a variant by effective name (respects #[facet(rename = "...")])
        if let Some((_, variant)) = enum_def
            .variants
            .iter()
            .enumerate()
            .find(|(_, v)| v.effective_name() == s.as_ref())
        {
            wip = wip.select_variant_named(variant.effective_name())?;
            return Ok(wip);
        }

        // No variant found - return an error
        return Err(DessertError::Reflect {
            error: ReflectError::OperationFailed {
                shape,
                operation: "no matching enum variant found for string value",
            },
            span,
        });
    }

    // Handle transparent structs (newtypes) by unwrapping to the inner type
    if shape.is_transparent() {
        wip = wip.begin_nth_field(0)?;
        wip = set_string_value(wip, s, span)?;
        wip = wip.end()?;
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

    // Arc<str>, Box<str>, Rc<str> - use begin_smart_ptr + set(String) + end()
    if let Def::Pointer(ptr_def) = shape.def
        && matches!(
            ptr_def.known,
            Some(KnownPointer::Arc | KnownPointer::Box | KnownPointer::Rc)
        )
        && ptr_def
            .pointee()
            .is_some_and(|p| p.type_identifier == "str")
    {
        wip = wip.begin_smart_ptr().map_err(&reflect_err)?;
        wip = wip.set(s.into_owned()).map_err(&reflect_err)?;
        wip = wip.end().map_err(&reflect_err)?;
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
