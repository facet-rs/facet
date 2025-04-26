use core::num::NonZero;
use facet_core::{Def, Facet, FieldAttribute, StructKind};
use facet_reflect::Peek;
use std::io::{self, Write};

use crate::First;

/// Serializes a value to JSON
pub fn to_string<'a, T: Facet<'a>>(value: &T) -> String {
    let peek = Peek::new(value);
    let mut output = Vec::new();
    serialize(&peek, true, &mut output).unwrap();
    String::from_utf8(output).unwrap()
}

/// Serializes a Peek instance to JSON
pub fn peek_to_string(peek: &Peek<'_, '_>) -> String {
    let mut output = Vec::new();
    serialize(peek, true, &mut output).unwrap();
    String::from_utf8(output).unwrap()
}

/// Serializes a value to a writer in JSON format
pub fn to_writer<'a, T: Facet<'a>, W: Write>(value: &T, writer: &mut W) -> io::Result<()> {
    let peek = Peek::new(value);
    serialize(&peek, true, writer)
}

/// Serializes a Peek instance to a writer in JSON format
pub fn peek_to_writer<W: Write>(peek: &Peek<'_, '_>, writer: &mut W) -> io::Result<()> {
    serialize(peek, true, writer)
}

/// The core serialization function
fn serialize<W: Write>(peek: &Peek<'_, '_>, delimit: bool, writer: &mut W) -> io::Result<()> {
    use facet_core::{
        StructDef,
        StructKind::{Tuple, TupleStruct},
    };

    match peek.shape().def {
        Def::Scalar(_) => serialize_scalar(peek, writer),
        Def::Struct(StructDef {
            kind: Tuple | TupleStruct,
            ..
        }) => serialize_tuple(peek, writer),
        Def::Struct(_) => serialize_struct(peek, delimit, writer),
        Def::List(_) => serialize_list(peek, writer),
        Def::Map(_) => serialize_map(peek, delimit, writer),
        Def::Enum(_) => serialize_enum(peek, delimit, writer),
        Def::Option(_) => serialize_option(peek, writer),
        _ => Err(io::Error::new(
            io::ErrorKind::Other,
            format!("Unsupported type: {}", peek.shape()),
        )),
    }
}

/// Serializes a scalar value to JSON
fn serialize_scalar<W: Write>(peek: &Peek<'_, '_>, writer: &mut W) -> io::Result<()> {
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
fn serialize_struct<W: Write>(
    peek: &Peek<'_, '_>,
    delimit: bool,
    writer: &mut W,
) -> io::Result<()> {
    let struct_peek = peek
        .into_struct()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Not a struct: {}", e)))?;

    if delimit {
        write!(writer, "{{")?;
    }

    for (first, (field, field_peek)) in struct_peek.fields_for_serialize().with_first() {
        if !first {
            write!(writer, ",")?;
        }

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
        let should_delimit = !field
            .attributes
            .iter()
            .any(|&attr| attr == FieldAttribute::Arbitrary("flatten"));

        // Write field name
        if should_delimit {
            write_json_string(writer, field_name)?;
            write!(writer, ":")?;
        }

        // Write field value
        serialize(&field_peek, should_delimit, writer)?;
    }

    if delimit {
        write!(writer, "}}")?;
    }

    Ok(())
}

/// Serializes a list to JSON
fn serialize_list<W: Write>(peek: &Peek<'_, '_>, writer: &mut W) -> io::Result<()> {
    let list_peek = peek
        .into_list()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Not a list: {}", e)))?;

    write!(writer, "[")?;

    for (first, item_peek) in list_peek.iter().with_first() {
        if !first {
            write!(writer, ",")?;
        }

        serialize(&item_peek, true, writer)?;
    }

    write!(writer, "]")?;

    Ok(())
}

/// Serializes a tuple (struct) to JSON
fn serialize_tuple<W: Write>(peek: &Peek<'_, '_>, writer: &mut W) -> io::Result<()> {
    let struct_peek = peek
        .into_struct()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Not a struct: {}", e)))?;

    write!(writer, "[")?;

    for (first, (_, item_peek)) in struct_peek.fields_for_serialize().with_first() {
        if !first {
            write!(writer, ",")?;
        }

        serialize(&item_peek, true, writer)?;
    }

    write!(writer, "]")?;

    Ok(())
}

/// Serializes a map to JSON
fn serialize_map<W: Write>(peek: &Peek<'_, '_>, delimit: bool, writer: &mut W) -> io::Result<()> {
    let map_peek = peek
        .into_map()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Not a map: {}", e)))?;

    if delimit {
        write!(writer, "{{")?;
    }

    for (first, (key, value)) in map_peek.iter().with_first() {
        if !first {
            write!(writer, ",")?;
        }

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
        serialize(&value, true, writer)?;
    }

    if delimit {
        write!(writer, "}}")?;
    }

    Ok(())
}

/// Serializes an enum to JSON
fn serialize_enum<W: Write>(peek: &Peek<'_, '_>, delimit: bool, writer: &mut W) -> io::Result<()> {
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
        if delimit {
            write!(writer, "{{")?;
        }
        write_json_string(writer, variant_name)?;
        write!(writer, ":")?;

        // Multi-field variant - output as an array or object depending on variant type
        let is_struct = variant.data.kind == StructKind::Struct;

        if is_struct {
            // Struct variant - output as an object
            write!(writer, "{{")?;

            for (first, (field, field_peek)) in enum_peek.fields_for_serialize().with_first() {
                if !first {
                    write!(writer, ",")?;
                }

                let should_delimit = field
                    .attributes
                    .iter()
                    .any(|&attr| attr == FieldAttribute::Arbitrary("flatten"));

                write_json_string(writer, field.name)?;
                write!(writer, ":")?;
                serialize(&field_peek, should_delimit, writer)?;
            }

            write!(writer, "}}")?
        } else {
            // Tuple variant - output as an array if has more than 1 element, otherwise just output
            // the element.

            if crate::variant_is_transparent(variant) {
                let field = enum_peek.field(0).ok_or_else(|| {
                    io::Error::new(io::ErrorKind::Other, "Failed to access enum field")
                })?;
                serialize(&field, true, writer)?;
            } else {
                write!(writer, "[")?;

                for (first, (field, field_peek)) in enum_peek.fields_for_serialize().with_first() {
                    if !first {
                        write!(writer, ",")?;
                    }

                    let should_delimit = field
                        .attributes
                        .iter()
                        .any(|&attr| attr == FieldAttribute::Arbitrary("flatten"));

                    serialize(&field_peek, should_delimit, writer)?;
                }

                write!(writer, "]")?;
            }
        }

        if delimit {
            write!(writer, "}}")?;
        }
        Ok(())
    }
}

/// Serializes an `Option<T>` to JSON
fn serialize_option<W: Write>(peek: &Peek<'_, '_>, writer: &mut W) -> io::Result<()> {
    let option_peek = peek
        .into_option()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Not an option: {}", e)))?;

    if option_peek.is_none() {
        write!(writer, "null")
    } else {
        let value = option_peek
            .value()
            .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "Failed to get option value"))?;
        serialize(&value, true, writer)
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
