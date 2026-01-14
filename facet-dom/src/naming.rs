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

/// Compute the DOM key for a field/type: use explicit rename if present, otherwise lowerCamelCase.
///
/// This is the single source of truth for name conversion in facet-dom.
#[inline]
pub fn dom_key(rename: Option<&str>, name: &str) -> String {
    match rename {
        Some(r) => r.to_string(),
        None => format!("{}", AsLowerCamelCase(name)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_names() {
        assert_eq!(&*to_element_name("Banana"), "banana");
        assert_eq!(&*to_element_name("MyPlaylist"), "myPlaylist");
        assert_eq!(&*to_element_name("XMLParser"), "xmlParser");
        assert_eq!(&*to_element_name("HTTPSConnection"), "httpsConnection");
    }

    #[test]
    fn test_field_names() {
        assert_eq!(&*to_element_name("field_name"), "fieldName");
        assert_eq!(&*to_element_name("my_field"), "myField");
    }

    #[test]
    fn test_already_lower_camel_borrows() {
        assert!(matches!(to_element_name("banana"), Cow::Borrowed(_)));
        assert!(matches!(to_element_name("myPlaylist"), Cow::Borrowed(_)));
    }

    #[test]
    fn test_needs_conversion_owns() {
        assert!(matches!(to_element_name("Banana"), Cow::Owned(_)));
        assert!(matches!(to_element_name("field_name"), Cow::Owned(_)));
    }
}
