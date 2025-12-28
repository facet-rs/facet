//! YAML parser and serializer using facet-format.
//!
//! This crate provides YAML support via the `FormatParser` trait,
//! using saphyr-parser for streaming event-based parsing.
//!
//! # Example
//!
//! ```
//! use facet::Facet;
//! use facet_format_yaml::{from_str, to_string};
//!
//! #[derive(Facet, Debug, PartialEq)]
//! struct Config {
//!     name: String,
//!     port: u16,
//! }
//!
//! let yaml = "name: myapp\nport: 8080";
//! let config: Config = from_str(yaml).unwrap();
//! assert_eq!(config.name, "myapp");
//! assert_eq!(config.port, 8080);
//!
//! let output = to_string(&config).unwrap();
//! assert!(output.contains("name: myapp"));
//! ```

extern crate alloc;

mod error;
mod parser;
mod serializer;

pub use error::{YamlError, YamlErrorKind};
pub use parser::YamlParser;
pub use serializer::{
    YamlSerializeError, YamlSerializer, peek_to_string, peek_to_writer, to_string, to_vec,
    to_writer,
};

// Re-export DeserializeError for convenience
pub use facet_format::DeserializeError;

/// Deserialize a value from a YAML string into an owned type.
///
/// This is the recommended default for most use cases. The input does not need
/// to outlive the result, making it suitable for deserializing from temporary
/// buffers (e.g., HTTP request bodies, config files read into a String).
///
/// Types containing `&str` fields cannot be deserialized with this function;
/// use `String` or `Cow<str>` instead. For zero-copy deserialization into
/// borrowed types, use [`from_str_borrowed`].
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_format_yaml::from_str;
///
/// #[derive(Facet, Debug, PartialEq)]
/// struct Config {
///     name: String,
///     port: u16,
/// }
///
/// let yaml = "name: myapp\nport: 8080";
/// let config: Config = from_str(yaml).unwrap();
/// assert_eq!(config.name, "myapp");
/// assert_eq!(config.port, 8080);
/// ```
pub fn from_str<T>(input: &str) -> Result<T, DeserializeError<YamlError>>
where
    T: facet_core::Facet<'static>,
{
    use facet_format::FormatDeserializer;
    let parser = YamlParser::new(input).map_err(DeserializeError::Parser)?;
    let mut de = FormatDeserializer::new_owned(parser);
    de.deserialize_root()
}

/// Deserialize a value from a YAML string, allowing zero-copy borrowing.
///
/// This variant requires the input to outlive the result (`'input: 'facet`),
/// enabling zero-copy deserialization of string fields as `&str` or `Cow<str>`.
///
/// Use this when you need maximum performance and can guarantee the input
/// buffer outlives the deserialized value. For most use cases, prefer
/// [`from_str`] which doesn't have lifetime requirements.
///
/// Note: Due to YAML's streaming parser model, string values are typically
/// owned. Zero-copy borrowing works best with `Cow<str>` fields.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_format_yaml::from_str_borrowed;
///
/// #[derive(Facet, Debug, PartialEq)]
/// struct Config {
///     name: String,
///     port: u16,
/// }
///
/// let yaml = "name: myapp\nport: 8080";
/// let config: Config = from_str_borrowed(yaml).unwrap();
/// assert_eq!(config.name, "myapp");
/// assert_eq!(config.port, 8080);
/// ```
pub fn from_str_borrowed<'input, 'facet, T>(
    input: &'input str,
) -> Result<T, DeserializeError<YamlError>>
where
    T: facet_core::Facet<'facet>,
    'input: 'facet,
{
    use facet_format::FormatDeserializer;
    let parser = YamlParser::new(input).map_err(DeserializeError::Parser)?;
    let mut de = FormatDeserializer::new(parser);
    de.deserialize_root()
}
