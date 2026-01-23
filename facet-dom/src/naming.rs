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
use heck::{AsKebabCase, AsPascalCase, AsShoutySnakeCase, AsSnakeCase};

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

/// Apply a rename_all transformation to a name.
///
/// Supported values (matching serde conventions):
/// - "lowercase" - all lowercase
/// - "UPPERCASE" - all uppercase
/// - "PascalCase" / "UpperCamelCase" - first letter of each word uppercase
/// - "camelCase" / "lowerCamelCase" - like PascalCase but first letter lowercase
/// - "snake_case" - lowercase with underscores
/// - "SCREAMING_SNAKE_CASE" / "UPPER_SNAKE_CASE" - uppercase with underscores
/// - "kebab-case" - lowercase with dashes
/// - "SCREAMING-KEBAB-CASE" / "UPPER-KEBAB-CASE" - uppercase with dashes
///
/// Returns the original name if the rename_all value is not recognized.
pub fn apply_rename_all(name: &str, rename_all: &str) -> String {
    match rename_all {
        "lowercase" => name.to_lowercase(),
        "UPPERCASE" => name.to_uppercase(),
        "PascalCase" | "UpperCamelCase" => format!("{}", AsPascalCase(name)),
        "camelCase" | "lowerCamelCase" => format!("{}", AsLowerCamelCase(name)),
        "snake_case" => format!("{}", AsSnakeCase(name)),
        "SCREAMING_SNAKE_CASE" | "UPPER_SNAKE_CASE" => format!("{}", AsShoutySnakeCase(name)),
        "kebab-case" => format!("{}", AsKebabCase(name)),
        "SCREAMING-KEBAB-CASE" | "UPPER-KEBAB-CASE" => {
            format!("{}", AsKebabCase(name)).to_uppercase()
        }
        _ => name.to_string(),
    }
}
