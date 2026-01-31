#![doc = include_str!("../README.md")]
//! Styx format support for facet.
//!
//! This crate provides Styx deserialization and serialization using the facet
//! reflection system.
//!
//! # Deserialization Example
//!
//! ```
//! use facet::Facet;
//! use facet_styx::from_str;
//!
//! #[derive(Facet, Debug, PartialEq)]
//! struct Config {
//!     name: String,
//!     port: u16,
//! }
//!
//! let styx = "name myapp\nport 8080";
//! let config: Config = from_str(styx).unwrap();
//! assert_eq!(config.name, "myapp");
//! assert_eq!(config.port, 8080);
//! ```
//!
//! # Serialization Example
//!
//! ```
//! use facet::Facet;
//! use facet_styx::to_string;
//!
//! #[derive(Facet, Debug)]
//! struct Config {
//!     name: String,
//!     port: u16,
//! }
//!
//! let config = Config { name: "myapp".into(), port: 8080 };
//! let styx = to_string(&config).unwrap();
//! assert!(styx.contains("name myapp"));
//! assert!(styx.contains("port 8080"));
//! ```

mod error;
#[cfg(feature = "figue")]
mod figue_format;
#[cfg(test)]
mod idempotency_test;
#[cfg(test)]
mod other_variant_test;
mod parser;
mod schema_error;
mod schema_gen;
mod schema_meta;
mod schema_types;
mod schema_validate;
mod serializer;
#[cfg(test)]
mod tag_events_test;
#[cfg(test)]
mod test_utils;
mod tracing_macros;
#[cfg(test)]
mod value_expr_test;

pub use error::RenderError;
pub use facet_format::DeserializeError;
pub use facet_format::SerializeError;
#[cfg(feature = "figue")]
pub use figue_format::StyxFormat;
pub use parser::StyxParser;
pub use schema_error::{ValidationError, ValidationErrorKind, ValidationResult, ValidationWarning};
pub use schema_gen::{GenerateSchema, schema_file_from_type, schema_from_type};
pub use schema_meta::META_SCHEMA_SOURCE;
pub use schema_types::*;
pub use schema_validate::{Validator, validate, validate_as};
pub use serializer::{
    SerializeOptions, StyxSerializeError, StyxSerializer, peek_to_string, peek_to_string_expr,
    peek_to_string_with_options, to_string, to_string_compact, to_string_with_options,
};

/// Deserialize a value from a Styx string into an owned type.
///
/// This is the recommended default for most use cases.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_styx::from_str;
///
/// #[derive(Facet, Debug, PartialEq)]
/// struct Person {
///     name: String,
///     age: u32,
/// }
///
/// let styx = "name Alice\nage 30";
/// let person: Person = from_str(styx).unwrap();
/// assert_eq!(person.name, "Alice");
/// assert_eq!(person.age, 30);
/// ```
pub fn from_str<T>(input: &str) -> Result<T, DeserializeError>
where
    T: facet_core::Facet<'static>,
{
    use facet_format::FormatDeserializer;
    let mut parser = StyxParser::new(input);
    let mut de = FormatDeserializer::new_owned(&mut parser);
    de.deserialize_root()
}

/// Deserialize a value from a Styx string, allowing zero-copy borrowing.
///
/// This variant requires the input to outlive the result, enabling
/// zero-copy deserialization of string fields as `&str` or `Cow<str>`.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_styx::from_str_borrowed;
///
/// #[derive(Facet, Debug, PartialEq)]
/// struct Person<'a> {
///     name: &'a str,
///     age: u32,
/// }
///
/// let styx = "name Alice\nage 30";
/// let person: Person = from_str_borrowed(styx).unwrap();
/// assert_eq!(person.name, "Alice");
/// assert_eq!(person.age, 30);
/// ```
pub fn from_str_borrowed<'input, 'facet, T>(input: &'input str) -> Result<T, DeserializeError>
where
    T: facet_core::Facet<'facet>,
    'input: 'facet,
{
    use facet_format::FormatDeserializer;
    let mut parser = StyxParser::new(input);
    let mut de = FormatDeserializer::new(&mut parser);
    de.deserialize_root()
}

/// Deserialize a single value from a Styx expression string.
///
/// Unlike `from_str`, this parses a single value rather than an implicit root object.
/// Use this for parsing embedded values like default values in schemas.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_styx::from_str_expr;
///
/// // Parse an object expression (note the braces)
/// #[derive(Facet, Debug, PartialEq)]
/// struct Point { x: i32, y: i32 }
///
/// let point: Point = from_str_expr("{x 10, y 20}").unwrap();
/// assert_eq!(point.x, 10);
/// assert_eq!(point.y, 20);
///
/// // Parse a scalar expression
/// let num: i32 = from_str_expr("42").unwrap();
/// assert_eq!(num, 42);
/// ```
pub fn from_str_expr<T>(input: &str) -> Result<T, DeserializeError>
where
    T: facet_core::Facet<'static>,
{
    use facet_format::FormatDeserializer;
    let mut parser = StyxParser::new_expr(input);
    let mut de = FormatDeserializer::new_owned(&mut parser);
    de.deserialize_root()
}

#[cfg(test)]
mod tests;
