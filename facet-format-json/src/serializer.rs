extern crate alloc;

use alloc::{string::String, vec::Vec};

use core::fmt::Write as _;

use facet_core::Facet;
use facet_format::{FormatSerializer, ScalarValue, SerializeError, serialize_root};
use facet_reflect::Peek;

#[derive(Debug)]
pub struct JsonSerializeError {
    msg: &'static str,
}

impl core::fmt::Display for JsonSerializeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.msg)
    }
}

impl std::error::Error for JsonSerializeError {}

#[derive(Debug, Clone, Copy)]
enum Ctx {
    Struct { first: bool },
    Seq { first: bool },
}

/// Minimal JSON serializer for the codex prototype.
pub struct JsonSerializer {
    out: Vec<u8>,
    stack: Vec<Ctx>,
}

impl JsonSerializer {
    pub fn new() -> Self {
        Self {
            out: Vec::new(),
            stack: Vec::new(),
        }
    }

    pub fn finish(self) -> Vec<u8> {
        self.out
    }

    fn before_value(&mut self) -> Result<(), JsonSerializeError> {
        match self.stack.last_mut() {
            Some(Ctx::Seq { first }) => {
                if !*first {
                    self.out.push(b',');
                }
                *first = false;
            }
            Some(Ctx::Struct { .. }) => {
                // struct values are separated by `field_key`
            }
            None => {}
        }
        Ok(())
    }

    fn write_json_string(&mut self, s: &str) {
        self.out.push(b'"');
        for ch in s.chars() {
            match ch {
                '"' => self.out.extend_from_slice(b"\\\""),
                '\\' => self.out.extend_from_slice(b"\\\\"),
                '\n' => self.out.extend_from_slice(b"\\n"),
                '\r' => self.out.extend_from_slice(b"\\r"),
                '\t' => self.out.extend_from_slice(b"\\t"),
                c if c <= '\u{1F}' => {
                    let mut buf = String::new();
                    let _ = write!(&mut buf, "\\u{:04X}", c as u32);
                    self.out.extend_from_slice(buf.as_bytes());
                }
                c => {
                    let mut buf = [0u8; 4];
                    self.out
                        .extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
                }
            }
        }
        self.out.push(b'"');
    }
}

impl Default for JsonSerializer {
    fn default() -> Self {
        Self::new()
    }
}

impl FormatSerializer for JsonSerializer {
    type Error = JsonSerializeError;

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
                self.write_json_string(key);
                self.out.push(b':');
                Ok(())
            }
            _ => Err(JsonSerializeError {
                msg: "field_key called outside of a struct",
            }),
        }
    }

    fn end_struct(&mut self) -> Result<(), Self::Error> {
        match self.stack.pop() {
            Some(Ctx::Struct { .. }) => {
                self.out.push(b'}');
                Ok(())
            }
            _ => Err(JsonSerializeError {
                msg: "end_struct called without matching begin_struct",
            }),
        }
    }

    fn begin_seq(&mut self) -> Result<(), Self::Error> {
        self.before_value()?;
        self.out.push(b'[');
        self.stack.push(Ctx::Seq { first: true });
        Ok(())
    }

    fn end_seq(&mut self) -> Result<(), Self::Error> {
        match self.stack.pop() {
            Some(Ctx::Seq { .. }) => {
                self.out.push(b']');
                Ok(())
            }
            _ => Err(JsonSerializeError {
                msg: "end_seq called without matching begin_seq",
            }),
        }
    }

    fn scalar(&mut self, scalar: ScalarValue<'_>) -> Result<(), Self::Error> {
        self.before_value()?;
        match scalar {
            ScalarValue::Null => self.out.extend_from_slice(b"null"),
            ScalarValue::Bool(v) => {
                if v {
                    self.out.extend_from_slice(b"true")
                } else {
                    self.out.extend_from_slice(b"false")
                }
            }
            ScalarValue::I64(v) => self.out.extend_from_slice(v.to_string().as_bytes()),
            ScalarValue::U64(v) => self.out.extend_from_slice(v.to_string().as_bytes()),
            ScalarValue::I128(v) => self.out.extend_from_slice(v.to_string().as_bytes()),
            ScalarValue::U128(v) => self.out.extend_from_slice(v.to_string().as_bytes()),
            ScalarValue::F64(v) => self.out.extend_from_slice(v.to_string().as_bytes()),
            ScalarValue::Str(s) => self.write_json_string(&s),
            ScalarValue::Bytes(_) => {
                return Err(JsonSerializeError {
                    msg: "bytes serialization unsupported for json",
                });
            }
        }
        Ok(())
    }
}

pub fn to_vec<'facet, T>(value: &'_ T) -> Result<Vec<u8>, SerializeError<JsonSerializeError>>
where
    T: Facet<'facet> + ?Sized,
{
    let mut serializer = JsonSerializer::new();
    serialize_root(&mut serializer, Peek::new(value))?;
    Ok(serializer.finish())
}
