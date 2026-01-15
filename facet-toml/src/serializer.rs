extern crate alloc;

use alloc::{string::String, vec::Vec};
use core::fmt::Write;

use facet_format::{FormatSerializer, ScalarValue, SerializeError};

/// Options for TOML serialization.
#[derive(Debug, Clone, Default)]
pub struct SerializeOptions {
    /// Whether to use inline tables for nested structures (default: false)
    pub inline_tables: bool,
}

impl SerializeOptions {
    /// Create new default options.
    pub fn new() -> Self {
        Self::default()
    }

    /// Enable inline tables for nested structures.
    pub const fn inline_tables(mut self) -> Self {
        self.inline_tables = true;
        self
    }
}

#[derive(Debug)]
pub struct TomlSerializeError {
    msg: String,
}

impl core::fmt::Display for TomlSerializeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.msg)
    }
}

impl std::error::Error for TomlSerializeError {}

#[derive(Debug, Clone)]
enum Ctx {
    /// Top-level table (root)
    Root { first: bool },
    /// Nested table (e.g., `[section]`)
    /// Note: Currently unused. Will be used for pretty printing with table headers.
    #[allow(dead_code)]
    Table { first: bool, path: Vec<String> },
    /// Inline table (e.g., `{ key = value }`)
    InlineTable { first: bool },
    /// Array (e.g., `[1, 2, 3]`)
    Array { first: bool },
}

/// TOML serializer with configurable formatting options.
pub struct TomlSerializer {
    out: String,
    stack: Vec<Ctx>,
    /// Formatting options (currently unused, reserved for pretty printing)
    #[allow(dead_code)]
    options: SerializeOptions,
    /// Current table path for dotted keys (reserved for pretty printing)
    #[allow(dead_code)]
    current_path: Vec<String>,
}

impl TomlSerializer {
    /// Create a new TOML serializer with default options.
    pub fn new() -> Self {
        Self::with_options(SerializeOptions::default())
    }

    /// Create a new TOML serializer with the given options.
    pub const fn with_options(options: SerializeOptions) -> Self {
        Self {
            out: String::new(),
            stack: Vec::new(),
            options,
            current_path: Vec::new(),
        }
    }

    /// Consume the serializer and return the output string.
    pub fn finish(self) -> String {
        self.out
    }

    /// Check if we're in an inline context (inline table or array)
    /// Note: Reserved for pretty printing implementation.
    #[allow(dead_code)]
    fn is_inline_context(&self) -> bool {
        matches!(
            self.stack.last(),
            Some(Ctx::InlineTable { .. }) | Some(Ctx::Array { .. })
        )
    }

    /// Write a TOML string value with proper escaping
    fn write_toml_string(&mut self, s: &str) {
        self.out.push('"');
        for c in s.chars() {
            match c {
                '"' => self.out.push_str(r#"\""#),
                '\\' => self.out.push_str(r"\\"),
                '\n' => self.out.push_str(r"\n"),
                '\r' => self.out.push_str(r"\r"),
                '\t' => self.out.push_str(r"\t"),
                c if c.is_control() => {
                    write!(self.out, "\\u{:04X}", c as u32).unwrap();
                }
                c => self.out.push(c),
            }
        }
        self.out.push('"');
    }
}

impl Default for TomlSerializer {
    fn default() -> Self {
        Self::new()
    }
}

impl FormatSerializer for TomlSerializer {
    type Error = TomlSerializeError;

    fn begin_struct(&mut self) -> Result<(), Self::Error> {
        match self.stack.last_mut() {
            None => {
                // Root level - just start tracking as root
                self.stack.push(Ctx::Root { first: true });
                Ok(())
            }
            Some(Ctx::InlineTable { .. }) | Some(Ctx::Array { .. }) => {
                // We're in an inline context - use inline table syntax
                self.out.push_str("{ ");
                self.stack.push(Ctx::InlineTable { first: true });
                Ok(())
            }
            Some(Ctx::Root { .. }) | Some(Ctx::Table { .. }) => {
                // Nested table - will be handled via dotted keys or [table] headers
                // For now, use inline table
                self.out.push_str("{ ");
                self.stack.push(Ctx::InlineTable { first: true });
                Ok(())
            }
        }
    }

    fn field_key(&mut self, key: &str) -> Result<(), Self::Error> {
        match self.stack.last_mut() {
            Some(Ctx::Root { first }) | Some(Ctx::Table { first, .. }) => {
                // Top-level or table field
                if !*first {
                    self.out.push('\n');
                }
                *first = false;

                // Write the key
                if key
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
                {
                    // Simple key
                    self.out.push_str(key);
                } else {
                    // Quoted key
                    self.write_toml_string(key);
                }
                self.out.push_str(" = ");
                Ok(())
            }
            Some(Ctx::InlineTable { first }) => {
                // Inline table field
                if !*first {
                    self.out.push_str(", ");
                }
                *first = false;

                // Write the key
                if key
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
                {
                    self.out.push_str(key);
                } else {
                    self.write_toml_string(key);
                }
                self.out.push_str(" = ");
                Ok(())
            }
            _ => Err(TomlSerializeError {
                msg: "field_key called outside of a struct context".into(),
            }),
        }
    }

    fn end_struct(&mut self) -> Result<(), Self::Error> {
        match self.stack.pop() {
            Some(Ctx::Root { .. }) => {
                // Root table ends - add final newline if there's content
                if !self.out.is_empty() && !self.out.ends_with('\n') {
                    self.out.push('\n');
                }
                Ok(())
            }
            Some(Ctx::InlineTable { .. }) => {
                self.out.push_str(" }");
                Ok(())
            }
            Some(Ctx::Table { .. }) => {
                // Nested table ends
                Ok(())
            }
            _ => Err(TomlSerializeError {
                msg: "end_struct called without matching begin_struct".into(),
            }),
        }
    }

    fn begin_seq(&mut self) -> Result<(), Self::Error> {
        self.out.push('[');
        self.stack.push(Ctx::Array { first: true });
        Ok(())
    }

    fn end_seq(&mut self) -> Result<(), Self::Error> {
        match self.stack.pop() {
            Some(Ctx::Array { .. }) => {
                self.out.push(']');
                Ok(())
            }
            _ => Err(TomlSerializeError {
                msg: "end_seq called without matching begin_seq".into(),
            }),
        }
    }

    fn scalar(&mut self, scalar: ScalarValue<'_>) -> Result<(), Self::Error> {
        // Handle comma separator for arrays
        if let Some(Ctx::Array { first }) = self.stack.last_mut() {
            if !*first {
                self.out.push_str(", ");
            }
            *first = false;
        }

        match scalar {
            ScalarValue::Null | ScalarValue::Unit => {
                // TOML doesn't have null - this is an error
                return Err(TomlSerializeError {
                    msg: "TOML does not support null values".into(),
                });
            }
            ScalarValue::Bool(v) => {
                self.out.push_str(if v { "true" } else { "false" });
            }
            ScalarValue::Char(c) => {
                self.write_toml_string(&c.to_string());
            }
            ScalarValue::I64(v) => {
                #[cfg(feature = "fast")]
                self.out.push_str(itoa::Buffer::new().format(v));
                #[cfg(not(feature = "fast"))]
                write!(self.out, "{}", v).unwrap();
            }
            ScalarValue::U64(v) => {
                #[cfg(feature = "fast")]
                self.out.push_str(itoa::Buffer::new().format(v));
                #[cfg(not(feature = "fast"))]
                write!(self.out, "{}", v).unwrap();
            }
            ScalarValue::I128(v) => {
                #[cfg(feature = "fast")]
                self.out.push_str(itoa::Buffer::new().format(v));
                #[cfg(not(feature = "fast"))]
                write!(self.out, "{}", v).unwrap();
            }
            ScalarValue::U128(v) => {
                #[cfg(feature = "fast")]
                self.out.push_str(itoa::Buffer::new().format(v));
                #[cfg(not(feature = "fast"))]
                write!(self.out, "{}", v).unwrap();
            }
            ScalarValue::F64(v) => {
                if v.is_nan() {
                    self.out.push_str("nan");
                } else if v.is_infinite() {
                    if v.is_sign_positive() {
                        self.out.push_str("inf");
                    } else {
                        self.out.push_str("-inf");
                    }
                } else {
                    #[cfg(feature = "fast")]
                    self.out.push_str(zmij::Buffer::new().format(v));
                    #[cfg(not(feature = "fast"))]
                    write!(self.out, "{}", v).unwrap();
                }
            }
            ScalarValue::Str(s) => {
                self.write_toml_string(&s);
            }
            ScalarValue::Bytes(_) => {
                return Err(TomlSerializeError {
                    msg: "TOML does not natively support byte arrays".into(),
                });
            }
        }
        Ok(())
    }
}

/// Serialize a value to TOML bytes
pub fn to_vec<'facet, T>(value: &T) -> Result<Vec<u8>, SerializeError<TomlSerializeError>>
where
    T: facet_core::Facet<'facet>,
{
    let mut ser = TomlSerializer::new();
    facet_format::serialize_root(&mut ser, facet_reflect::Peek::new(value))?;
    Ok(ser.finish().into_bytes())
}

/// Serialize a value to a TOML string
pub fn to_string<'facet, T>(value: &T) -> Result<String, SerializeError<TomlSerializeError>>
where
    T: facet_core::Facet<'facet>,
{
    let mut ser = TomlSerializer::new();
    facet_format::serialize_root(&mut ser, facet_reflect::Peek::new(value))?;
    Ok(ser.finish())
}

/// Serialize a value to a TOML string with custom options.
pub fn to_string_with_options<'facet, T>(
    value: &T,
    options: &SerializeOptions,
) -> Result<String, SerializeError<TomlSerializeError>>
where
    T: facet_core::Facet<'facet>,
{
    let mut ser = TomlSerializer::with_options(options.clone());
    facet_format::serialize_root(&mut ser, facet_reflect::Peek::new(value))?;
    Ok(ser.finish())
}
