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
mod tracing_macros;
#[cfg(test)]
mod value_expr_test;

pub use error::{RenderError, StyxError, StyxErrorKind};
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

    #[test]
    fn test_schema_directive_skipped_in_config_value() {
        // @schema at top level should be skipped even when parsing into ConfigValue
        use figue::ConfigValue;

        let input = r#"@schema {id crate:dibs@1, cli dibs}

db {
    crate reef-db
}
"#;
        let result: ConfigValue = from_str(input).unwrap();

        // Verify @schema was skipped, only db remains
        if let ConfigValue::Object(obj) = result {
            assert!(
                !obj.value.contains_key("@schema"),
                "Expected '@schema' to be skipped, got: {:?}",
                obj.value.keys().collect::<Vec<_>>()
            );
            assert!(
                obj.value.contains_key("db"),
                "Expected 'db' key, got: {:?}",
                obj.value.keys().collect::<Vec<_>>()
            );
        } else {
            panic!("Expected ConfigValue::Object, got: {:?}", result);
        }
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

    // =========================================================================
    // Documented<T> tests
    // =========================================================================

    #[test]
    fn test_documented_basic() {
        // Documented<T> should have the metadata_container flag
        let shape = <Documented<String>>::SHAPE;
        assert!(shape.is_metadata_container());
    }

    #[test]
    fn test_documented_helper_methods() {
        let doc = Documented::new(42);
        assert_eq!(*doc.value(), 42);
        assert!(doc.doc().is_none());

        let doc = Documented::with_doc(42, vec!["The answer".into()]);
        assert_eq!(*doc.value(), 42);
        assert_eq!(doc.doc(), Some(&["The answer".to_string()][..]));

        let doc = Documented::with_doc_line(42, "The answer");
        assert_eq!(doc.doc(), Some(&["The answer".to_string()][..]));
    }

    #[test]
    fn test_documented_deref() {
        let doc = Documented::new("hello".to_string());
        // Deref should give us access to the inner value
        assert_eq!(doc.len(), 5);
        assert!(doc.starts_with("hel"));
    }

    #[test]
    fn test_documented_from() {
        let doc: Documented<i32> = 42.into();
        assert_eq!(*doc.value(), 42);
        assert!(doc.doc().is_none());
    }

    #[test]
    fn test_documented_map() {
        let doc = Documented::with_doc_line(42, "The answer");
        let mapped = doc.map(|x| x.to_string());
        assert_eq!(*mapped.value(), "42");
        assert_eq!(mapped.doc(), Some(&["The answer".to_string()][..]));
    }

    #[test]
    fn test_unit_field_followed_by_another_field() {
        // When a field has unit value (no explicit value), followed by
        // another field on the next line, both should be parsed correctly.
        use std::collections::HashMap;

        #[derive(Facet, Debug, PartialEq)]
        struct Fields {
            #[facet(flatten)]
            fields: HashMap<String, Option<String>>,
        }

        let input = "foo\nbar baz";
        let result: Fields = from_str(input).unwrap();

        assert_eq!(result.fields.len(), 2);
        assert_eq!(result.fields.get("foo"), Some(&None));
        assert_eq!(result.fields.get("bar"), Some(&Some("baz".to_string())));
    }

    #[test]
    fn test_map_schema_spacing() {
        // When serializing a map with a unit-payload tag key (like @string)
        // followed by another type, there should be proper spacing.
        // i.e., `@map(@string @enum{...})` NOT `@map(@string@enum{...})`
        use crate::schema_types::{Documented, EnumSchema, MapSchema, Schema};
        use std::collections::HashMap;

        let mut enum_variants = HashMap::new();
        enum_variants.insert(Documented::new("a".to_string()), Schema::Unit);
        enum_variants.insert(Documented::new("b".to_string()), Schema::Unit);

        let map_schema = Schema::Map(MapSchema(vec![
            Documented::new(Schema::String(None)), // Key type: @string (no payload)
            Documented::new(Schema::Enum(EnumSchema(enum_variants))), // Value type: @enum{...}
        ]));

        let output = to_string(&map_schema).unwrap();

        // Check that there's a space between @string and @enum
        assert!(
            output.contains("@string @enum"),
            "Expected space between @string and @enum, got: {}",
            output
        );
    }
}
