use facet_core::Facet;
use facet_reflect::Peek;
use facet_serialize::{Serializer, serialize_iterative}; // Import the necessary items from facet-serialize
use log::trace;
use std::io::{self, Write};

/// Serializes any Facet type to MessagePack bytes
pub fn to_vec<'a, T: Facet<'a>>(value: &'a T) -> Vec<u8> {
    let mut buffer = Vec::new();
    let peek = Peek::new(value);
    let mut serializer = MessagePackSerializer {
        writer: &mut buffer,
    }; // Create the serializer
    serialize_iterative(peek, &mut serializer).unwrap(); // Use the iterative serializer
    buffer
}

// Define the MessagePackSerializer struct
struct MessagePackSerializer<'w, W: Write> {
    writer: &'w mut W,
}

// Implement the Serializer trait for MessagePackSerializer
impl<W: Write> Serializer for MessagePackSerializer<'_, W> {
    type Error = io::Error; // Use io::Error as the error type

    // Implement all methods required by the Serializer trait
    // Most implementations will simply call the existing write_* helper functions.

    fn serialize_u8(&mut self, value: u8) -> Result<(), Self::Error> {
        trace!("Serializing u8: {}", value);
        write_u8(self.writer, value)
    }

    fn serialize_u16(&mut self, value: u16) -> Result<(), Self::Error> {
        trace!("Serializing u16: {}", value);
        write_u16(self.writer, value)
    }

    fn serialize_u32(&mut self, value: u32) -> Result<(), Self::Error> {
        trace!("Serializing u32: {}", value);
        write_u32(self.writer, value)
    }

    fn serialize_u64(&mut self, value: u64) -> Result<(), Self::Error> {
        trace!("Serializing u64: {}", value);
        write_u64(self.writer, value)
    }

    // TODO: Implement serialize_u128 if needed for MessagePack, otherwise return error or panic
    fn serialize_u128(&mut self, _value: u128) -> Result<(), Self::Error> {
        Err(io::Error::new(
            io::ErrorKind::Other,
            "u128 is not directly supported by MessagePack",
        ))
    }

    // Map usize to u64 as MessagePack doesn't have a specific usize type
    fn serialize_usize(&mut self, value: usize) -> Result<(), Self::Error> {
        trace!("Serializing usize: {}", value);
        write_u64(self.writer, value as u64) // Assuming usize fits in u64
    }

    fn serialize_i8(&mut self, value: i8) -> Result<(), Self::Error> {
        trace!("Serializing i8: {}", value);
        write_i8(self.writer, value)
    }

    fn serialize_i16(&mut self, value: i16) -> Result<(), Self::Error> {
        trace!("Serializing i16: {}", value);
        write_i16(self.writer, value)
    }

    fn serialize_i32(&mut self, value: i32) -> Result<(), Self::Error> {
        trace!("Serializing i32: {}", value);
        write_i32(self.writer, value)
    }

    fn serialize_i64(&mut self, value: i64) -> Result<(), Self::Error> {
        trace!("Serializing i64: {}", value);
        write_i64(self.writer, value)
    }

    // TODO: Implement serialize_i128 if needed for MessagePack, otherwise return error or panic
    fn serialize_i128(&mut self, _value: i128) -> Result<(), Self::Error> {
        Err(io::Error::new(
            io::ErrorKind::Other,
            "i128 is not directly supported by MessagePack",
        ))
    }

    // Map isize to i64 as MessagePack doesn't have a specific isize type
    fn serialize_isize(&mut self, value: isize) -> Result<(), Self::Error> {
        trace!("Serializing isize: {}", value);
        write_i64(self.writer, value as i64) // Assuming isize fits in i64
    }

    fn serialize_f32(&mut self, value: f32) -> Result<(), Self::Error> {
        trace!("Serializing f32: {}", value);
        write_f32(self.writer, value)
    }

    fn serialize_f64(&mut self, value: f64) -> Result<(), Self::Error> {
        trace!("Serializing f64: {}", value);
        write_f64(self.writer, value)
    }

    fn serialize_bool(&mut self, value: bool) -> Result<(), Self::Error> {
        trace!("Serializing bool: {}", value);
        write_bool(self.writer, value)
    }

    // Characters are often serialized as strings in MessagePack
    fn serialize_char(&mut self, value: char) -> Result<(), Self::Error> {
        trace!("Serializing char: {}", value);
        let mut buf = [0; 4];
        write_str(self.writer, value.encode_utf8(&mut buf))
    }

    fn serialize_str(&mut self, value: &str) -> Result<(), Self::Error> {
        trace!("Serializing str: {}", value);
        write_str(self.writer, value)
    }

    fn serialize_bytes(&mut self, value: &[u8]) -> Result<(), Self::Error> {
        trace!("Serializing bytes, len: {}", value.len());
        write_bin(self.writer, value)
    }

    fn serialize_none(&mut self) -> Result<(), Self::Error> {
        trace!("Serializing none");
        write_nil(self.writer)
    }

    fn serialize_unit(&mut self) -> Result<(), Self::Error> {
        trace!("Serializing unit");
        write_nil(self.writer) // Represent unit as nil
    }

    // Unit variants can be represented as strings or specific codes if needed.
    // Using string representation for now.
    fn serialize_unit_variant(
        &mut self,
        _variant_index: usize,
        variant_name: &'static str,
    ) -> Result<(), Self::Error> {
        trace!("Serializing unit variant: {}", variant_name);
        write_str(self.writer, variant_name)
    }

    fn start_object(&mut self, len: Option<usize>) -> Result<(), Self::Error> {
        trace!("Starting object, len: {:?}", len);
        if let Some(l) = len {
            write_map_len(self.writer, l)
        } else {
            // MessagePack doesn't have an indefinite length map marker.
            // This might require buffering or a different approach if the length is unknown.
            // For now, assume length is always known by `facet-serialize`.
            Err(io::Error::new(
                io::ErrorKind::Other,
                "MessagePack requires map length upfront",
            ))
        }
    }

    fn end_object(&mut self) -> Result<(), Self::Error> {
        trace!("Ending object");
        // No explicit end marker needed for fixed-length maps in MessagePack
        Ok(())
    }

    fn start_array(&mut self, len: Option<usize>) -> Result<(), Self::Error> {
        trace!("Starting array, len: {:?}", len);
        if let Some(l) = len {
            write_array_len(self.writer, l)
        } else {
            Err(io::Error::new(
                io::ErrorKind::Other,
                "MessagePack requires array length upfront",
            ))
        }
    }

    fn end_array(&mut self) -> Result<(), Self::Error> {
        trace!("Ending array");
        // No explicit end marker needed for fixed-length arrays in MessagePack
        Ok(())
    }

    // Maps in facet-serialize correspond to MessagePack maps
    fn start_map(&mut self, len: Option<usize>) -> Result<(), Self::Error> {
        trace!("Starting map, len: {:?}", len);
        if let Some(l) = len {
            write_map_len(self.writer, l)
        } else {
            Err(io::Error::new(
                io::ErrorKind::Other,
                "MessagePack requires map length upfront",
            ))
        }
    }

    fn end_map(&mut self) -> Result<(), Self::Error> {
        trace!("Ending map");
        // No explicit end marker needed for fixed-length maps in MessagePack
        Ok(())
    }

    // Field names are serialized as strings (keys) in MessagePack maps
    fn serialize_field_name(&mut self, name: &'static str) -> Result<(), Self::Error> {
        trace!("Serializing field name: {}", name);
        write_str(self.writer, name)
    }
}

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

// --- Existing write_* functions from the original file ---
// (write_str, write_u8, write_u16, write_u32, write_u64, write_i8, write_i16, write_i32, write_i64, write_map_len)
// These remain largely unchanged.

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
            write_u8(writer, n as u8) // Reuse u8 logic for positive values
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
    use serde::Serialize; // Import serde::Serialize

    // Helper function to serialize with rmp_serde
    fn rmp_serialize<T: Serialize>(value: &T) -> Vec<u8> {
        // Configure rmp_serde to serialize structs as maps
        let mut buf = Vec::new();
        let mut ser = rmp_serde::Serializer::new(&mut buf).with_struct_map();
        value.serialize(&mut ser).unwrap();
        buf
    }

    #[derive(Facet, Serialize, PartialEq, Debug)] // Add Serialize
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

    #[derive(Facet, Serialize, PartialEq, Debug)] // Add Serialize
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
            e: vec![],
        };

        let facet_bytes = to_vec(&value);
        let rmp_bytes = rmp_serialize(&value);

        assert_eq!(facet_bytes, rmp_bytes);
    }

    #[derive(Facet, Serialize, PartialEq, Debug)] // Add Serialize
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
    fn test_various_types() {
        #[derive(Facet, Serialize, PartialEq, Debug)]
        struct Various {
            f1: f32,
            f2: f64,
            i1: i8,
            i2: i16,
            i3: i32,
            i4: i64,
            u1: u8,
            u2: u16,
            u3: u32,
            u4: u64,
            b: Vec<u8>,
            s: String,
            c: char,
            opt_some: Option<i32>,
            opt_none: Option<String>,
            unit: (),
        }

        let value = Various {
            f1: 1.23,
            f2: -4.56e7,
            i1: -10,
            i2: -1000,
            i3: -100000,
            i4: -10000000000,
            u1: 10,
            u2: 1000,
            u3: 100000,
            u4: 10000000000,
            b: b"binary data".to_vec(),
            s: "string data".to_string(),
            c: '✅',
            opt_some: Some(99),
            opt_none: None,
            unit: (),
        };

        let facet_bytes = to_vec(&value);
        let rmp_bytes = rmp_serialize(&value);

        assert_eq!(facet_bytes, rmp_bytes);
    }
}
