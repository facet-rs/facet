//! Name conversion utilities for DOM serialization/deserialization.
//!
//! facet-dom uses lowerCamelCase as the default naming convention for element and
//! attribute names. This matches common usage in XML formats like SVG and Atom.
//!
//! Examples:
//! - `struct Banana` → `<banana>`
//! - `struct MyPlaylist` → `<myPlaylist>`
//! - `field_name: String` → `<fieldName>`

use std::borrow::Cow;

pub use heck::AsLowerCamelCase;

/// Convert a Rust identifier to lowerCamelCase, returning Cow::Borrowed if unchanged.
///
/// Uses `AsLowerCamelCase` for the conversion, but checks if allocation is needed.
#[inline]
pub fn to_element_name(name: &str) -> Cow<'_, str> {
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
