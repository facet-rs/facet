//! Serialize Rust values to Lua table constructor syntax.

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

use facet_core::Facet;
use facet_format::{FormatSerializer, ScalarValue, SerializeError, serialize_root};
use facet_reflect::Peek;

use crate::consts::{self, is_lua_identifier};

/// Options for Lua serialization.
#[derive(Debug, Clone)]
pub struct SerializeOptions {
    /// Whether to pretty-print with indentation (default: false)
    pub pretty: bool,

    /// Indentation string for pretty-printing (default: "    ")
    pub indent: &'static str,
}

impl Default for SerializeOptions {
    fn default() -> Self {
        Self {
            pretty: false,
            indent: "    ",
        }
    }
}

impl SerializeOptions {
    /// Create new default options (compact output).
    pub fn new() -> Self {
        Self::default()
    }

    /// Enable pretty-printing with default indentation.
    pub const fn pretty(mut self) -> Self {
        self.pretty = true;
        self
    }

    /// Set a custom indentation string (implies pretty-printing).
    pub const fn indent(mut self, indent: &'static str) -> Self {
        self.indent = indent;
        self.pretty = true;
        self
    }
}

/// Lua-specific serialization error.
#[derive(Debug)]
pub struct LuaSerializeError {
    msg: &'static str,
}

impl core::fmt::Display for LuaSerializeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.msg)
    }
}

impl std::error::Error for LuaSerializeError {}

#[derive(Debug, Clone, Copy)]
enum Ctx {
    Struct { first: bool },
    Seq { first: bool },
}

/// Lua table serializer with configurable formatting options.
pub struct LuaSerializer {
    out: Vec<u8>,
    stack: Vec<Ctx>,
    options: SerializeOptions,
}

impl LuaSerializer {
    /// Create a new Lua serializer with default (compact) options.
    pub fn new() -> Self {
        Self::with_options(SerializeOptions::default())
    }

    /// Create a new Lua serializer with the given options.
    pub const fn with_options(options: SerializeOptions) -> Self {
        Self {
            out: Vec::new(),
            stack: Vec::new(),
            options,
        }
    }

    /// Consume the serializer and return the output bytes.
    pub fn finish(self) -> Vec<u8> {
        self.out
    }

    /// Current nesting depth (for indentation).
    const fn depth(&self) -> usize {
        self.stack.len()
    }

    /// Write a newline and indentation if in pretty mode.
    fn write_indent(&mut self) {
        if self.options.pretty {
            self.out.push(b'\n');
            for _ in 0..self.depth() {
                self.out.extend_from_slice(self.options.indent.as_bytes());
            }
        }
    }

    fn before_value(&mut self) -> Result<(), LuaSerializeError> {
        match self.stack.last_mut() {
            Some(Ctx::Seq { first }) => {
                if !*first {
                    self.out.push(b',');
                }
                *first = false;
                self.write_indent();
            }
            Some(Ctx::Struct { .. }) => {
                // struct values are separated by `field_key`
            }
            None => {}
        }
        Ok(())
    }

    /// Write a Lua string with proper escaping.
    fn write_lua_string(&mut self, s: &str) {
        self.out.push(b'"');
        for c in s.chars() {
            self.write_lua_escaped_char(c);
        }
        self.out.push(b'"');
    }

    #[inline]
    fn write_lua_escaped_char(&mut self, c: char) {
        match c {
            '"' => self.out.extend_from_slice(b"\\\""),
            '\\' => self.out.extend_from_slice(b"\\\\"),
            '\n' => self.out.extend_from_slice(b"\\n"),
            '\r' => self.out.extend_from_slice(b"\\r"),
            '\t' => self.out.extend_from_slice(b"\\t"),
            '\0' => self.out.extend_from_slice(b"\\0"),
            c if c.is_ascii_control() => {
                // Lua uses \ddd decimal escaping for control chars (0–31)
                let b = c as u8;
                self.out.push(b'\\');
                if b >= 100 {
                    self.out.push(b'0' + b / 100);
                }
                if b >= 10 {
                    self.out.push(b'0' + (b / 10) % 10);
                }
                self.out.push(b'0' + b % 10);
            }
            c if c.is_ascii() => {
                self.out.push(c as u8);
            }
            c => {
                let mut buf = [0u8; 4];
                let len = c.encode_utf8(&mut buf).len();
                self.out.extend_from_slice(&buf[..len]);
            }
        }
    }
}

impl Default for LuaSerializer {
    fn default() -> Self {
        Self::new()
    }
}

impl FormatSerializer for LuaSerializer {
    type Error = LuaSerializeError;

    fn begin_struct(&mut self) -> Result<(), Self::Error> {
        self.before_value()?;
        self.out.push(b'{');
        self.stack.push(Ctx::Struct { first: true });
        Ok(())
    }

    fn field_key(&mut self, key: &str) -> Result<(), Self::Error> {
        match self.stack.last_mut() {
            Some(Ctx::Struct { first }) => {
                if !*first {
                    self.out.push(b',');
                }
                *first = false;
                self.write_indent();
                // Lua field syntax: key = value
                // If key is a valid Lua identifier, use bare name; otherwise use ["key"]
                if is_lua_identifier(key) {
                    self.out.extend_from_slice(key.as_bytes());
                } else {
                    self.out.push(b'[');
                    self.write_lua_string(key);
                    self.out.push(b']');
                }
                if self.options.pretty {
                    self.out.extend_from_slice(b" = ");
                } else {
                    self.out.extend_from_slice(b"=");
                }
                Ok(())
            }
            _ => Err(LuaSerializeError {
                msg: "field_key called outside of a struct",
            }),
        }
    }

    fn end_struct(&mut self) -> Result<(), Self::Error> {
        match self.stack.pop() {
            Some(Ctx::Struct { first }) => {
                if !first {
                    // Add trailing comma in pretty mode
                    if self.options.pretty {
                        self.out.push(b',');
                    }
                    self.write_indent();
                }
                self.out.push(b'}');
                Ok(())
            }
            _ => Err(LuaSerializeError {
                msg: "end_struct called without matching begin_struct",
            }),
        }
    }

    fn begin_seq(&mut self) -> Result<(), Self::Error> {
        self.before_value()?;
        self.out.push(b'{');
        self.stack.push(Ctx::Seq { first: true });
        Ok(())
    }

    fn end_seq(&mut self) -> Result<(), Self::Error> {
        match self.stack.pop() {
            Some(Ctx::Seq { first }) => {
                if !first {
                    if self.options.pretty {
                        self.out.push(b',');
                    }
                    self.write_indent();
                }
                self.out.push(b'}');
                Ok(())
            }
            _ => Err(LuaSerializeError {
                msg: "end_seq called without matching begin_seq",
            }),
        }
    }

    fn scalar(&mut self, scalar: ScalarValue<'_>) -> Result<(), Self::Error> {
        self.before_value()?;
        match scalar {
            ScalarValue::Null | ScalarValue::Unit => self.out.extend_from_slice(consts::KW_NIL),
            ScalarValue::Bool(v) => {
                if v {
                    self.out.extend_from_slice(consts::KW_TRUE)
                } else {
                    self.out.extend_from_slice(consts::KW_FALSE)
                }
            }
            ScalarValue::Char(c) => {
                self.out.push(b'"');
                self.write_lua_escaped_char(c);
                self.out.push(b'"');
            }
            ScalarValue::I64(v) => {
                self.out.extend_from_slice(v.to_string().as_bytes());
            }
            ScalarValue::U64(v) => {
                self.out.extend_from_slice(v.to_string().as_bytes());
            }
            ScalarValue::I128(v) => {
                self.out.extend_from_slice(v.to_string().as_bytes());
            }
            ScalarValue::U128(v) => {
                self.out.extend_from_slice(v.to_string().as_bytes());
            }
            ScalarValue::F64(v) => {
                if v.is_nan() {
                    self.out.extend_from_slice(consts::NAN_LITERAL);
                } else if v.is_infinite() {
                    if v.is_sign_positive() {
                        self.out.extend_from_slice(consts::MATH_HUGE);
                    } else {
                        self.out.push(b'-');
                        self.out.extend_from_slice(consts::MATH_HUGE);
                    }
                } else {
                    self.out.extend_from_slice(v.to_string().as_bytes());
                }
            }
            ScalarValue::Str(s) => self.write_lua_string(&s),
            ScalarValue::Bytes(_) => {
                return Err(LuaSerializeError {
                    msg: "bytes serialization unsupported for lua",
                });
            }
        }
        Ok(())
    }

    fn format_namespace(&self) -> Option<&'static str> {
        Some("lua")
    }
}

/// Serialize a value to a Lua table string.
pub fn to_string<'facet, T>(value: &T) -> Result<String, SerializeError<LuaSerializeError>>
where
    T: Facet<'facet> + ?Sized,
{
    let mut serializer = LuaSerializer::new();
    serialize_root(&mut serializer, Peek::new(value))?;
    let bytes = serializer.finish();
    Ok(String::from_utf8(bytes).expect("Lua output should always be valid UTF-8"))
}

/// Serialize a value to a pretty-printed Lua table string.
pub fn to_string_pretty<'facet, T>(value: &T) -> Result<String, SerializeError<LuaSerializeError>>
where
    T: Facet<'facet> + ?Sized,
{
    let mut serializer = LuaSerializer::with_options(SerializeOptions::default().pretty());
    serialize_root(&mut serializer, Peek::new(value))?;
    let bytes = serializer.finish();
    Ok(String::from_utf8(bytes).expect("Lua output should always be valid UTF-8"))
}

/// Serialize a value to a Lua table string with custom options.
pub fn to_string_with_options<'facet, T>(
    value: &T,
    options: &SerializeOptions,
) -> Result<String, SerializeError<LuaSerializeError>>
where
    T: Facet<'facet> + ?Sized,
{
    let mut serializer = LuaSerializer::with_options(options.clone());
    serialize_root(&mut serializer, Peek::new(value))?;
    let bytes = serializer.finish();
    Ok(String::from_utf8(bytes).expect("Lua output should always be valid UTF-8"))
}

/// Serialize a value to Lua table bytes.
pub fn to_vec<'facet, T>(value: &T) -> Result<Vec<u8>, SerializeError<LuaSerializeError>>
where
    T: Facet<'facet> + ?Sized,
{
    to_vec_with_options(value, &SerializeOptions::default())
}

/// Serialize a value to pretty-printed Lua table bytes.
pub fn to_vec_pretty<'facet, T>(value: &T) -> Result<Vec<u8>, SerializeError<LuaSerializeError>>
where
    T: Facet<'facet> + ?Sized,
{
    to_vec_with_options(value, &SerializeOptions::default().pretty())
}

/// Serialize a value to Lua table bytes with custom options.
pub fn to_vec_with_options<'facet, T>(
    value: &T,
    options: &SerializeOptions,
) -> Result<Vec<u8>, SerializeError<LuaSerializeError>>
where
    T: Facet<'facet> + ?Sized,
{
    let mut serializer = LuaSerializer::with_options(options.clone());
    serialize_root(&mut serializer, Peek::new(value))?;
    Ok(serializer.finish())
}

/// Serialize a value to a `std::io::Write` writer as Lua table syntax.
pub fn to_writer_std<'facet, W, T>(writer: W, value: &T) -> std::io::Result<()>
where
    W: std::io::Write,
    T: Facet<'facet> + ?Sized,
{
    to_writer_std_with_options(writer, value, &SerializeOptions::default())
}

/// Serialize a value to a `std::io::Write` writer as pretty-printed Lua table syntax.
pub fn to_writer_std_pretty<'facet, W, T>(writer: W, value: &T) -> std::io::Result<()>
where
    W: std::io::Write,
    T: Facet<'facet> + ?Sized,
{
    to_writer_std_with_options(writer, value, &SerializeOptions::default().pretty())
}

/// Serialize a value to a `std::io::Write` writer as Lua table syntax with custom options.
pub fn to_writer_std_with_options<'facet, W, T>(
    mut writer: W,
    value: &T,
    options: &SerializeOptions,
) -> std::io::Result<()>
where
    W: std::io::Write,
    T: Facet<'facet> + ?Sized,
{
    let bytes =
        to_vec_with_options(value, options).map_err(|e| std::io::Error::other(e.to_string()))?;
    writer.write_all(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use facet::Facet;

    #[test]
    fn test_simple_struct() {
        #[derive(Facet)]
        struct User {
            name: String,
            age: u32,
        }

        let user = User {
            name: "Alice".to_string(),
            age: 30,
        };
        let lua = to_string(&user).unwrap();
        insta::assert_snapshot!(lua);
    }

    #[test]
    fn test_simple_struct_pretty() {
        #[derive(Facet)]
        struct User {
            name: String,
            age: u32,
        }

        let user = User {
            name: "Alice".to_string(),
            age: 30,
        };
        let lua = to_string_pretty(&user).unwrap();
        insta::assert_snapshot!(lua);
    }

    #[test]
    fn test_nested_struct() {
        #[derive(Facet)]
        struct Inner {
            value: i32,
        }

        #[derive(Facet)]
        struct Outer {
            inner: Inner,
            name: String,
        }

        let outer = Outer {
            inner: Inner { value: 42 },
            name: "test".to_string(),
        };
        let lua = to_string_pretty(&outer).unwrap();
        insta::assert_snapshot!(lua);
    }

    #[test]
    fn test_vec() {
        #[derive(Facet)]
        struct Data {
            items: Vec<String>,
        }

        let data = Data {
            items: vec!["hello".to_string(), "world".to_string()],
        };
        let lua = to_string_pretty(&data).unwrap();
        insta::assert_snapshot!(lua);
    }

    #[test]
    fn test_optional_field() {
        #[derive(Facet)]
        struct Config {
            required: String,
            optional: Option<String>,
        }

        let config = Config {
            required: "yes".to_string(),
            optional: None,
        };
        let lua = to_string_pretty(&config).unwrap();
        insta::assert_snapshot!("optional_none", lua);

        let config_some = Config {
            required: "yes".to_string(),
            optional: Some("value".to_string()),
        };
        let lua_some = to_string_pretty(&config_some).unwrap();
        insta::assert_snapshot!("optional_some", lua_some);
    }

    #[test]
    fn test_bool_and_numbers() {
        #[derive(Facet)]
        struct Mixed {
            flag: bool,
            count: u64,
            score: f64,
        }

        let mixed = Mixed {
            flag: true,
            count: 42,
            score: 3.125,
        };
        let lua = to_string_pretty(&mixed).unwrap();
        insta::assert_snapshot!(lua);
    }

    #[test]
    fn test_string_escaping() {
        #[derive(Facet)]
        struct Text {
            content: String,
        }

        let text = Text {
            content: "hello \"world\"\nnew\tline\\backslash".to_string(),
        };
        let lua = to_string(&text).unwrap();
        insta::assert_snapshot!(lua);
    }

    #[test]
    fn test_hashmap() {
        use std::collections::BTreeMap;

        #[derive(Facet)]
        struct Registry {
            entries: BTreeMap<String, i32>,
        }

        let mut entries = BTreeMap::new();
        entries.insert("alpha".to_string(), 1);
        entries.insert("beta".to_string(), 2);

        let registry = Registry { entries };
        let lua = to_string_pretty(&registry).unwrap();
        insta::assert_snapshot!(lua);
    }

    #[test]
    fn test_enum_unit_variant() {
        #[derive(Facet)]
        #[repr(u8)]
        enum Status {
            Active,
            #[allow(dead_code)]
            Inactive,
        }

        let status = Status::Active;
        let lua = to_string(&status).unwrap();
        insta::assert_snapshot!(lua);
    }

    #[test]
    fn test_compact_output() {
        #[derive(Facet)]
        struct Point {
            x: i32,
            y: i32,
        }

        let point = Point { x: 10, y: 20 };
        let lua = to_string(&point).unwrap();
        insta::assert_snapshot!(lua);
    }
}
