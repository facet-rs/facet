#![forbid(unsafe_code)]

//! JSON parser and serializer using facet-format.
//!
//! This crate provides JSON support via the `FormatParser` trait.

mod parser;
mod serializer;

pub use parser::{JsonError, JsonParser};
pub use serializer::{JsonSerializeError, JsonSerializer, to_string, to_vec};

// Re-export DeserializeError for convenience
pub use facet_format::DeserializeError;

/// Deserialize a value from a JSON string.
///
/// This is a convenience wrapper around `FormatDeserializer` that handles
/// the common case of deserializing from a complete JSON string.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_format_json::from_str;
///
/// #[derive(Facet, Debug, PartialEq)]
/// struct Person {
///     name: String,
///     age: u32,
/// }
///
/// let json = r#"{"name": "Alice", "age": 30}"#;
/// let person: Person = from_str(json).unwrap();
/// assert_eq!(person.name, "Alice");
/// assert_eq!(person.age, 30);
/// ```
pub fn from_str<'de, T>(input: &'de str) -> Result<T, DeserializeError<JsonError>>
where
    T: facet_core::Facet<'de>,
{
    from_slice(input.as_bytes())
}

/// Deserialize a value from JSON bytes.
///
/// This is a convenience wrapper around `FormatDeserializer` that handles
/// the common case of deserializing from a complete JSON byte slice.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_format_json::from_slice;
///
/// #[derive(Facet, Debug, PartialEq)]
/// struct Point {
///     x: i32,
///     y: i32,
/// }
///
/// let json = br#"{"x": 10, "y": 20}"#;
/// let point: Point = from_slice(json).unwrap();
/// assert_eq!(point.x, 10);
/// assert_eq!(point.y, 20);
/// ```
pub fn from_slice<'de, T>(input: &'de [u8]) -> Result<T, DeserializeError<JsonError>>
where
    T: facet_core::Facet<'de>,
{
    use facet_format::FormatDeserializer;
    let parser = JsonParser::new(input);
    let mut de = FormatDeserializer::new(parser);
    de.deserialize_root()
}

#[cfg(feature = "streaming")]
mod streaming;
#[cfg(feature = "futures-io")]
pub use streaming::from_async_reader_futures;
#[cfg(feature = "tokio")]
pub use streaming::from_async_reader_tokio;
#[cfg(feature = "streaming")]
pub use streaming::from_reader;
