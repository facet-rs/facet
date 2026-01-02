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

use alloc::vec::Vec;

use facet_core::{Def, ScalarType};
use facet_format::{
    DynamicValueEncoding, DynamicValueTag, EnumVariantEncoding, FormatSerializer, MapEncoding,
    SerializeError as FormatSerializeError, StructFieldMode, serialize_root,
};
use facet_reflect::Peek;

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
/// use facet_postcard::{Writer, SerializeError};
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
/// use facet_postcard::to_vec;
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
/// use facet_postcard::{to_writer_fallible, Writer, SerializeError};
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
    let mut serializer = PostcardSerializer::new(writer);
    serialize_root(&mut serializer, peek).map_err(map_format_error)
}

/// Serializes a [`Peek`] reference to postcard bytes.
///
/// This is useful when you have a type-erased reference via reflection
/// and need to serialize it without knowing the concrete type at compile time.
///
/// # Example
/// ```
/// use facet::Facet;
/// use facet_reflect::Peek;
/// use facet_postcard::peek_to_vec;
///
/// #[derive(Debug, Facet)]
/// struct Point {
///     x: i32,
///     y: i32,
/// }
///
/// let point = Point { x: 10, y: 20 };
/// let peek = Peek::new(&point);
/// let bytes = peek_to_vec(peek).unwrap();
/// ```
pub fn peek_to_vec(peek: Peek<'_, '_>) -> Result<Vec<u8>, SerializeError> {
    let mut buffer = Vec::new();
    let mut serializer = PostcardSerializer::new(&mut buffer);
    serialize_root(&mut serializer, peek).map_err(map_format_error)?;
    Ok(buffer)
}

fn map_format_error(error: FormatSerializeError<SerializeError>) -> SerializeError {
    match error {
        FormatSerializeError::Backend(err) => err,
        FormatSerializeError::Reflect(err) => SerializeError::Custom(alloc::format!("{err}")),
        FormatSerializeError::Unsupported(message) => SerializeError::Custom(message.into_owned()),
        FormatSerializeError::Internal(message) => SerializeError::Custom(message.into_owned()),
    }
}

struct PostcardSerializer<'a, W> {
    writer: &'a mut W,
}

impl<'a, W> PostcardSerializer<'a, W> {
    fn new(writer: &'a mut W) -> Self {
        Self { writer }
    }

    fn write_str(&mut self, s: &str) -> Result<(), SerializeError>
    where
        W: Writer,
    {
        write_varint(s.len() as u64, self.writer)?;
        self.writer.write_bytes(s.as_bytes())
    }

    fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), SerializeError>
    where
        W: Writer,
    {
        write_varint(bytes.len() as u64, self.writer)?;
        self.writer.write_bytes(bytes)
    }

    fn write_dynamic_tag(&mut self, tag: DynamicValueTag) -> Result<(), SerializeError>
    where
        W: Writer,
    {
        let byte = match tag {
            DynamicValueTag::Null => 0,
            DynamicValueTag::Bool => 1,
            DynamicValueTag::I64 => 2,
            DynamicValueTag::U64 => 3,
            DynamicValueTag::F64 => 4,
            DynamicValueTag::String => 5,
            DynamicValueTag::Bytes => 6,
            DynamicValueTag::Array => 7,
            DynamicValueTag::Object => 8,
            DynamicValueTag::DateTime => 9,
        };
        self.writer.write_byte(byte)
    }
}

impl<W: Writer> FormatSerializer for PostcardSerializer<'_, W> {
    type Error = SerializeError;

    fn begin_struct(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn field_key(&mut self, _key: &str) -> Result<(), Self::Error> {
        Err(SerializeError::Custom(
            "postcard does not support named fields".into(),
        ))
    }

    fn end_struct(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn begin_seq(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn end_seq(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn scalar(&mut self, scalar: facet_format::ScalarValue<'_>) -> Result<(), Self::Error> {
        match scalar {
            facet_format::ScalarValue::Null => Ok(()),
            facet_format::ScalarValue::Bool(v) => self.writer.write_byte(if v { 1 } else { 0 }),
            facet_format::ScalarValue::I64(n) => write_varint_signed(n, self.writer),
            facet_format::ScalarValue::U64(n) => write_varint(n, self.writer),
            facet_format::ScalarValue::I128(n) => write_varint_signed_i128(n, self.writer),
            facet_format::ScalarValue::U128(n) => write_varint_u128(n, self.writer),
            facet_format::ScalarValue::F64(n) => self.writer.write_bytes(&n.to_le_bytes()),
            facet_format::ScalarValue::Str(s) => self.write_str(&s),
            facet_format::ScalarValue::Bytes(bytes) => self.write_bytes(&bytes),
        }
    }

    fn struct_field_mode(&self) -> StructFieldMode {
        StructFieldMode::Unnamed
    }

    fn map_encoding(&self) -> MapEncoding {
        MapEncoding::Pairs
    }

    fn enum_variant_encoding(&self) -> EnumVariantEncoding {
        EnumVariantEncoding::Index
    }

    fn dynamic_value_encoding(&self) -> DynamicValueEncoding {
        DynamicValueEncoding::Tagged
    }

    fn dynamic_value_tag(&mut self, tag: DynamicValueTag) -> Result<(), Self::Error> {
        self.write_dynamic_tag(tag)
    }

    fn begin_seq_with_len(&mut self, len: usize) -> Result<(), Self::Error> {
        write_varint(len as u64, self.writer)
    }

    fn begin_map_with_len(&mut self, len: usize) -> Result<(), Self::Error> {
        write_varint(len as u64, self.writer)
    }

    fn end_map(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn typed_scalar(
        &mut self,
        scalar_type: ScalarType,
        value: Peek<'_, '_>,
    ) -> Result<(), Self::Error> {
        match scalar_type {
            ScalarType::Unit => Ok(()),
            ScalarType::Bool => {
                let v = *value.get::<bool>().map_err(|e| {
                    SerializeError::Custom(alloc::format!("Failed to get bool: {}", e))
                })?;
                self.writer.write_byte(if v { 1 } else { 0 })
            }
            ScalarType::Char => {
                let c = *value.get::<char>().map_err(|e| {
                    SerializeError::Custom(alloc::format!("Failed to get char: {}", e))
                })?;
                let mut buf = [0u8; 4];
                let s = c.encode_utf8(&mut buf);
                self.write_str(s)
            }
            ScalarType::Str | ScalarType::String | ScalarType::CowStr => {
                let s = value
                    .as_str()
                    .ok_or_else(|| SerializeError::Custom("Failed to get string value".into()))?;
                self.write_str(s)
            }
            ScalarType::F32 => {
                let v = *value.get::<f32>().map_err(|e| {
                    SerializeError::Custom(alloc::format!("Failed to get f32: {}", e))
                })?;
                self.writer.write_bytes(&v.to_le_bytes())
            }
            ScalarType::F64 => {
                let v = *value.get::<f64>().map_err(|e| {
                    SerializeError::Custom(alloc::format!("Failed to get f64: {}", e))
                })?;
                self.writer.write_bytes(&v.to_le_bytes())
            }
            ScalarType::U8 => {
                let v = *value.get::<u8>().map_err(|e| {
                    SerializeError::Custom(alloc::format!("Failed to get u8: {}", e))
                })?;
                self.writer.write_byte(v)
            }
            ScalarType::U16 => {
                let v = *value.get::<u16>().map_err(|e| {
                    SerializeError::Custom(alloc::format!("Failed to get u16: {}", e))
                })?;
                write_varint(v as u64, self.writer)
            }
            ScalarType::U32 => {
                let v = *value.get::<u32>().map_err(|e| {
                    SerializeError::Custom(alloc::format!("Failed to get u32: {}", e))
                })?;
                write_varint(v as u64, self.writer)
            }
            ScalarType::U64 => {
                let v = *value.get::<u64>().map_err(|e| {
                    SerializeError::Custom(alloc::format!("Failed to get u64: {}", e))
                })?;
                write_varint(v, self.writer)
            }
            ScalarType::U128 => {
                let v = *value.get::<u128>().map_err(|e| {
                    SerializeError::Custom(alloc::format!("Failed to get u128: {}", e))
                })?;
                write_varint_u128(v, self.writer)
            }
            ScalarType::USize => {
                let v = *value.get::<usize>().map_err(|e| {
                    SerializeError::Custom(alloc::format!("Failed to get usize: {}", e))
                })?;
                write_varint(v as u64, self.writer)
            }
            ScalarType::I8 => {
                let v = *value.get::<i8>().map_err(|e| {
                    SerializeError::Custom(alloc::format!("Failed to get i8: {}", e))
                })?;
                self.writer.write_byte(v as u8)
            }
            ScalarType::I16 => {
                let v = *value.get::<i16>().map_err(|e| {
                    SerializeError::Custom(alloc::format!("Failed to get i16: {}", e))
                })?;
                write_varint_signed(v as i64, self.writer)
            }
            ScalarType::I32 => {
                let v = *value.get::<i32>().map_err(|e| {
                    SerializeError::Custom(alloc::format!("Failed to get i32: {}", e))
                })?;
                write_varint_signed(v as i64, self.writer)
            }
            ScalarType::I64 => {
                let v = *value.get::<i64>().map_err(|e| {
                    SerializeError::Custom(alloc::format!("Failed to get i64: {}", e))
                })?;
                write_varint_signed(v, self.writer)
            }
            ScalarType::I128 => {
                let v = *value.get::<i128>().map_err(|e| {
                    SerializeError::Custom(alloc::format!("Failed to get i128: {}", e))
                })?;
                write_varint_signed_i128(v, self.writer)
            }
            ScalarType::ISize => {
                let v = *value.get::<isize>().map_err(|e| {
                    SerializeError::Custom(alloc::format!("Failed to get isize: {}", e))
                })?;
                write_varint_signed(v as i64, self.writer)
            }
            #[cfg(feature = "net")]
            ScalarType::SocketAddr => {
                let v = *value.get::<core::net::SocketAddr>().map_err(|e| {
                    SerializeError::Custom(alloc::format!("Failed to get SocketAddr: {}", e))
                })?;
                self.write_str(&v.to_string())
            }
            #[cfg(feature = "net")]
            ScalarType::IpAddr => {
                let v = *value.get::<core::net::IpAddr>().map_err(|e| {
                    SerializeError::Custom(alloc::format!("Failed to get IpAddr: {}", e))
                })?;
                self.write_str(&v.to_string())
            }
            #[cfg(feature = "net")]
            ScalarType::Ipv4Addr => {
                let v = *value.get::<core::net::Ipv4Addr>().map_err(|e| {
                    SerializeError::Custom(alloc::format!("Failed to get Ipv4Addr: {}", e))
                })?;
                self.write_str(&v.to_string())
            }
            #[cfg(feature = "net")]
            ScalarType::Ipv6Addr => {
                let v = *value.get::<core::net::Ipv6Addr>().map_err(|e| {
                    SerializeError::Custom(alloc::format!("Failed to get Ipv6Addr: {}", e))
                })?;
                self.write_str(&v.to_string())
            }
            _ => Err(SerializeError::Custom(alloc::format!(
                "Unsupported scalar type: {:?}",
                scalar_type
            ))),
        }
    }

    fn begin_option_some(&mut self) -> Result<(), Self::Error> {
        self.writer.write_byte(1)
    }

    fn serialize_none(&mut self) -> Result<(), Self::Error> {
        self.writer.write_byte(0)
    }

    fn begin_enum_variant(
        &mut self,
        variant_index: usize,
        _variant_name: &'static str,
    ) -> Result<(), Self::Error> {
        write_varint(variant_index as u64, self.writer)
    }

    fn serialize_opaque_scalar(
        &mut self,
        shape: &'static facet_core::Shape,
        value: Peek<'_, '_>,
    ) -> Result<bool, Self::Error> {
        if value.scalar_type().is_some() {
            return Ok(false);
        }

        // Camino types (UTF-8 paths)
        #[cfg(feature = "camino")]
        if shape.is_type::<camino::Utf8PathBuf>() {
            use camino::Utf8PathBuf;
            let path = value.get::<Utf8PathBuf>().map_err(|e| {
                SerializeError::Custom(alloc::format!("Failed to get Utf8PathBuf: {}", e))
            })?;
            self.write_str(path.as_str())?;
            return Ok(true);
        }
        #[cfg(feature = "camino")]
        if shape.id == facet_core::Shape::id_of::<camino::Utf8Path>() {
            use camino::Utf8Path;
            let path = value.get::<Utf8Path>().map_err(|e| {
                SerializeError::Custom(alloc::format!("Failed to get Utf8Path: {}", e))
            })?;
            self.write_str(path.as_str())?;
            return Ok(true);
        }

        // UUID - serialize as 16 bytes (native format)
        #[cfg(feature = "uuid")]
        if shape.is_type::<uuid::Uuid>() {
            use uuid::Uuid;
            let uuid = value
                .get::<Uuid>()
                .map_err(|e| SerializeError::Custom(alloc::format!("Failed to get Uuid: {}", e)))?;
            self.writer.write_bytes(uuid.as_bytes())?;
            return Ok(true);
        }

        // ULID - serialize as 16 bytes (native format)
        #[cfg(feature = "ulid")]
        if shape.is_type::<ulid::Ulid>() {
            use ulid::Ulid;
            let ulid = value
                .get::<Ulid>()
                .map_err(|e| SerializeError::Custom(alloc::format!("Failed to get Ulid: {}", e)))?;
            self.writer.write_bytes(&ulid.to_bytes())?;
            return Ok(true);
        }

        // Jiff date/time types - serialize as RFC3339 strings
        #[cfg(feature = "jiff02")]
        if shape.is_type::<jiff::Zoned>() {
            use jiff::Zoned;
            let zoned = value.get::<Zoned>().map_err(|e| {
                SerializeError::Custom(alloc::format!("Failed to get Zoned: {}", e))
            })?;
            self.write_str(&zoned.to_string())?;
            return Ok(true);
        }
        #[cfg(feature = "jiff02")]
        if shape.is_type::<jiff::Timestamp>() {
            use jiff::Timestamp;
            let ts = value.get::<Timestamp>().map_err(|e| {
                SerializeError::Custom(alloc::format!("Failed to get Timestamp: {}", e))
            })?;
            self.write_str(&ts.to_string())?;
            return Ok(true);
        }
        #[cfg(feature = "jiff02")]
        if shape.is_type::<jiff::civil::DateTime>() {
            use jiff::civil::DateTime;
            let dt = value.get::<DateTime>().map_err(|e| {
                SerializeError::Custom(alloc::format!("Failed to get DateTime: {}", e))
            })?;
            self.write_str(&dt.to_string())?;
            return Ok(true);
        }

        // Chrono date/time types - serialize as RFC3339 strings
        #[cfg(feature = "chrono")]
        if shape.is_type::<chrono::DateTime<chrono::Utc>>() {
            use chrono::{DateTime, SecondsFormat, Utc};
            let dt = value.get::<DateTime<Utc>>().map_err(|e| {
                SerializeError::Custom(alloc::format!("Failed to get DateTime<Utc>: {}", e))
            })?;
            self.write_str(&dt.to_rfc3339_opts(SecondsFormat::AutoSi, true))?;
            return Ok(true);
        }
        #[cfg(feature = "chrono")]
        if shape.is_type::<chrono::DateTime<chrono::Local>>() {
            use chrono::{DateTime, Local, SecondsFormat};
            let dt = value.get::<DateTime<Local>>().map_err(|e| {
                SerializeError::Custom(alloc::format!("Failed to get DateTime<Local>: {}", e))
            })?;
            self.write_str(&dt.to_rfc3339_opts(SecondsFormat::AutoSi, false))?;
            return Ok(true);
        }
        #[cfg(feature = "chrono")]
        if shape.is_type::<chrono::DateTime<chrono::FixedOffset>>() {
            use chrono::{DateTime, FixedOffset, SecondsFormat};
            let dt = value.get::<DateTime<FixedOffset>>().map_err(|e| {
                SerializeError::Custom(alloc::format!("Failed to get DateTime<FixedOffset>: {}", e))
            })?;
            self.write_str(&dt.to_rfc3339_opts(SecondsFormat::AutoSi, false))?;
            return Ok(true);
        }
        #[cfg(feature = "chrono")]
        if shape.is_type::<chrono::NaiveDateTime>() {
            use chrono::NaiveDateTime;
            let dt = value.get::<NaiveDateTime>().map_err(|e| {
                SerializeError::Custom(alloc::format!("Failed to get NaiveDateTime: {}", e))
            })?;
            self.write_str(&dt.format("%Y-%m-%dT%H:%M:%S").to_string())?;
            return Ok(true);
        }
        #[cfg(feature = "chrono")]
        if shape.is_type::<chrono::NaiveDate>() {
            use chrono::NaiveDate;
            let date = value.get::<NaiveDate>().map_err(|e| {
                SerializeError::Custom(alloc::format!("Failed to get NaiveDate: {}", e))
            })?;
            self.write_str(&date.to_string())?;
            return Ok(true);
        }
        #[cfg(feature = "chrono")]
        if shape.is_type::<chrono::NaiveTime>() {
            use chrono::NaiveTime;
            let time = value.get::<NaiveTime>().map_err(|e| {
                SerializeError::Custom(alloc::format!("Failed to get NaiveTime: {}", e))
            })?;
            self.write_str(&time.to_string())?;
            return Ok(true);
        }

        // Time crate date/time types - serialize as RFC3339 strings
        #[cfg(feature = "time")]
        if shape.is_type::<time::UtcDateTime>() {
            use time::UtcDateTime;
            let dt = value.get::<UtcDateTime>().map_err(|e| {
                SerializeError::Custom(alloc::format!("Failed to get UtcDateTime: {}", e))
            })?;
            let s = dt
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_else(|_| "<invalid>".to_string());
            self.write_str(&s)?;
            return Ok(true);
        }
        #[cfg(feature = "time")]
        if shape.is_type::<time::OffsetDateTime>() {
            use time::OffsetDateTime;
            let dt = value.get::<OffsetDateTime>().map_err(|e| {
                SerializeError::Custom(alloc::format!("Failed to get OffsetDateTime: {}", e))
            })?;
            let s = dt
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_else(|_| "<invalid>".to_string());
            self.write_str(&s)?;
            return Ok(true);
        }

        // OrderedFloat - serialize as the inner float
        #[cfg(feature = "ordered-float")]
        if shape.is_type::<ordered_float::OrderedFloat<f32>>() {
            use ordered_float::OrderedFloat;
            let val = value.get::<OrderedFloat<f32>>().map_err(|e| {
                SerializeError::Custom(alloc::format!("Failed to get OrderedFloat<f32>: {}", e))
            })?;
            self.writer.write_bytes(&val.0.to_le_bytes())?;
            return Ok(true);
        } else if shape.is_type::<ordered_float::OrderedFloat<f64>>() {
            use ordered_float::OrderedFloat;
            let val = value.get::<OrderedFloat<f64>>().map_err(|e| {
                SerializeError::Custom(alloc::format!("Failed to get OrderedFloat<f64>: {}", e))
            })?;
            self.writer.write_bytes(&val.0.to_le_bytes())?;
            return Ok(true);
        }

        // NotNan - serialize as the inner float
        #[cfg(feature = "ordered-float")]
        if shape.is_type::<ordered_float::NotNan<f32>>() {
            use ordered_float::NotNan;
            let val = value.get::<NotNan<f32>>().map_err(|e| {
                SerializeError::Custom(alloc::format!("Failed to get NotNan<f32>: {}", e))
            })?;
            self.writer.write_bytes(&val.into_inner().to_le_bytes())?;
            return Ok(true);
        } else if shape.is_type::<ordered_float::NotNan<f64>>() {
            use ordered_float::NotNan;
            let val = value.get::<NotNan<f64>>().map_err(|e| {
                SerializeError::Custom(alloc::format!("Failed to get NotNan<f64>: {}", e))
            })?;
            self.writer.write_bytes(&val.into_inner().to_le_bytes())?;
            return Ok(true);
        }

        // bytestring::ByteString
        #[cfg(feature = "bytestring")]
        if shape == <bytestring::ByteString as facet_core::Facet>::SHAPE {
            let bs = value.get::<bytestring::ByteString>().map_err(|e| {
                SerializeError::Custom(alloc::format!("Failed to get ByteString: {}", e))
            })?;
            self.write_str(bs.as_ref())?;
            return Ok(true);
        }

        // compact_str::CompactString
        #[cfg(feature = "compact_str")]
        if shape == <compact_str::CompactString as facet_core::Facet>::SHAPE {
            let cs = value.get::<compact_str::CompactString>().map_err(|e| {
                SerializeError::Custom(alloc::format!("Failed to get CompactString: {}", e))
            })?;
            self.write_str(cs.as_str())?;
            return Ok(true);
        }

        // smartstring::SmartString<LazyCompact>
        #[cfg(feature = "smartstring")]
        if shape == <smartstring::SmartString<smartstring::LazyCompact> as facet_core::Facet>::SHAPE
        {
            let ss = value
                .get::<smartstring::SmartString<smartstring::LazyCompact>>()
                .map_err(|e| {
                    SerializeError::Custom(alloc::format!("Failed to get SmartString: {}", e))
                })?;
            self.write_str(ss.as_str())?;
            return Ok(true);
        }

        if shape.inner.is_some() {
            return Ok(false);
        }

        // Fallback to string or Display for non-standard scalars.
        if matches!(shape.def, Def::Scalar) {
            if let Some(s) = value.as_str() {
                self.write_str(s)?;
                return Ok(true);
            }
            if shape.vtable.has_display() {
                let s = alloc::format!("{}", value);
                self.write_str(&s)?;
                return Ok(true);
            }
        }

        Ok(false)
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
    use facet_value::{VArray, VBytes, VNumber, VObject, VString, Value};
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

    #[test]
    fn test_value_roundtrip() {
        facet_testhelpers::setup();

        let mut array = VArray::new();
        array.push(Value::from(VNumber::from_i64(1)));
        array.push(Value::from(VString::new("two")));
        array.push(Value::TRUE);

        let mut object = VObject::new();
        object.insert("n", Value::from(VNumber::from_u64(42)));
        object.insert("s", Value::from(VString::new("hello")));
        object.insert("b", Value::from(VBytes::new(&[1, 2, 3])));
        object.insert("a", Value::from(array));

        let value = Value::from(object);
        let bytes = to_vec(&value).unwrap();
        let decoded: Value = crate::from_slice(&bytes).unwrap();

        assert_eq!(decoded, value);
    }
}
