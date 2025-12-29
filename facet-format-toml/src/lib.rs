//! TOML serialization for facet using the new format architecture.
//!
//! This is the successor to `facet-toml`, using the unified `facet-format` traits.
//!
//! # Deserialization
//!
//! ```
//! use facet::Facet;
//! use facet_format_toml::from_str;
//!
//! #[derive(Facet, Debug)]
//! struct Config {
//!     name: String,
//!     port: u16,
//! }
//!
//! let toml = r#"
//! name = "my-app"
//! port = 8080
//! "#;
//!
//! let config: Config = from_str(toml).unwrap();
//! assert_eq!(config.name, "my-app");
//! assert_eq!(config.port, 8080);
//! ```

extern crate alloc;

mod error;
mod parser;
mod serializer;

#[cfg(feature = "axum")]
mod axum;

pub use error::{TomlError, TomlErrorKind};
pub use parser::{TomlParser, TomlProbe};
pub use serializer::{SerializeOptions, TomlSerializeError, TomlSerializer, to_string, to_vec};

// Re-export DeserializeError for convenience
pub use facet_format::DeserializeError;

#[cfg(feature = "axum")]
pub use axum::{Toml, TomlRejection};

/// Deserialize a value from a TOML string into an owned type.
///
/// This is the recommended default for most use cases. The input does not need
/// to outlive the result, making it suitable for deserializing from temporary
/// buffers (e.g., HTTP request bodies).
///
/// Types containing `&str` fields cannot be deserialized with this function;
/// use `String` or `Cow<str>` instead. For zero-copy deserialization into
/// borrowed types, use [`from_str_borrowed`].
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_format_toml::from_str;
///
/// #[derive(Facet, Debug, PartialEq)]
/// struct Config {
///     name: String,
///     port: u16,
/// }
///
/// let toml = r#"
/// name = "my-app"
/// port = 8080
/// "#;
///
/// let config: Config = from_str(toml).unwrap();
/// assert_eq!(config.name, "my-app");
/// assert_eq!(config.port, 8080);
/// ```
pub fn from_str<T>(input: &str) -> Result<T, DeserializeError<TomlError>>
where
    T: facet_core::Facet<'static>,
{
    use facet_format::FormatDeserializer;
    let parser = TomlParser::new(input).map_err(DeserializeError::Parser)?;
    let mut de = FormatDeserializer::new_owned(parser);
    de.deserialize()
}

/// Deserialize a value from TOML bytes into an owned type.
///
/// This is the recommended default for most use cases. The input does not need
/// to outlive the result, making it suitable for deserializing from temporary
/// buffers (e.g., HTTP request bodies).
///
/// # Errors
///
/// Returns an error if the input is not valid UTF-8 or if deserialization fails.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_format_toml::from_slice;
///
/// #[derive(Facet, Debug, PartialEq)]
/// struct Config {
///     name: String,
///     port: u16,
/// }
///
/// let toml = b"name = \"my-app\"\nport = 8080";
/// let config: Config = from_slice(toml).unwrap();
/// assert_eq!(config.name, "my-app");
/// assert_eq!(config.port, 8080);
/// ```
pub fn from_slice<T>(input: &[u8]) -> Result<T, DeserializeError<TomlError>>
where
    T: facet_core::Facet<'static>,
{
    let s = core::str::from_utf8(input).map_err(|e| {
        DeserializeError::Parser(TomlError::without_span(TomlErrorKind::InvalidUtf8(e)))
    })?;
    from_str(s)
}

/// Deserialize a value from a TOML string, allowing zero-copy borrowing.
///
/// This variant requires the input to outlive the result (`'input: 'facet`),
/// enabling zero-copy deserialization of string fields as `&str` or `Cow<str>`.
///
/// Use this when you need maximum performance and can guarantee the input
/// buffer outlives the deserialized value. For most use cases, prefer
/// [`from_str`] which doesn't have lifetime requirements.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_format_toml::from_str_borrowed;
///
/// #[derive(Facet, Debug, PartialEq)]
/// struct Config<'a> {
///     name: &'a str,
///     port: u16,
/// }
///
/// let toml = r#"
/// name = "my-app"
/// port = 8080
/// "#;
///
/// let config: Config = from_str_borrowed(toml).unwrap();
/// assert_eq!(config.name, "my-app");
/// assert_eq!(config.port, 8080);
/// ```
pub fn from_str_borrowed<'input, 'facet, T>(
    input: &'input str,
) -> Result<T, DeserializeError<TomlError>>
where
    T: facet_core::Facet<'facet>,
    'input: 'facet,
{
    use facet_format::FormatDeserializer;
    let parser = TomlParser::new(input).map_err(DeserializeError::Parser)?;
    let mut de = FormatDeserializer::new(parser);
    de.deserialize()
}

/// Deserialize a value from TOML bytes, allowing zero-copy borrowing.
///
/// This variant requires the input to outlive the result (`'input: 'facet`),
/// enabling zero-copy deserialization of string fields as `&str` or `Cow<str>`.
///
/// Use this when you need maximum performance and can guarantee the input
/// buffer outlives the deserialized value. For most use cases, prefer
/// [`from_slice`] which doesn't have lifetime requirements.
///
/// # Errors
///
/// Returns an error if the input is not valid UTF-8 or if deserialization fails.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_format_toml::from_slice_borrowed;
///
/// #[derive(Facet, Debug, PartialEq)]
/// struct Config<'a> {
///     name: &'a str,
///     port: u16,
/// }
///
/// let toml = b"name = \"my-app\"\nport = 8080";
/// let config: Config = from_slice_borrowed(toml).unwrap();
/// assert_eq!(config.name, "my-app");
/// assert_eq!(config.port, 8080);
/// ```
pub fn from_slice_borrowed<'input, 'facet, T>(
    input: &'input [u8],
) -> Result<T, DeserializeError<TomlError>>
where
    T: facet_core::Facet<'facet>,
    'input: 'facet,
{
    let s = core::str::from_utf8(input).map_err(|e| {
        DeserializeError::Parser(TomlError::without_span(TomlErrorKind::InvalidUtf8(e)))
    })?;
    from_str_borrowed(s)
}
