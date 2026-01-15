//! XDR serializer implementing FormatSerializer.
//!
//! XDR (External Data Representation) serialization follows RFC 4506:
//! - Big-endian byte order
//! - All values are padded to 4-byte boundaries
//! - Fixed-size integers (4 bytes for i32/u32, 8 bytes for i64/u64)
//! - Strings and variable-length data are length-prefixed with 4-byte padding

extern crate alloc;

use alloc::vec::Vec;

use crate::error::XdrSerializeError;
use facet_core::ScalarType;
use facet_format::{FormatSerializer, ScalarValue, SerializeError};
use facet_reflect::Peek;

/// XDR serializer.
pub struct XdrSerializer {
    out: Vec<u8>,
    /// Stack tracking nested structures for validation
    stack: Vec<ContainerState>,
}

#[derive(Debug)]
enum ContainerState {
    Struct,
    Seq { count: usize, count_pos: usize },
}

impl XdrSerializer {
    /// Create a new XDR serializer.
    pub const fn new() -> Self {
        Self {
            out: Vec::new(),
            stack: Vec::new(),
        }
    }

    /// Consume the serializer and return the output bytes.
    pub fn finish(mut self) -> Vec<u8> {
        // Patch up any remaining sequence counts
        while let Some(state) = self.stack.pop() {
            if let ContainerState::Seq { count, count_pos } = state {
                self.patch_seq_count(count_pos, count);
            }
        }
        self.out
    }

    /// Write padding bytes to align to 4-byte boundary.
    fn write_padding(&mut self, data_len: usize) {
        let pad = (4 - (data_len % 4)) % 4;
        for _ in 0..pad {
            self.out.push(0);
        }
    }

    /// Write a u32 in big-endian.
    fn write_u32(&mut self, val: u32) {
        self.out.extend_from_slice(&val.to_be_bytes());
    }

    /// Write a u64 in big-endian.
    fn write_u64(&mut self, val: u64) {
        self.out.extend_from_slice(&val.to_be_bytes());
    }

    /// Write an i32 in big-endian.
    fn write_i32(&mut self, val: i32) {
        self.out.extend_from_slice(&val.to_be_bytes());
    }

    /// Write an i64 in big-endian.
    fn write_i64(&mut self, val: i64) {
        self.out.extend_from_slice(&val.to_be_bytes());
    }

    /// Write an f32 in big-endian.
    fn write_f32(&mut self, val: f32) {
        self.out.extend_from_slice(&val.to_be_bytes());
    }

    /// Write an f64 in big-endian.
    fn write_f64(&mut self, val: f64) {
        self.out.extend_from_slice(&val.to_be_bytes());
    }

    /// Write a boolean (4 bytes: 0 or 1).
    fn write_bool(&mut self, val: bool) {
        self.write_u32(if val { 1 } else { 0 });
    }

    /// Write a string (length-prefixed with padding).
    fn write_string(&mut self, s: &str) {
        let bytes = s.as_bytes();
        self.write_u32(bytes.len() as u32);
        self.out.extend_from_slice(bytes);
        self.write_padding(bytes.len());
    }

    /// Write opaque bytes (length-prefixed with padding).
    fn write_opaque(&mut self, bytes: &[u8]) {
        self.write_u32(bytes.len() as u32);
        self.out.extend_from_slice(bytes);
        self.write_padding(bytes.len());
    }

    /// Begin a sequence, writing a placeholder count and returning its position.
    fn begin_seq(&mut self) -> usize {
        let count_pos = self.out.len();
        self.write_u32(0); // Placeholder
        count_pos
    }

    /// Patch the sequence count at the given position.
    fn patch_seq_count(&mut self, count_pos: usize, count: usize) {
        let count_bytes = (count as u32).to_be_bytes();
        self.out[count_pos..count_pos + 4].copy_from_slice(&count_bytes);
    }
}

impl Default for XdrSerializer {
    fn default() -> Self {
        Self::new()
    }
}

impl FormatSerializer for XdrSerializer {
    type Error = XdrSerializeError;

    fn begin_struct(&mut self) -> Result<(), Self::Error> {
        // XDR structs don't have headers - fields are just serialized in order
        self.stack.push(ContainerState::Struct);
        Ok(())
    }

    fn begin_option_some(&mut self) -> Result<(), Self::Error> {
        // XDR encodes Option as discriminated union: 1 for Some
        self.write_u32(1);
        Ok(())
    }

    fn serialize_none(&mut self) -> Result<(), Self::Error> {
        // XDR encodes Option as discriminated union: 0 for None
        self.write_u32(0);
        Ok(())
    }

    fn field_key(&mut self, _key: &str) -> Result<(), Self::Error> {
        // XDR is positional - field names are not serialized
        Ok(())
    }

    fn end_struct(&mut self) -> Result<(), Self::Error> {
        match self.stack.pop() {
            Some(ContainerState::Struct) => Ok(()),
            _ => Err(XdrSerializeError::new(
                "end_struct called without matching begin_struct",
            )),
        }
    }

    fn begin_seq(&mut self) -> Result<(), Self::Error> {
        let count_pos = self.begin_seq();
        self.stack.push(ContainerState::Seq {
            count: 0,
            count_pos,
        });
        Ok(())
    }

    fn end_seq(&mut self) -> Result<(), Self::Error> {
        match self.stack.pop() {
            Some(ContainerState::Seq { count, count_pos }) => {
                self.patch_seq_count(count_pos, count);
                Ok(())
            }
            _ => Err(XdrSerializeError::new(
                "end_seq called without matching begin_seq",
            )),
        }
    }

    fn scalar(&mut self, scalar: ScalarValue<'_>) -> Result<(), Self::Error> {
        // Increment count in current sequence
        if let Some(ContainerState::Seq { count, .. }) = self.stack.last_mut() {
            *count += 1;
        }

        match scalar {
            ScalarValue::Null | ScalarValue::Unit => {
                // XDR doesn't have a null type, but Option<T> uses discriminant 0 for None
                // This shouldn't normally be called directly for null
                self.write_u32(0);
            }
            ScalarValue::Bool(v) => self.write_bool(v),
            ScalarValue::Char(c) => {
                let mut buf = [0u8; 4];
                self.write_string(c.encode_utf8(&mut buf));
            }
            ScalarValue::U64(n) => {
                // Determine size based on value
                if n <= u32::MAX as u64 {
                    self.write_u32(n as u32);
                } else {
                    self.write_u64(n);
                }
            }
            ScalarValue::I64(n) => {
                // Determine size based on value
                if n >= i32::MIN as i64 && n <= i32::MAX as i64 {
                    self.write_i32(n as i32);
                } else {
                    self.write_i64(n);
                }
            }
            ScalarValue::U128(_n) => {
                return Err(XdrSerializeError::new("XDR does not support u128"));
            }
            ScalarValue::I128(_n) => {
                return Err(XdrSerializeError::new("XDR does not support i128"));
            }
            ScalarValue::F64(n) => {
                // XDR always uses IEEE 754 floats
                // Check if it fits in f32 without precision loss
                let as_f32 = n as f32;
                if as_f32 as f64 == n && n.is_finite() {
                    self.write_f32(as_f32);
                } else {
                    self.write_f64(n);
                }
            }
            ScalarValue::Str(s) => self.write_string(&s),
            ScalarValue::Bytes(bytes) => self.write_opaque(&bytes),
        }
        Ok(())
    }

    fn typed_scalar(
        &mut self,
        scalar_type: ScalarType,
        value: Peek<'_, '_>,
    ) -> Result<(), Self::Error> {
        // Increment count in current sequence
        if let Some(ContainerState::Seq { count, .. }) = self.stack.last_mut() {
            *count += 1;
        }

        match scalar_type {
            ScalarType::Unit => {
                // Unit has no representation in XDR
            }
            ScalarType::Bool => {
                let v = *value.get::<bool>().unwrap();
                self.write_bool(v);
            }
            ScalarType::Char => {
                // XDR encodes char as u32
                let c = *value.get::<char>().unwrap();
                self.write_u32(c as u32);
            }
            ScalarType::Str | ScalarType::String | ScalarType::CowStr => {
                if let Some(s) = value.as_str() {
                    self.write_string(s);
                }
            }
            ScalarType::F32 => {
                let v = *value.get::<f32>().unwrap();
                self.write_f32(v);
            }
            ScalarType::F64 => {
                let v = *value.get::<f64>().unwrap();
                self.write_f64(v);
            }
            ScalarType::U8 => {
                let v = *value.get::<u8>().unwrap();
                self.write_u32(v as u32);
            }
            ScalarType::U16 => {
                let v = *value.get::<u16>().unwrap();
                self.write_u32(v as u32);
            }
            ScalarType::U32 => {
                let v = *value.get::<u32>().unwrap();
                self.write_u32(v);
            }
            ScalarType::U64 => {
                let v = *value.get::<u64>().unwrap();
                self.write_u64(v);
            }
            ScalarType::U128 => {
                return Err(XdrSerializeError::new("XDR does not support u128"));
            }
            ScalarType::USize => {
                let v = *value.get::<usize>().unwrap();
                self.write_u64(v as u64);
            }
            ScalarType::I8 => {
                let v = *value.get::<i8>().unwrap();
                self.write_i32(v as i32);
            }
            ScalarType::I16 => {
                let v = *value.get::<i16>().unwrap();
                self.write_i32(v as i32);
            }
            ScalarType::I32 => {
                let v = *value.get::<i32>().unwrap();
                self.write_i32(v);
            }
            ScalarType::I64 => {
                let v = *value.get::<i64>().unwrap();
                self.write_i64(v);
            }
            ScalarType::I128 => {
                return Err(XdrSerializeError::new("XDR does not support i128"));
            }
            ScalarType::ISize => {
                let v = *value.get::<isize>().unwrap();
                self.write_i64(v as i64);
            }
            _ => {
                // Unknown scalar type - try string representation
                if let Some(s) = value.as_str() {
                    self.write_string(s);
                }
            }
        }
        Ok(())
    }
}

/// Serialize a value to XDR bytes.
pub fn to_vec<'facet, T>(value: &T) -> Result<Vec<u8>, SerializeError<XdrSerializeError>>
where
    T: facet_core::Facet<'facet>,
{
    let mut ser = XdrSerializer::new();
    facet_format::serialize_root(&mut ser, facet_reflect::Peek::new(value))?;
    Ok(ser.finish())
}

/// Serialize a value to XDR bytes using a writer.
pub fn to_writer<'facet, T, W>(writer: &mut W, value: &T) -> Result<(), std::io::Error>
where
    T: facet_core::Facet<'facet>,
    W: std::io::Write,
{
    let bytes = to_vec(value).map_err(|e| std::io::Error::other(e.to_string()))?;
    writer.write_all(&bytes)
}
