//! CSV parser and serializer using facet-format.
//!
//! **Note:** CSV is a fundamentally different format from JSON/XML/YAML.
//! While those formats are tree-structured and map naturally to nested types,
//! CSV is a flat, row-based format where each row represents a single record
//! and each column represents a field.
//!
//! This crate provides basic CSV support via the `FormatParser` trait, but
//! has significant limitations:
//!
//! - No support for nested structures (CSV is inherently flat)
//! - No support for arrays/sequences as field values
//! - No support for enums beyond unit variants (encoded as strings)
//! - All values are strings and must be parseable to target types
//!
//! For more sophisticated CSV handling, consider a dedicated CSV library.

#![forbid(unsafe_code)]

extern crate alloc;

mod error;
mod parser;
mod serializer;

pub use error::{CsvError, CsvErrorKind};
pub use parser::CsvParser;
pub use serializer::{CsvSerializeError, CsvSerializer, to_string, to_vec, to_writer};

// Re-export DeserializeError for convenience
pub use facet_format::DeserializeError;

use bumpalo::Bump;

/// Deserialize a value from a CSV string into an owned type.
///
/// Note: This parses a single CSV row (not including the header).
/// For multiple rows, iterate over lines and call this for each.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_csv::from_str;
///
/// #[derive(Facet, Debug, PartialEq)]
/// struct Person {
///     name: String,
///     age: u32,
/// }
///
/// let csv = "Alice,30";
/// let person: Person = from_str(csv).unwrap();
/// assert_eq!(person.name, "Alice");
/// assert_eq!(person.age, 30);
/// ```
pub fn from_str<T>(input: &str) -> Result<T, DeserializeError>
where
    T: facet_core::Facet<'static>,
{
    use facet_format::FormatDeserializer;
    let bump = Bump::new();
    let mut parser = CsvParser::new(input);
    let mut de = FormatDeserializer::new_owned(&bump, &mut parser);
    de.deserialize_root()
}

/// Deserialize a value from a CSV string, allowing zero-copy borrowing.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_csv::from_str_borrowed;
///
/// #[derive(Facet, Debug, PartialEq)]
/// struct Person {
///     name: String,
///     age: u32,
/// }
///
/// let csv = "Alice,30";
/// let person: Person = from_str_borrowed(csv).unwrap();
/// assert_eq!(person.name, "Alice");
/// assert_eq!(person.age, 30);
/// ```
pub fn from_str_borrowed<'input, 'facet, T>(input: &'input str) -> Result<T, DeserializeError>
where
    T: facet_core::Facet<'facet>,
    'input: 'facet,
{
    use facet_format::FormatDeserializer;
    let bump = Bump::new();
    let mut parser = CsvParser::new(input);
    let mut de = FormatDeserializer::new(&bump, &mut parser);
    de.deserialize_root()
}

/// Deserialize a value from CSV bytes into an owned type.
///
/// # Errors
///
/// Returns an error if the input is not valid UTF-8 or if deserialization fails.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_csv::from_slice;
///
/// #[derive(Facet, Debug, PartialEq)]
/// struct Person {
///     name: String,
///     age: u32,
/// }
///
/// let csv = b"Alice,30";
/// let person: Person = from_slice(csv).unwrap();
/// assert_eq!(person.name, "Alice");
/// assert_eq!(person.age, 30);
/// ```
pub fn from_slice<T>(input: &[u8]) -> Result<T, DeserializeError>
where
    T: facet_core::Facet<'static>,
{
    let s = core::str::from_utf8(input).map_err(|e| {
        let mut context = [0u8; 16];
        let context_len = e.valid_up_to().min(16);
        context[..context_len].copy_from_slice(&input[..context_len]);
        facet_format::DeserializeErrorKind::InvalidUtf8 {
            context,
            context_len: context_len as u8,
        }
        .with_span(facet_reflect::Span::new(e.valid_up_to(), 1))
    })?;
    from_str(s)
}

/// Deserialize a value from CSV bytes, allowing zero-copy borrowing.
///
/// # Errors
///
/// Returns an error if the input is not valid UTF-8 or if deserialization fails.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_csv::from_slice_borrowed;
///
/// #[derive(Facet, Debug, PartialEq)]
/// struct Person {
///     name: String,
///     age: u32,
/// }
///
/// let csv = b"Alice,30";
/// let person: Person = from_slice_borrowed(csv).unwrap();
/// assert_eq!(person.name, "Alice");
/// assert_eq!(person.age, 30);
/// ```
pub fn from_slice_borrowed<'input, 'facet, T>(input: &'input [u8]) -> Result<T, DeserializeError>
where
    T: facet_core::Facet<'facet>,
    'input: 'facet,
{
    let s = core::str::from_utf8(input).map_err(|e| {
        let mut context = [0u8; 16];
        let context_len = e.valid_up_to().min(16);
        context[..context_len].copy_from_slice(&input[..context_len]);
        facet_format::DeserializeErrorKind::InvalidUtf8 {
            context,
            context_len: context_len as u8,
        }
        .with_span(facet_reflect::Span::new(e.valid_up_to(), 1))
    })?;
    from_str_borrowed(s)
}
