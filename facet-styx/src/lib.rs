//! Styx format support for facet.
//!
//! This crate provides Styx deserialization and serialization using the facet
//! reflection system.
//!
//! # Example
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

mod error;
mod parser;

pub use error::{StyxError, StyxErrorKind};
pub use facet_format::DeserializeError;
pub use parser::StyxParser;

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
pub fn from_str<T>(input: &str) -> Result<T, DeserializeError<StyxError>>
where
    T: facet_core::Facet<'static>,
{
    use facet_format::FormatDeserializer;
    let parser = StyxParser::new(input);
    let mut de = FormatDeserializer::new_owned(parser);
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
pub fn from_str_borrowed<'input, 'facet, T>(
    input: &'input str,
) -> Result<T, DeserializeError<StyxError>>
where
    T: facet_core::Facet<'facet>,
    'input: 'facet,
{
    use facet_format::FormatDeserializer;
    let parser = StyxParser::new(input);
    let mut de = FormatDeserializer::new(parser);
    de.deserialize_root()
}

#[cfg(test)]
mod tests {
    use super::*;
    use facet::Facet;

    #[derive(Facet, Debug, PartialEq)]
    struct Simple {
        name: String,
        value: i32,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct WithOptional {
        required: String,
        optional: Option<i32>,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Nested {
        inner: Simple,
    }

    #[test]
    fn test_simple_struct() {
        let input = "name hello\nvalue 42";
        let result: Simple = from_str(input).unwrap();
        assert_eq!(result.name, "hello");
        assert_eq!(result.value, 42);
    }

    #[test]
    fn test_quoted_string() {
        let input = r#"name "hello world"
value 123"#;
        let result: Simple = from_str(input).unwrap();
        assert_eq!(result.name, "hello world");
        assert_eq!(result.value, 123);
    }

    #[test]
    fn test_optional_present() {
        let input = "required hello\noptional 42";
        let result: WithOptional = from_str(input).unwrap();
        assert_eq!(result.required, "hello");
        assert_eq!(result.optional, Some(42));
    }

    #[test]
    fn test_optional_absent() {
        let input = "required hello";
        let result: WithOptional = from_str(input).unwrap();
        assert_eq!(result.required, "hello");
        assert_eq!(result.optional, None);
    }

    #[test]
    fn test_bool_values() {
        #[derive(Facet, Debug, PartialEq)]
        struct Flags {
            enabled: bool,
            debug: bool,
        }

        let input = "enabled true\ndebug false";
        let result: Flags = from_str(input).unwrap();
        assert!(result.enabled);
        assert!(!result.debug);
    }

    #[test]
    fn test_vec() {
        #[derive(Facet, Debug, PartialEq)]
        struct WithVec {
            items: Vec<i32>,
        }

        let input = "items (1 2 3)";
        let result: WithVec = from_str(input).unwrap();
        assert_eq!(result.items, vec![1, 2, 3]);
    }
}
