use facet_core::{Def, Facet, StructKind, Type, UserType};
use facet_reflect::{HasFields, Peek, ScalarType};
use log::trace;
use std::io::{self, Write};

/// Serializes any Facet type to MessagePack bytes
pub fn to_vec<T: Facet<'static>>(value: &T) -> Vec<u8> {
    let mut buffer = Vec::new();
    to_writer(&mut buffer, value).unwrap();
    buffer
}

/// Serializes any Facet type to MessagePack bytes, writing to the given writer
pub fn to_writer<T: Facet<'static>, W: Write>(writer: &mut W, value: &T) -> io::Result<()> {
    let peek = Peek::new(value);
    serialize_value(peek, writer)
}

fn serialize_value<W: Write>(peek: Peek<'_, '_>, writer: &mut W) -> io::Result<()> {
    trace!("Serializing value, shape is {}", peek.shape());

    match (peek.shape().def, peek.shape().ty) {
        (Def::Scalar, _) => {
            let peek = peek.innermost_peek();
            serialize_scalar(peek, writer)
        }
        (Def::List(ld), _) => {
            // Special case for Vec<u8> - serialize as binary
            if ld.t().is_type::<u8>() && peek.shape().is_type::<Vec<u8>>() {
                let bytes = peek.get::<Vec<u8>>().unwrap();
                write_bin(writer, bytes)
            } else {
                let list = peek.into_list_like().unwrap();
                let items: Vec<_> = list.iter().collect();
                serialize_array(items, writer)
            }
        }
        (Def::Array(ad), _) => {
            if ad.t().is_type::<u8>() {
                // Collect bytes from array
                let bytes: Vec<u8> = peek
                    .into_list_like()
                    .unwrap()
                    .iter()
                    .map(|p| *p.get::<u8>().unwrap())
                    .collect();
                write_bin(writer, &bytes)
            } else {
                let list = peek.into_list_like().unwrap();
                let items: Vec<_> = list.iter().collect();
                serialize_array(items, writer)
            }
        }
        (Def::Slice(sd), _) => {
            if sd.t().is_type::<u8>() {
                let bytes = peek.get::<[u8]>().unwrap();
                write_bin(writer, bytes)
            } else {
                let list = peek.into_list_like().unwrap();
                let items: Vec<_> = list.iter().collect();
                serialize_array(items, writer)
            }
        }
        (Def::Map(_), _) => {
            let map = peek.into_map().unwrap();
            let entries: Vec<_> = map.iter().collect();
            write_map_len(writer, entries.len())?;
            for (key, value) in entries {
                serialize_value(key, writer)?;
                serialize_value(value, writer)?;
            }
            Ok(())
        }
        (Def::Set(_), _) => {
            let set = peek.into_set().unwrap();
            let items: Vec<_> = set.iter().collect();
            serialize_array(items, writer)
        }
        (Def::Option(_), _) => {
            let opt = peek.into_option().unwrap();
            if let Some(inner) = opt.value() {
                serialize_value(inner, writer)
            } else {
                write_nil(writer)
            }
        }
        (Def::Pointer(_), _) => {
            let ptr = peek.into_pointer().unwrap();
            if let Some(inner) = ptr.borrow_inner() {
                serialize_value(inner, writer)
            } else {
                Err(io::Error::other(
                    "Smart pointer without borrow support cannot be serialized",
                ))
            }
        }
        (_, Type::User(UserType::Struct(sd))) => {
            match sd.kind {
                StructKind::Unit => {
                    // Unit structs serialize as nil
                    write_nil(writer)
                }
                StructKind::Tuple => {
                    let ps = peek.into_struct().unwrap();
                    let fields: Vec<_> = ps.fields().map(|(_, v)| v).collect();
                    if fields.is_empty() {
                        // Empty tuple (unit type) -> nil for rmp_serde compatibility
                        write_nil(writer)
                    } else {
                        write_array_len(writer, fields.len())?;
                        for field_value in fields {
                            serialize_value(field_value, writer)?;
                        }
                        Ok(())
                    }
                }
                StructKind::TupleStruct => {
                    let ps = peek.into_struct().unwrap();
                    let fields: Vec<_> = ps.fields_for_serialize().collect();
                    write_array_len(writer, fields.len())?;
                    for (_, field_value) in fields {
                        serialize_value(field_value, writer)?;
                    }
                    Ok(())
                }
                StructKind::Struct => {
                    let ps = peek.into_struct().unwrap();
                    let fields: Vec<_> = ps.fields_for_serialize().collect();
                    write_map_len(writer, fields.len())?;
                    for (field, field_value) in fields {
                        write_str(writer, field.name)?;
                        serialize_value(field_value, writer)?;
                    }
                    Ok(())
                }
            }
        }
        (_, Type::User(UserType::Enum(_))) => {
            let pe = peek.into_enum().unwrap();
            let variant = pe.active_variant().expect("Failed to get active variant");
            trace!("Serializing enum variant: {}", variant.name);

            if variant.data.fields.is_empty() {
                // Unit variant - just the name as a string
                write_str(writer, variant.name)
            } else if variant.data.kind == StructKind::Tuple && variant.data.fields.len() == 1 {
                // Newtype variant - serialize as {"VariantName": inner_value}
                write_map_len(writer, 1)?;
                write_str(writer, variant.name)?;
                let fields: Vec<_> = pe.fields_for_serialize().collect();
                serialize_value(fields[0].1, writer)
            } else if variant.data.kind == StructKind::Tuple
                || variant.data.kind == StructKind::TupleStruct
            {
                // Tuple variant - serialize as {"VariantName": [values...]}
                write_map_len(writer, 1)?;
                write_str(writer, variant.name)?;
                let fields: Vec<_> = pe.fields_for_serialize().collect();
                write_array_len(writer, fields.len())?;
                for (_, field_value) in fields {
                    serialize_value(field_value, writer)?;
                }
                Ok(())
            } else {
                // Struct variant - serialize as {"VariantName": {fields...}}
                write_map_len(writer, 1)?;
                write_str(writer, variant.name)?;
                let fields: Vec<_> = pe.fields_for_serialize().collect();
                write_map_len(writer, fields.len())?;
                for (field, field_value) in fields {
                    write_str(writer, field.name)?;
                    serialize_value(field_value, writer)?;
                }
                Ok(())
            }
        }
        (_, Type::Pointer(_)) => {
            // Handle string types
            if let Some(s) = peek.as_str() {
                write_str(writer, s)
            } else if let Some(bytes) = peek.as_bytes() {
                write_bin(writer, bytes)
            } else {
                let innermost = peek.innermost_peek();
                if innermost.shape() != peek.shape() {
                    serialize_value(innermost, writer)
                } else {
                    write_nil(writer)
                }
            }
        }
        _ => {
            trace!("Unhandled type: {:?}, serializing as nil", peek.shape().ty);
            write_nil(writer)
        }
    }
}

fn serialize_scalar<W: Write>(peek: Peek<'_, '_>, writer: &mut W) -> io::Result<()> {
    match peek.scalar_type() {
        Some(ScalarType::Unit) => write_nil(writer),
        Some(ScalarType::Bool) => {
            let v = *peek.get::<bool>().unwrap();
            write_bool(writer, v)
        }
        Some(ScalarType::Char) => {
            let c = *peek.get::<char>().unwrap();
            let mut buf = [0; 4];
            write_str(writer, c.encode_utf8(&mut buf))
        }
        Some(ScalarType::Str) => write_str(writer, peek.get::<str>().unwrap()),
        Some(ScalarType::String) => write_str(writer, peek.get::<String>().unwrap()),
        Some(ScalarType::CowStr) => {
            write_str(writer, peek.get::<std::borrow::Cow<'_, str>>().unwrap())
        }
        Some(ScalarType::F32) => {
            let v = *peek.get::<f32>().unwrap();
            write_f32(writer, v)
        }
        Some(ScalarType::F64) => {
            let v = *peek.get::<f64>().unwrap();
            write_f64(writer, v)
        }
        Some(ScalarType::U8) => {
            let v = *peek.get::<u8>().unwrap();
            write_u8(writer, v)
        }
        Some(ScalarType::U16) => {
            let v = *peek.get::<u16>().unwrap();
            write_u16(writer, v)
        }
        Some(ScalarType::U32) => {
            let v = *peek.get::<u32>().unwrap();
            write_u32(writer, v)
        }
        Some(ScalarType::U64) => {
            let v = *peek.get::<u64>().unwrap();
            write_u64(writer, v)
        }
        Some(ScalarType::U128) => Err(io::Error::other(
            "u128 is not directly supported by MessagePack",
        )),
        Some(ScalarType::USize) => {
            let v = *peek.get::<usize>().unwrap();
            write_u64(writer, v as u64)
        }
        Some(ScalarType::I8) => {
            let v = *peek.get::<i8>().unwrap();
            write_i8(writer, v)
        }
        Some(ScalarType::I16) => {
            let v = *peek.get::<i16>().unwrap();
            write_i16(writer, v)
        }
        Some(ScalarType::I32) => {
            let v = *peek.get::<i32>().unwrap();
            write_i32(writer, v)
        }
        Some(ScalarType::I64) => {
            let v = *peek.get::<i64>().unwrap();
            write_i64(writer, v)
        }
        Some(ScalarType::I128) => Err(io::Error::other(
            "i128 is not directly supported by MessagePack",
        )),
        Some(ScalarType::ISize) => {
            let v = *peek.get::<isize>().unwrap();
            write_i64(writer, v as i64)
        }
        Some(other) => Err(io::Error::other(format!(
            "Unsupported scalar type: {other:?}"
        ))),
        None => Err(io::Error::other(format!(
            "Unknown scalar shape: {}",
            peek.shape()
        ))),
    }
}

fn serialize_array<W: Write>(items: Vec<Peek<'_, '_>>, writer: &mut W) -> io::Result<()> {
    if items.is_empty() {
        // Empty arrays serialize as nil for rmp_serde compatibility
        write_nil(writer)
    } else {
        write_array_len(writer, items.len())?;
        for item in items {
            serialize_value(item, writer)?;
        }
        Ok(())
    }
}

// --- MessagePack encoding functions ---

fn write_nil<W: Write>(writer: &mut W) -> io::Result<()> {
    writer.write_all(&[0xc0])
}

fn write_bool<W: Write>(writer: &mut W, val: bool) -> io::Result<()> {
    if val {
        writer.write_all(&[0xc3]) // true
    } else {
        writer.write_all(&[0xc2]) // false
    }
}

fn write_f32<W: Write>(writer: &mut W, n: f32) -> io::Result<()> {
    writer.write_all(&[0xca])?; // float 32
    writer.write_all(&n.to_be_bytes())
}

fn write_f64<W: Write>(writer: &mut W, n: f64) -> io::Result<()> {
    writer.write_all(&[0xcb])?; // float 64
    writer.write_all(&n.to_be_bytes())
}

fn write_bin<W: Write>(writer: &mut W, bytes: &[u8]) -> io::Result<()> {
    let len = bytes.len();
    match len {
        0..=255 => {
            // bin 8
            writer.write_all(&[0xc4, len as u8])?;
        }
        256..=65535 => {
            // bin 16
            writer.write_all(&[0xc5])?;
            writer.write_all(&(len as u16).to_be_bytes())?;
        }
        _ => {
            // bin 32
            writer.write_all(&[0xc6])?;
            writer.write_all(&(len as u32).to_be_bytes())?;
        }
    }
    writer.write_all(bytes)
}

fn write_array_len<W: Write>(writer: &mut W, len: usize) -> io::Result<()> {
    match len {
        0..=15 => {
            // fixarray
            writer.write_all(&[(0x90 | len as u8)])
        }
        16..=65535 => {
            // array 16
            writer.write_all(&[0xdc])?;
            writer.write_all(&(len as u16).to_be_bytes())
        }
        _ => {
            // array 32
            writer.write_all(&[0xdd])?;
            writer.write_all(&(len as u32).to_be_bytes())
        }
    }
}

fn write_str<W: Write>(writer: &mut W, s: &str) -> io::Result<()> {
    let bytes = s.as_bytes();
    let len = bytes.len();

    match len {
        0..=31 => {
            // fixstr
            writer.write_all(&[(0xa0 | len as u8)])?;
        }
        32..=255 => {
            // str8
            writer.write_all(&[0xd9, len as u8])?;
        }
        256..=65535 => {
            // str16
            writer.write_all(&[0xda])?;
            writer.write_all(&(len as u16).to_be_bytes())?;
        }
        _ => {
            // str32
            writer.write_all(&[0xdb])?;
            writer.write_all(&(len as u32).to_be_bytes())?;
        }
    }
    writer.write_all(bytes)
}

fn write_u8<W: Write>(writer: &mut W, n: u8) -> io::Result<()> {
    match n {
        0..=127 => {
            // positive fixint
            writer.write_all(&[n])
        }
        _ => {
            // uint8
            writer.write_all(&[0xcc, n])
        }
    }
}

fn write_u16<W: Write>(writer: &mut W, n: u16) -> io::Result<()> {
    match n {
        0..=127 => {
            // positive fixint
            writer.write_all(&[n as u8])
        }
        128..=255 => {
            // uint8
            writer.write_all(&[0xcc, n as u8])
        }
        _ => {
            // uint16
            writer.write_all(&[0xcd])?;
            writer.write_all(&n.to_be_bytes())
        }
    }
}

fn write_u32<W: Write>(writer: &mut W, n: u32) -> io::Result<()> {
    match n {
        0..=127 => {
            // positive fixint
            writer.write_all(&[n as u8])
        }
        128..=255 => {
            // uint8
            writer.write_all(&[0xcc, n as u8])
        }
        256..=65535 => {
            // uint16
            writer.write_all(&[0xcd])?;
            writer.write_all(&(n as u16).to_be_bytes())
        }
        _ => {
            // uint32
            writer.write_all(&[0xce])?;
            writer.write_all(&n.to_be_bytes())
        }
    }
}

fn write_u64<W: Write>(writer: &mut W, n: u64) -> io::Result<()> {
    match n {
        0..=127 => {
            // positive fixint
            writer.write_all(&[n as u8])
        }
        128..=255 => {
            // uint8
            writer.write_all(&[0xcc, n as u8])
        }
        256..=65535 => {
            // uint16
            writer.write_all(&[0xcd])?;
            writer.write_all(&(n as u16).to_be_bytes())
        }
        65536..=4294967295 => {
            // uint32
            writer.write_all(&[0xce])?;
            writer.write_all(&(n as u32).to_be_bytes())
        }
        _ => {
            // uint64
            writer.write_all(&[0xcf])?;
            writer.write_all(&n.to_be_bytes())
        }
    }
}

fn write_i8<W: Write>(writer: &mut W, n: i8) -> io::Result<()> {
    match n {
        -32..=-1 => {
            // negative fixint
            writer.write_all(&[n as u8])
        }
        -128..=-33 => {
            // int8
            writer.write_all(&[0xd0, n as u8])
        }
        0..=127 => {
            // positive fixint or uint8
            write_u8(writer, n as u8)
        }
    }
}

fn write_i16<W: Write>(writer: &mut W, n: i16) -> io::Result<()> {
    match n {
        -32..=-1 => {
            // negative fixint
            writer.write_all(&[n as u8])
        }
        -128..=-33 => {
            // int8
            writer.write_all(&[0xd0, n as u8])
        }
        -32768..=-129 => {
            // int16
            writer.write_all(&[0xd1])?;
            writer.write_all(&n.to_be_bytes())
        }
        0..=32767 => {
            // Use unsigned logic for positive range
            write_u16(writer, n as u16)
        }
    }
}

fn write_i32<W: Write>(writer: &mut W, n: i32) -> io::Result<()> {
    match n {
        -32..=-1 => {
            // negative fixint
            writer.write_all(&[n as u8])
        }
        -128..=-33 => {
            // int8
            writer.write_all(&[0xd0, n as u8])
        }
        -32768..=-129 => {
            // int16
            writer.write_all(&[0xd1])?;
            writer.write_all(&(n as i16).to_be_bytes())
        }
        -2147483648..=-32769 => {
            // int32
            writer.write_all(&[0xd2])?;
            writer.write_all(&n.to_be_bytes())
        }
        0..=2147483647 => {
            // Use unsigned logic for positive range
            write_u32(writer, n as u32)
        }
    }
}

fn write_i64<W: Write>(writer: &mut W, n: i64) -> io::Result<()> {
    match n {
        -32..=-1 => {
            // negative fixint
            writer.write_all(&[n as u8])
        }
        -128..=-33 => {
            // int8
            writer.write_all(&[0xd0, n as u8])
        }
        -32768..=-129 => {
            // int16
            writer.write_all(&[0xd1])?;
            writer.write_all(&(n as i16).to_be_bytes())
        }
        -2147483648..=-32769 => {
            // int32
            writer.write_all(&[0xd2])?;
            writer.write_all(&(n as i32).to_be_bytes())
        }
        i64::MIN..=-2147483649 => {
            // int64
            writer.write_all(&[0xd3])?;
            writer.write_all(&n.to_be_bytes())
        }
        0..=i64::MAX => {
            // Use unsigned logic for positive range
            write_u64(writer, n as u64)
        }
    }
}

fn write_map_len<W: Write>(writer: &mut W, len: usize) -> io::Result<()> {
    match len {
        0..=15 => {
            // fixmap
            writer.write_all(&[(0x80 | len as u8)])
        }
        16..=65535 => {
            // map16
            writer.write_all(&[0xde])?;
            writer.write_all(&(len as u16).to_be_bytes())
        }
        _ => {
            // map32
            writer.write_all(&[0xdf])?;
            writer.write_all(&(len as u32).to_be_bytes())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use facet::Facet;
    use serde::Serialize;

    // Helper function to serialize with rmp_serde
    fn rmp_serialize<T: Serialize>(value: &T) -> Vec<u8> {
        // Configure rmp_serde to serialize structs as maps
        let mut buf = Vec::new();
        let mut ser = rmp_serde::Serializer::new(&mut buf)
            .with_bytes(rmp_serde::config::BytesMode::ForceIterables)
            .with_struct_map();
        value.serialize(&mut ser).unwrap();
        buf
    }

    #[derive(Facet, Serialize, PartialEq, Debug)]
    struct SimpleStruct {
        a: u32,
        b: String,
        c: bool,
    }

    #[test]
    fn test_simple_struct() {
        let value = SimpleStruct {
            a: 123,
            b: "hello".to_string(),
            c: true,
        };

        let facet_bytes = to_vec(&value);
        let rmp_bytes = rmp_serialize(&value);

        assert_eq!(facet_bytes, rmp_bytes);
    }

    #[derive(Facet, Serialize, PartialEq, Debug)]
    struct NestedStruct {
        inner: SimpleStruct,
        d: Option<i8>,
        e: Vec<u8>,
    }

    #[test]
    fn test_nested_struct() {
        let value = NestedStruct {
            inner: SimpleStruct {
                a: 456,
                b: "world".to_string(),
                c: false,
            },
            d: Some(-5),
            e: vec![1, 2, 3, 4, 5],
        };

        let facet_bytes = to_vec(&value);
        let rmp_bytes = rmp_serialize(&value);

        assert_eq!(facet_bytes, rmp_bytes);
    }

    #[test]
    fn test_nested_struct_none() {
        let value = NestedStruct {
            inner: SimpleStruct {
                a: 789,
                b: "another".to_string(),
                c: true,
            },
            d: None,
            e: vec![0], // rmp can't serialize empty bin8 correctly
        };

        let facet_bytes = to_vec(&value);
        let rmp_bytes = rmp_serialize(&value);

        assert_eq!(facet_bytes, rmp_bytes);
    }

    #[derive(Facet, Serialize, PartialEq, Debug)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum TestEnum {
        Unit,
        Tuple(u32, String),
        Struct { name: String, value: i64 },
    }

    #[test]
    fn test_enum_unit() {
        let value = TestEnum::Unit;
        let facet_bytes = to_vec(&value);
        // rmp-serde serializes unit variants as just the string name
        let rmp_bytes = rmp_serialize(&"Unit");
        assert_eq!(facet_bytes, rmp_bytes);
    }

    #[test]
    fn test_f32() {
        #[derive(Facet, Serialize, PartialEq, Debug)]
        struct FloatStruct {
            value: f32,
        }

        let value = FloatStruct { value: 1.23 };
        let facet_bytes = to_vec(&value);
        let rmp_bytes = rmp_serialize(&value);
        assert_eq!(facet_bytes, rmp_bytes);
    }

    #[test]
    fn test_f64() {
        #[derive(Facet, Serialize, PartialEq, Debug)]
        struct DoubleStruct {
            value: f64,
        }

        let value = DoubleStruct { value: -4.56e7 };
        let facet_bytes = to_vec(&value);
        let rmp_bytes = rmp_serialize(&value);
        assert_eq!(facet_bytes, rmp_bytes);
    }

    #[test]
    fn test_i8() {
        #[derive(Facet, Serialize, PartialEq, Debug)]
        struct I8Struct {
            value: i8,
        }

        let value = I8Struct { value: -10 };
        let facet_bytes = to_vec(&value);
        let rmp_bytes = rmp_serialize(&value);
        assert_eq!(facet_bytes, rmp_bytes);
    }

    #[test]
    fn test_i16() {
        #[derive(Facet, Serialize, PartialEq, Debug)]
        struct I16Struct {
            value: i16,
        }

        let value = I16Struct { value: -1000 };
        let facet_bytes = to_vec(&value);
        let rmp_bytes = rmp_serialize(&value);
        assert_eq!(facet_bytes, rmp_bytes);
    }

    #[test]
    fn test_i32() {
        #[derive(Facet, Serialize, PartialEq, Debug)]
        struct I32Struct {
            value: i32,
        }

        let value = I32Struct { value: -100000 };
        let facet_bytes = to_vec(&value);
        let rmp_bytes = rmp_serialize(&value);
        assert_eq!(facet_bytes, rmp_bytes);
    }

    #[test]
    fn test_i64() {
        #[derive(Facet, Serialize, PartialEq, Debug)]
        struct I64Struct {
            value: i64,
        }

        let value = I64Struct {
            value: -10000000000,
        };
        let facet_bytes = to_vec(&value);
        let rmp_bytes = rmp_serialize(&value);
        assert_eq!(facet_bytes, rmp_bytes);
    }

    #[test]
    fn test_u8() {
        #[derive(Facet, Serialize, PartialEq, Debug)]
        struct U8Struct {
            value: u8,
        }

        let value = U8Struct { value: 10 };
        let facet_bytes = to_vec(&value);
        let rmp_bytes = rmp_serialize(&value);
        assert_eq!(facet_bytes, rmp_bytes);
    }

    #[test]
    fn test_u16() {
        #[derive(Facet, Serialize, PartialEq, Debug)]
        struct U16Struct {
            value: u16,
        }

        let value = U16Struct { value: 1000 };
        let facet_bytes = to_vec(&value);
        let rmp_bytes = rmp_serialize(&value);
        assert_eq!(facet_bytes, rmp_bytes);
    }

    #[test]
    fn test_u32() {
        #[derive(Facet, Serialize, PartialEq, Debug)]
        struct U32Struct {
            value: u32,
        }

        let value = U32Struct { value: 100000 };
        let facet_bytes = to_vec(&value);
        let rmp_bytes = rmp_serialize(&value);
        assert_eq!(facet_bytes, rmp_bytes);
    }

    #[test]
    fn test_u64() {
        #[derive(Facet, Serialize, PartialEq, Debug)]
        struct U64Struct {
            value: u64,
        }

        let value = U64Struct { value: 10000000000 };
        let facet_bytes = to_vec(&value);
        let rmp_bytes = rmp_serialize(&value);
        assert_eq!(facet_bytes, rmp_bytes);
    }

    #[test]
    fn test_bytes() {
        #[derive(Facet, Serialize, PartialEq, Debug)]
        struct BytesStruct {
            value: Vec<u8>,
        }

        let value = BytesStruct {
            value: b"binary data".to_vec(),
        };
        let facet_bytes = to_vec(&value);
        let rmp_bytes = rmp_serialize(&value);
        assert_eq!(facet_bytes, rmp_bytes);
    }

    #[test]
    fn test_string() {
        #[derive(Facet, Serialize, PartialEq, Debug)]
        struct StringStruct {
            value: String,
        }

        let value = StringStruct {
            value: "string data".to_string(),
        };
        let facet_bytes = to_vec(&value);
        let rmp_bytes = rmp_serialize(&value);
        assert_eq!(facet_bytes, rmp_bytes);
    }

    #[test]
    fn test_char() {
        #[derive(Facet, Serialize, PartialEq, Debug)]
        struct CharStruct {
            value: char,
        }

        let value = CharStruct { value: '✅' };
        let facet_bytes = to_vec(&value);
        let rmp_bytes = rmp_serialize(&value);
        assert_eq!(facet_bytes, rmp_bytes);
    }

    #[test]
    fn test_option_some() {
        #[derive(Facet, Serialize, PartialEq, Debug)]
        struct OptionSomeStruct {
            value: Option<i32>,
        }

        let value = OptionSomeStruct { value: Some(99) };
        let facet_bytes = to_vec(&value);
        let rmp_bytes = rmp_serialize(&value);
        assert_eq!(facet_bytes, rmp_bytes);
    }

    #[test]
    fn test_option_none() {
        #[derive(Facet, Serialize, PartialEq, Debug)]
        struct OptionNoneStruct {
            value: Option<String>,
        }

        let value = OptionNoneStruct { value: None };
        let facet_bytes = to_vec(&value);
        let rmp_bytes = rmp_serialize(&value);
        assert_eq!(facet_bytes, rmp_bytes);
    }

    #[test]
    fn test_unit() {
        #[derive(Facet, Serialize, PartialEq, Debug)]
        struct UnitStruct {
            value: (),
        }

        let value = UnitStruct { value: () };
        let facet_bytes = to_vec(&value);
        let rmp_bytes = rmp_serialize(&value);
        assert_eq!(facet_bytes, rmp_bytes);
    }

    #[test]
    fn test_empty_vec() {
        #[derive(Facet, Serialize, PartialEq, Debug)]
        struct EmptyVecStruct {
            value: Vec<i32>,
        }

        let value = EmptyVecStruct { value: vec![] };
        let facet_bytes = to_vec(&value);

        // Empty collections are serialized as nil in facet-msgpack to maintain consistency
        // with how unit types are handled. This ensures uniform behavior for "empty" values.
        let expected = vec![0x81, 0xa5, b'v', b'a', b'l', b'u', b'e', 0xc0]; // map with "value" -> nil
        assert_eq!(facet_bytes, expected);
    }
}
