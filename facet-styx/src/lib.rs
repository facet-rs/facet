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
mod tracing_macros;

pub use error::{StyxError, StyxErrorKind};
pub use facet_format::DeserializeError;
pub use facet_format::SerializeError;
pub use parser::StyxParser;
pub use schema_error::{ValidationError, ValidationErrorKind, ValidationResult, ValidationWarning};
pub use schema_gen::{GenerateSchema, schema_from_type};
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
pub fn from_str_expr<T>(input: &str) -> Result<T, DeserializeError<StyxError>>
where
    T: facet_core::Facet<'static>,
{
    use facet_format::FormatDeserializer;
    let parser = StyxParser::new_expr(input);
    let mut de = FormatDeserializer::new_owned(parser);
    de.deserialize_root()
}

#[cfg(test)]
mod tests {
    use super::*;
    use facet::Facet;
    use facet_testhelpers::test;

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

    #[test]
    fn test_schema_directive_skipped() {
        // @schema directive should be skipped during deserialization
        // See: https://github.com/bearcove/styx/issues/3
        #[derive(Facet, Debug, PartialEq)]
        struct Config {
            name: String,
            port: u16,
        }

        let input = r#"@schema {source crate:test@1, cli test}

name myapp
port 8080"#;
        let result: Config = from_str(input).unwrap();
        assert_eq!(result.name, "myapp");
        assert_eq!(result.port, 8080);
    }

    // =========================================================================
    // Expression mode tests
    // =========================================================================

    #[test]
    fn test_from_str_expr_scalar() {
        let num: i32 = from_str_expr("42").unwrap();
        assert_eq!(num, 42);

        let s: String = from_str_expr("hello").unwrap();
        assert_eq!(s, "hello");

        let b: bool = from_str_expr("true").unwrap();
        assert!(b);
    }

    #[test]
    fn test_from_str_expr_object() {
        #[derive(Facet, Debug, PartialEq)]
        struct Point {
            x: i32,
            y: i32,
        }

        let point: Point = from_str_expr("{x 10, y 20}").unwrap();
        assert_eq!(point.x, 10);
        assert_eq!(point.y, 20);
    }

    #[test]
    fn test_from_str_expr_sequence() {
        let items: Vec<i32> = from_str_expr("(1 2 3)").unwrap();
        assert_eq!(items, vec![1, 2, 3]);
    }

    #[test]
    fn test_expr_roundtrip() {
        // Serialize with expr mode, deserialize with expr mode
        #[derive(Facet, Debug, PartialEq)]
        struct Config {
            name: String,
            port: u16,
        }

        let original = Config {
            name: "test".into(),
            port: 8080,
        };

        // Serialize as expression (with braces)
        let serialized = to_string_compact(&original).unwrap();
        assert!(serialized.starts_with('{'));

        // Parse back as expression
        let parsed: Config = from_str_expr(&serialized).unwrap();
        assert_eq!(original, parsed);
    }
}
