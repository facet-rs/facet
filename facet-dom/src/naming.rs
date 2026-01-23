//! Name conversion utilities for DOM serialization/deserialization.
//!
//! facet-dom uses lowerCamelCase as the default naming convention for element and
//! attribute names. This matches common usage in XML formats like SVG and Atom.
//!
//! Examples:
//! - `struct Banana` → `<banana>`
//! - `struct MyPlaylist` → `<myPlaylist>`
//! - `field_name: String` → `<fieldName>`
//! - tuple field `0` → `<_0>` (XML names can't start with digits)

use std::borrow::Cow;

pub use heck::AsLowerCamelCase;

/// Convert a Rust identifier to a valid XML element name in lowerCamelCase.
///
/// Uses `AsLowerCamelCase` for the conversion, but checks if allocation is needed.
/// Also handles numeric field names (from tuple structs/variants) by prefixing with underscore,
/// since XML element names cannot start with a digit.
#[inline]
pub fn to_element_name(name: &str) -> Cow<'_, str> {
    // Handle numeric field names (tuple fields like "0", "1", etc.)
    // XML element names cannot start with a digit, so prefix with underscore
    if name.starts_with(|c: char| c.is_ascii_digit()) {
        return Cow::Owned(format!("_{name}"));
    }

    // Fast path: check if already lowerCamelCase by comparing formatted output
    let converted = format!("{}", AsLowerCamelCase(name));
    if converted == name {
        Cow::Borrowed(name)
    } else {
        Cow::Owned(converted)
    }
}

/// Compute the DOM key for a field.
///
/// If `rename` is `Some`, use it directly (explicit rename or rename_all transformation).
/// Otherwise, apply lowerCamelCase to the raw field name as the default convention.
#[inline]
pub fn dom_key<'a>(name: &'a str, rename: Option<&'a str>) -> Cow<'a, str> {
    match rename {
        Some(r) => Cow::Borrowed(r),
        None => to_element_name(name),
    }
}
