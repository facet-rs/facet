use crate::error::SerializeError;
use facet_core::{Def, Facet, StructKind, Type, UserType};
use facet_reflect::{HasFields, Peek, ScalarType};
use log::trace;

#[cfg(feature = "alloc")]
use alloc::{borrow::Cow, string::String, vec::Vec};

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
#[cfg(feature = "alloc")]
pub fn to_vec<T: Facet<'static>>(value: &T) -> Result<Vec<u8>, SerializeError> {
    let mut buffer = Vec::new();
    let peek = Peek::new(value);
    serialize_value(peek, &mut buffer)?;
    Ok(buffer)
}

/// Serializes any Facet type to a provided byte slice.
///
/// Returns the number of bytes written.
///
/// # Example
/// ```
/// use facet::Facet;
/// use facet_postcard::to_slice;
///
/// #[derive(Debug, Facet)]
/// struct Point {
///     x: i32,
///     y: i32,
/// }
///
/// let point = Point { x: 10, y: 20 };
/// let mut buffer = [0u8; 64];
/// let len = to_slice(&point, &mut buffer).unwrap();
/// let bytes = &buffer[..len];
/// ```
pub fn to_slice<T: Facet<'static>>(value: &T, buffer: &mut [u8]) -> Result<usize, SerializeError> {
    let peek = Peek::new(value);
    let mut writer = SliceWriter::new(buffer);
    serialize_value(peek, &mut writer)?;
    Ok(writer.pos)
}

/// A trait for writing bytes during serialization
trait Writer {
    fn write_byte(&mut self, byte: u8) -> Result<(), SerializeError>;
    fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), SerializeError>;
}

#[cfg(feature = "alloc")]
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

struct SliceWriter<'a> {
    buffer: &'a mut [u8],
    pos: usize,
}

impl<'a> SliceWriter<'a> {
    fn new(buffer: &'a mut [u8]) -> Self {
        Self { buffer, pos: 0 }
    }
}

impl Writer for SliceWriter<'_> {
    fn write_byte(&mut self, byte: u8) -> Result<(), SerializeError> {
        if self.pos >= self.buffer.len() {
            return Err(SerializeError::BufferTooSmall);
        }
        self.buffer[self.pos] = byte;
        self.pos += 1;
        Ok(())
    }

    fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), SerializeError> {
        if self.pos + bytes.len() > self.buffer.len() {
            return Err(SerializeError::BufferTooSmall);
        }
        self.buffer[self.pos..self.pos + bytes.len()].copy_from_slice(bytes);
        self.pos += bytes.len();
        Ok(())
    }
}

fn serialize_value<W: Writer>(peek: Peek<'_, '_>, writer: &mut W) -> Result<(), SerializeError> {
    trace!("Serializing value, shape is {}", peek.shape());

    match (peek.shape().def, peek.shape().ty) {
        (Def::Scalar, _) => {
            let peek = peek.innermost_peek();
            serialize_scalar(peek, writer)
        }
        (Def::List(ld), _) => {
            // Special case for Vec<u8> - serialize as bytes
            if ld.t().is_type::<u8>() && peek.shape().is_type::<Vec<u8>>() {
                let bytes = peek.get::<Vec<u8>>().unwrap();
                write_varint(bytes.len() as u64, writer)?;
                writer.write_bytes(bytes)
            } else {
                let list = peek.into_list_like().unwrap();
                let items: Vec<_> = list.iter().collect();
                write_varint(items.len() as u64, writer)?;
                for item in items {
                    serialize_value(item, writer)?;
                }
                Ok(())
            }
        }
        (Def::Array(ad), _) => {
            if ad.t().is_type::<u8>() {
                // Serialize byte arrays directly (length already known from type)
                let bytes: Vec<u8> = peek
                    .into_list_like()
                    .unwrap()
                    .iter()
                    .map(|p| *p.get::<u8>().unwrap())
                    .collect();
                writer.write_bytes(&bytes)
            } else {
                // For fixed-size arrays, postcard doesn't write length
                let list = peek.into_list_like().unwrap();
                for item in list.iter() {
                    serialize_value(item, writer)?;
                }
                Ok(())
            }
        }
        (Def::Slice(sd), _) => {
            if sd.t().is_type::<u8>() {
                let bytes = peek.get::<[u8]>().unwrap();
                write_varint(bytes.len() as u64, writer)?;
                writer.write_bytes(bytes)
            } else {
                let list = peek.into_list_like().unwrap();
                let items: Vec<_> = list.iter().collect();
                write_varint(items.len() as u64, writer)?;
                for item in items {
                    serialize_value(item, writer)?;
                }
                Ok(())
            }
        }
        (Def::Map(_), _) => {
            let map = peek.into_map().unwrap();
            let entries: Vec<_> = map.iter().collect();
            write_varint(entries.len() as u64, writer)?;
            for (key, value) in entries {
                serialize_value(key, writer)?;
                serialize_value(value, writer)?;
            }
            Ok(())
        }
        (Def::Set(_), _) => {
            let set = peek.into_set().unwrap();
            let items: Vec<_> = set.iter().collect();
            write_varint(items.len() as u64, writer)?;
            for item in items {
                serialize_value(item, writer)?;
            }
            Ok(())
        }
        (Def::Option(_), _) => {
            let opt = peek.into_option().unwrap();
            if let Some(inner) = opt.value() {
                writer.write_byte(1)?; // Some
                serialize_value(inner, writer)
            } else {
                writer.write_byte(0) // None
            }
        }
        (Def::Pointer(_), _) => {
            let ptr = peek.into_pointer().unwrap();
            if let Some(inner) = ptr.borrow_inner() {
                serialize_value(inner, writer)
            } else {
                Err(SerializeError::UnsupportedType(
                    "Smart pointer without borrow support",
                ))
            }
        }
        (_, Type::User(UserType::Struct(sd))) => {
            match sd.kind {
                StructKind::Unit => {
                    // Unit structs serialize as nothing
                    Ok(())
                }
                StructKind::Tuple => {
                    let ps = peek.into_struct().unwrap();
                    for (_, field_value) in ps.fields() {
                        serialize_value(field_value, writer)?;
                    }
                    Ok(())
                }
                StructKind::TupleStruct => {
                    let ps = peek.into_struct().unwrap();
                    for (_, field_value) in ps.fields_for_serialize() {
                        serialize_value(field_value, writer)?;
                    }
                    Ok(())
                }
                StructKind::Struct => {
                    // Postcard serializes structs in field order without names
                    let ps = peek.into_struct().unwrap();
                    for (_, field_value) in ps.fields_for_serialize() {
                        serialize_value(field_value, writer)?;
                    }
                    Ok(())
                }
            }
        }
        (_, Type::User(UserType::Enum(et))) => {
            let pe = peek.into_enum().unwrap();
            let variant = pe.active_variant().expect("Failed to get active variant");
            let variant_idx = et
                .variants
                .iter()
                .position(|v| v.name == variant.name)
                .unwrap_or(0);
            trace!(
                "Serializing enum variant {} at index {}",
                variant.name, variant_idx
            );

            // Write variant index as varint
            write_varint(variant_idx as u64, writer)?;

            if variant.data.fields.is_empty() {
                // Unit variant - nothing more to write
                Ok(())
            } else if variant.data.kind == StructKind::Tuple
                || variant.data.kind == StructKind::TupleStruct
            {
                // Tuple variant - serialize fields in order
                for (_, field_value) in pe.fields_for_serialize() {
                    serialize_value(field_value, writer)?;
                }
                Ok(())
            } else {
                // Struct variant - serialize fields in order (no names)
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
                    Err(SerializeError::UnsupportedType("Unknown pointer type"))
                }
            }
        }
        _ => {
            trace!("Unhandled type: {:?}", peek.shape().ty);
            Err(SerializeError::UnsupportedType("Unknown type"))
        }
    }
}

fn serialize_scalar<W: Writer>(peek: Peek<'_, '_>, writer: &mut W) -> Result<(), SerializeError> {
    match peek.scalar_type() {
        Some(ScalarType::Unit) => Ok(()),
        Some(ScalarType::Bool) => {
            let v = *peek.get::<bool>().unwrap();
            writer.write_byte(if v { 1 } else { 0 })
        }
        Some(ScalarType::Char) => {
            let c = *peek.get::<char>().unwrap();
            let mut buf = [0; 4];
            let s = c.encode_utf8(&mut buf);
            write_varint(s.len() as u64, writer)?;
            writer.write_bytes(s.as_bytes())
        }
        Some(ScalarType::Str) => {
            let s = peek.get::<str>().unwrap();
            write_varint(s.len() as u64, writer)?;
            writer.write_bytes(s.as_bytes())
        }
        Some(ScalarType::String) => {
            let s = peek.get::<String>().unwrap();
            write_varint(s.len() as u64, writer)?;
            writer.write_bytes(s.as_bytes())
        }
        Some(ScalarType::CowStr) => {
            let s = peek.get::<Cow<'_, str>>().unwrap();
            write_varint(s.len() as u64, writer)?;
            writer.write_bytes(s.as_bytes())
        }
        Some(ScalarType::F32) => {
            let v = *peek.get::<f32>().unwrap();
            writer.write_bytes(&v.to_le_bytes())
        }
        Some(ScalarType::F64) => {
            let v = *peek.get::<f64>().unwrap();
            writer.write_bytes(&v.to_le_bytes())
        }
        Some(ScalarType::U8) => {
            let v = *peek.get::<u8>().unwrap();
            writer.write_byte(v)
        }
        Some(ScalarType::U16) => {
            let v = *peek.get::<u16>().unwrap();
            write_varint(v as u64, writer)
        }
        Some(ScalarType::U32) => {
            let v = *peek.get::<u32>().unwrap();
            write_varint(v as u64, writer)
        }
        Some(ScalarType::U64) => {
            let v = *peek.get::<u64>().unwrap();
            write_varint(v, writer)
        }
        Some(ScalarType::U128) => {
            let v = *peek.get::<u128>().unwrap();
            write_varint_u128(v, writer)
        }
        Some(ScalarType::USize) => {
            let v = *peek.get::<usize>().unwrap();
            write_varint(v as u64, writer)
        }
        Some(ScalarType::I8) => {
            let v = *peek.get::<i8>().unwrap();
            writer.write_byte(v as u8)
        }
        Some(ScalarType::I16) => {
            let v = *peek.get::<i16>().unwrap();
            write_varint_signed(v as i64, writer)
        }
        Some(ScalarType::I32) => {
            let v = *peek.get::<i32>().unwrap();
            write_varint_signed(v as i64, writer)
        }
        Some(ScalarType::I64) => {
            let v = *peek.get::<i64>().unwrap();
            write_varint_signed(v, writer)
        }
        Some(ScalarType::I128) => {
            let v = *peek.get::<i128>().unwrap();
            write_varint_signed_i128(v, writer)
        }
        Some(ScalarType::ISize) => {
            let v = *peek.get::<isize>().unwrap();
            write_varint_signed(v as i64, writer)
        }
        Some(_) => Err(SerializeError::UnsupportedType("unsupported scalar")),
        None => Err(SerializeError::UnsupportedType("Unknown scalar")),
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
        b: String,
        c: bool,
    }

    #[test]
    fn test_simple_struct() {
        facet_testhelpers::setup();

        let value = SimpleStruct {
            a: 123,
            b: "hello".to_string(),
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
    fn test_u16() {
        facet_testhelpers::setup();

        #[derive(Facet, Serialize, PartialEq, Debug)]
        struct U16Struct {
            value: u16,
        }

        let value = U16Struct { value: 1000 };
        let facet_bytes = to_vec(&value).unwrap();
        let postcard_bytes = postcard_to_vec(&value).unwrap();
        assert_eq!(facet_bytes, postcard_bytes);
    }

    #[test]
    fn test_u32() {
        facet_testhelpers::setup();

        #[derive(Facet, Serialize, PartialEq, Debug)]
        struct U32Struct {
            value: u32,
        }

        let value = U32Struct { value: 100000 };
        let facet_bytes = to_vec(&value).unwrap();
        let postcard_bytes = postcard_to_vec(&value).unwrap();
        assert_eq!(facet_bytes, postcard_bytes);
    }

    #[test]
    fn test_bool() {
        facet_testhelpers::setup();

        #[derive(Facet, Serialize, PartialEq, Debug)]
        struct BoolStruct {
            t: bool,
            f: bool,
        }

        let value = BoolStruct { t: true, f: false };
        let facet_bytes = to_vec(&value).unwrap();
        let postcard_bytes = postcard_to_vec(&value).unwrap();
        assert_eq!(facet_bytes, postcard_bytes);
    }

    #[test]
    fn test_string() {
        facet_testhelpers::setup();

        #[derive(Facet, Serialize, PartialEq, Debug)]
        struct StringStruct {
            value: String,
        }

        let value = StringStruct {
            value: "hello world".to_string(),
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
    fn test_vec() {
        facet_testhelpers::setup();

        #[derive(Facet, Serialize, PartialEq, Debug)]
        struct VecStruct {
            values: Vec<u32>,
        }

        let value = VecStruct {
            values: vec![1, 2, 3, 4, 5],
        };
        let facet_bytes = to_vec(&value).unwrap();
        let postcard_bytes = postcard_to_vec(&value).unwrap();
        assert_eq!(facet_bytes, postcard_bytes);
    }

    #[test]
    fn test_enum_unit() {
        facet_testhelpers::setup();

        #[derive(Facet, Serialize, PartialEq, Debug)]
        #[repr(u8)]
        #[allow(dead_code)]
        enum TestEnum {
            Unit,
            Other,
        }

        let value = TestEnum::Unit;
        let facet_bytes = to_vec(&value).unwrap();
        let postcard_bytes = postcard_to_vec(&value).unwrap();
        assert_eq!(facet_bytes, postcard_bytes);
    }

    #[test]
    fn test_enum_tuple() {
        facet_testhelpers::setup();

        #[derive(Facet, Serialize, PartialEq, Debug)]
        #[repr(u8)]
        #[allow(dead_code)]
        enum TestEnum {
            Unit,
            Tuple(u32, String),
        }

        let value = TestEnum::Tuple(42, "hello".to_string());
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

        // Test positive
        let value = I32Struct { value: 100 };
        let facet_bytes = to_vec(&value).unwrap();
        let postcard_bytes = postcard_to_vec(&value).unwrap();
        assert_eq!(facet_bytes, postcard_bytes);

        // Test negative
        let value = I32Struct { value: -100 };
        let facet_bytes = to_vec(&value).unwrap();
        let postcard_bytes = postcard_to_vec(&value).unwrap();
        assert_eq!(facet_bytes, postcard_bytes);
    }

    #[test]
    fn test_f32() {
        facet_testhelpers::setup();

        #[derive(Facet, Serialize, PartialEq, Debug)]
        struct F32Struct {
            value: f32,
        }

        let value = F32Struct { value: 1.5 };
        let facet_bytes = to_vec(&value).unwrap();
        let postcard_bytes = postcard_to_vec(&value).unwrap();
        assert_eq!(facet_bytes, postcard_bytes);
    }

    #[test]
    fn test_f64() {
        facet_testhelpers::setup();

        #[derive(Facet, Serialize, PartialEq, Debug)]
        struct F64Struct {
            value: f64,
        }

        let value = F64Struct {
            value: 1.23456789012345,
        };
        let facet_bytes = to_vec(&value).unwrap();
        let postcard_bytes = postcard_to_vec(&value).unwrap();
        assert_eq!(facet_bytes, postcard_bytes);
    }
}
