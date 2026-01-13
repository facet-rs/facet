//! Serde support for the Styx configuration language.
//!
//! This crate provides Styx serialization and deserialization using serde.
//!
//! # Deserialization Example
//!
//! ```
//! use serde::Deserialize;
//! use serde_styx::from_str;
//!
//! #[derive(Deserialize, Debug, PartialEq)]
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
//! use serde::Serialize;
//! use serde_styx::to_string;
//!
//! #[derive(Serialize, Debug)]
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

mod de;
mod error;
mod ser;

pub use de::Deserializer;
pub use error::{Error, Result};
pub use ser::Serializer;
pub use styx_format::FormatOptions;

/// Deserialize a value from a Styx string.
///
/// # Example
///
/// ```
/// use serde::Deserialize;
/// use serde_styx::from_str;
///
/// #[derive(Deserialize, Debug, PartialEq)]
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
pub fn from_str<'de, T>(s: &'de str) -> Result<T>
where
    T: serde::de::Deserialize<'de>,
{
    let mut deserializer = Deserializer::new(s);
    T::deserialize(&mut deserializer)
}

/// Serialize a value to a Styx string.
///
/// # Example
///
/// ```
/// use serde::Serialize;
/// use serde_styx::to_string;
///
/// #[derive(Serialize)]
/// struct Config {
///     name: String,
///     port: u16,
/// }
///
/// let config = Config { name: "myapp".into(), port: 8080 };
/// let styx = to_string(&config).unwrap();
/// assert!(styx.contains("name myapp"));
/// assert!(styx.contains("port 8080"));
/// ```
pub fn to_string<T>(value: &T) -> Result<String>
where
    T: serde::ser::Serialize + ?Sized,
{
    to_string_with_options(value, &FormatOptions::default())
}

/// Serialize a value to a compact Styx string (single line, comma separators).
///
/// # Example
///
/// ```
/// use serde::Serialize;
/// use serde_styx::to_string_compact;
///
/// #[derive(Serialize)]
/// struct Point { x: i32, y: i32 }
///
/// let point = Point { x: 10, y: 20 };
/// let styx = to_string_compact(&point).unwrap();
/// assert_eq!(styx, "{x 10, y 20}");
/// ```
pub fn to_string_compact<T>(value: &T) -> Result<String>
where
    T: serde::ser::Serialize + ?Sized,
{
    let options = FormatOptions::default().inline();
    let mut serializer = ser::CompactSerializer::with_options(options);
    value.serialize(&mut serializer)?;
    Ok(serializer.finish())
}

/// Serialize a value to a Styx string with custom options.
pub fn to_string_with_options<T>(value: &T, options: &FormatOptions) -> Result<String>
where
    T: serde::ser::Serialize + ?Sized,
{
    let mut serializer = Serializer::with_options(options.clone());
    value.serialize(&mut serializer)?;
    Ok(serializer.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct Simple {
        name: String,
        value: i32,
    }

    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct Nested {
        inner: Simple,
    }

    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct WithVec {
        items: Vec<i32>,
    }

    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct WithOptional {
        required: String,
        optional: Option<i32>,
    }

    #[test]
    fn test_deserialize_simple_struct() {
        let input = "name hello\nvalue 42";
        let result: Simple = from_str(input).unwrap();
        assert_eq!(result.name, "hello");
        assert_eq!(result.value, 42);
    }

    #[test]
    fn test_deserialize_quoted_string() {
        let input = r#"name "hello world"
value 123"#;
        let result: Simple = from_str(input).unwrap();
        assert_eq!(result.name, "hello world");
        assert_eq!(result.value, 123);
    }

    #[test]
    fn test_deserialize_optional_present() {
        let input = "required hello\noptional 42";
        let result: WithOptional = from_str(input).unwrap();
        assert_eq!(result.required, "hello");
        assert_eq!(result.optional, Some(42));
    }

    #[test]
    fn test_deserialize_optional_absent() {
        let input = "required hello";
        let result: WithOptional = from_str(input).unwrap();
        assert_eq!(result.required, "hello");
        assert_eq!(result.optional, None);
    }

    #[test]
    fn test_deserialize_bool_values() {
        #[derive(Deserialize, Debug, PartialEq)]
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
    fn test_deserialize_vec() {
        let input = "items (1 2 3)";
        let result: WithVec = from_str(input).unwrap();
        assert_eq!(result.items, vec![1, 2, 3]);
    }

    #[test]
    fn test_serialize_simple_struct() {
        let value = Simple {
            name: "hello".into(),
            value: 42,
        };
        let result = to_string(&value).unwrap();
        assert!(result.contains("name hello"));
        assert!(result.contains("value 42"));
    }

    #[test]
    fn test_serialize_compact_struct() {
        let value = Simple {
            name: "hello".into(),
            value: 42,
        };
        let result = to_string_compact(&value).unwrap();
        assert_eq!(result, "{name hello, value 42}");
    }

    #[test]
    fn test_serialize_nested_struct() {
        let value = Nested {
            inner: Simple {
                name: "test".into(),
                value: 123,
            },
        };
        let result = to_string(&value).unwrap();
        assert!(result.contains("inner"));
        assert!(result.contains("{name test, value 123}"));
    }

    #[test]
    fn test_serialize_sequence() {
        let value = WithVec {
            items: vec![1, 2, 3, 4, 5],
        };
        let result = to_string(&value).unwrap();
        assert!(result.contains("items (1 2 3 4 5)"));
    }

    #[test]
    fn test_serialize_quoted_string() {
        let value = Simple {
            name: "hello world".into(),
            value: 42,
        };
        let result = to_string(&value).unwrap();
        assert!(result.contains("name \"hello world\""));
    }

    #[test]
    fn test_serialize_optional_none() {
        let value = WithOptional {
            required: "hello".into(),
            optional: None,
        };
        let result = to_string(&value).unwrap();
        assert!(result.contains("required hello"));
        assert!(result.contains("optional @"));
    }

    #[test]
    fn test_serialize_optional_some() {
        let value = WithOptional {
            required: "hello".into(),
            optional: Some(42),
        };
        let result = to_string(&value).unwrap();
        assert!(result.contains("required hello"));
        assert!(result.contains("optional 42"));
    }

    #[test]
    fn test_roundtrip_simple() {
        let original = Simple {
            name: "myapp".into(),
            value: 8080,
        };

        let serialized = to_string(&original).unwrap();
        let parsed: Simple = from_str(&serialized).unwrap();

        assert_eq!(original, parsed);
    }

    #[test]
    fn test_roundtrip_nested() {
        let original = Nested {
            inner: Simple {
                name: "origin".into(),
                value: 123,
            },
        };

        let serialized = to_string(&original).unwrap();
        let parsed: Nested = from_str(&serialized).unwrap();

        assert_eq!(original, parsed);
    }

    #[test]
    fn test_roundtrip_with_vec() {
        let original = WithVec {
            items: vec![1, 2, 3, 4, 5],
        };

        let serialized = to_string(&original).unwrap();
        let parsed: WithVec = from_str(&serialized).unwrap();

        assert_eq!(original, parsed);
    }

    #[test]
    fn test_roundtrip_quoted_string() {
        let original = Simple {
            name: "hello world with spaces".into(),
            value: 42,
        };

        let serialized = to_string(&original).unwrap();
        let parsed: Simple = from_str(&serialized).unwrap();

        assert_eq!(original, parsed);
    }
}
