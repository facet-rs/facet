//! Serialize Rust values to Lua table constructor syntax.

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

use facet_core::{Facet, ScalarType};
use facet_format::{FormatSerializer, ScalarValue, SerializeError, serialize_root};
use facet_reflect::Peek;

use crate::consts::{self, is_lua_identifier};

/// How to encode integers outside Lua 5.4's signed 64-bit integer range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BigIntEncoding {
    /// Encode as a decimal string (default).
    ///
    /// Value-preserving for every consumer; deserialization parses the
    /// string back into the integer type.
    #[default]
    String,
    /// Encode as a bare numeral.
    ///
    /// A real Lua 5.4 reader coerces an overflowing decimal numeral to a
    /// float (precision loss above 2^53), but the textual value stays
    /// numeric for consumers that want it that way.
    Bare,
}

/// Options for Lua serialization.
#[derive(Debug, Clone)]
pub struct SerializeOptions {
    /// Whether to pretty-print with indentation (default: false)
    pub pretty: bool,

    /// Indentation string for pretty-printing (default: "    ")
    pub indent: &'static str,

    /// Encoding for integers above Lua's integer range (default: `String`)
    pub big_int_encoding: BigIntEncoding,
}

impl Default for SerializeOptions {
    fn default() -> Self {
        Self {
            pretty: false,
            indent: "    ",
            big_int_encoding: BigIntEncoding::String,
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

    /// Set the encoding for integers above Lua's signed 64-bit range.
    pub const fn big_int_encoding(mut self, encoding: BigIntEncoding) -> Self {
        self.big_int_encoding = encoding;
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

    /// Write the separator and indentation that precede a struct entry's key.
    fn begin_struct_entry(&mut self) -> Result<(), LuaSerializeError> {
        match self.stack.last_mut() {
            Some(Ctx::Struct { first }) => {
                if !*first {
                    self.out.push(b',');
                }
                *first = false;
                self.write_indent();
                Ok(())
            }
            _ => Err(LuaSerializeError {
                msg: "field_key called outside of a struct",
            }),
        }
    }

    /// Write the `=` between a table key and its value.
    fn write_assign(&mut self) {
        if self.options.pretty {
            self.out.extend_from_slice(b" = ");
        } else {
            self.out.extend_from_slice(b"=");
        }
    }

    /// Write an integer that may exceed Lua's signed 64-bit range.
    ///
    /// Lua 5.4 reads a bare decimal literal above `i64::MAX` as a float
    /// (manual §3.1), silently losing precision — so by default such values
    /// are written as decimal strings and parsed back during
    /// deserialization. See [`BigIntEncoding`].
    fn write_lua_integer(&mut self, v: i128) {
        if i64::try_from(v).is_ok() || self.options.big_int_encoding == BigIntEncoding::Bare {
            self.out.extend_from_slice(v.to_string().as_bytes());
        } else {
            self.write_lua_string(&v.to_string());
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
            c if c.is_ascii_control() => {
                // \ddd decimal escape, always zero-padded to 3 digits: a shorter
                // escape like `\1` would absorb a following literal digit
                // (`"\1" .. "5"` serialized adjacently reads back as `\15`).
                let b = c as u8;
                self.out.push(b'\\');
                self.out.push(b'0' + b / 100);
                self.out.push(b'0' + (b / 10) % 10);
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
        self.begin_struct_entry()?;
        // Lua field syntax: key = value
        // If key is a valid Lua identifier, use bare name; otherwise use ["key"]
        if is_lua_identifier(key) {
            self.out.extend_from_slice(key.as_bytes());
        } else {
            self.out.push(b'[');
            self.write_lua_string(key);
            self.out.push(b']');
        }
        self.write_assign();
        Ok(())
    }

    fn serialize_map_key(&mut self, key: Peek<'_, '_>) -> Result<bool, Self::Error> {
        // In Lua, `t[1]` and `t["1"]` are different entries, so integer map
        // keys must be written as integer table keys, not stringified. Keys
        // outside Lua's signed 64-bit integer range fall back to the default
        // string-key encoding.
        let Some(scalar_type) = ScalarType::try_from_shape(key.shape()) else {
            return Ok(false);
        };
        let int_key: i64 = match scalar_type {
            ScalarType::U8 => *key.get::<u8>().unwrap() as i64,
            ScalarType::U16 => *key.get::<u16>().unwrap() as i64,
            ScalarType::U32 => *key.get::<u32>().unwrap() as i64,
            ScalarType::I8 => *key.get::<i8>().unwrap() as i64,
            ScalarType::I16 => *key.get::<i16>().unwrap() as i64,
            ScalarType::I32 => *key.get::<i32>().unwrap() as i64,
            ScalarType::I64 => *key.get::<i64>().unwrap(),
            ScalarType::ISize => *key.get::<isize>().unwrap() as i64,
            ScalarType::U64 => match i64::try_from(*key.get::<u64>().unwrap()) {
                Ok(v) => v,
                Err(_) => return Ok(false),
            },
            ScalarType::USize => match i64::try_from(*key.get::<usize>().unwrap()) {
                Ok(v) => v,
                Err(_) => return Ok(false),
            },
            ScalarType::U128 => match i64::try_from(*key.get::<u128>().unwrap()) {
                Ok(v) => v,
                Err(_) => return Ok(false),
            },
            ScalarType::I128 => match i64::try_from(*key.get::<i128>().unwrap()) {
                Ok(v) => v,
                Err(_) => return Ok(false),
            },
            _ => return Ok(false),
        };
        self.begin_struct_entry()?;
        self.out.push(b'[');
        self.out.extend_from_slice(int_key.to_string().as_bytes());
        self.out.push(b']');
        self.write_assign();
        Ok(true)
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
                self.write_lua_integer(v as i128);
            }
            ScalarValue::I128(v) => {
                self.write_lua_integer(v);
            }
            ScalarValue::U128(v) => {
                if v <= i64::MAX as u128 || self.options.big_int_encoding == BigIntEncoding::Bare {
                    self.out.extend_from_slice(v.to_string().as_bytes());
                } else {
                    self.write_lua_string(&v.to_string());
                }
            }
            ScalarValue::F64(v) => {
                if v.is_nan() {
                    if v.is_sign_negative() {
                        self.out.push(b'-');
                    }
                    self.out.extend_from_slice(consts::NAN_LITERAL);
                } else if v.is_infinite() {
                    if v.is_sign_positive() {
                        self.out.extend_from_slice(consts::MATH_HUGE);
                    } else {
                        self.out.push(b'-');
                        self.out.extend_from_slice(consts::MATH_HUGE);
                    }
                } else {
                    // Debug formatting keeps a `.0` on integral values (`1.0`,
                    // not `1`) so Lua 5.4 reads the number back as a float
                    // subtype, and preserves the sign of negative zero.
                    self.out.extend_from_slice(format!("{v:?}").as_bytes());
                }
            }
            ScalarValue::Str(s) => self.write_lua_string(&s),
            ScalarValue::Bytes(_) => {
                return Err(LuaSerializeError {
                    msg: "bytes serialization unsupported for lua",
                });
            }
            _ => {
                return Err(LuaSerializeError {
                    msg: "unsupported scalar value kind",
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

    // ── Wire-format drift detectors ────────────────────────────────
    //
    // Broad snapshots of the serialized output so any accidental change to
    // the wire format shows up as a reviewable diff.

    #[test]
    fn test_wire_format_kitchen_sink() {
        use std::collections::BTreeMap;

        #[derive(Facet)]
        struct Numbers {
            small_u64: u64,
            big_u64: u64,
            big_u128: u128,
            min_i128: i128,
            whole_float: f64,
            neg_zero: f64,
            pos_inf: f64,
            neg_inf: f64,
            nan: f64,
            neg_nan: f64,
        }

        #[derive(Facet)]
        struct Strings {
            escapes: String,
            control_then_digit: String,
            unicode: String,
            single_char: char,
        }

        #[derive(Facet)]
        struct Sink {
            numbers: Numbers,
            strings: Strings,
            int_keys: BTreeMap<i32, String>,
            numeric_looking_string_keys: BTreeMap<String, i32>,
            keyword_key: BTreeMap<String, bool>,
            nested: Vec<Vec<i32>>,
            empty_list: Vec<i32>,
            none: Option<i32>,
            some: Option<i32>,
        }

        let mut int_keys = BTreeMap::new();
        int_keys.insert(-2, "neg".to_string());
        int_keys.insert(1, "one".to_string());
        let mut string_keys = BTreeMap::new();
        string_keys.insert("1".to_string(), 1);
        let mut keyword_key = BTreeMap::new();
        keyword_key.insert("end".to_string(), true);

        let sink = Sink {
            numbers: Numbers {
                small_u64: 7,
                big_u64: u64::MAX,
                big_u128: u128::MAX,
                min_i128: i128::MIN,
                whole_float: 1.0,
                neg_zero: -0.0,
                pos_inf: f64::INFINITY,
                neg_inf: f64::NEG_INFINITY,
                nan: f64::NAN,
                neg_nan: f64::NAN.copysign(-1.0),
            },
            strings: Strings {
                escapes: "quote:\" backslash:\\ newline:\n tab:\t".to_string(),
                control_then_digit: "\u{01}5".to_string(),
                unicode: "héllo 🦀".to_string(),
                single_char: 'A',
            },
            int_keys,
            numeric_looking_string_keys: string_keys,
            keyword_key,
            nested: vec![vec![1, 2], vec![]],
            empty_list: vec![],
            none: None,
            some: Some(42),
        };
        insta::assert_snapshot!(to_string_pretty(&sink).unwrap());
    }

    #[test]
    fn test_enum_tagging_wire_formats() {
        #[derive(Facet)]
        #[repr(C)]
        #[allow(dead_code)]
        enum External {
            Unit,
            Newtype(u32),
            Tuple(u32, bool),
            Struct { a: u32, b: String },
        }

        #[derive(Facet)]
        #[facet(tag = "type")]
        #[repr(C)]
        #[allow(dead_code)]
        enum Internal {
            Ping,
            Echo { message: String },
        }

        #[derive(Facet)]
        #[facet(tag = "t", content = "c")]
        #[repr(C)]
        #[allow(dead_code)]
        enum Adjacent {
            Stop,
            Move(f64),
            Resize { w: u32, h: u32 },
        }

        #[derive(Facet)]
        #[facet(untagged)]
        #[repr(C)]
        #[allow(dead_code)]
        enum Untagged {
            Text(String),
            Number(f64),
        }

        #[derive(Facet)]
        struct AllModes {
            ext_unit: External,
            ext_newtype: External,
            ext_tuple: External,
            ext_struct: External,
            int_unit: Internal,
            int_struct: Internal,
            adj_unit: Adjacent,
            adj_newtype: Adjacent,
            adj_struct: Adjacent,
            untagged_text: Untagged,
            untagged_number: Untagged,
        }

        let all = AllModes {
            ext_unit: External::Unit,
            ext_newtype: External::Newtype(7),
            ext_tuple: External::Tuple(1, true),
            ext_struct: External::Struct {
                a: 2,
                b: "x".to_string(),
            },
            int_unit: Internal::Ping,
            int_struct: Internal::Echo {
                message: "hi".to_string(),
            },
            adj_unit: Adjacent::Stop,
            adj_newtype: Adjacent::Move(1.5),
            adj_struct: Adjacent::Resize { w: 3, h: 4 },
            untagged_text: Untagged::Text("t".to_string()),
            untagged_number: Untagged::Number(2.5),
        };
        insta::assert_snapshot!(to_string_pretty(&all).unwrap());
    }

    // ── Lua 5.4 wire-format conformance ────────────────────────────

    #[test]
    fn test_u64_within_lua_integer_range_stays_bare() {
        #[derive(Facet)]
        struct S {
            x: u64,
        }
        let lua = to_string(&S { x: i64::MAX as u64 }).unwrap();
        assert_eq!(lua, "{x=9223372036854775807}");
    }

    #[test]
    fn test_u64_above_lua_integer_range_serializes_as_string() {
        // Lua 5.4 integers are signed 64-bit; a bare decimal literal above
        // i64::MAX silently turns into a float in a real Lua reader.
        #[derive(Facet)]
        struct S {
            x: u64,
        }
        let lua = to_string(&S { x: u64::MAX }).unwrap();
        assert_eq!(lua, r#"{x="18446744073709551615"}"#);
    }

    #[test]
    fn test_dynamic_128_bit_scalars_follow_lua_integer_range() {
        // The dynamic-value path hands 128-bit scalars directly to the
        // serializer; values within Lua's integer range stay bare numbers,
        // larger magnitudes become strings.
        let mut s = LuaSerializer::new();
        s.scalar(ScalarValue::I128(5)).unwrap();
        assert_eq!(s.finish(), b"5".to_vec());

        let mut s = LuaSerializer::new();
        s.scalar(ScalarValue::U128(u128::MAX)).unwrap();
        assert_eq!(s.finish(), format!("\"{}\"", u128::MAX).into_bytes());

        let mut s = LuaSerializer::new();
        s.scalar(ScalarValue::I128(i128::MIN)).unwrap();
        assert_eq!(s.finish(), format!("\"{}\"", i128::MIN).into_bytes());
    }

    #[test]
    fn test_negative_nan_keeps_sign() {
        #[derive(Facet)]
        struct S {
            x: f64,
        }
        let lua = to_string(&S {
            x: f64::NAN.copysign(-1.0),
        })
        .unwrap();
        assert_eq!(lua, "{x=-0/0}");
    }

    #[test]
    fn test_integer_map_keys_serialize_as_integer_keys() {
        // In Lua, `t[1]` and `t["1"]` are different entries; integer-keyed
        // maps must produce integer table keys.
        use std::collections::BTreeMap;
        #[derive(Facet)]
        struct S {
            m: BTreeMap<i32, String>,
        }
        let mut m = BTreeMap::new();
        m.insert(-2, "a".to_string());
        m.insert(7, "b".to_string());
        let lua = to_string(&S { m }).unwrap();
        assert_eq!(lua, r#"{m={[-2]="a",[7]="b"}}"#);
    }

    #[test]
    fn test_string_map_keys_stay_string_keys() {
        // A String key that merely looks numeric must stay a string key.
        use std::collections::BTreeMap;
        #[derive(Facet)]
        struct S {
            m: BTreeMap<String, i32>,
        }
        let mut m = BTreeMap::new();
        m.insert("1".to_string(), 5);
        let lua = to_string(&S { m }).unwrap();
        assert_eq!(lua, r#"{m={["1"]=5}}"#);
    }

    #[test]
    fn test_big_int_bare_encoding_option() {
        // Consumers that prefer numeric text over strings (and accept Lua's
        // float coercion) can opt out of the string encoding.
        #[derive(Facet)]
        struct S {
            x: u64,
        }
        let opts = SerializeOptions::new().big_int_encoding(BigIntEncoding::Bare);
        let lua = to_string_with_options(&S { x: u64::MAX }, &opts).unwrap();
        assert_eq!(lua, "{x=18446744073709551615}");
    }

    #[test]
    fn test_big_int_string_encoding_is_default() {
        assert_eq!(
            SerializeOptions::default().big_int_encoding,
            BigIntEncoding::String
        );
    }

    #[test]
    fn test_u64_map_key_above_lua_integer_range_falls_back_to_string() {
        use std::collections::BTreeMap;
        #[derive(Facet)]
        struct S {
            m: BTreeMap<u64, i32>,
        }
        let mut m = BTreeMap::new();
        m.insert(u64::MAX, 1);
        let lua = to_string(&S { m }).unwrap();
        assert_eq!(lua, r#"{m={["18446744073709551615"]=1}}"#);
    }
}
