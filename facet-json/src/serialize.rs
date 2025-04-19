use core::num::NonZero;
use facet_core::{Def, Facet, FieldAttribute};
use facet_reflect::Peek;
use std::io::{self, Write};

/// Serializes a value to JSON
pub fn to_string<T: Facet>(value: &T) -> String {
    let peek = Peek::new(value);
    let mut output = Vec::new();
    serialize(&peek, &mut output).unwrap();
    String::from_utf8(output).unwrap()
}

/// Serializes a Peek instance to JSON
pub fn peek_to_string(peek: &Peek<'_>) -> String {
    let mut output = Vec::new();
    serialize(peek, &mut output).unwrap();
    String::from_utf8(output).unwrap()
}

/// Serializes a value to a writer in JSON format
pub fn to_writer<T: Facet, W: Write>(value: &T, writer: &mut W) -> io::Result<()> {
    let peek = Peek::new(value);
    serialize(&peek, writer)
}

/// Serializes a Peek instance to a writer in JSON format
pub fn peek_to_writer<W: Write>(peek: &Peek<'_>, writer: &mut W) -> io::Result<()> {
    serialize(peek, writer)
}

/// The core serialization function
fn serialize<W: Write>(peek: &Peek<'_>, writer: &mut W) -> io::Result<()> {
    use facet_core::{
        Struct,
        StructKind::{Tuple, TupleStruct},
    };

    match peek.shape().def {
        Def::Scalar(_) => serialize_scalar(peek, writer),
        Def::Struct(Struct {
            kind: Tuple | TupleStruct,
            ..
        }) => serialize_tuple(peek, writer),
        Def::Struct(_) => serialize_struct(peek, writer),
        Def::List(_) => serialize_list(peek, writer),
        Def::Map(_) => serialize_map(peek, writer),
        Def::Enum(_) => serialize_enum(peek, writer),
        Def::Option(_) => serialize_option(peek, writer),
        _ => Err(io::Error::new(
            io::ErrorKind::Other,
            format!("Unsupported type: {}", peek.shape()),
        )),
    }
}

/// Serializes a scalar value to JSON
fn serialize_scalar<W: Write>(peek: &Peek<'_>, writer: &mut W) -> io::Result<()> {
    // Handle basic scalar types
    if peek.shape().is_type::<bool>() {
        let value = peek.get::<bool>().unwrap();
        write!(writer, "{}", if *value { "true" } else { "false" })
    } else if peek.shape().is_type::<String>() {
        let value = peek.get::<String>().unwrap();
        write_json_string(writer, value)
    } else if peek.shape().is_type::<&str>() {
        let value = peek.get::<&str>().unwrap();
        write_json_string(writer, value)
    } else if peek.shape().is_type::<alloc::borrow::Cow<'_, str>>() {
        let value = peek.get::<alloc::borrow::Cow<'_, str>>().unwrap();
        write_json_string(writer, value)
    }
    // Integer types
    else if peek.shape().is_type::<u8>() {
        let value = peek.get::<u8>().unwrap();
        write!(writer, "{}", value)
    } else if peek.shape().is_type::<u16>() {
        let value = peek.get::<u16>().unwrap();
        write!(writer, "{}", value)
    } else if peek.shape().is_type::<u32>() {
        let value = peek.get::<u32>().unwrap();
        write!(writer, "{}", value)
    } else if peek.shape().is_type::<u64>() {
        let value = peek.get::<u64>().unwrap();
        write!(writer, "{}", value)
    } else if peek.shape().is_type::<usize>() {
        let value = peek.get::<usize>().unwrap();
        write!(writer, "{}", value)
    } else if peek.shape().is_type::<i8>() {
        let value = peek.get::<i8>().unwrap();
        write!(writer, "{}", value)
    } else if peek.shape().is_type::<i16>() {
        let value = peek.get::<i16>().unwrap();
        write!(writer, "{}", value)
    } else if peek.shape().is_type::<i32>() {
        let value = peek.get::<i32>().unwrap();
        write!(writer, "{}", value)
    } else if peek.shape().is_type::<i64>() {
        let value = peek.get::<i64>().unwrap();
        write!(writer, "{}", value)
    } else if peek.shape().is_type::<isize>() {
        let value = peek.get::<isize>().unwrap();
        write!(writer, "{}", value)
    }
    // NonZero types
    else if peek.shape().is_type::<NonZero<u8>>() {
        let value = peek.get::<NonZero<u8>>().unwrap();
        write!(writer, "{}", value)
    } else if peek.shape().is_type::<NonZero<u16>>() {
        let value = peek.get::<NonZero<u16>>().unwrap();
        write!(writer, "{}", value)
    } else if peek.shape().is_type::<NonZero<u32>>() {
        let value = peek.get::<NonZero<u32>>().unwrap();
        write!(writer, "{}", value)
    } else if peek.shape().is_type::<NonZero<u64>>() {
        let value = peek.get::<NonZero<u64>>().unwrap();
        write!(writer, "{}", value)
    } else if peek.shape().is_type::<NonZero<usize>>() {
        let value = peek.get::<NonZero<usize>>().unwrap();
        write!(writer, "{}", value)
    } else if peek.shape().is_type::<NonZero<i8>>() {
        let value = peek.get::<NonZero<i8>>().unwrap();
        write!(writer, "{}", value)
    } else if peek.shape().is_type::<NonZero<i16>>() {
        let value = peek.get::<NonZero<i16>>().unwrap();
        write!(writer, "{}", value)
    } else if peek.shape().is_type::<NonZero<i32>>() {
        let value = peek.get::<NonZero<i32>>().unwrap();
        write!(writer, "{}", value)
    } else if peek.shape().is_type::<NonZero<i64>>() {
        let value = peek.get::<NonZero<i64>>().unwrap();
        write!(writer, "{}", value)
    } else if peek.shape().is_type::<NonZero<isize>>() {
        let value = peek.get::<NonZero<isize>>().unwrap();
        write!(writer, "{}", value)
    }
    // Float types
    else if peek.shape().is_type::<f32>() {
        let value = peek.get::<f32>().unwrap();
        write!(writer, "{}", value)
    } else if peek.shape().is_type::<f64>() {
        let value = peek.get::<f64>().unwrap();
        write!(writer, "{}", value)
    } else {
        Err(io::Error::new(
            io::ErrorKind::Other,
            format!("Unsupported scalar type: {}", peek.shape()),
        ))
    }
}

/// Serializes a struct to JSON
fn serialize_struct<W: Write>(peek: &Peek<'_>, writer: &mut W) -> io::Result<()> {
    let struct_peek = peek
        .into_struct()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Not a struct: {}", e)))?;

    write!(writer, "{{")?;

    let mut first = true;
    for (field, field_peek) in struct_peek.fields() {
        if !first {
            write!(writer, ",")?;
        }
        first = false;

        // Check for rename attribute
        let field_name = field
            .attributes
            .iter()
            .find_map(|attr| {
                if let FieldAttribute::Rename(name) = attr {
                    Some(*name)
                } else {
                    None
                }
            })
            .unwrap_or(field.name);

        // Write field name
        write_json_string(writer, field_name)?;
        write!(writer, ":")?;

        // Write field value
        serialize(&field_peek, writer)?;
    }

    write!(writer, "}}")?;

    Ok(())
}

/// Serializes a list to JSON
fn serialize_list<W: Write>(peek: &Peek<'_>, writer: &mut W) -> io::Result<()> {
    let list_peek = peek
        .into_list()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Not a list: {}", e)))?;

    write!(writer, "[")?;

    let mut first = true;
    for item_peek in list_peek.iter() {
        if !first {
            write!(writer, ",")?;
        }
        first = false;

        serialize(&item_peek, writer)?;
    }

    write!(writer, "]")?;

    Ok(())
}

/// Serializes a tuple (struct) to JSON
fn serialize_tuple<W: Write>(peek: &Peek<'_>, writer: &mut W) -> io::Result<()> {
    let struct_peek = peek
        .into_struct()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Not a struct: {}", e)))?;

    write!(writer, "[")?;

    let mut first = true;
    for (_, item_peek) in struct_peek.fields() {
        if !first {
            write!(writer, ",")?;
        }
        first = false;

        serialize(&item_peek, writer)?;
    }

    write!(writer, "]")?;

    Ok(())
}

/// Serializes a map to JSON
fn serialize_map<W: Write>(peek: &Peek<'_>, writer: &mut W) -> io::Result<()> {
    let map_peek = peek
        .into_map()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Not a map: {}", e)))?;

    write!(writer, "{{")?;

    let mut first = true;
    for (key, value) in map_peek.iter() {
        if !first {
            write!(writer, ",")?;
        }
        first = false;

        // For map, keys must be converted to strings
        match key.shape().def {
            Def::Scalar(_) => {
                // Try to convert key to string
                if key.shape().is_type::<String>() {
                    let key_str = key.get::<String>().unwrap();
                    write_json_string(writer, key_str)?;
                } else {
                    // For other scalar types, use their Display implementation
                    write!(writer, "\"{}\"", key)?;
                }
            }
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!("Map keys must be scalar types, got: {}", key.shape()),
                ));
            }
        }

        write!(writer, ":")?;

        // Write map value
        serialize(&value, writer)?;
    }

    write!(writer, "}}")?;

    Ok(())
}

/// Serializes an enum to JSON
fn serialize_enum<W: Write>(peek: &Peek<'_>, writer: &mut W) -> io::Result<()> {
    let enum_peek = peek
        .into_enum()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Not an enum: {}", e)))?;

    let variant = enum_peek.active_variant();
    let variant_name = variant.name;

    // Check if this is a unit variant or a variant with data
    if variant.data.fields.is_empty() {
        // Unit variant - just output the name as a string
        write_json_string(writer, variant_name)
    } else {
        // Variant with data - output as an object with a single key
        write!(writer, "{{")?;
        write_json_string(writer, variant_name)?;
        write!(writer, ":")?;

        // If it's a single-field tuple variant, output just the value
        if variant.data.fields.len() == 1 {
            let field = enum_peek.field(0).ok_or_else(|| {
                io::Error::new(io::ErrorKind::Other, "Failed to access enum field")
            })?;
            serialize(&field, writer)?;
        } else {
            // Multi-field variant - output as an array or object depending on variant type
            let is_struct = variant
                .data
                .fields
                .iter()
                .any(|f| !f.name.starts_with("__"));

            if is_struct {
                // Struct variant - output as an object
                write!(writer, "{{")?;

                let mut first = true;
                for i in 0..variant.data.fields.len() {
                    let field = enum_peek.field(i).ok_or_else(|| {
                        io::Error::new(
                            io::ErrorKind::Other,
                            format!("Failed to access enum field {}", i),
                        )
                    })?;
                    let field_name = variant.data.fields[i].name;

                    if !first {
                        write!(writer, ",")?;
                    }
                    first = false;

                    write_json_string(writer, field_name)?;
                    write!(writer, ":")?;
                    serialize(&field, writer)?;
                }

                write!(writer, "}}")?
            } else {
                // Tuple variant - output as an array
                write!(writer, "[")?;

                let mut first = true;
                for i in 0..variant.data.fields.len() {
                    if !first {
                        write!(writer, ",")?;
                    }
                    first = false;

                    let field = enum_peek.field(i).ok_or_else(|| {
                        io::Error::new(
                            io::ErrorKind::Other,
                            format!("Failed to access enum field {}", i),
                        )
                    })?;
                    serialize(&field, writer)?;
                }

                write!(writer, "]")?;
            }
        }

        write!(writer, "}}")?;
        Ok(())
    }
}

/// Serializes an `Option<T>` to JSON
fn serialize_option<W: Write>(peek: &Peek<'_>, writer: &mut W) -> io::Result<()> {
    let option_peek = peek
        .into_option()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Not an option: {}", e)))?;

    if option_peek.is_none() {
        write!(writer, "null")
    } else {
        let value = option_peek
            .value()
            .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "Failed to get option value"))?;
        serialize(&value, writer)
    }
}

/// Properly escapes and writes a JSON string
fn write_json_string<W: Write>(writer: &mut W, s: &str) -> io::Result<()> {
    write!(writer, "\"")?;

    for c in s.chars() {
        match c {
            '"' => write!(writer, "\\\"")?,
            '\\' => write!(writer, "\\\\")?,
            '\n' => write!(writer, "\\n")?,
            '\r' => write!(writer, "\\r")?,
            '\t' => write!(writer, "\\t")?,
            '\u{08}' => write!(writer, "\\b")?,
            '\u{0C}' => write!(writer, "\\f")?,
            c if c.is_control() => write!(writer, "\\u{:04x}", c as u32)?,
            c => write!(writer, "{}", c)?,
        }
    }

    write!(writer, "\"")
}
