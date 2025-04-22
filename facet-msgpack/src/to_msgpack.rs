use facet_core::{Def, Facet};

use facet_reflect::Peek;
use log::trace;
use std::io::{self, Write};

/// Serializes any Facet type to MessagePack bytes
pub fn to_vec<'a, T: Facet<'a>>(value: &'a T) -> Vec<u8> {
    let mut buffer = Vec::new();
    let peek = Peek::new(value);
    serialize(peek, &mut buffer).unwrap();
    buffer
}

/// Serializes any Facet type to a writer in MessagePack format
fn serialize<W: Write>(pv: Peek<'_>, writer: &mut W) -> io::Result<()> {
    let shape = pv.shape();
    match shape.def {
        Def::Scalar(_) => {
            trace!("Serializing scalar");
            if pv.shape().is_type::<String>() {
                let value = pv.get::<String>().unwrap();
                write_str(writer, value)
            } else if pv.shape().is_type::<u64>() {
                let value = pv.get::<u64>().unwrap();
                write_u64(writer, *value)
            } else if pv.shape().is_type::<u32>() {
                let value = pv.get::<u32>().unwrap();
                write_u32(writer, *value)
            } else if pv.shape().is_type::<u16>() {
                let value = pv.get::<u16>().unwrap();
                write_u16(writer, *value)
            } else if pv.shape().is_type::<u8>() {
                let value = pv.get::<u8>().unwrap();
                write_u8(writer, *value)
            } else if pv.shape().is_type::<i64>() {
                let value = pv.get::<i64>().unwrap();
                write_i64(writer, *value)
            } else if pv.shape().is_type::<i32>() {
                let value = pv.get::<i32>().unwrap();
                write_i32(writer, *value)
            } else if pv.shape().is_type::<i16>() {
                let value = pv.get::<i16>().unwrap();
                write_i16(writer, *value)
            } else if pv.shape().is_type::<i8>() {
                let value = pv
                    .get::<i8>()
                    .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
                write_i8(writer, *value)
            } else {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!("Unsupported scalar type: {}", pv.shape()),
                ));
            }
        }
        Def::Struct(sd) => {
            trace!("Serializing struct");
            let ps = pv
                .into_struct()
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

            // Write map header
            let fields = sd.fields;
            write_map_len(writer, fields.len())?;

            // Write fields
            for (field, field_peek) in ps.fields() {
                write_str(writer, field.name)?;
                serialize(field_peek, writer)?;
            }
            Ok(())
        }
        _ => Err(io::Error::new(
            io::ErrorKind::Other,
            format!("Unsupported type: {:?}", pv.shape()),
        )),
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
            // positive fixint
            writer.write_all(&[n as u8])
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
        0..=127 => {
            // positive fixint
            writer.write_all(&[n as u8])
        }
        128..=255 => {
            // uint8
            writer.write_all(&[0xcc, n as u8])
        }
        256..=32767 => {
            // uint16
            writer.write_all(&[0xcd])?;
            writer.write_all(&(n as u16).to_be_bytes())
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
        65536..=2147483647 => {
            // uint32
            writer.write_all(&[0xce])?;
            writer.write_all(&(n as u32).to_be_bytes())
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
        4294967296..=i64::MAX => {
            // uint64
            writer.write_all(&[0xcf])?;
            writer.write_all(&(n as u64).to_be_bytes())
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
