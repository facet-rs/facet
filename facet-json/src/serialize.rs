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

/// Task items for the serialization stack.
#[derive(Debug)]
enum SerializeTask<'a, 'mem, 'facet> {
    Value {
        peek: Peek<'mem, 'facet>,
        delimit: bool,
    },
    ScalarValue(Peek<'mem, 'facet>),
    StartStruct {
        struct_peek: Peek<'mem, 'facet>,
        delimit: bool,
    },
    StructField {
        field_peek: Peek<'mem, 'facet>,
        field_name: &'a str,
        should_delimit: bool,
        is_first: bool,
    },
    EndStruct {
        delimit: bool,
    },
    StartList(Peek<'mem, 'facet>),
    ListItem {
        item_peek: Peek<'mem, 'facet>,
        is_first: bool,
    },
    EndList,
    StartTuple(Peek<'mem, 'facet>),
    TupleItem {
        item_peek: Peek<'mem, 'facet>,
        is_first: bool,
    },
    EndTuple,
    StartMap {
        map_peek: Peek<'mem, 'facet>,
        delimit: bool,
    },
    MapEntry {
        key: Peek<'mem, 'facet>,
        value: Peek<'mem, 'facet>,
        is_first: bool,
    },
    EndMap {
        delimit: bool,
    },
    StartEnum {
        enum_peek: Peek<'mem, 'facet>,
        // variant_name: &'a str,
        delimit: bool,
        // is_empty: bool,
        // is_struct: bool,
    },
    EnumStructField {
        field: &'a facet_core::Field,
        field_peek: Peek<'mem, 'facet>,
        field_name: &'a str,
        is_first: bool,
    },
    EnumTupleField {
        field: &'a facet_core::Field,
        field_peek: Peek<'mem, 'facet>,
        is_first: bool,
    },
    EndEnum {
        delimit: bool,
        is_struct: bool,
        is_transparent: bool,
    },
    StartOption(Peek<'mem, 'facet>),
}

/// The core serialization function - iterative approach
fn serialize<W: Write>(peek: &Peek<'_, '_>, delimit: bool, writer: &mut W) -> io::Result<()> {
    use facet_core::{
        StructDef,
        StructKind::{Tuple, TupleStruct},
    };

    let mut stack = Vec::new();
    stack.push(SerializeTask::Value {
        peek: *peek,
        delimit,
    });

    while let Some(task) = stack.pop() {
        match task {
            SerializeTask::Value { peek, delimit } => match peek.shape().def {
                Def::Scalar(_) => {
                    stack.push(SerializeTask::ScalarValue(peek));
                }
                Def::Struct(StructDef {
                    kind: Tuple | TupleStruct,
                    ..
                }) => {
                    stack.push(SerializeTask::StartTuple(peek));
                }
                Def::Struct(_) => {
                    stack.push(SerializeTask::StartStruct {
                        struct_peek: peek,
                        delimit,
                    });
                }
                Def::List(_) => {
                    stack.push(SerializeTask::StartList(peek));
                }
                Def::Map(_) => {
                    stack.push(SerializeTask::StartMap {
                        map_peek: peek,
                        delimit,
                    });
                }
                Def::Enum(_) => {
                    stack.push(SerializeTask::StartEnum {
                        enum_peek: peek,
                        delimit,
                    });
                }
                Def::Option(_) => {
                    stack.push(SerializeTask::StartOption(peek));
                }
                _ => {
                    return Err(io::Error::new(
                        io::ErrorKind::Other,
                        format!("Unsupported type: {}", peek.shape()),
                    ));
                }
            },
            SerializeTask::ScalarValue(peek) => {
                serialize_scalar(&peek, writer)?;
            }
            SerializeTask::StartStruct {
                struct_peek,
                delimit,
            } => {
                let struct_peek = struct_peek.into_struct().map_err(|e| {
                    io::Error::new(io::ErrorKind::Other, format!("Not a struct: {}", e))
                })?;

                if delimit {
                    write!(writer, "{{")?;
                }

                // Process fields in reverse order for the stack
                let fields = struct_peek
                    .fields_for_serialize()
                    .with_first()
                    .collect::<Vec<_>>();

                stack.push(SerializeTask::EndStruct { delimit });

                for (first, (field, field_peek)) in fields.into_iter().rev() {
                    let field_name = field.name;

                    // FIXME: flatten is well-known, not arbitrary
                    let should_delimit = !field.has_arbitrary_attr("flatten");

                    stack.push(SerializeTask::StructField {
                        field_peek,
                        field_name,
                        should_delimit,
                        is_first: first,
                    });
                }
            }
            SerializeTask::StructField {
                field_peek,
                field_name,
                should_delimit,
                is_first,
                ..
            } => {
                if !is_first {
                    write!(writer, ",")?;
                }

                // Write field name
                if should_delimit {
                    write_json_string(writer, field_name)?;
                    write!(writer, ":")?;
                }

                // Push the field value to serialize
                stack.push(SerializeTask::Value {
                    peek: field_peek,
                    delimit: should_delimit,
                });
            }
            SerializeTask::EndStruct { delimit } => {
                if delimit {
                    write!(writer, "}}")?;
                }
            }
            SerializeTask::StartList(peek) => {
                let list_peek = peek.into_list().map_err(|e| {
                    io::Error::new(io::ErrorKind::Other, format!("Not a list: {}", e))
                })?;

                write!(writer, "[")?;

                // Process items in reverse order for the stack
                let items = list_peek.iter().with_first().collect::<Vec<_>>();
                stack.push(SerializeTask::EndList);

                for (first, item_peek) in items.into_iter().rev() {
                    stack.push(SerializeTask::ListItem {
                        item_peek,
                        is_first: first,
                    });
                }
            }
            SerializeTask::ListItem {
                item_peek,
                is_first,
            } => {
                if !is_first {
                    write!(writer, ",")?;
                }

                stack.push(SerializeTask::Value {
                    peek: item_peek,
                    delimit: true,
                });
            }
            SerializeTask::EndList => {
                write!(writer, "]")?;
            }
            SerializeTask::StartTuple(peek) => {
                let struct_peek = peek.into_struct().map_err(|e| {
                    io::Error::new(io::ErrorKind::Other, format!("Not a struct: {}", e))
                })?;

                write!(writer, "[")?;

                // Process fields in reverse order for the stack
                let fields = struct_peek
                    .fields_for_serialize()
                    .with_first()
                    .collect::<Vec<_>>();

                stack.push(SerializeTask::EndTuple);

                for (first, (_, item_peek)) in fields.into_iter().rev() {
                    stack.push(SerializeTask::TupleItem {
                        item_peek,
                        is_first: first,
                    });
                }
            }
            SerializeTask::TupleItem {
                item_peek,
                is_first,
            } => {
                if !is_first {
                    write!(writer, ",")?;
                }

                stack.push(SerializeTask::Value {
                    peek: item_peek,
                    delimit: true,
                });
            }
            SerializeTask::EndTuple => {
                write!(writer, "]")?;
            }
            SerializeTask::StartMap { map_peek, delimit } => {
                let map_peek = map_peek.into_map().map_err(|e| {
                    io::Error::new(io::ErrorKind::Other, format!("Not a map: {}", e))
                })?;

                if delimit {
                    write!(writer, "{{")?;
                }

                // Process entries in reverse order for the stack
                let entries = map_peek.iter().with_first().collect::<Vec<_>>();
                stack.push(SerializeTask::EndMap { delimit });

                for (first, (key, value)) in entries.into_iter().rev() {
                    stack.push(SerializeTask::MapEntry {
                        key,
                        value,
                        is_first: first,
                    });
                }
            }
            SerializeTask::MapEntry {
                key,
                value,
                is_first,
            } => {
                if !is_first {
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

                // Push the value to serialize
                stack.push(SerializeTask::Value {
                    peek: value,
                    delimit: true,
                });
            }
            SerializeTask::EndMap { delimit } => {
                if delimit {
                    write!(writer, "}}")?;
                }
            }
            SerializeTask::StartEnum { enum_peek, delimit } => {
                let enum_peek = enum_peek.into_enum().map_err(|e| {
                    io::Error::new(io::ErrorKind::Other, format!("Not a map: {}", e))
                })?;
                let variant = enum_peek.active_variant();
                let variant_name = variant.name;
                let is_empty = variant.data.fields.is_empty();
                let is_struct = variant.data.kind == StructKind::Struct;
                if is_empty {
                    // Unit variant - just output the name as a string
                    write_json_string(writer, variant_name)?;
                } else {
                    // Variant with data - output as an object with a single key
                    if delimit {
                        write!(writer, "{{")?;
                    }
                    write_json_string(writer, variant_name)?;
                    write!(writer, ":")?;

                    let is_transparent = crate::variant_is_transparent(enum_peek.active_variant());

                    stack.push(SerializeTask::EndEnum {
                        delimit,
                        is_struct,
                        is_transparent,
                    });

                    if is_struct {
                        // Struct variant - output as an object
                        write!(writer, "{{")?;

                        // Process fields in reverse order for the stack
                        let fields = enum_peek
                            .fields_for_serialize()
                            .with_first()
                            .collect::<Vec<_>>();

                        for (first, (field, field_peek)) in fields.into_iter().rev() {
                            stack.push(SerializeTask::EnumStructField {
                                field,
                                field_peek,
                                field_name: field.name,
                                is_first: first,
                            });
                        }
                    } else if is_transparent {
                        // Transparent variant - output the field directly
                        if let Some(field) = enum_peek.field(0) {
                            stack.push(SerializeTask::Value {
                                peek: field,
                                delimit: true,
                            });
                        } else {
                            return Err(io::Error::new(
                                io::ErrorKind::Other,
                                "Failed to access enum field",
                            ));
                        }
                    } else {
                        // Tuple variant - output as an array
                        write!(writer, "[")?;

                        // Process fields in reverse order for the stack
                        let fields = enum_peek
                            .fields_for_serialize()
                            .with_first()
                            .collect::<Vec<_>>();

                        for (first, (field, field_peek)) in fields.into_iter().rev() {
                            stack.push(SerializeTask::EnumTupleField {
                                field,
                                field_peek,
                                is_first: first,
                            });
                        }
                    }
                }
            }
            SerializeTask::EnumStructField {
                field_peek,
                field_name,
                is_first,
                field,
            } => {
                if !is_first {
                    write!(writer, ",")?;
                }

                let should_delimit = field
                    .attributes
                    .iter()
                    .any(|&attr| attr == FieldAttribute::Arbitrary("flatten"));

                write_json_string(writer, field_name)?;
                write!(writer, ":")?;

                stack.push(SerializeTask::Value {
                    peek: field_peek,
                    delimit: should_delimit,
                });
            }
            SerializeTask::EnumTupleField {
                field_peek,
                is_first,
                field,
                ..
            } => {
                if !is_first {
                    write!(writer, ",")?;
                }

                let should_delimit = field
                    .attributes
                    .iter()
                    .any(|&attr| attr == FieldAttribute::Arbitrary("flatten"));

                stack.push(SerializeTask::Value {
                    peek: field_peek,
                    delimit: should_delimit,
                });
            }
            SerializeTask::EndEnum {
                delimit,
                is_struct,
                is_transparent,
            } => {
                if is_struct {
                    write!(writer, "}}")?;
                } else if !is_transparent {
                    write!(writer, "]")?;
                }

                if delimit {
                    write!(writer, "}}")?;
                }
            }
            SerializeTask::StartOption(peek) => {
                let option_peek = peek.into_option().map_err(|e| {
                    io::Error::new(io::ErrorKind::Other, format!("Not an option: {}", e))
                })?;

                if option_peek.is_none() {
                    write!(writer, "null")?;
                } else {
                    let value = option_peek.value().ok_or_else(|| {
                        io::Error::new(io::ErrorKind::Other, "Failed to get option value")
                    })?;

                    stack.push(SerializeTask::Value {
                        peek: value,
                        delimit: true,
                    });
                }
            }
        }
    }

    Ok(())
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
