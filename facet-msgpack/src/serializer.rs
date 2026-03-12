//! MsgPack serializer implementing FormatSerializer.

extern crate alloc;

use alloc::{string::String, vec::Vec};
use core::fmt::Write as _;

use facet_format::{FormatSerializer, ScalarValue, SerializeError};

/// MsgPack serializer error.
#[derive(Debug)]
pub struct MsgPackSerializeError {
    message: String,
}

impl core::fmt::Display for MsgPackSerializeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for MsgPackSerializeError {}

/// MsgPack serializer.
pub struct MsgPackSerializer {
    out: Vec<u8>,
    /// Stack tracking whether we're in a struct or sequence, and item counts
    stack: Vec<ContainerState>,
}

#[derive(Debug)]
enum ContainerState {
    Struct { count: usize, count_pos: usize },
    Seq { count: usize, count_pos: usize },
}

impl MsgPackSerializer {
    /// Create a new MsgPack serializer.
    pub const fn new() -> Self {
        Self {
            out: Vec::new(),
            stack: Vec::new(),
        }
    }

    /// Consume the serializer and return the output bytes.
    pub fn finish(mut self) -> Vec<u8> {
        // Patch up any remaining container counts (shouldn't happen with well-formed input)
        while let Some(state) = self.stack.pop() {
            match state {
                ContainerState::Struct { count, count_pos } => {
                    self.patch_map_count(count_pos, count);
                }
                ContainerState::Seq { count, count_pos } => {
                    self.patch_array_count(count_pos, count);
                }
            }
        }
        self.out
    }

    fn write_nil(&mut self) {
        self.out.push(0xc0);
    }

    fn write_bool(&mut self, v: bool) {
        self.out.push(if v { 0xc3 } else { 0xc2 });
    }

    fn write_u64(&mut self, n: u64) {
        match n {
            0..=127 => {
                // positive fixint
                self.out.push(n as u8);
            }
            128..=255 => {
                // uint8
                self.out.push(0xcc);
                self.out.push(n as u8);
            }
            256..=65535 => {
                // uint16
                self.out.push(0xcd);
                self.out.extend_from_slice(&(n as u16).to_be_bytes());
            }
            65536..=4294967295 => {
                // uint32
                self.out.push(0xce);
                self.out.extend_from_slice(&(n as u32).to_be_bytes());
            }
            _ => {
                // uint64
                self.out.push(0xcf);
                self.out.extend_from_slice(&n.to_be_bytes());
            }
        }
    }

    fn write_i64(&mut self, n: i64) {
        match n {
            // Positive range - use unsigned encoding
            0..=i64::MAX => self.write_u64(n as u64),
            // Negative fixint (-32 to -1)
            -32..=-1 => {
                self.out.push(n as u8);
            }
            // int8 (-128 to -33)
            -128..=-33 => {
                self.out.push(0xd0);
                self.out.push(n as u8);
            }
            // int16
            -32768..=-129 => {
                self.out.push(0xd1);
                self.out.extend_from_slice(&(n as i16).to_be_bytes());
            }
            // int32
            -2147483648..=-32769 => {
                self.out.push(0xd2);
                self.out.extend_from_slice(&(n as i32).to_be_bytes());
            }
            // int64
            _ => {
                self.out.push(0xd3);
                self.out.extend_from_slice(&n.to_be_bytes());
            }
        }
    }

    fn write_f64(&mut self, n: f64) {
        self.out.push(0xcb);
        self.out.extend_from_slice(&n.to_be_bytes());
    }

    fn write_str(&mut self, s: &str) {
        let bytes = s.as_bytes();
        let len = bytes.len();

        match len {
            0..=31 => {
                // fixstr
                self.out.push(0xa0 | len as u8);
            }
            32..=255 => {
                // str8
                self.out.push(0xd9);
                self.out.push(len as u8);
            }
            256..=65535 => {
                // str16
                self.out.push(0xda);
                self.out.extend_from_slice(&(len as u16).to_be_bytes());
            }
            _ => {
                // str32
                self.out.push(0xdb);
                self.out.extend_from_slice(&(len as u32).to_be_bytes());
            }
        }
        self.out.extend_from_slice(bytes);
    }

    fn write_bin(&mut self, bytes: &[u8]) {
        let len = bytes.len();

        match len {
            0..=255 => {
                // bin8
                self.out.push(0xc4);
                self.out.push(len as u8);
            }
            256..=65535 => {
                // bin16
                self.out.push(0xc5);
                self.out.extend_from_slice(&(len as u16).to_be_bytes());
            }
            _ => {
                // bin32
                self.out.push(0xc6);
                self.out.extend_from_slice(&(len as u32).to_be_bytes());
            }
        }
        self.out.extend_from_slice(bytes);
    }

    /// Write a map header with placeholder count, return position of count.
    fn begin_map(&mut self) -> usize {
        // Use map32 format for flexibility (we'll patch it later)
        // Actually, we'll use fixmap initially and upgrade if needed
        let count_pos = self.out.len();
        self.out.push(0x80); // fixmap with 0 elements (placeholder)
        count_pos
    }

    fn patch_map_count(&mut self, count_pos: usize, count: usize) {
        match count {
            0..=15 => {
                // fixmap - just update the byte
                self.out[count_pos] = 0x80 | count as u8;
            }
            16..=65535 => {
                // Need to convert to map16
                // First, save everything after the placeholder
                let tail = self.out[count_pos + 1..].to_vec();
                self.out.truncate(count_pos);
                self.out.push(0xde); // map16
                self.out.extend_from_slice(&(count as u16).to_be_bytes());
                self.out.extend_from_slice(&tail);
            }
            _ => {
                // Need to convert to map32
                let tail = self.out[count_pos + 1..].to_vec();
                self.out.truncate(count_pos);
                self.out.push(0xdf); // map32
                self.out.extend_from_slice(&(count as u32).to_be_bytes());
                self.out.extend_from_slice(&tail);
            }
        }
    }

    /// Write an array header with placeholder count, return position of count.
    fn begin_array(&mut self) -> usize {
        let count_pos = self.out.len();
        self.out.push(0x90); // fixarray with 0 elements (placeholder)
        count_pos
    }

    fn patch_array_count(&mut self, count_pos: usize, count: usize) {
        match count {
            0..=15 => {
                // fixarray - just update the byte
                self.out[count_pos] = 0x90 | count as u8;
            }
            16..=65535 => {
                // Need to convert to array16
                let tail = self.out[count_pos + 1..].to_vec();
                self.out.truncate(count_pos);
                self.out.push(0xdc); // array16
                self.out.extend_from_slice(&(count as u16).to_be_bytes());
                self.out.extend_from_slice(&tail);
            }
            _ => {
                // Need to convert to array32
                let tail = self.out[count_pos + 1..].to_vec();
                self.out.truncate(count_pos);
                self.out.push(0xdd); // array32
                self.out.extend_from_slice(&(count as u32).to_be_bytes());
                self.out.extend_from_slice(&tail);
            }
        }
    }

    /// Record a value emission in the current sequence, if any.
    fn bump_seq_count_for_value(&mut self) {
        if let Some(ContainerState::Seq { count, .. }) = self.stack.last_mut() {
            *count += 1;
        }
    }
}

impl Default for MsgPackSerializer {
    fn default() -> Self {
        Self::new()
    }
}

impl FormatSerializer for MsgPackSerializer {
    type Error = MsgPackSerializeError;

    fn begin_struct(&mut self) -> Result<(), Self::Error> {
        self.bump_seq_count_for_value();
        let count_pos = self.begin_map();
        self.stack.push(ContainerState::Struct {
            count: 0,
            count_pos,
        });
        Ok(())
    }

    fn field_key(&mut self, key: &str) -> Result<(), Self::Error> {
        // Increment count in current struct
        if let Some(ContainerState::Struct { count, .. }) = self.stack.last_mut() {
            *count += 1;
        }
        self.write_str(key);
        Ok(())
    }

    fn end_struct(&mut self) -> Result<(), Self::Error> {
        match self.stack.pop() {
            Some(ContainerState::Struct { count, count_pos }) => {
                self.patch_map_count(count_pos, count);
                Ok(())
            }
            _ => Err(MsgPackSerializeError {
                message: "end_struct called without matching begin_struct".into(),
            }),
        }
    }

    fn begin_seq(&mut self) -> Result<(), Self::Error> {
        self.bump_seq_count_for_value();
        let count_pos = self.begin_array();
        self.stack.push(ContainerState::Seq {
            count: 0,
            count_pos,
        });
        Ok(())
    }

    fn end_seq(&mut self) -> Result<(), Self::Error> {
        match self.stack.pop() {
            Some(ContainerState::Seq { count, count_pos }) => {
                self.patch_array_count(count_pos, count);
                Ok(())
            }
            _ => Err(MsgPackSerializeError {
                message: "end_seq called without matching begin_seq".into(),
            }),
        }
    }

    fn scalar(&mut self, scalar: ScalarValue<'_>) -> Result<(), Self::Error> {
        self.bump_seq_count_for_value();

        match scalar {
            ScalarValue::Null | ScalarValue::Unit => self.write_nil(),
            ScalarValue::Bool(v) => self.write_bool(v),
            ScalarValue::Char(c) => {
                let mut buf = [0u8; 4];
                self.write_str(c.encode_utf8(&mut buf));
            }
            ScalarValue::U64(n) => self.write_u64(n),
            ScalarValue::I64(n) => self.write_i64(n),
            ScalarValue::U128(n) => {
                // MsgPack doesn't natively support u128, serialize as string
                let mut buf = String::new();
                write!(buf, "{}", n).unwrap();
                self.write_str(&buf);
            }
            ScalarValue::I128(n) => {
                // MsgPack doesn't natively support i128, serialize as string
                let mut buf = String::new();
                write!(buf, "{}", n).unwrap();
                self.write_str(&buf);
            }
            ScalarValue::F64(n) => self.write_f64(n),
            ScalarValue::Str(s) => self.write_str(&s),
            ScalarValue::Bytes(bytes) => self.write_bin(&bytes),
        }
        Ok(())
    }

    fn is_self_describing(&self) -> bool {
        false
    }
}

/// Serialize a value to MsgPack bytes.
pub fn to_vec<'facet, T>(value: &T) -> Result<Vec<u8>, SerializeError<MsgPackSerializeError>>
where
    T: facet_core::Facet<'facet>,
{
    let mut ser = MsgPackSerializer::new();
    facet_format::serialize_root(&mut ser, facet_reflect::Peek::new(value))?;
    Ok(ser.finish())
}

/// Serialize a value to MsgPack bytes using a writer.
pub fn to_writer<'facet, T, W>(writer: &mut W, value: &T) -> Result<(), std::io::Error>
where
    T: facet_core::Facet<'facet>,
    W: std::io::Write,
{
    let bytes = to_vec(value).map_err(|e| std::io::Error::other(e.to_string()))?;
    writer.write_all(&bytes)
}
