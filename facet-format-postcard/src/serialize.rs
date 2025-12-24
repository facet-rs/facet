//! Serialization support for postcard format.
//!
//! This module provides serialization functions using custom traversal logic
//! optimized for binary formats. Unlike text formats (JSON, YAML), postcard
//! needs:
//! - No struct delimiters or field names
//! - Variant indices instead of variant names
//! - Type-precise integer encoding (u8 raw, larger varint, signed zigzag)
//! - Length prefixes before sequences

extern crate alloc;

use alloc::borrow::Cow;
use alloc::vec::Vec;

use facet_core::{Def, StructKind, Type, UserType};
use facet_reflect::{HasFields, Peek};

use crate::error::SerializeError;

/// A trait for writing bytes during serialization with error handling.
///
/// This trait enables custom serialization targets that can report errors,
/// such as buffer overflow. It's designed to support use cases like buffer
/// pooling where you need to detect when a fixed-size buffer is too small.
///
/// # Example
///
/// ```
/// use facet_format_postcard::{Writer, SerializeError};
///
/// struct PooledWriter {
///     buf: Vec<u8>,  // In practice, this would be from a buffer pool
///     overflow: Option<Vec<u8>>,
/// }
///
/// impl Writer for PooledWriter {
///     fn write_byte(&mut self, byte: u8) -> Result<(), SerializeError> {
///         // Try pooled buffer first, fall back to Vec on overflow
///         if let Some(ref mut overflow) = self.overflow {
///             overflow.push(byte);
///         } else if self.buf.len() < self.buf.capacity() {
///             self.buf.push(byte);
///         } else {
///             // Overflow - allocate Vec and transfer contents
///             let mut overflow = Vec::new();
///             overflow.extend_from_slice(&self.buf);
///             overflow.push(byte);
///             self.overflow = Some(overflow);
///         }
///         Ok(())
///     }
///
///     fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), SerializeError> {
///         if let Some(ref mut overflow) = self.overflow {
///             overflow.extend_from_slice(bytes);
///         } else if self.buf.len() + bytes.len() <= self.buf.capacity() {
///             self.buf.extend_from_slice(bytes);
///         } else {
///             // Overflow - allocate Vec and transfer contents
///             let mut overflow = Vec::new();
///             overflow.extend_from_slice(&self.buf);
///             overflow.extend_from_slice(bytes);
///             self.overflow = Some(overflow);
///         }
///         Ok(())
///     }
/// }
/// ```
pub trait Writer {
    /// Write a single byte to the writer.
    fn write_byte(&mut self, byte: u8) -> Result<(), SerializeError>;

    /// Write a slice of bytes to the writer.
    fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), SerializeError>;
}

impl Writer for Vec<u8> {
    fn write_byte(&mut self, byte: u8) -> Result<(), SerializeError> {
        self.push(byte);
        Ok(())
    }

    fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), SerializeError> {
        self.extend_from_slice(bytes);
        Ok(())
    }
}

/// Serializes any Facet type to postcard bytes.
///
/// # Example
/// ```
/// use facet::Facet;
/// use facet_format_postcard::to_vec;
///
/// #[derive(Debug, Facet)]
/// struct Point {
///     x: i32,
///     y: i32,
/// }
///
/// let point = Point { x: 10, y: 20 };
/// let bytes = to_vec(&point).unwrap();
/// ```
pub fn to_vec<T>(value: &T) -> Result<Vec<u8>, SerializeError>
where
    T: facet_core::Facet<'static>,
{
    let mut buffer = Vec::new();
    to_writer_fallible(value, &mut buffer)?;
    Ok(buffer)
}

/// Serializes any Facet type to a custom writer implementing the fallible `Writer` trait.
///
/// This function allows external crates to implement custom serialization targets
/// that can report errors, such as buffer overflow. This is useful for use cases
/// like buffer pooling where you need to detect when a fixed-size buffer is too
/// small and transparently fall back to heap allocation.
///
/// # Example
/// ```
/// use facet::Facet;
/// use facet_format_postcard::{to_writer_fallible, Writer, SerializeError};
///
/// #[derive(Debug, Facet)]
/// struct Point {
///     x: i32,
///     y: i32,
/// }
///
/// struct CustomWriter {
///     buffer: Vec<u8>,
/// }
///
/// impl Writer for CustomWriter {
///     fn write_byte(&mut self, byte: u8) -> Result<(), SerializeError> {
///         self.buffer.push(byte);
///         Ok(())
///     }
///
///     fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), SerializeError> {
///         self.buffer.extend_from_slice(bytes);
///         Ok(())
///     }
/// }
///
/// let point = Point { x: 10, y: 20 };
/// let mut writer = CustomWriter { buffer: Vec::new() };
/// to_writer_fallible(&point, &mut writer).unwrap();
/// ```
pub fn to_writer_fallible<T, W>(value: &T, writer: &mut W) -> Result<(), SerializeError>
where
    T: facet_core::Facet<'static>,
    W: Writer,
{
    let peek = Peek::new(value);
    serialize_value(peek, writer)
}

/// Core serialization function using custom traversal for postcard format.
fn serialize_value<W: Writer>(peek: Peek<'_, '_>, writer: &mut W) -> Result<(), SerializeError> {
    match (peek.shape().def, peek.shape().ty) {
        (Def::Scalar, _) => {
            let peek = peek.innermost_peek();
            serialize_scalar(peek, writer)
        }
        (Def::List(ld), _) => {
            // Special case for Vec<u8> - serialize as bytes
            if ld.t().is_type::<u8>() && peek.shape().is_type::<Vec<u8>>() {
                let bytes = peek.get::<Vec<u8>>().map_err(|e| {
                    SerializeError::Custom(alloc::format!("Failed to get Vec<u8>: {}", e))
                })?;
                write_varint(bytes.len() as u64, writer)?;
                return writer.write_bytes(bytes);
            }
            // General list handling
            let list = peek.into_list_like().map_err(|e| {
                SerializeError::Custom(alloc::format!("Failed to convert to list: {}", e))
            })?;
            let items: Vec<_> = list.iter().collect();
            write_varint(items.len() as u64, writer)?;
            for item in items {
                serialize_value(item, writer)?;
            }
            Ok(())
        }
        (Def::Array(ad), _) => {
            if ad.t().is_type::<u8>() {
                // Serialize byte arrays directly (length already known from type)
                let list = peek.into_list_like().map_err(|e| {
                    SerializeError::Custom(alloc::format!("Failed to convert to list: {}", e))
                })?;
                let bytes: Vec<u8> = list
                    .iter()
                    .map(|p| {
                        *p.get::<u8>()
                            .expect("Failed to get u8 from byte array element")
                    })
                    .collect();
                writer.write_bytes(&bytes)
            } else {
                // For fixed-size arrays, postcard doesn't write length
                let list = peek.into_list_like().map_err(|e| {
                    SerializeError::Custom(alloc::format!("Failed to convert to list: {}", e))
                })?;
                for item in list.iter() {
                    serialize_value(item, writer)?;
                }
                Ok(())
            }
        }
        (Def::Slice(sd), _) => {
            if sd.t().is_type::<u8>() {
                let bytes = peek.get::<[u8]>().map_err(|e| {
                    SerializeError::Custom(alloc::format!("Failed to get [u8]: {}", e))
                })?;
                write_varint(bytes.len() as u64, writer)?;
                writer.write_bytes(bytes)
            } else {
                let list = peek.into_list_like().map_err(|e| {
                    SerializeError::Custom(alloc::format!("Failed to convert to list: {}", e))
                })?;
                let items: Vec<_> = list.iter().collect();
                write_varint(items.len() as u64, writer)?;
                for item in items {
                    serialize_value(item, writer)?;
                }
                Ok(())
            }
        }
        (Def::Map(_), _) => {
            let map = peek.into_map().map_err(|e| {
                SerializeError::Custom(alloc::format!("Failed to convert to map: {}", e))
            })?;
            let entries: Vec<_> = map.iter().collect();
            write_varint(entries.len() as u64, writer)?;
            for (key, value) in entries {
                serialize_value(key, writer)?;
                serialize_value(value, writer)?;
            }
            Ok(())
        }
        (Def::Set(_), _) => {
            let set = peek.into_set().map_err(|e| {
                SerializeError::Custom(alloc::format!("Failed to convert to set: {}", e))
            })?;
            let items: Vec<_> = set.iter().collect();
            write_varint(items.len() as u64, writer)?;
            for item in items {
                serialize_value(item, writer)?;
            }
            Ok(())
        }
        (Def::Option(_), _) => {
            let opt = peek.into_option().map_err(|e| {
                SerializeError::Custom(alloc::format!("Failed to convert to option: {}", e))
            })?;
            if let Some(inner) = opt.value() {
                writer.write_byte(1)?; // Some
                serialize_value(inner, writer)
            } else {
                writer.write_byte(0) // None
            }
        }
        (Def::Result(_), _) => {
            let res = peek.into_result().map_err(|e| {
                SerializeError::Custom(alloc::format!("Failed to convert to result: {}", e))
            })?;
            if let Some(ok_value) = res.ok() {
                // Ok variant - write 0 as variant index, then the value
                write_varint(0, writer)?;
                serialize_value(ok_value, writer)
            } else if let Some(err_value) = res.err() {
                // Err variant - write 1 as variant index, then the value
                write_varint(1, writer)?;
                serialize_value(err_value, writer)
            } else {
                Err(SerializeError::Custom("Invalid Result state".into()))
            }
        }
        (Def::Pointer(_), _) => {
            let ptr = peek.into_pointer().map_err(|e| {
                SerializeError::Custom(alloc::format!("Failed to convert to pointer: {}", e))
            })?;
            if let Some(inner) = ptr.borrow_inner() {
                serialize_value(inner, writer)
            } else {
                Err(SerializeError::Custom(
                    "Smart pointer without borrow support".into(),
                ))
            }
        }
        (_, Type::User(UserType::Struct(sd))) => {
            match sd.kind {
                StructKind::Unit => {
                    // Unit structs serialize as nothing
                    Ok(())
                }
                StructKind::Tuple | StructKind::TupleStruct | StructKind::Struct => {
                    // All struct kinds serialize fields in order without names
                    let ps = peek.into_struct().map_err(|e| {
                        SerializeError::Custom(alloc::format!("Failed to convert to struct: {}", e))
                    })?;
                    for (_, field_value) in ps.fields_for_serialize() {
                        serialize_value(field_value, writer)?;
                    }
                    Ok(())
                }
            }
        }
        (_, Type::User(UserType::Enum(et))) => {
            let pe = peek.into_enum().map_err(|e| {
                SerializeError::Custom(alloc::format!("Failed to convert to enum: {}", e))
            })?;
            let variant = pe
                .active_variant()
                .map_err(|_| SerializeError::Custom("Failed to get active variant".into()))?;
            let variant_idx = et
                .variants
                .iter()
                .position(|v| v.name == variant.name)
                .unwrap_or(0);

            // Write variant index as varint
            write_varint(variant_idx as u64, writer)?;

            if variant.data.fields.is_empty() {
                // Unit variant - nothing more to write
                Ok(())
            } else {
                // Serialize fields in order
                for (_, field_value) in pe.fields_for_serialize() {
                    serialize_value(field_value, writer)?;
                }
                Ok(())
            }
        }
        (_, Type::Pointer(_)) => {
            // Handle string types
            if let Some(s) = peek.as_str() {
                write_varint(s.len() as u64, writer)?;
                writer.write_bytes(s.as_bytes())
            } else if let Some(bytes) = peek.as_bytes() {
                write_varint(bytes.len() as u64, writer)?;
                writer.write_bytes(bytes)
            } else {
                let innermost = peek.innermost_peek();
                if innermost.shape() != peek.shape() {
                    serialize_value(innermost, writer)
                } else {
                    Err(SerializeError::Custom("Unknown pointer type".into()))
                }
            }
        }
        _ => Err(SerializeError::Custom(alloc::format!(
            "Unsupported type: {:?}",
            peek.shape().ty
        ))),
    }
}

/// Serialize a scalar value with type-precise encoding.
fn serialize_scalar<W: Writer>(peek: Peek<'_, '_>, writer: &mut W) -> Result<(), SerializeError> {
    use facet_reflect::ScalarType;

    match peek.scalar_type() {
        Some(ScalarType::Unit) => Ok(()),
        Some(ScalarType::Bool) => {
            let v = *peek
                .get::<bool>()
                .map_err(|e| SerializeError::Custom(alloc::format!("Failed to get bool: {}", e)))?;
            writer.write_byte(if v { 1 } else { 0 })
        }
        Some(ScalarType::Char) => {
            let c = *peek
                .get::<char>()
                .map_err(|e| SerializeError::Custom(alloc::format!("Failed to get char: {}", e)))?;
            let mut buf = [0; 4];
            let s = c.encode_utf8(&mut buf);
            write_varint(s.len() as u64, writer)?;
            writer.write_bytes(s.as_bytes())
        }
        Some(ScalarType::Str) => {
            let s = peek
                .get::<str>()
                .map_err(|e| SerializeError::Custom(alloc::format!("Failed to get str: {}", e)))?;
            write_varint(s.len() as u64, writer)?;
            writer.write_bytes(s.as_bytes())
        }
        Some(ScalarType::String) => {
            let s = peek.get::<alloc::string::String>().map_err(|e| {
                SerializeError::Custom(alloc::format!("Failed to get String: {}", e))
            })?;
            write_varint(s.len() as u64, writer)?;
            writer.write_bytes(s.as_bytes())
        }
        Some(ScalarType::CowStr) => {
            let s = peek.get::<Cow<'_, str>>().map_err(|e| {
                SerializeError::Custom(alloc::format!("Failed to get Cow<str>: {}", e))
            })?;
            write_varint(s.len() as u64, writer)?;
            writer.write_bytes(s.as_bytes())
        }
        Some(ScalarType::F32) => {
            let v = *peek
                .get::<f32>()
                .map_err(|e| SerializeError::Custom(alloc::format!("Failed to get f32: {}", e)))?;
            writer.write_bytes(&v.to_le_bytes())
        }
        Some(ScalarType::F64) => {
            let v = *peek
                .get::<f64>()
                .map_err(|e| SerializeError::Custom(alloc::format!("Failed to get f64: {}", e)))?;
            writer.write_bytes(&v.to_le_bytes())
        }
        Some(ScalarType::U8) => {
            let v = *peek
                .get::<u8>()
                .map_err(|e| SerializeError::Custom(alloc::format!("Failed to get u8: {}", e)))?;
            writer.write_byte(v)
        }
        Some(ScalarType::U16) => {
            let v = *peek
                .get::<u16>()
                .map_err(|e| SerializeError::Custom(alloc::format!("Failed to get u16: {}", e)))?;
            write_varint(v as u64, writer)
        }
        Some(ScalarType::U32) => {
            let v = *peek
                .get::<u32>()
                .map_err(|e| SerializeError::Custom(alloc::format!("Failed to get u32: {}", e)))?;
            write_varint(v as u64, writer)
        }
        Some(ScalarType::U64) => {
            let v = *peek
                .get::<u64>()
                .map_err(|e| SerializeError::Custom(alloc::format!("Failed to get u64: {}", e)))?;
            write_varint(v, writer)
        }
        Some(ScalarType::U128) => {
            let v = *peek
                .get::<u128>()
                .map_err(|e| SerializeError::Custom(alloc::format!("Failed to get u128: {}", e)))?;
            write_varint_u128(v, writer)
        }
        Some(ScalarType::USize) => {
            let v = *peek.get::<usize>().map_err(|e| {
                SerializeError::Custom(alloc::format!("Failed to get usize: {}", e))
            })?;
            write_varint(v as u64, writer)
        }
        Some(ScalarType::I8) => {
            let v = *peek
                .get::<i8>()
                .map_err(|e| SerializeError::Custom(alloc::format!("Failed to get i8: {}", e)))?;
            writer.write_byte(v as u8)
        }
        Some(ScalarType::I16) => {
            let v = *peek
                .get::<i16>()
                .map_err(|e| SerializeError::Custom(alloc::format!("Failed to get i16: {}", e)))?;
            write_varint_signed(v as i64, writer)
        }
        Some(ScalarType::I32) => {
            let v = *peek
                .get::<i32>()
                .map_err(|e| SerializeError::Custom(alloc::format!("Failed to get i32: {}", e)))?;
            write_varint_signed(v as i64, writer)
        }
        Some(ScalarType::I64) => {
            let v = *peek
                .get::<i64>()
                .map_err(|e| SerializeError::Custom(alloc::format!("Failed to get i64: {}", e)))?;
            write_varint_signed(v, writer)
        }
        Some(ScalarType::I128) => {
            let v = *peek
                .get::<i128>()
                .map_err(|e| SerializeError::Custom(alloc::format!("Failed to get i128: {}", e)))?;
            write_varint_signed_i128(v, writer)
        }
        Some(ScalarType::ISize) => {
            let v = *peek.get::<isize>().map_err(|e| {
                SerializeError::Custom(alloc::format!("Failed to get isize: {}", e))
            })?;
            write_varint_signed(v as i64, writer)
        }
        Some(scalar_type) => Err(SerializeError::Custom(alloc::format!(
            "Unsupported scalar type: {:?}",
            scalar_type
        ))),
        None => {
            // Handle camino path types
            #[cfg(feature = "camino")]
            if peek.shape().type_identifier == "Utf8PathBuf" {
                let path = peek.get::<camino::Utf8PathBuf>().map_err(|e| {
                    SerializeError::Custom(alloc::format!("Failed to get Utf8PathBuf: {}", e))
                })?;
                let s = path.as_str();
                write_varint(s.len() as u64, writer)?;
                return writer.write_bytes(s.as_bytes());
            }
            #[cfg(feature = "camino")]
            if peek.shape().type_identifier == "Utf8Path" {
                let path = peek.get::<camino::Utf8Path>().map_err(|e| {
                    SerializeError::Custom(alloc::format!("Failed to get Utf8Path: {}", e))
                })?;
                let s = path.as_str();
                write_varint(s.len() as u64, writer)?;
                return writer.write_bytes(s.as_bytes());
            }

            // Try string as fallback for opaque scalars
            if let Some(s) = peek.as_str() {
                write_varint(s.len() as u64, writer)?;
                writer.write_bytes(s.as_bytes())
            } else {
                Err(SerializeError::Custom(alloc::format!(
                    "Unknown scalar type: {}",
                    peek.shape().type_identifier
                )))
            }
        }
    }
}

/// Write an unsigned varint (LEB128-like encoding used by postcard)
fn write_varint<W: Writer>(mut value: u64, writer: &mut W) -> Result<(), SerializeError> {
    loop {
        let mut byte = (value & 0x7F) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        writer.write_byte(byte)?;
        if value == 0 {
            break;
        }
    }
    Ok(())
}

/// Write an unsigned 128-bit varint
fn write_varint_u128<W: Writer>(mut value: u128, writer: &mut W) -> Result<(), SerializeError> {
    loop {
        let mut byte = (value & 0x7F) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        writer.write_byte(byte)?;
        if value == 0 {
            break;
        }
    }
    Ok(())
}

/// Write a signed varint using zigzag encoding
fn write_varint_signed<W: Writer>(value: i64, writer: &mut W) -> Result<(), SerializeError> {
    // Zigzag encoding: (value << 1) ^ (value >> 63)
    let encoded = ((value << 1) ^ (value >> 63)) as u64;
    write_varint(encoded, writer)
}

/// Write a signed 128-bit varint using zigzag encoding
fn write_varint_signed_i128<W: Writer>(value: i128, writer: &mut W) -> Result<(), SerializeError> {
    // Zigzag encoding: (value << 1) ^ (value >> 127)
    let encoded = ((value << 1) ^ (value >> 127)) as u128;
    write_varint_u128(encoded, writer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use facet::Facet;
    use postcard::to_allocvec as postcard_to_vec;
    use serde::Serialize;

    #[derive(Facet, Serialize, PartialEq, Debug)]
    struct SimpleStruct {
        a: u32,
        b: alloc::string::String,
        c: bool,
    }

    #[test]
    fn test_simple_struct() {
        facet_testhelpers::setup();

        let value = SimpleStruct {
            a: 123,
            b: "hello".into(),
            c: true,
        };

        let facet_bytes = to_vec(&value).unwrap();
        let postcard_bytes = postcard_to_vec(&value).unwrap();

        assert_eq!(facet_bytes, postcard_bytes);
    }

    #[test]
    fn test_u8() {
        facet_testhelpers::setup();

        #[derive(Facet, Serialize, PartialEq, Debug)]
        struct U8Struct {
            value: u8,
        }

        let value = U8Struct { value: 42 };
        let facet_bytes = to_vec(&value).unwrap();
        let postcard_bytes = postcard_to_vec(&value).unwrap();
        assert_eq!(facet_bytes, postcard_bytes);
    }

    #[test]
    fn test_i32() {
        facet_testhelpers::setup();

        #[derive(Facet, Serialize, PartialEq, Debug)]
        struct I32Struct {
            value: i32,
        }

        let value = I32Struct { value: -100000 };
        let facet_bytes = to_vec(&value).unwrap();
        let postcard_bytes = postcard_to_vec(&value).unwrap();
        assert_eq!(facet_bytes, postcard_bytes);
    }

    #[test]
    fn test_string() {
        facet_testhelpers::setup();

        #[derive(Facet, Serialize, PartialEq, Debug)]
        struct StringStruct {
            value: alloc::string::String,
        }

        let value = StringStruct {
            value: "hello world".into(),
        };
        let facet_bytes = to_vec(&value).unwrap();
        let postcard_bytes = postcard_to_vec(&value).unwrap();
        assert_eq!(facet_bytes, postcard_bytes);
    }

    #[test]
    fn test_vec() {
        facet_testhelpers::setup();

        #[derive(Facet, Serialize, PartialEq, Debug)]
        struct VecStruct {
            values: Vec<u32>,
        }

        let value = VecStruct {
            values: alloc::vec![1, 2, 3, 4, 5],
        };
        let facet_bytes = to_vec(&value).unwrap();
        let postcard_bytes = postcard_to_vec(&value).unwrap();
        assert_eq!(facet_bytes, postcard_bytes);
    }

    #[test]
    fn test_option_some() {
        facet_testhelpers::setup();

        #[derive(Facet, Serialize, PartialEq, Debug)]
        struct OptionStruct {
            value: Option<u32>,
        }

        let value = OptionStruct { value: Some(42) };
        let facet_bytes = to_vec(&value).unwrap();
        let postcard_bytes = postcard_to_vec(&value).unwrap();
        assert_eq!(facet_bytes, postcard_bytes);
    }

    #[test]
    fn test_option_none() {
        facet_testhelpers::setup();

        #[derive(Facet, Serialize, PartialEq, Debug)]
        struct OptionStruct {
            value: Option<u32>,
        }

        let value = OptionStruct { value: None };
        let facet_bytes = to_vec(&value).unwrap();
        let postcard_bytes = postcard_to_vec(&value).unwrap();
        assert_eq!(facet_bytes, postcard_bytes);
    }

    #[test]
    fn test_unit_enum() {
        facet_testhelpers::setup();

        #[derive(Facet, Serialize, PartialEq, Debug)]
        #[repr(C)]
        enum Color {
            Red,
            Green,
            Blue,
        }

        let facet_bytes = to_vec(&Color::Red).unwrap();
        let postcard_bytes = postcard_to_vec(&Color::Red).unwrap();
        assert_eq!(facet_bytes, postcard_bytes);

        let facet_bytes = to_vec(&Color::Green).unwrap();
        let postcard_bytes = postcard_to_vec(&Color::Green).unwrap();
        assert_eq!(facet_bytes, postcard_bytes);

        let facet_bytes = to_vec(&Color::Blue).unwrap();
        let postcard_bytes = postcard_to_vec(&Color::Blue).unwrap();
        assert_eq!(facet_bytes, postcard_bytes);
    }

    #[test]
    fn test_tuple_enum() {
        facet_testhelpers::setup();

        #[derive(Facet, Serialize, PartialEq, Debug)]
        #[repr(C)]
        enum Value {
            Int(i32),
            Text(alloc::string::String),
        }

        let facet_bytes = to_vec(&Value::Int(42)).unwrap();
        let postcard_bytes = postcard_to_vec(&Value::Int(42)).unwrap();
        assert_eq!(facet_bytes, postcard_bytes);

        let facet_bytes = to_vec(&Value::Text("hello".into())).unwrap();
        let postcard_bytes = postcard_to_vec(&Value::Text("hello".into())).unwrap();
        assert_eq!(facet_bytes, postcard_bytes);
    }

    #[test]
    fn test_struct_enum() {
        facet_testhelpers::setup();

        #[derive(Facet, Serialize, PartialEq, Debug)]
        #[repr(C)]
        enum Message {
            Quit,
            Move { x: i32, y: i32 },
        }

        let facet_bytes = to_vec(&Message::Quit).unwrap();
        let postcard_bytes = postcard_to_vec(&Message::Quit).unwrap();
        assert_eq!(facet_bytes, postcard_bytes);

        let facet_bytes = to_vec(&Message::Move { x: 10, y: 20 }).unwrap();
        let postcard_bytes = postcard_to_vec(&Message::Move { x: 10, y: 20 }).unwrap();
        assert_eq!(facet_bytes, postcard_bytes);
    }

    #[test]
    fn test_to_writer_fallible() {
        facet_testhelpers::setup();

        struct CustomWriter {
            buffer: Vec<u8>,
        }

        impl Writer for CustomWriter {
            fn write_byte(&mut self, byte: u8) -> Result<(), SerializeError> {
                self.buffer.push(byte);
                Ok(())
            }

            fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), SerializeError> {
                self.buffer.extend_from_slice(bytes);
                Ok(())
            }
        }

        let value = SimpleStruct {
            a: 123,
            b: "hello".into(),
            c: true,
        };

        let mut writer = CustomWriter { buffer: Vec::new() };
        to_writer_fallible(&value, &mut writer).unwrap();

        let postcard_bytes = postcard_to_vec(&value).unwrap();
        assert_eq!(writer.buffer, postcard_bytes);
    }
}
