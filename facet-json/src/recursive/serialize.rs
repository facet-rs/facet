use std::io::{self, Write};

use facet_core::Facet;
use facet_core::PointerType;
use facet_core::ScalarAffinity;
use facet_core::SequenceType;
use facet_core::StructKind;
use facet_core::Type;
use facet_core::UserType;
use facet_reflect::HasFields;
use facet_reflect::{Peek, ScalarType};
use log::debug;
// use std::io::{self, Write};

/// Serializes a value to JSON
pub(crate) fn to_string<'facet: 'input, 'input: 'facet, T: Facet<'facet>>(
    value: &'input T,
    recursion_depth: usize,
) -> String {
    let peek = Peek::new(value);
    if recursion_depth > crate::MAX_RECURSION_DEPTH {
        return crate::iterative::to_string(value).into();
    }
    let mut out = Vec::new();
    write_peek(&peek, recursion_depth, &mut out).unwrap();
    String::from_utf8(out).unwrap()
}

/// Serializes a Peek instance to JSON
pub(crate) fn peek_to_string<'facet: 'input, 'input: 'facet>(
    peek: &Peek<'input, 'facet>,
    recursion_depth: usize,
) -> String {
    let mut out = Vec::new();
    write_peek(peek, recursion_depth, &mut out).unwrap();
    String::from_utf8(out).unwrap()
}

/// Serializes a value to a writer in JSON format
pub(crate) fn to_writer<'mem: 'facet, 'facet, T: Facet<'facet>, W: Write>(
    value: &'mem T,
    writer: &mut W,
) -> io::Result<()> {
    let peek = Peek::new(value);
    write_peek(&peek, 0, writer)
}

/// Serializes a Peek instance to a writer in JSON format
pub(crate) fn peek_to_writer<'mem: 'facet, 'facet, W: Write>(
    peek: &Peek<'mem, 'facet>,
    writer: &mut W,
) -> io::Result<()> {
    write_peek(peek, 0, writer)
}

fn write_peek<'mem: 'facet, 'facet, W: Write>(
    peek: &Peek<'mem, 'facet>,
    recursion_depth: usize,
    output: &mut W,
) -> io::Result<()> {
    use facet_core::Def::*;
    match (peek.shape().def, peek.shape().ty) {
        // (Undefined, _) => todo!(),
        (Scalar(scalar_def), _) => {
            let scalar_peek = peek.innermost_peek();
            match scalar_peek.scalar_type() {
                Some(ScalarType::Unit) => write!(output, "null"),
                Some(ScalarType::Bool) => write!(output, "{}", scalar_peek.get::<bool>().unwrap()),
                Some(ScalarType::Char) => {
                    write!(output, r#""{}""#, scalar_peek.get::<char>().unwrap())
                }

                // String types
                Some(ScalarType::Str) => {
                    write!(output, r#""{}""#, scalar_peek.get::<&str>().unwrap())
                }
                Some(ScalarType::String) => {
                    write!(output, r#""{}""#, scalar_peek.get::<String>().unwrap())
                }
                Some(ScalarType::CowStr) => write!(
                    output,
                    r#""{}""#,
                    scalar_peek.get::<alloc::borrow::Cow<'_, str>>().unwrap()
                ),

                // Float types
                Some(ScalarType::F32) => write!(output, "{}", scalar_peek.get::<f32>().unwrap()),
                Some(ScalarType::F64) => write!(output, "{}", scalar_peek.get::<f64>().unwrap()),

                // Integer types
                Some(ScalarType::U8) => write!(output, "{}", scalar_peek.get::<u8>().unwrap()),
                Some(ScalarType::U16) => write!(output, "{}", scalar_peek.get::<u16>().unwrap()),
                Some(ScalarType::U32) => write!(output, "{}", scalar_peek.get::<u32>().unwrap()),
                Some(ScalarType::U64) => write!(output, "{}", scalar_peek.get::<u64>().unwrap()),
                Some(ScalarType::U128) => write!(output, "{}", scalar_peek.get::<u128>().unwrap()),
                Some(ScalarType::USize) => {
                    write!(output, "{}", scalar_peek.get::<usize>().unwrap())
                }
                Some(ScalarType::I8) => write!(output, "{}", scalar_peek.get::<i8>().unwrap()),
                Some(ScalarType::I16) => write!(output, "{}", scalar_peek.get::<i16>().unwrap()),
                Some(ScalarType::I32) => write!(output, "{}", scalar_peek.get::<i32>().unwrap()),
                Some(ScalarType::I64) => write!(output, "{}", scalar_peek.get::<i64>().unwrap()),
                Some(ScalarType::I128) => write!(output, "{}", scalar_peek.get::<i128>().unwrap()),
                Some(ScalarType::ISize) => {
                    write!(output, "{}", scalar_peek.get::<isize>().unwrap())
                }
                Some(unsupported) => panic!("Unsupported scalar type: {unsupported:?}"),
                None => {
                    match scalar_def.affinity {
                        ScalarAffinity::Time(_)
                        | ScalarAffinity::Path(_)
                        | ScalarAffinity::ULID(_)
                        | ScalarAffinity::UUID(_) => {
                            if let Some(_display) = scalar_peek.shape().vtable.display {
                                // Use display formatting if available
                                write!(output, "{}", scalar_peek)
                            } else {
                                panic!("Unsupported shape: {}", scalar_peek.shape())
                            }
                        }
                        _ => {
                            panic!("Unsupported shape: {}", scalar_peek.shape())
                        }
                    }
                }
            }
        }
        (Map(_map_def), _) => {
            let map_peek = peek.into_map().unwrap();
            write!(output, "{{")?;
            let mut first = true;
            for (key, value) in map_peek.iter() {
                if !first {
                    write!(output, ",")?;
                }
                first = false;
                write!(output, r#""{}""#, key)?;
                write!(output, ":")?;
                write_peek(&value, recursion_depth + 1, output)?;
            }
            write!(output, "}}")
        }
        (List(_) | Array(_) | Slice(_) | Set(_), _) => {
            let set_peek = peek.into_list_like().unwrap();
            write!(output, "[")?;
            let mut first = true;
            for value in set_peek.iter() {
                if !first {
                    write!(output, ",")?;
                }
                first = false;
                write_peek(&value, recursion_depth + 1, output)?;
            }
            write!(output, "]")
        }
        (SmartPointer(_smart_pointer_def), _) => {
            write_peek(&peek.innermost_peek(), recursion_depth + 1, output)
        }
        (Option(_option_def), _) => write_peek(&peek.innermost_peek(), recursion_depth + 1, output),
        (_, Type::User(UserType::Struct(sd))) => {
            debug!("Serializing struct: shape={}", peek.shape(),);
            debug!(
                "  Struct details: kind={:?}, field_count={}",
                sd.kind,
                sd.fields.len()
            );

            match sd.kind {
                StructKind::Unit => {
                    debug!("  Handling unit struct (no fields)");
                    // Correctly handle unit struct type when encountered as a value
                    write!(output, "null")
                }
                StructKind::Tuple | StructKind::TupleStruct => {
                    let peek = peek.into_struct().unwrap();
                    write!(output, "[")?;
                    let mut first = true;
                    for (_field, peek) in peek.fields_for_serialize() {
                        if !first {
                            write!(output, ",")?;
                        }
                        first = false;
                        write_peek(&peek, recursion_depth + 1, output)?;
                    }

                    write!(output, "]")
                }
                StructKind::Struct => {
                    let peek = peek.into_struct().unwrap();
                    write!(output, "{{")?;
                    let mut first = true;
                    for (field, peek) in peek.fields_for_serialize() {
                        if !first {
                            write!(output, ",")?;
                        }
                        first = false;
                        write!(output, r#""{}""#, field.name)?;
                        write!(output, ":")?;
                        write_peek(&peek, recursion_depth + 1, output)?;
                    }

                    write!(output, "}}")
                }
                _ => {
                    unreachable!()
                }
            }
        }
        (_, Type::Sequence(SequenceType::Tuple(_))) => {
            debug!("Serializing tuple: shape={}", peek.shape());

            // Now we can use our dedicated PeekTuple type
            if let Ok(peek_tuple) = peek.into_tuple() {
                let count = peek_tuple.len();
                debug!("  Tuple fields count: {}", count);

                write!(output, "[")?;
                let mut first = true;
                for (_, field_peek) in peek_tuple.fields().rev() {
                    if !first {
                        write!(output, ",")?;
                    }
                    first = false;
                    let innermost_peek = field_peek.innermost_peek();
                    write_peek(&innermost_peek, recursion_depth + 1, output)?;
                }
                write!(output, "]")
            } else {
                // This shouldn't happen if into_tuple is implemented correctly,
                // but we'll handle it as a fallback
                debug!("  Could not convert to PeekTuple, falling back to list_like approach");

                if let Ok(peek_list_like) = peek.into_list_like() {
                    write!(output, "[")?;
                    let mut first = true;
                    for elem in peek_list_like.iter() {
                        if !first {
                            write!(output, ",")?;
                        }
                        first = false;
                        let innermost_peek = elem.innermost_peek();
                        write_peek(&innermost_peek, recursion_depth + 1, output)?;
                    }
                    write!(output, "]")
                } else {
                    write!(output, "[]")
                }
            }
        }
        (_, Type::User(UserType::Enum(_))) => {
            let peek_enum = peek.into_enum().unwrap();
            let variant = peek_enum
                .active_variant()
                .expect("Failed to get active variant");
            let variant_index = peek_enum
                .variant_index()
                .expect("Failed to get variant index");
            // let flattened = maybe_field.map(|f| f.flattened).unwrap_or_default();

            if variant.data.fields.is_empty() {
                write!(output, "{}", variant.name)
            } else {
                todo!()
            }
        }
        (_, Type::Pointer(pointer_type)) => {
            // Handle pointer types using our new safe abstraction
            if let Some(str_value) = peek.as_str() {
                write!(output, "\"{}\"", str_value)?;
                Ok(())
            } else if let PointerType::Function(_) = pointer_type {
                write!(output, "null")
            } else {
                // Handle other pointer types with innermost_peek which is safe
                let innermost = peek.innermost_peek();
                if innermost.shape() != peek.shape() {
                    write_peek(&innermost, recursion_depth + 1, output)
                } else {
                    write!(output, "null")
                }
            }
        }
        otherwise => {
            dbg!(otherwise);
            todo!()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_scalar_serialization() {
        let my_string = String::from("world");

        // Test basic scalar types
        assert_eq!(to_string(&42i32, 0), "42");
        assert_eq!(to_string(&true, 0), "true");
        assert_eq!(to_string(&3.14f64, 0), "3.14");
        assert_eq!(to_string(&"hello", 0), "\"hello\"");
        assert_eq!(to_string(&&*my_string, 0), "\"world\"");
    }

    #[test]
    fn test_container_serialization() {
        // Test Vec
        let vec = vec![1, 2, 3];
        assert_eq!(to_string(&vec, 0), "[1,2,3]");

        // Test HashMap
        let mut map = HashMap::new();
        map.insert("a", 1);
        map.insert("b", 2);

        // Since HashMap order is non-deterministic, we can't assert exact string
        let result = to_string(&map, 0);
        assert!(result == r#"{"a":1,"b":2}"# || result == r#"{"b":2,"a":1}"#);
    }

    #[test]
    fn test_nested_structures() {
        // Test nested structures
        let mut map = HashMap::new();
        map.insert("x", 10);
        map.insert("y", 20);

        let result = to_string(&map, 0);
        assert!(result.contains("\"x\":10"));
        assert!(result.contains("\"y\":20"));
    }
}
