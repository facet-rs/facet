use alloc::string::String;
use alloc::vec::Vec;
use facet_core::{Def, Facet, Field, PointerType, ShapeAttribute, StructKind, Type, UserType};
use facet_reflect::{HasFields, Peek, ScalarType};
use log::trace;

/// Serializes a value implementing `Facet` to a JSON string.
pub fn to_string<'facet, T: Facet<'facet> + ?Sized>(value: &T) -> String {
    peek_to_string(Peek::new(value))
}

/// Serializes a value implementing `Facet` to a pretty-printed JSON string.
pub fn to_string_pretty<'facet, T: Facet<'facet> + ?Sized>(value: &T) -> String {
    peek_to_string_pretty(Peek::new(value))
}

/// Serializes a `Peek` instance to a JSON string.
pub fn peek_to_string<'input, 'facet>(peek: Peek<'input, 'facet>) -> String {
    let mut s = Vec::new();
    peek_to_writer(peek, &mut s).unwrap();
    String::from_utf8(s).unwrap()
}

/// Serializes a `Peek` instance to a pretty-printed JSON string.
pub fn peek_to_string_pretty<'input, 'facet>(peek: Peek<'input, 'facet>) -> String {
    let mut s = Vec::new();
    peek_to_writer_pretty(peek, &mut s).unwrap();
    String::from_utf8(s).unwrap()
}

/// Serializes a `Facet` value to JSON and writes it to the given writer.
pub fn to_writer<'mem, 'facet, T: Facet<'facet>, W: crate::JsonWrite>(
    value: &'mem T,
    writer: W,
) -> Result<(), SerializeError> {
    peek_to_writer(Peek::new(value), writer)
}

/// Serializes a `Facet` value to pretty-printed JSON and writes it to the given writer.
pub fn to_writer_pretty<'mem, 'facet, T: Facet<'facet>, W: crate::JsonWrite>(
    value: &'mem T,
    writer: W,
) -> Result<(), SerializeError> {
    peek_to_writer_pretty(Peek::new(value), writer)
}

/// Serializes a `Peek` value to JSON and writes it to the given writer.
pub fn peek_to_writer<'mem, 'facet, W: crate::JsonWrite>(
    peek: Peek<'mem, 'facet>,
    mut writer: W,
) -> Result<(), SerializeError> {
    serialize_value(peek, None, &mut writer, None, 0)
}

/// Serializes a `Peek` value to pretty-printed JSON and writes it to the given writer.
pub fn peek_to_writer_pretty<'mem, 'facet, W: crate::JsonWrite>(
    peek: Peek<'mem, 'facet>,
    mut writer: W,
) -> Result<(), SerializeError> {
    serialize_value(peek, None, &mut writer, Some("  "), 0)
}

/// Serialization error for json, which cannot fail.
#[derive(Debug)]
pub enum SerializeError {}

fn variant_is_newtype_like(variant: &facet_core::Variant) -> bool {
    variant.data.kind == StructKind::Tuple && variant.data.fields.len() == 1
}

/// Write indentation for pretty printing
fn write_indent<W: crate::JsonWrite>(writer: &mut W, indent: Option<&str>, depth: usize) {
    if let Some(indent_str) = indent {
        for _ in 0..depth {
            writer.write(indent_str.as_bytes());
        }
    }
}

/// Write a newline for pretty printing
fn write_newline<W: crate::JsonWrite>(writer: &mut W, indent: Option<&str>) {
    if indent.is_some() {
        writer.write(b"\n");
    }
}

/// Write a space after colon for pretty printing
fn write_colon<W: crate::JsonWrite>(writer: &mut W, indent: Option<&str>) {
    if indent.is_some() {
        writer.write(b": ");
    } else {
        writer.write(b":");
    }
}

fn serialize_value<'mem, 'facet, W: crate::JsonWrite>(
    mut peek: Peek<'mem, 'facet>,
    maybe_field: Option<Field>,
    writer: &mut W,
    indent: Option<&str>,
    depth: usize,
) -> Result<(), SerializeError> {
    trace!("Serializing a value, shape is {}", peek.shape());

    // Handle custom serialization
    #[cfg(feature = "alloc")]
    if let Some(f) = maybe_field {
        if f.vtable.serialize_with.is_some() {
            let owned_peek = peek.custom_serialization(f).unwrap();
            let old_shape = peek.shape();
            let new_shape = owned_peek.shape();
            trace!("{old_shape} has custom serialization, serializing as {new_shape} instead");
            return serialize_value(owned_peek.as_peek(), None, writer, indent, depth);
        }
    }

    // Handle transparent types
    if peek
        .shape()
        .attributes
        .contains(&ShapeAttribute::Transparent)
    {
        let old_shape = peek.shape();
        let ps = peek.into_struct().unwrap();
        peek = ps.field(0).unwrap();
        let new_shape = peek.shape();
        trace!("{old_shape} is transparent, let's serialize the inner {new_shape} instead");
    }

    trace!(
        "Matching def={:?}, ty={:?} for shape={}",
        peek.shape().def,
        peek.shape().ty,
        peek.shape()
    );

    match (peek.shape().def, peek.shape().ty) {
        (Def::Scalar, _) => {
            let peek = peek.innermost_peek();
            serialize_scalar(peek, writer)?;
        }
        (Def::List(ld), _) => {
            if ld.t().is_type::<u8>() && peek.shape().is_type::<Vec<u8>>() {
                // Special case for Vec<u8> - serialize as array of numbers
                let bytes = peek.get::<Vec<u8>>().unwrap();
                serialize_byte_array(bytes, writer, indent, depth)?;
            } else {
                let peek_list = peek.into_list_like().unwrap();
                serialize_array(peek_list.iter(), writer, indent, depth)?;
            }
        }
        (Def::Array(ad), _) => {
            if ad.t().is_type::<u8>() {
                let bytes: Vec<u8> = peek
                    .into_list_like()
                    .unwrap()
                    .iter()
                    .map(|p| *p.get::<u8>().unwrap())
                    .collect();
                serialize_byte_array(&bytes, writer, indent, depth)?;
            } else {
                let peek_list = peek.into_list_like().unwrap();
                serialize_array(peek_list.iter(), writer, indent, depth)?;
            }
        }
        (Def::Slice(sd), _) => {
            if sd.t().is_type::<u8>() {
                let bytes = peek.get::<[u8]>().unwrap();
                serialize_byte_array(bytes, writer, indent, depth)?;
            } else {
                let peek_list = peek.into_list_like().unwrap();
                serialize_array(peek_list.iter(), writer, indent, depth)?;
            }
        }
        (Def::Map(_), _) => {
            let peek_map = peek.into_map().unwrap();
            writer.write(b"{");
            let mut first = true;
            for (key, value) in peek_map.iter() {
                if !first {
                    writer.write(b",");
                }
                first = false;
                write_newline(writer, indent);
                write_indent(writer, indent, depth + 1);
                serialize_map_key(key, writer)?;
                write_colon(writer, indent);
                serialize_value(value, None, writer, indent, depth + 1)?;
            }
            if !first {
                write_newline(writer, indent);
                write_indent(writer, indent, depth);
            }
            writer.write(b"}");
        }
        (Def::Set(_), _) => {
            let peek_set = peek.into_set().unwrap();
            writer.write(b"[");
            let mut first = true;
            for item in peek_set.iter() {
                if !first {
                    writer.write(b",");
                }
                first = false;
                write_newline(writer, indent);
                write_indent(writer, indent, depth + 1);
                serialize_value(item, None, writer, indent, depth + 1)?;
            }
            if !first {
                write_newline(writer, indent);
                write_indent(writer, indent, depth);
            }
            writer.write(b"]");
        }
        (Def::Option(_), _) => {
            let opt = peek.into_option().unwrap();
            if let Some(inner_peek) = opt.value() {
                serialize_value(inner_peek, None, writer, indent, depth)?;
            } else {
                writer.write(b"null");
            }
        }
        (Def::Pointer(_), _) => {
            let sp = peek.into_pointer().unwrap();
            if let Some(inner_peek) = sp.borrow_inner() {
                serialize_value(inner_peek, None, writer, indent, depth)?;
            } else {
                panic!(
                    "Smart pointer without borrow support or with opaque pointee cannot be serialized"
                );
            }
        }
        (_, Type::User(UserType::Struct(sd))) => {
            trace!("Serializing struct: shape={}", peek.shape());
            trace!(
                "  Struct details: kind={:?}, field_count={}",
                sd.kind,
                sd.fields.len()
            );

            match sd.kind {
                StructKind::Unit => {
                    writer.write(b"null");
                }
                StructKind::Tuple => {
                    let peek_struct = peek.into_struct().unwrap();
                    writer.write(b"[");
                    let mut first = true;
                    for (field, value) in peek_struct.fields() {
                        if !first {
                            writer.write(b",");
                        }
                        first = false;
                        write_newline(writer, indent);
                        write_indent(writer, indent, depth + 1);
                        serialize_value(value, Some(field), writer, indent, depth + 1)?;
                    }
                    if !first {
                        write_newline(writer, indent);
                        write_indent(writer, indent, depth);
                    }
                    writer.write(b"]");
                }
                StructKind::TupleStruct => {
                    let peek_struct = peek.into_struct().unwrap();
                    writer.write(b"[");
                    let mut first = true;
                    for (field, value) in peek_struct.fields_for_serialize() {
                        if !first {
                            writer.write(b",");
                        }
                        first = false;
                        write_newline(writer, indent);
                        write_indent(writer, indent, depth + 1);
                        serialize_value(value, Some(field), writer, indent, depth + 1)?;
                    }
                    if !first {
                        write_newline(writer, indent);
                        write_indent(writer, indent, depth);
                    }
                    writer.write(b"]");
                }
                StructKind::Struct => {
                    let peek_struct = peek.into_struct().unwrap();
                    writer.write(b"{");
                    let mut first = true;
                    for (field, value) in peek_struct.fields_for_serialize() {
                        if !first {
                            writer.write(b",");
                        }
                        first = false;
                        write_newline(writer, indent);
                        write_indent(writer, indent, depth + 1);
                        crate::write_json_string(writer, field.name);
                        write_colon(writer, indent);
                        serialize_value(value, Some(field), writer, indent, depth + 1)?;
                    }
                    if !first {
                        write_newline(writer, indent);
                        write_indent(writer, indent, depth);
                    }
                    writer.write(b"}");
                }
            }
        }
        (_, Type::User(UserType::Enum(_))) => {
            let shape = peek.shape();
            let peek_enum = peek.into_enum().unwrap();
            let variant = peek_enum
                .active_variant()
                .expect("Failed to get active variant");
            let variant_index = peek_enum
                .variant_index()
                .expect("Failed to get variant index");
            trace!("Active variant index is {variant_index}, variant is {variant:?}");

            // Determine enum tagging strategy
            let is_untagged = shape.is_untagged();
            let tag_field = shape.get_tag_attr();
            let content_field = shape.get_content_attr();

            if is_untagged {
                // Untagged: serialize content directly without any tag
                serialize_enum_content(&peek_enum, variant, writer, indent, depth)?;
            } else if let Some(tag) = tag_field {
                if let Some(content) = content_field {
                    // Adjacently tagged: {"tag": "Variant", "content": ...}
                    writer.write(b"{");
                    write_newline(writer, indent);
                    write_indent(writer, indent, depth + 1);
                    crate::write_json_string(writer, tag);
                    write_colon(writer, indent);
                    crate::write_json_string(writer, variant.name);

                    // Only include content field if variant has data
                    if !variant.data.fields.is_empty() {
                        writer.write(b",");
                        write_newline(writer, indent);
                        write_indent(writer, indent, depth + 1);
                        crate::write_json_string(writer, content);
                        write_colon(writer, indent);
                        serialize_enum_content(&peek_enum, variant, writer, indent, depth + 1)?;
                    }

                    write_newline(writer, indent);
                    write_indent(writer, indent, depth);
                    writer.write(b"}");
                } else {
                    // Internally tagged: {"tag": "Variant", ...fields...}
                    writer.write(b"{");
                    write_newline(writer, indent);
                    write_indent(writer, indent, depth + 1);
                    crate::write_json_string(writer, tag);
                    write_colon(writer, indent);
                    crate::write_json_string(writer, variant.name);

                    // Add struct fields at same level as tag
                    for (field, field_peek) in peek_enum.fields_for_serialize() {
                        writer.write(b",");
                        write_newline(writer, indent);
                        write_indent(writer, indent, depth + 1);
                        crate::write_json_string(writer, field.name);
                        write_colon(writer, indent);
                        serialize_value(field_peek, Some(field), writer, indent, depth + 1)?;
                    }

                    write_newline(writer, indent);
                    write_indent(writer, indent, depth);
                    writer.write(b"}");
                }
            } else {
                // Externally tagged (default): {"Variant": content} or "Variant" for unit
                let flattened = maybe_field.map(|f| f.flattened).unwrap_or_default();

                if variant.data.fields.is_empty() {
                    // Unit variant - just the name as a string
                    crate::write_json_string(writer, variant.name);
                } else {
                    if !flattened {
                        // Wrap in object with variant name as key
                        writer.write(b"{");
                        write_newline(writer, indent);
                        write_indent(writer, indent, depth + 1);
                        crate::write_json_string(writer, variant.name);
                        write_colon(writer, indent);
                    }

                    let inner_depth = if flattened { depth } else { depth + 1 };
                    serialize_enum_content(&peek_enum, variant, writer, indent, inner_depth)?;

                    if !flattened {
                        write_newline(writer, indent);
                        write_indent(writer, indent, depth);
                        writer.write(b"}");
                    }
                }
            }
        }
        (_, Type::Pointer(pointer_type)) => {
            if let Some(str_value) = peek.as_str() {
                crate::write_json_string(writer, str_value);
            } else if let Some(bytes) = peek.as_bytes() {
                serialize_byte_array(bytes, writer, indent, depth)?;
            } else if let PointerType::Function(_) = pointer_type {
                writer.write(b"null");
            } else {
                let innermost = peek.innermost_peek();
                if innermost.shape() != peek.shape() {
                    serialize_value(innermost, None, writer, indent, depth)?;
                } else {
                    writer.write(b"null");
                }
            }
        }
        _ => {
            trace!(
                "Unhandled type: {:?}, falling back to null",
                peek.shape().ty
            );
            writer.write(b"null");
        }
    }

    Ok(())
}

/// Serialize a map key - JSON requires object keys to be strings
fn serialize_map_key<W: crate::JsonWrite>(
    peek: Peek<'_, '_>,
    writer: &mut W,
) -> Result<(), SerializeError> {
    // First try as_str() which handles &str, String, Cow<str>, etc uniformly
    if let Some(s) = peek.as_str() {
        crate::write_json_string(writer, s);
        return Ok(());
    }

    let peek = peek.innermost_peek();
    match peek.scalar_type() {
        // For numeric types, convert to string representation
        Some(ScalarType::U8) => {
            let v = *peek.get::<u8>().unwrap();
            writer.write(b"\"");
            writer.write(itoa::Buffer::new().format(v).as_bytes());
            writer.write(b"\"");
        }
        Some(ScalarType::U16) => {
            let v = *peek.get::<u16>().unwrap();
            writer.write(b"\"");
            writer.write(itoa::Buffer::new().format(v).as_bytes());
            writer.write(b"\"");
        }
        Some(ScalarType::U32) => {
            let v = *peek.get::<u32>().unwrap();
            writer.write(b"\"");
            writer.write(itoa::Buffer::new().format(v).as_bytes());
            writer.write(b"\"");
        }
        Some(ScalarType::U64) => {
            let v = *peek.get::<u64>().unwrap();
            writer.write(b"\"");
            writer.write(itoa::Buffer::new().format(v).as_bytes());
            writer.write(b"\"");
        }
        Some(ScalarType::U128) => {
            let v = *peek.get::<u128>().unwrap();
            writer.write(b"\"");
            writer.write(itoa::Buffer::new().format(v).as_bytes());
            writer.write(b"\"");
        }
        Some(ScalarType::USize) => {
            let v = *peek.get::<usize>().unwrap();
            writer.write(b"\"");
            writer.write(itoa::Buffer::new().format(v).as_bytes());
            writer.write(b"\"");
        }
        Some(ScalarType::I8) => {
            let v = *peek.get::<i8>().unwrap();
            writer.write(b"\"");
            writer.write(itoa::Buffer::new().format(v).as_bytes());
            writer.write(b"\"");
        }
        Some(ScalarType::I16) => {
            let v = *peek.get::<i16>().unwrap();
            writer.write(b"\"");
            writer.write(itoa::Buffer::new().format(v).as_bytes());
            writer.write(b"\"");
        }
        Some(ScalarType::I32) => {
            let v = *peek.get::<i32>().unwrap();
            writer.write(b"\"");
            writer.write(itoa::Buffer::new().format(v).as_bytes());
            writer.write(b"\"");
        }
        Some(ScalarType::I64) => {
            let v = *peek.get::<i64>().unwrap();
            writer.write(b"\"");
            writer.write(itoa::Buffer::new().format(v).as_bytes());
            writer.write(b"\"");
        }
        Some(ScalarType::I128) => {
            let v = *peek.get::<i128>().unwrap();
            writer.write(b"\"");
            writer.write(itoa::Buffer::new().format(v).as_bytes());
            writer.write(b"\"");
        }
        Some(ScalarType::ISize) => {
            let v = *peek.get::<isize>().unwrap();
            writer.write(b"\"");
            writer.write(itoa::Buffer::new().format(v).as_bytes());
            writer.write(b"\"");
        }
        _ => {
            // Fallback: use Display if available
            if peek.shape().vtable.display.is_some() {
                crate::write_json_string(writer, &alloc::format!("{peek}"));
            } else {
                panic!("Unsupported map key type: {}", peek.shape())
            }
        }
    }
    Ok(())
}

fn serialize_scalar<W: crate::JsonWrite>(
    peek: Peek<'_, '_>,
    writer: &mut W,
) -> Result<(), SerializeError> {
    match peek.scalar_type() {
        Some(ScalarType::Unit) => writer.write(b"null"),
        Some(ScalarType::Bool) => {
            let v = *peek.get::<bool>().unwrap();
            writer.write(if v { b"true" } else { b"false" });
        }
        Some(ScalarType::Char) => {
            let c = *peek.get::<char>().unwrap();
            writer.write(b"\"");
            crate::write_json_escaped_char(writer, c);
            writer.write(b"\"");
        }
        Some(ScalarType::Str) => {
            crate::write_json_string(writer, peek.get::<str>().unwrap());
        }
        Some(ScalarType::String) => {
            crate::write_json_string(writer, peek.get::<String>().unwrap());
        }
        Some(ScalarType::CowStr) => {
            crate::write_json_string(
                writer,
                peek.get::<alloc::borrow::Cow<'_, str>>().unwrap().as_ref(),
            );
        }
        Some(ScalarType::F32) => {
            let v = *peek.get::<f32>().unwrap();
            writer.write(ryu::Buffer::new().format(v).as_bytes());
        }
        Some(ScalarType::F64) => {
            let v = *peek.get::<f64>().unwrap();
            writer.write(ryu::Buffer::new().format(v).as_bytes());
        }
        Some(ScalarType::U8) => {
            let v = *peek.get::<u8>().unwrap();
            writer.write(itoa::Buffer::new().format(v).as_bytes());
        }
        Some(ScalarType::U16) => {
            let v = *peek.get::<u16>().unwrap();
            writer.write(itoa::Buffer::new().format(v).as_bytes());
        }
        Some(ScalarType::U32) => {
            let v = *peek.get::<u32>().unwrap();
            writer.write(itoa::Buffer::new().format(v).as_bytes());
        }
        Some(ScalarType::U64) => {
            let v = *peek.get::<u64>().unwrap();
            writer.write(itoa::Buffer::new().format(v).as_bytes());
        }
        Some(ScalarType::U128) => {
            let v = *peek.get::<u128>().unwrap();
            writer.write(itoa::Buffer::new().format(v).as_bytes());
        }
        Some(ScalarType::USize) => {
            let v = *peek.get::<usize>().unwrap();
            writer.write(itoa::Buffer::new().format(v).as_bytes());
        }
        Some(ScalarType::I8) => {
            let v = *peek.get::<i8>().unwrap();
            writer.write(itoa::Buffer::new().format(v).as_bytes());
        }
        Some(ScalarType::I16) => {
            let v = *peek.get::<i16>().unwrap();
            writer.write(itoa::Buffer::new().format(v).as_bytes());
        }
        Some(ScalarType::I32) => {
            let v = *peek.get::<i32>().unwrap();
            writer.write(itoa::Buffer::new().format(v).as_bytes());
        }
        Some(ScalarType::I64) => {
            let v = *peek.get::<i64>().unwrap();
            writer.write(itoa::Buffer::new().format(v).as_bytes());
        }
        Some(ScalarType::I128) => {
            let v = *peek.get::<i128>().unwrap();
            writer.write(itoa::Buffer::new().format(v).as_bytes());
        }
        Some(ScalarType::ISize) => {
            let v = *peek.get::<isize>().unwrap();
            writer.write(itoa::Buffer::new().format(v).as_bytes());
        }
        Some(unsupported) => {
            panic!("Unsupported scalar type: {unsupported:?}")
        }
        None => {
            // Try Display formatting if available
            if peek.shape().vtable.display.is_some() {
                crate::write_json_string(writer, &alloc::format!("{peek}"));
            } else {
                panic!("Unsupported shape (no display): {}", peek.shape())
            }
        }
    }
    Ok(())
}

fn serialize_array<'mem, 'facet, W: crate::JsonWrite>(
    iter: facet_reflect::PeekListLikeIter<'mem, 'facet>,
    writer: &mut W,
    indent: Option<&str>,
    depth: usize,
) -> Result<(), SerializeError> {
    writer.write(b"[");
    let mut first = true;
    for item in iter {
        if !first {
            writer.write(b",");
        }
        first = false;
        write_newline(writer, indent);
        write_indent(writer, indent, depth + 1);
        serialize_value(item, None, writer, indent, depth + 1)?;
    }
    if !first {
        write_newline(writer, indent);
        write_indent(writer, indent, depth);
    }
    writer.write(b"]");
    Ok(())
}

fn serialize_byte_array<W: crate::JsonWrite>(
    bytes: &[u8],
    writer: &mut W,
    indent: Option<&str>,
    depth: usize,
) -> Result<(), SerializeError> {
    writer.write(b"[");
    let mut first = true;
    for &byte in bytes {
        if !first {
            writer.write(b",");
        }
        first = false;
        write_newline(writer, indent);
        write_indent(writer, indent, depth + 1);
        writer.write(itoa::Buffer::new().format(byte).as_bytes());
    }
    if !first {
        write_newline(writer, indent);
        write_indent(writer, indent, depth);
    }
    writer.write(b"]");
    Ok(())
}

/// Serialize enum variant content (without any wrapper/tag)
fn serialize_enum_content<'mem, 'facet, W: crate::JsonWrite>(
    peek_enum: &facet_reflect::PeekEnum<'mem, 'facet>,
    variant: &facet_core::Variant,
    writer: &mut W,
    indent: Option<&str>,
    depth: usize,
) -> Result<(), SerializeError> {
    if variant.data.fields.is_empty() {
        // Unit variant - serialize as null for untagged
        writer.write(b"null");
    } else if variant_is_newtype_like(variant) {
        // Newtype variant - serialize the inner value directly
        let fields: Vec<_> = peek_enum.fields_for_serialize().collect();
        let (field, field_peek) = fields[0];
        serialize_value(field_peek, Some(field), writer, indent, depth)?;
    } else if variant.data.kind == StructKind::Tuple || variant.data.kind == StructKind::TupleStruct
    {
        // Tuple variant - serialize as array
        writer.write(b"[");
        let mut first = true;
        for (field, field_peek) in peek_enum.fields_for_serialize() {
            if !first {
                writer.write(b",");
            }
            first = false;
            write_newline(writer, indent);
            write_indent(writer, indent, depth + 1);
            serialize_value(field_peek, Some(field), writer, indent, depth + 1)?;
        }
        if !first {
            write_newline(writer, indent);
            write_indent(writer, indent, depth);
        }
        writer.write(b"]");
    } else {
        // Struct variant - serialize as object
        writer.write(b"{");
        let mut first = true;
        for (field, field_peek) in peek_enum.fields_for_serialize() {
            if !first {
                writer.write(b",");
            }
            first = false;
            write_newline(writer, indent);
            write_indent(writer, indent, depth + 1);
            crate::write_json_string(writer, field.name);
            write_colon(writer, indent);
            serialize_value(field_peek, Some(field), writer, indent, depth + 1)?;
        }
        if !first {
            write_newline(writer, indent);
            write_indent(writer, indent, depth);
        }
        writer.write(b"}");
    }
    Ok(())
}
