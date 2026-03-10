//! Generate LuaLS type annotations, serialize values to Lua table syntax,
//! and parse Lua table syntax back into Rust values.
//!
//! This crate provides three complementary features:
//! - **Annotations**: Generate LuaLS `---@class` / `---@alias` annotations from type metadata
//! - **Serialization**: Serialize Rust values to Lua table constructor syntax
//! - **Deserialization**: Parse Lua table constructor syntax back into Rust values
//!
//! # Example
//!
//! ```
//! use facet::Facet;
//! use facet_lua::{to_lua_annotations, to_string_pretty, to_lua_annotated, from_str};
//!
//! #[derive(Facet, Debug, PartialEq)]
//! struct User {
//!     name: String,
//!     age: u32,
//! }
//!
//! // Generate just the annotations
//! let annotations = to_lua_annotations::<User>();
//! assert!(annotations.contains("---@class User"));
//!
//! // Serialize a value
//! let user = User { name: "Alice".into(), age: 30 };
//! let lua = to_string_pretty(&user).unwrap();
//! assert!(lua.contains("name"));
//!
//! // Parse it back
//! let parsed: User = from_str(&lua).unwrap();
//! assert_eq!(parsed.name, "Alice");
//! assert_eq!(parsed.age, 30);
//!
//! // Combined: annotations + typed local variable
//! let annotated = to_lua_annotated(&user, "user").unwrap();
//! assert!(annotated.contains("---@class User"));
//! assert!(annotated.contains("---@type User"));
//! assert!(annotated.contains("local user ="));
//! ```
//!
//! # Lua syntax coverage
//!
//! The parser accepts a broad subset of Lua table syntax:
//! - Table constructors with `,` or `;` separators
//! - Bare identifier keys, bracket keys (`["key"]`), and long-bracket keys
//! - Double-quoted, single-quoted, and long-bracket strings (`[[...]]`, `[=[...]=]`)
//! - All standard string escapes: `\n`, `\t`, `\r`, `\a`, `\b`, `\f`, `\v`, `\\`, `\"`, `\'`
//! - Extended escapes: `\xNN` (hex), `\u{XXXX}` (Unicode), `\z` (whitespace skip), `\ddd` (decimal)
//! - Decimal and hex integer literals (`0xFF`)
//! - Decimal floats with scientific notation (`1.5e3`) and hex floats (`0x1.8p1`)
//! - Special floats: `math.huge`, `-math.huge`, `0/0`
//! - Line comments (`--`) and block comments (`--[[ ]]`, `--[=[ ]=]`)
//! - Function values are skipped (deserialized as `nil`)

// Note: unsafe code is used for lifetime transmutes in from_str_into
// when BORROW=false, mirroring the approach used in facet-json.

extern crate alloc;

mod annotations;
pub(crate) mod consts;
mod parser;
pub(crate) mod scanner;
mod serializer;

pub use annotations::{LuaGenerator, to_lua_annotations};
pub use parser::LuaParser;
pub use serializer::{
    LuaSerializeError, LuaSerializer, SerializeOptions, to_string, to_string_pretty,
    to_string_with_options, to_vec, to_vec_pretty, to_vec_with_options, to_writer_std,
    to_writer_std_pretty, to_writer_std_with_options,
};

use facet_reflect::Partial;

// Re-export DeserializeError for convenience
pub use facet_format::DeserializeError;

/// Convert a UTF-8 validation error into a `DeserializeError` with context.
fn utf8_parse_error(input: &[u8], e: core::str::Utf8Error) -> DeserializeError {
    let mut context = [0u8; 16];
    let context_len = e.valid_up_to().min(16);
    context[..context_len].copy_from_slice(&input[..context_len]);
    facet_format::DeserializeErrorKind::InvalidUtf8 {
        context,
        context_len: context_len as u8,
    }
    .with_span(facet_reflect::Span::new(e.valid_up_to(), 1))
}

/// Deserialize a value from a Lua table string into an owned type.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_lua::from_str;
///
/// #[derive(Facet, Debug, PartialEq)]
/// struct User {
///     name: String,
///     age: u32,
/// }
///
/// let lua = r#"{name = "Alice", age = 30}"#;
/// let user: User = from_str(lua).unwrap();
/// assert_eq!(user.name, "Alice");
/// assert_eq!(user.age, 30);
/// ```
pub fn from_str<T>(input: &str) -> Result<T, DeserializeError>
where
    T: facet_core::Facet<'static>,
{
    use facet_format::FormatDeserializer;
    let mut parser = LuaParser::new(input);
    let mut de = FormatDeserializer::new_owned(&mut parser);
    de.deserialize_root()
}

/// Deserialize a value from Lua table bytes into an owned type.
///
/// # Errors
///
/// Returns an error if the input is not valid UTF-8 or if deserialization fails.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_lua::from_slice;
///
/// #[derive(Facet, Debug, PartialEq)]
/// struct User {
///     name: String,
///     age: u32,
/// }
///
/// let lua = b"{name = \"Alice\", age = 30}";
/// let user: User = from_slice(lua).unwrap();
/// assert_eq!(user.name, "Alice");
/// assert_eq!(user.age, 30);
/// ```
pub fn from_slice<T>(input: &[u8]) -> Result<T, DeserializeError>
where
    T: facet_core::Facet<'static>,
{
    let s = core::str::from_utf8(input).map_err(|e| utf8_parse_error(input, e))?;
    from_str(s)
}

/// Deserialize a value from a Lua table string, allowing zero-copy borrowing.
///
/// This variant requires the input to outlive the result (`'input: 'facet`),
/// enabling zero-copy deserialization of string fields as `&str` or `Cow<str>`.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_lua::from_str_borrowed;
///
/// #[derive(Facet, Debug, PartialEq)]
/// struct Person<'a> {
///     name: &'a str,
///     age: u32,
/// }
///
/// let lua = r#"{name = "Alice", age = 30}"#;
/// let person: Person = from_str_borrowed(lua).unwrap();
/// assert_eq!(person.name, "Alice");
/// assert_eq!(person.age, 30);
/// ```
pub fn from_str_borrowed<'input, 'facet, T>(input: &'input str) -> Result<T, DeserializeError>
where
    T: facet_core::Facet<'facet>,
    'input: 'facet,
{
    use facet_format::FormatDeserializer;
    let mut parser = LuaParser::new(input);
    let mut de = FormatDeserializer::new(&mut parser);
    de.deserialize_root()
}

/// Deserialize a value from Lua table bytes, allowing zero-copy borrowing.
///
/// This variant requires the input to outlive the result (`'input: 'facet`),
/// enabling zero-copy deserialization of string fields as `&str` or `Cow<str>`.
///
/// # Errors
///
/// Returns an error if the input is not valid UTF-8 or if deserialization fails.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_lua::from_slice_borrowed;
///
/// #[derive(Facet, Debug, PartialEq)]
/// struct Person<'a> {
///     name: &'a str,
///     age: u32,
/// }
///
/// let lua = b"{name = \"Alice\", age = 30}";
/// let person: Person = from_slice_borrowed(lua).unwrap();
/// assert_eq!(person.name, "Alice");
/// assert_eq!(person.age, 30);
/// ```
pub fn from_slice_borrowed<'input, 'facet, T>(input: &'input [u8]) -> Result<T, DeserializeError>
where
    T: facet_core::Facet<'facet>,
    'input: 'facet,
{
    let s = core::str::from_utf8(input).map_err(|e| utf8_parse_error(input, e))?;
    from_str_borrowed(s)
}

/// Deserialize Lua from a string into an existing Partial.
///
/// This is useful for reflection-based deserialization where you don't have
/// a concrete type `T` at compile time, only its Shape metadata. The Partial
/// must already be allocated for the target type.
///
/// This version produces owned strings (no borrowing from input).
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_lua::from_str_into;
/// use facet_reflect::Partial;
///
/// #[derive(Facet, Debug, PartialEq)]
/// struct User {
///     name: String,
///     age: u32,
/// }
///
/// let lua = r#"{name = "Alice", age = 30}"#;
/// let partial = Partial::alloc_owned::<User>().unwrap();
/// let partial = from_str_into(lua, partial).unwrap();
/// let value = partial.build().unwrap();
/// let user: User = value.materialize().unwrap();
/// assert_eq!(user.name, "Alice");
/// assert_eq!(user.age, 30);
/// ```
pub fn from_str_into<'facet>(
    input: &str,
    partial: Partial<'facet, false>,
) -> Result<Partial<'facet, false>, DeserializeError> {
    use facet_format::{FormatDeserializer, MetaSource};
    let mut parser = LuaParser::new(input);
    let mut de = FormatDeserializer::new_owned(&mut parser);

    // SAFETY: The deserializer expects Partial<'input, false> where 'input is the
    // lifetime of the Lua bytes. Since BORROW=false, no data is borrowed from the
    // input, so the actual 'facet lifetime of the Partial is independent of 'input.
    // We transmute to satisfy the type system, then transmute back after deserialization.
    #[allow(unsafe_code)]
    let partial: Partial<'_, false> =
        unsafe { core::mem::transmute::<Partial<'facet, false>, Partial<'_, false>>(partial) };

    let partial = de.deserialize_into(partial, MetaSource::FromEvents)?;

    // SAFETY: Same reasoning - no borrowed data since BORROW=false.
    #[allow(unsafe_code)]
    let partial: Partial<'facet, false> =
        unsafe { core::mem::transmute::<Partial<'_, false>, Partial<'facet, false>>(partial) };

    Ok(partial)
}

/// Deserialize Lua from bytes into an existing Partial.
///
/// This is useful for reflection-based deserialization where you don't have
/// a concrete type `T` at compile time, only its Shape metadata.
///
/// This version produces owned strings (no borrowing from input).
///
/// # Errors
///
/// Returns an error if the input is not valid UTF-8 or if deserialization fails.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_lua::from_slice_into;
/// use facet_reflect::Partial;
///
/// #[derive(Facet, Debug, PartialEq)]
/// struct User {
///     name: String,
///     age: u32,
/// }
///
/// let lua = b"{name = \"Alice\", age = 30}";
/// let partial = Partial::alloc_owned::<User>().unwrap();
/// let partial = from_slice_into(lua, partial).unwrap();
/// let value = partial.build().unwrap();
/// let user: User = value.materialize().unwrap();
/// assert_eq!(user.name, "Alice");
/// assert_eq!(user.age, 30);
/// ```
pub fn from_slice_into<'facet>(
    input: &[u8],
    partial: Partial<'facet, false>,
) -> Result<Partial<'facet, false>, DeserializeError> {
    let s = core::str::from_utf8(input).map_err(|e| utf8_parse_error(input, e))?;
    from_str_into(s, partial)
}

/// Deserialize Lua from a string into an existing Partial, allowing zero-copy borrowing.
///
/// This variant requires the input to outlive the Partial's lifetime (`'input: 'facet`),
/// enabling zero-copy deserialization of string fields as `&str` or `Cow<str>`.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_lua::from_str_into_borrowed;
/// use facet_reflect::Partial;
///
/// #[derive(Facet, Debug, PartialEq)]
/// struct Person<'a> {
///     name: &'a str,
///     age: u32,
/// }
///
/// let lua = r#"{name = "Alice", age = 30}"#;
/// let partial = Partial::alloc::<Person>().unwrap();
/// let partial = from_str_into_borrowed(lua, partial).unwrap();
/// let value = partial.build().unwrap();
/// let person: Person = value.materialize().unwrap();
/// assert_eq!(person.name, "Alice");
/// assert_eq!(person.age, 30);
/// ```
pub fn from_str_into_borrowed<'input, 'facet>(
    input: &'input str,
    partial: Partial<'facet, true>,
) -> Result<Partial<'facet, true>, DeserializeError>
where
    'input: 'facet,
{
    use facet_format::{FormatDeserializer, MetaSource};
    let mut parser = LuaParser::new(input);
    let mut de = FormatDeserializer::new(&mut parser);
    de.deserialize_into(partial, MetaSource::FromEvents)
}

/// Deserialize Lua from bytes into an existing Partial, allowing zero-copy borrowing.
///
/// This variant requires the input to outlive the Partial's lifetime (`'input: 'facet`),
/// enabling zero-copy deserialization of string fields as `&str` or `Cow<str>`.
///
/// # Errors
///
/// Returns an error if the input is not valid UTF-8 or if deserialization fails.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_lua::from_slice_into_borrowed;
/// use facet_reflect::Partial;
///
/// #[derive(Facet, Debug, PartialEq)]
/// struct Person<'a> {
///     name: &'a str,
///     age: u32,
/// }
///
/// let lua = b"{name = \"Alice\", age = 30}";
/// let partial = Partial::alloc::<Person>().unwrap();
/// let partial = from_slice_into_borrowed(lua, partial).unwrap();
/// let value = partial.build().unwrap();
/// let person: Person = value.materialize().unwrap();
/// assert_eq!(person.name, "Alice");
/// assert_eq!(person.age, 30);
/// ```
pub fn from_slice_into_borrowed<'input, 'facet>(
    input: &'input [u8],
    partial: Partial<'facet, true>,
) -> Result<Partial<'facet, true>, DeserializeError>
where
    'input: 'facet,
{
    let s = core::str::from_utf8(input).map_err(|e| utf8_parse_error(input, e))?;
    from_str_into_borrowed(s, partial)
}

/// Generate LuaLS annotations followed by a typed local variable with the serialized value.
///
/// Output format:
/// ```lua
/// ---@class MyType
/// ---@field name string
///
/// ---@type MyType
/// local var_name = {
///     name = "value",
/// }
/// ```
pub fn to_lua_annotated<T>(
    value: &T,
    var_name: &str,
) -> Result<String, facet_format::SerializeError<LuaSerializeError>>
where
    T: facet_core::Facet<'static>,
{
    let annotations = to_lua_annotations::<T>();
    let serialized = to_string_pretty(value)?;

    let type_identifier = T::SHAPE.type_identifier;
    let mut output = String::new();
    output.push_str("---@meta\n\n");
    output.push_str(&annotations);
    output.push('\n');
    use core::fmt::Write;
    writeln!(output, "---@type {}", type_identifier).unwrap();
    writeln!(output, "local {} = {}", var_name, serialized).unwrap();
    Ok(output)
}

#[cfg(test)]
mod parser_tests {
    use super::*;
    use facet::Facet;
    use std::collections::BTreeMap;

    #[derive(Facet, Debug, PartialEq)]
    struct User {
        name: String,
        age: u32,
    }

    #[test]
    fn test_roundtrip_simple_struct() {
        let user = User {
            name: "Alice".to_string(),
            age: 30,
        };
        let lua = to_string(&user).unwrap();
        let parsed: User = from_str(&lua).unwrap();
        assert_eq!(parsed.name, "Alice");
        assert_eq!(parsed.age, 30);
    }

    #[test]
    fn test_roundtrip_pretty_struct() {
        let user = User {
            name: "Alice".to_string(),
            age: 30,
        };
        let lua = to_string_pretty(&user).unwrap();
        let parsed: User = from_str(&lua).unwrap();
        assert_eq!(parsed.name, "Alice");
        assert_eq!(parsed.age, 30);
    }

    #[test]
    fn test_parse_struct_from_string() {
        let parsed: User = from_str(r#"{name = "Alice", age = 30}"#).unwrap();
        assert_eq!(parsed.name, "Alice");
        assert_eq!(parsed.age, 30);
    }

    #[test]
    fn test_parse_vec_string() {
        let parsed: Vec<String> = from_str(r#"{"a", "b", "c"}"#).unwrap();
        assert_eq!(parsed, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_parse_vec_int() {
        let parsed: Vec<i32> = from_str(r#"{1, 2, 3}"#).unwrap();
        assert_eq!(parsed, vec![1, 2, 3]);
    }

    #[test]
    fn test_roundtrip_nested_struct() {
        #[derive(Facet, Debug, PartialEq)]
        struct Inner {
            value: i32,
        }
        #[derive(Facet, Debug, PartialEq)]
        struct Outer {
            inner: Inner,
            name: String,
        }

        let outer = Outer {
            inner: Inner { value: 42 },
            name: "test".to_string(),
        };
        let lua = to_string_pretty(&outer).unwrap();
        let parsed: Outer = from_str(&lua).unwrap();
        assert_eq!(parsed.inner.value, 42);
        assert_eq!(parsed.name, "test");
    }

    #[test]
    fn test_roundtrip_optional_none() {
        #[derive(Facet, Debug, PartialEq)]
        struct Config {
            required: String,
            optional: Option<String>,
        }

        let config = Config {
            required: "yes".to_string(),
            optional: None,
        };
        let lua = to_string_pretty(&config).unwrap();
        let parsed: Config = from_str(&lua).unwrap();
        assert_eq!(parsed.required, "yes");
        assert_eq!(parsed.optional, None);
    }

    #[test]
    fn test_roundtrip_optional_some() {
        #[derive(Facet, Debug, PartialEq)]
        struct Config {
            required: String,
            optional: Option<String>,
        }

        let config = Config {
            required: "yes".to_string(),
            optional: Some("value".to_string()),
        };
        let lua = to_string_pretty(&config).unwrap();
        let parsed: Config = from_str(&lua).unwrap();
        assert_eq!(parsed.required, "yes");
        assert_eq!(parsed.optional, Some("value".to_string()));
    }

    #[test]
    fn test_parse_special_floats() {
        #[derive(Facet, Debug)]
        struct Floats {
            pos_inf: f64,
            neg_inf: f64,
            nan: f64,
        }

        let lua = r#"{pos_inf = math.huge, neg_inf = -math.huge, nan = 0/0}"#;
        let parsed: Floats = from_str(lua).unwrap();
        assert!(parsed.pos_inf.is_infinite() && parsed.pos_inf.is_sign_positive());
        assert!(parsed.neg_inf.is_infinite() && parsed.neg_inf.is_sign_negative());
        assert!(parsed.nan.is_nan());
    }

    #[test]
    fn test_roundtrip_special_floats() {
        #[derive(Facet, Debug)]
        struct Floats {
            pos_inf: f64,
            neg_inf: f64,
            nan: f64,
        }

        let floats = Floats {
            pos_inf: f64::INFINITY,
            neg_inf: f64::NEG_INFINITY,
            nan: f64::NAN,
        };
        let lua = to_string(&floats).unwrap();
        let parsed: Floats = from_str(&lua).unwrap();
        assert!(parsed.pos_inf.is_infinite() && parsed.pos_inf.is_sign_positive());
        assert!(parsed.neg_inf.is_infinite() && parsed.neg_inf.is_sign_negative());
        assert!(parsed.nan.is_nan());
    }

    #[test]
    fn test_roundtrip_btreemap() {
        #[derive(Facet, Debug, PartialEq)]
        struct Registry {
            entries: BTreeMap<String, i32>,
        }

        let mut entries = BTreeMap::new();
        entries.insert("alpha".to_string(), 1);
        entries.insert("beta".to_string(), 2);

        let registry = Registry { entries };
        let lua = to_string_pretty(&registry).unwrap();
        let parsed: Registry = from_str(&lua).unwrap();
        assert_eq!(parsed.entries.get("alpha"), Some(&1));
        assert_eq!(parsed.entries.get("beta"), Some(&2));
    }

    #[test]
    fn test_roundtrip_enum_unit_variant() {
        #[derive(Facet, Debug, PartialEq)]
        #[repr(u8)]
        enum Status {
            Active,
            #[allow(dead_code)]
            Inactive,
        }

        let status = Status::Active;
        let lua = to_string(&status).unwrap();
        let parsed: Status = from_str(&lua).unwrap();
        assert_eq!(parsed, Status::Active);
    }

    #[test]
    fn test_roundtrip_booleans() {
        #[derive(Facet, Debug, PartialEq)]
        struct Flags {
            a: bool,
            b: bool,
        }

        let flags = Flags { a: true, b: false };
        let lua = to_string(&flags).unwrap();
        let parsed: Flags = from_str(&lua).unwrap();
        assert_eq!(parsed, flags);
    }

    #[test]
    fn test_roundtrip_string_escapes() {
        #[derive(Facet, Debug, PartialEq)]
        struct Text {
            content: String,
        }

        let text = Text {
            content: "hello \"world\"\nnew\tline\\backslash".to_string(),
        };
        let lua = to_string(&text).unwrap();
        let parsed: Text = from_str(&lua).unwrap();
        assert_eq!(parsed.content, text.content);
    }

    #[test]
    fn test_roundtrip_struct_with_vec() {
        #[derive(Facet, Debug, PartialEq)]
        struct Data {
            items: Vec<String>,
        }

        let data = Data {
            items: vec!["hello".to_string(), "world".to_string()],
        };
        let lua = to_string_pretty(&data).unwrap();
        let parsed: Data = from_str(&lua).unwrap();
        assert_eq!(parsed.items, data.items);
    }

    #[test]
    fn test_parse_empty_table_as_struct() {
        #[derive(Facet, Debug, PartialEq)]
        struct Empty {}

        let parsed: Empty = from_str("{}").unwrap();
        let _ = parsed;
    }

    #[test]
    fn test_parse_trailing_comma() {
        let parsed: User = from_str(r#"{name = "Alice", age = 30,}"#).unwrap();
        assert_eq!(parsed.name, "Alice");
        assert_eq!(parsed.age, 30);
    }

    #[test]
    fn test_parse_negative_integer() {
        #[derive(Facet, Debug, PartialEq)]
        struct Val {
            x: i32,
        }
        let parsed: Val = from_str("{x = -42}").unwrap();
        assert_eq!(parsed.x, -42);
    }

    #[test]
    fn test_parse_float() {
        #[derive(Facet, Debug, PartialEq)]
        struct Val {
            x: f64,
        }
        let parsed: Val = from_str("{x = 3.125}").unwrap();
        assert!((parsed.x - 3.125).abs() < f64::EPSILON);
    }

    #[test]
    fn test_parse_zero() {
        #[derive(Facet, Debug, PartialEq)]
        struct Val {
            x: i32,
            y: f64,
        }
        let parsed: Val = from_str("{x = 0, y = 0.0}").unwrap();
        assert_eq!(parsed.x, 0);
        assert_eq!(parsed.y, 0.0);
    }

    #[test]
    fn test_parse_zero_in_seq() {
        let parsed: Vec<i32> = from_str("{0, 1, 0}").unwrap();
        assert_eq!(parsed, vec![0, 1, 0]);
    }

    #[test]
    fn test_parse_bracket_key() {
        let parsed: BTreeMap<String, i32> =
            from_str(r#"{["hello world"] = 1, ["foo bar"] = 2}"#).unwrap();
        assert_eq!(parsed.get("hello world"), Some(&1));
        assert_eq!(parsed.get("foo bar"), Some(&2));
    }

    #[test]
    fn test_error_unterminated_string() {
        let result = from_str::<User>(r#"{name = "Alice}"#);
        assert!(result.is_err());
    }

    #[test]
    fn test_error_missing_closing_brace() {
        let result = from_str::<User>(r#"{name = "Alice", age = 30"#);
        assert!(result.is_err());
    }

    #[test]
    fn test_from_slice() {
        let lua = b"{name = \"Alice\", age = 30}";
        let user: User = from_slice(lua).unwrap();
        assert_eq!(user.name, "Alice");
        assert_eq!(user.age, 30);
    }

    #[test]
    fn test_from_slice_invalid_utf8() {
        let bad = b"{name = \xff}";
        let result = from_slice::<User>(bad);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_single_quoted_string() {
        let parsed: User = from_str("{name = 'Alice', age = 30}").unwrap();
        assert_eq!(parsed.name, "Alice");
        assert_eq!(parsed.age, 30);
    }

    #[test]
    fn test_parse_single_quoted_with_escape() {
        #[derive(Facet, Debug, PartialEq)]
        struct Text {
            content: String,
        }
        let parsed: Text = from_str(r#"{content = 'it\'s a test'}"#).unwrap();
        assert_eq!(parsed.content, "it's a test");
    }

    #[test]
    fn test_parse_with_line_comment() {
        let lua = r#"{
            -- this is a comment
            name = "Alice", -- inline comment
            age = 30
        }"#;
        let parsed: User = from_str(lua).unwrap();
        assert_eq!(parsed.name, "Alice");
        assert_eq!(parsed.age, 30);
    }

    #[test]
    fn test_parse_with_block_comment() {
        let lua = r#"{
            --[[ this is a
            block comment ]]
            name = "Alice",
            age = 30
        }"#;
        let parsed: User = from_str(lua).unwrap();
        assert_eq!(parsed.name, "Alice");
        assert_eq!(parsed.age, 30);
    }

    #[test]
    fn test_parse_long_string() {
        #[derive(Facet, Debug, PartialEq)]
        struct Text {
            content: String,
        }
        let parsed: Text = from_str("{content = [[hello world]]}").unwrap();
        assert_eq!(parsed.content, "hello world");
    }

    #[test]
    fn test_parse_long_string_with_level() {
        #[derive(Facet, Debug, PartialEq)]
        struct Text {
            content: String,
        }
        let parsed: Text = from_str("{content = [==[contains ]] and [[ brackets]==]}").unwrap();
        assert_eq!(parsed.content, "contains ]] and [[ brackets");
    }

    #[test]
    fn test_parse_long_string_strips_leading_newline() {
        #[derive(Facet, Debug, PartialEq)]
        struct Text {
            content: String,
        }
        let parsed: Text = from_str("{content = [[\nhello]]}").unwrap();
        assert_eq!(parsed.content, "hello");
    }

    #[test]
    fn test_parse_long_string_in_sequence() {
        let parsed: Vec<String> = from_str("{[[first]], [[second]]}").unwrap();
        assert_eq!(parsed, vec!["first", "second"]);
    }

    #[test]
    fn test_parse_leveled_block_comment() {
        let lua = r#"{
            --[=[ this contains ]] and [[ ]=]
            name = "Alice",
            age = 30
        }"#;
        let parsed: User = from_str(lua).unwrap();
        assert_eq!(parsed.name, "Alice");
        assert_eq!(parsed.age, 30);
    }

    #[test]
    fn test_parse_escape_bell_backspace_formfeed_vtab() {
        #[derive(Facet, Debug, PartialEq)]
        struct Text {
            content: String,
        }
        let parsed: Text = from_str(r#"{content = "\a\b\f\v"}"#).unwrap();
        assert_eq!(parsed.content, "\x07\x08\x0C\x0B");
    }

    #[test]
    fn test_parse_unicode_escape() {
        #[derive(Facet, Debug, PartialEq)]
        struct Text {
            content: String,
        }
        let parsed: Text = from_str(r#"{content = "\u{48}\u{65}\u{6C}\u{6C}\u{6F}"}"#).unwrap();
        assert_eq!(parsed.content, "Hello");
    }

    #[test]
    fn test_parse_unicode_escape_emoji() {
        #[derive(Facet, Debug, PartialEq)]
        struct Text {
            content: String,
        }
        let parsed: Text = from_str(r#"{content = "\u{1F980}"}"#).unwrap();
        assert_eq!(parsed.content, "🦀");
    }

    #[test]
    fn test_parse_z_escape() {
        #[derive(Facet, Debug, PartialEq)]
        struct Text {
            content: String,
        }
        let lua = "{content = \"hello\\z\n    world\"}";
        let parsed: Text = from_str(lua).unwrap();
        assert_eq!(parsed.content, "helloworld");
    }

    #[test]
    fn test_skip_function_value() {
        #[derive(Facet, Debug, PartialEq)]
        struct Config {
            name: String,
            callback: Option<String>,
        }
        let lua = r#"{
            name = "test",
            callback = function(x) return x + 1 end
        }"#;
        let parsed: Config = from_str(lua).unwrap();
        assert_eq!(parsed.name, "test");
        assert_eq!(parsed.callback, None);
    }

    #[test]
    fn test_skip_nested_function() {
        #[derive(Facet, Debug, PartialEq)]
        struct Config {
            name: String,
            handler: Option<String>,
        }
        let lua = r#"{
            name = "test",
            handler = function()
                local inner = function() return 1 end
                if true then
                    for i = 1, 10 do
                        print(i)
                    end
                end
                return inner()
            end
        }"#;
        let parsed: Config = from_str(lua).unwrap();
        assert_eq!(parsed.name, "test");
        assert_eq!(parsed.handler, None);
    }

    #[test]
    fn test_parse_semicolon_separator_struct() {
        let parsed: User = from_str(r#"{name = "Alice"; age = 30}"#).unwrap();
        assert_eq!(parsed.name, "Alice");
        assert_eq!(parsed.age, 30);
    }

    #[test]
    fn test_parse_semicolon_separator_seq() {
        let parsed: Vec<i32> = from_str("{1; 2; 3}").unwrap();
        assert_eq!(parsed, vec![1, 2, 3]);
    }

    #[test]
    fn test_parse_mixed_separators() {
        let parsed: User = from_str(r#"{name = "Alice", age = 30; }"#).unwrap();
        assert_eq!(parsed.name, "Alice");
        assert_eq!(parsed.age, 30);
    }

    #[test]
    fn test_parse_hex_integer() {
        #[derive(Facet, Debug, PartialEq)]
        struct Val {
            x: u32,
        }
        let parsed: Val = from_str("{x = 0xFF}").unwrap();
        assert_eq!(parsed.x, 255);
    }

    #[test]
    fn test_parse_hex_integer_uppercase() {
        #[derive(Facet, Debug, PartialEq)]
        struct Val {
            x: u32,
        }
        let parsed: Val = from_str("{x = 0X1A}").unwrap();
        assert_eq!(parsed.x, 26);
    }

    #[test]
    fn test_parse_negative_hex() {
        #[derive(Facet, Debug, PartialEq)]
        struct Val {
            x: i32,
        }
        let parsed: Val = from_str("{x = -0x10}").unwrap();
        assert_eq!(parsed.x, -16);
    }

    #[test]
    fn test_parse_negative_hex_i64_min() {
        // -0x8000000000000000 == i64::MIN — tests the overflow edge case
        #[derive(Facet, Debug, PartialEq)]
        struct Val {
            x: i64,
        }
        let parsed: Val = from_str("{x = -0x8000000000000000}").unwrap();
        assert_eq!(parsed.x, i64::MIN);
    }

    #[test]
    fn test_parse_negative_hex_large() {
        // -0xFFFFFFFFFFFFFFFF is too large for i64, should promote to i128
        #[derive(Facet, Debug, PartialEq)]
        struct Val {
            x: i128,
        }
        let parsed: Val = from_str("{x = -0xFFFFFFFFFFFFFFFF}").unwrap();
        assert_eq!(parsed.x, -(0xFFFFFFFFFFFFFFFFi128));
    }

    #[test]
    fn test_parse_hex_float() {
        #[derive(Facet, Debug, PartialEq)]
        struct Val {
            x: f64,
        }
        // 0x1.8p1 = 1.5 * 2^1 = 3.0
        let parsed: Val = from_str("{x = 0x1.8p1}").unwrap();
        assert_eq!(parsed.x, 3.0);
    }

    #[test]
    fn test_parse_hex_float_no_frac() {
        #[derive(Facet, Debug, PartialEq)]
        struct Val {
            x: f64,
        }
        // 0xAp2 = 10 * 2^2 = 40.0
        let parsed: Val = from_str("{x = 0xAp2}").unwrap();
        assert_eq!(parsed.x, 40.0);
    }

    #[test]
    fn test_parse_hex_float_negative_exp() {
        #[derive(Facet, Debug, PartialEq)]
        struct Val {
            x: f64,
        }
        // 0x1p-2 = 1 * 2^-2 = 0.25
        let parsed: Val = from_str("{x = 0x1p-2}").unwrap();
        assert_eq!(parsed.x, 0.25);
    }

    #[test]
    fn test_parse_negative_hex_float() {
        #[derive(Facet, Debug, PartialEq)]
        struct Val {
            x: f64,
        }
        // -0x1.8p1 = -3.0
        let parsed: Val = from_str("{x = -0x1.8p1}").unwrap();
        assert_eq!(parsed.x, -3.0);
    }

    #[test]
    fn test_parse_hex_float_dot_only() {
        #[derive(Facet, Debug, PartialEq)]
        struct Val {
            x: f64,
        }
        // 0x1.8 = 1.5 (no exponent, implicit p0)
        let parsed: Val = from_str("{x = 0x1.8}").unwrap();
        assert_eq!(parsed.x, 1.5);
    }

    #[test]
    fn test_parse_hex_escape_in_string() {
        #[derive(Facet, Debug, PartialEq)]
        struct Text {
            content: String,
        }
        let parsed: Text = from_str(r#"{content = "\x48\x65\x6C\x6Co"}"#).unwrap();
        assert_eq!(parsed.content, "Hello");
    }

    #[test]
    fn test_parse_hex_escape_control_char() {
        #[derive(Facet, Debug, PartialEq)]
        struct Text {
            content: String,
        }
        let parsed: Text = from_str(r#"{content = "tab\x09here"}"#).unwrap();
        assert_eq!(parsed.content, "tab\there");
    }

    #[test]
    fn test_skip_function_with_strings() {
        #[derive(Facet, Debug, PartialEq)]
        struct Config {
            name: String,
            init: Option<String>,
        }
        let lua = r#"{
            name = "test",
            init = function()
                local s = "contains end keyword"
                return s
            end
        }"#;
        let parsed: Config = from_str(lua).unwrap();
        assert_eq!(parsed.name, "test");
        assert_eq!(parsed.init, None);
    }

    #[test]
    fn test_skip_function_among_other_fields() {
        #[derive(Facet, Debug, PartialEq)]
        struct Config {
            name: String,
            on_click: Option<String>,
            enabled: bool,
        }
        let lua = r#"{
            name = "button",
            on_click = function(self) self:activate() end,
            enabled = true
        }"#;
        let parsed: Config = from_str(lua).unwrap();
        assert_eq!(parsed.name, "button");
        assert_eq!(parsed.on_click, None);
        assert_eq!(parsed.enabled, true);
    }

    #[test]
    fn test_skip_function_in_sequence() {
        let lua = r#"{"hello", function() end, "world"}"#;
        let parsed: Vec<Option<String>> = from_str(lua).unwrap();
        assert_eq!(
            parsed,
            vec![Some("hello".to_string()), None, Some("world".to_string())]
        );
    }

    #[test]
    fn test_roundtrip_with_function_field() {
        #[derive(Facet, Debug, PartialEq)]
        struct Config {
            name: String,
            hook: Option<String>,
        }
        // Parse Lua with function → hook becomes None
        let lua = r#"{name = "test", hook = function() return 1 end}"#;
        let parsed: Config = from_str(lua).unwrap();
        assert_eq!(parsed.hook, None);

        // Serialize back → hook is omitted (no function in output)
        let serialized = to_string(&parsed).unwrap();
        assert!(!serialized.contains("function"));
        assert!(serialized.contains("name"));

        // Re-parse the serialized output
        let reparsed: Config = from_str(&serialized).unwrap();
        assert_eq!(reparsed, parsed);
    }

    // ── Number edge cases ──────────────────────────────────────────

    #[test]
    fn test_parse_i64_min() {
        #[derive(Facet, Debug, PartialEq)]
        struct S {
            x: i64,
        }
        let s: S = from_str("{x = -9223372036854775808}").unwrap();
        assert_eq!(s.x, i64::MIN);
    }

    #[test]
    fn test_parse_u64_max() {
        #[derive(Facet, Debug, PartialEq)]
        struct S {
            x: u64,
        }
        let s: S = from_str("{x = 18446744073709551615}").unwrap();
        assert_eq!(s.x, u64::MAX);
    }

    #[test]
    fn test_parse_negative_zero() {
        #[derive(Facet, Debug)]
        struct S {
            x: f64,
        }
        let s: S = from_str("{x = -0.0}").unwrap();
        assert!(s.x.is_sign_negative());
        assert_eq!(s.x, 0.0);
    }

    #[test]
    fn test_parse_scientific_notation() {
        #[derive(Facet, Debug, PartialEq)]
        struct S {
            a: f64,
            b: f64,
            c: f64,
        }
        let s: S = from_str("{a = 1e10, b = 1.5e-3, c = 1E+5}").unwrap();
        assert_eq!(s.a, 1e10);
        assert_eq!(s.b, 1.5e-3);
        assert_eq!(s.c, 1E+5);
    }

    #[test]
    fn test_roundtrip_large_integers() {
        #[derive(Facet, Debug, PartialEq)]
        struct Signed {
            x: i64,
        }
        #[derive(Facet, Debug, PartialEq)]
        struct Unsigned {
            x: u64,
        }
        let orig_signed = Signed { x: i64::MIN };
        let lua = to_string(&orig_signed).unwrap();
        let parsed: Signed = from_str(&lua).unwrap();
        assert_eq!(parsed, orig_signed);

        let orig_unsigned = Unsigned { x: u64::MAX };
        let lua = to_string(&orig_unsigned).unwrap();
        let parsed: Unsigned = from_str(&lua).unwrap();
        assert_eq!(parsed, orig_unsigned);
    }

    #[test]
    fn test_roundtrip_scientific_notation_float() {
        #[derive(Facet, Debug, PartialEq)]
        struct S {
            x: f64,
        }
        let orig = S { x: 1e10_f64 };
        let lua = to_string(&orig).unwrap();
        let parsed: S = from_str(&lua).unwrap();
        assert_eq!(parsed, orig);
    }

    // ── String edge cases ──────────────────────────────────────────

    #[test]
    fn test_parse_empty_string() {
        #[derive(Facet, Debug, PartialEq)]
        struct S {
            name: String,
        }
        let s: S = from_str(r#"{name = ""}"#).unwrap();
        assert_eq!(s.name, "");
    }

    #[test]
    fn test_roundtrip_empty_string() {
        #[derive(Facet, Debug, PartialEq)]
        struct S {
            name: String,
        }
        let orig = S {
            name: String::new(),
        };
        let lua = to_string(&orig).unwrap();
        let parsed: S = from_str(&lua).unwrap();
        assert_eq!(parsed, orig);
    }

    #[test]
    fn test_parse_decimal_escape() {
        #[derive(Facet, Debug, PartialEq)]
        struct S {
            content: String,
        }
        let s: S = from_str(r#"{content = "\065\066\067"}"#).unwrap();
        assert_eq!(s.content, "ABC");
    }

    #[test]
    fn test_roundtrip_null_byte() {
        #[derive(Facet, Debug, PartialEq)]
        struct S {
            content: String,
        }
        let orig = S {
            content: String::from("hello\0world"),
        };
        let lua = to_string(&orig).unwrap();
        let parsed: S = from_str(&lua).unwrap();
        assert_eq!(parsed, orig);
    }

    #[test]
    fn test_roundtrip_control_chars() {
        #[derive(Facet, Debug, PartialEq)]
        struct S {
            content: String,
        }
        let orig = S {
            content: String::from("\x01\x02\x1F"),
        };
        let lua = to_string(&orig).unwrap();
        let parsed: S = from_str(&lua).unwrap();
        assert_eq!(parsed, orig);
    }

    #[test]
    fn test_parse_single_quoted_bracket_key() {
        let m: BTreeMap<String, i32> = from_str("{['hello world'] = 1}").unwrap();
        assert_eq!(m.get("hello world"), Some(&1));
    }

    #[test]
    fn test_parse_keyword_bracket_keys() {
        let m: BTreeMap<String, i32> =
            from_str(r#"{["end"] = 1, ["function"] = 2, ["for"] = 3}"#).unwrap();
        assert_eq!(m.get("end"), Some(&1));
        assert_eq!(m.get("function"), Some(&2));
        assert_eq!(m.get("for"), Some(&3));
    }

    // ── Structure edge cases ───────────────────────────────────────

    #[test]
    fn test_parse_empty_table_is_struct_not_vec() {
        // In Lua, `{}` is ambiguous — could be empty struct or empty sequence.
        // We parse it as a struct (matching JSON `{}` behavior), so deserializing
        // as Vec will fail.
        let result = from_str::<Vec<i32>>("{}");
        assert!(result.is_err());
    }

    #[test]
    fn test_roundtrip_empty_vec() {
        // Empty Vec serializes as `{}`, which parses back as struct, not sequence.
        // This is a known limitation of Lua's single-table-constructor syntax.
        #[derive(Facet, Debug, PartialEq)]
        struct Data {
            items: Vec<String>,
        }
        let data = Data { items: vec![] };
        let lua = to_string(&data).unwrap();
        assert_eq!(lua, "{items={}}");
        // Roundtrip fails because inner `{}` is parsed as struct, not sequence.
        let result = from_str::<Data>(&lua);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_nested_vec() {
        let parsed: Vec<Vec<i32>> = from_str("{{1, 2}, {3, 4}}").unwrap();
        assert_eq!(parsed, vec![vec![1, 2], vec![3, 4]]);
    }

    #[test]
    fn test_roundtrip_nested_vec() {
        #[derive(Facet, Debug, PartialEq)]
        struct Data {
            matrix: Vec<Vec<i32>>,
        }
        let data = Data {
            matrix: vec![vec![1, 2], vec![3, 4]],
        };
        let lua = to_string_pretty(&data).unwrap();
        let parsed: Data = from_str(&lua).unwrap();
        assert_eq!(parsed, data);
    }

    #[test]
    fn test_parse_deeply_nested_struct() {
        #[derive(Facet, Debug, PartialEq)]
        struct Inner {
            value: i32,
        }
        #[derive(Facet, Debug, PartialEq)]
        struct Middle {
            inner: Inner,
            label: String,
        }
        #[derive(Facet, Debug, PartialEq)]
        struct Outer {
            middle: Middle,
            active: bool,
        }
        let lua = r#"{
            middle = {
                inner = {value = 99},
                label = "hello"
            },
            active = true
        }"#;
        let parsed: Outer = from_str(lua).unwrap();
        assert_eq!(parsed.middle.inner.value, 99);
        assert_eq!(parsed.middle.label, "hello");
        assert!(parsed.active);
    }

    #[test]
    fn test_roundtrip_deeply_nested() {
        #[derive(Facet, Debug, PartialEq)]
        struct Inner {
            value: i32,
        }
        #[derive(Facet, Debug, PartialEq)]
        struct Middle {
            inner: Inner,
            label: String,
        }
        #[derive(Facet, Debug, PartialEq)]
        struct Outer {
            middle: Middle,
            active: bool,
        }
        let outer = Outer {
            middle: Middle {
                inner: Inner { value: 99 },
                label: "hello".to_string(),
            },
            active: true,
        };
        let lua = to_string_pretty(&outer).unwrap();
        let parsed: Outer = from_str(&lua).unwrap();
        assert_eq!(parsed, outer);
    }

    #[test]
    fn test_roundtrip_keyword_map_keys() {
        #[derive(Facet, Debug, PartialEq)]
        struct Data {
            counts: BTreeMap<String, i32>,
        }
        let mut counts = BTreeMap::new();
        counts.insert("end".to_string(), 1);
        counts.insert("function".to_string(), 2);
        counts.insert("for".to_string(), 3);
        counts.insert("while".to_string(), 4);
        counts.insert("nil".to_string(), 5);
        counts.insert("true".to_string(), 6);
        let data = Data { counts };
        let lua = to_string_pretty(&data).unwrap();
        // Keywords must use bracket notation
        assert!(lua.contains(r#"["end"]"#));
        assert!(lua.contains(r#"["function"]"#));
        assert!(lua.contains(r#"["for"]"#));
        assert!(lua.contains(r#"["while"]"#));
        assert!(lua.contains(r#"["nil"]"#));
        assert!(lua.contains(r#"["true"]"#));
        let parsed: Data = from_str(&lua).unwrap();
        assert_eq!(parsed, data);
    }

    #[test]
    fn test_roundtrip_char_field() {
        #[derive(Facet, Debug, PartialEq)]
        struct CharVal {
            c: char,
        }
        let val = CharVal { c: 'A' };
        let lua = to_string(&val).unwrap();
        let parsed: CharVal = from_str(&lua).unwrap();
        assert_eq!(parsed, val);
    }

    #[test]
    fn test_roundtrip_tuple_struct() {
        #[derive(Facet, Debug, PartialEq)]
        struct Pair(i32, i32);

        let pair = Pair(10, 20);
        let lua = to_string(&pair).unwrap();
        let parsed: Pair = from_str(&lua).unwrap();
        assert_eq!(parsed, pair);
    }

    // ── Comment edge cases ─────────────────────────────────────────

    #[test]
    fn test_parse_comment_between_key_and_equals() {
        let lua = r#"{name --[[comment]]= "Alice", age = 30}"#;
        let parsed: User = from_str(lua).unwrap();
        assert_eq!(parsed.name, "Alice");
        assert_eq!(parsed.age, 30);
    }

    #[test]
    fn test_parse_comment_between_equals_and_value() {
        let lua = r#"{name = --[[x]] "Alice", age = 30}"#;
        let parsed: User = from_str(lua).unwrap();
        assert_eq!(parsed.name, "Alice");
        assert_eq!(parsed.age, 30);
    }

    #[test]
    fn test_parse_comment_before_closing_brace() {
        let lua = "{name = \"Alice\", age = 30 --trailing\n}";
        let parsed: User = from_str(lua).unwrap();
        assert_eq!(parsed.name, "Alice");
        assert_eq!(parsed.age, 30);
    }

    #[test]
    fn test_parse_comment_only_lines() {
        let lua = r#"{
            -- comment before first field
            name = "Alice",
            -- comment between fields
            -- another comment
            age = 30
            -- comment after last field
        }"#;
        let parsed: User = from_str(lua).unwrap();
        assert_eq!(parsed.name, "Alice");
        assert_eq!(parsed.age, 30);
    }

    // ── Error cases ────────────────────────────────────────────────

    #[test]
    fn test_error_empty_input() {
        let result = from_str::<User>("");
        assert!(result.is_err());
    }

    #[test]
    fn test_error_whitespace_only() {
        let result = from_str::<User>("   \n\t  ");
        assert!(result.is_err());
    }

    #[test]
    fn test_error_missing_equals() {
        let result = from_str::<User>(r#"{name "Alice"}"#);
        assert!(result.is_err());
    }

    // ── Root scalar parsing ────────────────────────────────────────

    #[test]
    fn test_parse_bare_integer_root() {
        let parsed: u32 = from_str("42").unwrap();
        assert_eq!(parsed, 42);
    }

    #[test]
    fn test_parse_bare_string_root() {
        let parsed: String = from_str(r#""hello""#).unwrap();
        assert_eq!(parsed, "hello");
    }

    #[test]
    fn test_parse_bare_bool_root() {
        let parsed: bool = from_str("true").unwrap();
        assert!(parsed);
    }

    #[test]
    fn test_parse_bare_nil_root() {
        let parsed: Option<String> = from_str("nil").unwrap();
        assert_eq!(parsed, None);
    }

    // ── Roundtrip edge cases ───────────────────────────────────────

    #[test]
    fn test_roundtrip_option_option() {
        #[derive(Facet, Debug, PartialEq)]
        struct Wrap {
            x: Option<Option<i32>>,
        }
        // Some(Some(42)) roundtrips cleanly
        let w = Wrap { x: Some(Some(42)) };
        let lua = to_string(&w).unwrap();
        let parsed: Wrap = from_str(&lua).unwrap();
        assert_eq!(parsed, w);

        // None serializes as nil and roundtrips to None
        let w_none = Wrap { x: None };
        let lua_none = to_string(&w_none).unwrap();
        let parsed_none: Wrap = from_str(&lua_none).unwrap();
        assert_eq!(parsed_none.x, None);

        // Some(None) also serializes as nil, so it roundtrips to None (lossy)
        let w_some_none = Wrap { x: Some(None) };
        let lua_some_none = to_string(&w_some_none).unwrap();
        let parsed_some_none: Wrap = from_str(&lua_some_none).unwrap();
        assert_eq!(parsed_some_none.x, None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use facet::Facet;

    #[test]
    fn test_to_lua_annotated() {
        #[derive(Facet)]
        struct User {
            name: String,
            age: u32,
        }

        let user = User {
            name: "DT".to_string(),
            age: 25,
        };
        let lua = to_lua_annotated(&user, "user").unwrap();
        insta::assert_snapshot!(lua);
    }

    #[test]
    fn test_to_lua_annotated_nested() {
        #[derive(Facet)]
        struct Address {
            street: String,
            city: String,
        }

        #[derive(Facet)]
        struct Person {
            name: String,
            address: Address,
        }

        let person = Person {
            name: "Alice".to_string(),
            address: Address {
                street: "123 Main St".to_string(),
                city: "Springfield".to_string(),
            },
        };
        let lua = to_lua_annotated(&person, "person").unwrap();
        insta::assert_snapshot!(lua);
    }

    #[test]
    fn test_to_lua_annotated_with_option() {
        #[derive(Facet)]
        struct Config {
            host: String,
            port: Option<u16>,
        }

        let config = Config {
            host: "localhost".to_string(),
            port: Some(8080),
        };
        let lua = to_lua_annotated(&config, "config").unwrap();
        insta::assert_snapshot!(lua);
    }

    #[test]
    fn test_to_lua_annotated_with_vec() {
        #[derive(Facet)]
        struct Settings {
            name: String,
            tags: Vec<String>,
        }

        let settings = Settings {
            name: "test".to_string(),
            tags: vec!["a".to_string(), "b".to_string()],
        };
        let lua = to_lua_annotated(&settings, "settings").unwrap();
        insta::assert_snapshot!(lua);
    }
}
