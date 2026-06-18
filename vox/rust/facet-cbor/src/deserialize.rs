use facet::Facet;
use facet_core::{Def, ScalarType, StructKind, Type, UserType};
use facet_reflect::Partial;

use crate::decode;
use crate::error::CborError;

/// Deserialize a CBOR byte slice into a value of type `T`.
pub fn from_slice<T: Facet<'static>>(bytes: &[u8]) -> Result<T, CborError> {
    let partial =
        Partial::alloc_owned::<T>().map_err(|e| CborError::ReflectError(e.to_string()))?;
    let mut offset = 0;
    let partial = deserialize_into(partial, bytes, &mut offset)?;
    let heap_value = partial
        .build()
        .map_err(|e| CborError::ReflectError(e.to_string()))?;
    heap_value
        .materialize()
        .map_err(|e| CborError::ReflectError(e.to_string()))
}

/// Recursively deserialize CBOR data into a Partial, dispatching on the shape.
fn deserialize_into<'facet>(
    partial: Partial<'facet, false>,
    input: &[u8],
    offset: &mut usize,
) -> Result<Partial<'facet, false>, CborError> {
    let shape = partial.shape();

    // Unwrap transparent wrappers (newtypes, NonZero, etc.) to match serialization
    if shape.is_transparent() {
        let re = |e: facet_reflect::ReflectError| CborError::ReflectError(e.to_string());
        let partial = partial.begin_inner().map_err(re)?;
        let partial = deserialize_into(partial, input, offset)?;
        return partial.end().map_err(re);
    }

    // Check for scalar types first
    if let Some(scalar_type) = shape.scalar_type() {
        return deserialize_scalar(partial, scalar_type, input, offset);
    }

    // Check def-based types (Option, List, Map, etc.) before user types,
    // mirroring the serialization order where Def::Option is checked before UserType::Enum.
    match shape.def {
        Def::Option(_) => {
            return deserialize_option(partial, input, offset);
        }
        Def::List(list_def) => {
            // Special case: Vec<u8> → byte string
            if list_def.t().is_type::<u8>() {
                return deserialize_byte_list(partial, input, offset);
            }
            return deserialize_list(partial, input, offset);
        }
        Def::Array(array_def) => {
            return deserialize_array(partial, array_def.n, input, offset);
        }
        Def::Map(_) => {
            return deserialize_map(partial, input, offset);
        }
        Def::Pointer(_) => {
            return deserialize_pointer(partial, input, offset);
        }
        _ => {}
    }

    // Try struct/enum (user types)
    match shape.ty {
        Type::User(UserType::Struct(struct_type)) => match struct_type.kind {
            StructKind::Struct => deserialize_struct(partial, input, offset),
            StructKind::TupleStruct | StructKind::Tuple => {
                deserialize_tuple(partial, struct_type.fields.len(), input, offset)
            }
            StructKind::Unit => {
                decode::read_null(input, offset)?;
                Ok(partial)
            }
        },
        Type::User(UserType::Enum(_)) => {
            if shape.tag.is_some() {
                deserialize_enum_internally_tagged(partial, input, offset)
            } else {
                deserialize_enum(partial, input, offset)
            }
        }
        _ => Err(CborError::UnsupportedType(format!("{}", shape))),
    }
}

fn deserialize_scalar<'facet>(
    partial: Partial<'facet, false>,
    scalar_type: ScalarType,
    input: &[u8],
    offset: &mut usize,
) -> Result<Partial<'facet, false>, CborError> {
    let re = |e: facet_reflect::ReflectError| CborError::ReflectError(e.to_string());
    match scalar_type {
        ScalarType::Unit => {
            decode::read_null(input, offset)?;
            partial.set(()).map_err(re)
        }
        ScalarType::Bool => {
            let v = decode::read_bool(input, offset)?;
            partial.set(v).map_err(re)
        }
        ScalarType::Char => {
            let s = decode::read_text(input, offset)?;
            let c = s
                .chars()
                .next()
                .ok_or_else(|| CborError::InvalidCbor("empty text string for char".into()))?;
            partial.set(c).map_err(re)
        }
        ScalarType::U8 => {
            let v = decode::read_int_as_u64(input, offset)?;
            partial.set(v as u8).map_err(re)
        }
        ScalarType::U16 => {
            let v = decode::read_int_as_u64(input, offset)?;
            partial.set(v as u16).map_err(re)
        }
        ScalarType::U32 => {
            let v = decode::read_int_as_u64(input, offset)?;
            partial.set(v as u32).map_err(re)
        }
        ScalarType::U64 => {
            let v = decode::read_int_as_u64(input, offset)?;
            partial.set(v).map_err(re)
        }
        ScalarType::USize => {
            let v = decode::read_int_as_u64(input, offset)?;
            partial.set(v as usize).map_err(re)
        }
        ScalarType::I8 => {
            let v = decode::read_int_as_i64(input, offset)?;
            partial.set(v as i8).map_err(re)
        }
        ScalarType::I16 => {
            let v = decode::read_int_as_i64(input, offset)?;
            partial.set(v as i16).map_err(re)
        }
        ScalarType::I32 => {
            let v = decode::read_int_as_i64(input, offset)?;
            partial.set(v as i32).map_err(re)
        }
        ScalarType::I64 => {
            let v = decode::read_int_as_i64(input, offset)?;
            partial.set(v).map_err(re)
        }
        ScalarType::ISize => {
            let v = decode::read_int_as_i64(input, offset)?;
            partial.set(v as isize).map_err(re)
        }
        ScalarType::F32 => {
            let v = decode::read_f32(input, offset)?;
            partial.set(v).map_err(re)
        }
        ScalarType::F64 => {
            let v = decode::read_f64(input, offset)?;
            partial.set(v).map_err(re)
        }
        ScalarType::String => {
            let s = decode::read_text(input, offset)?;
            partial.set(s.to_owned()).map_err(re)
        }
        ScalarType::Str => {
            // &str can't be deserialized into owned Partial (would need borrowed)
            let s = decode::read_text(input, offset)?;
            partial.set(s.to_owned()).map_err(re)
        }
        ScalarType::CowStr => {
            let s = decode::read_text(input, offset)?;
            partial
                .set(std::borrow::Cow::<'static, str>::Owned(s.to_owned()))
                .map_err(re)
        }
        _ => Err(CborError::UnsupportedType(format!(
            "scalar type {scalar_type:?}"
        ))),
    }
}

fn deserialize_option<'facet>(
    partial: Partial<'facet, false>,
    input: &[u8],
    offset: &mut usize,
) -> Result<Partial<'facet, false>, CborError> {
    let re = |e: facet_reflect::ReflectError| CborError::ReflectError(e.to_string());
    if decode::is_null(input, *offset) {
        // Consume the null byte, leave Option as None
        *offset += 1;
        Ok(partial)
    } else {
        // begin_some, deserialize inner, end
        let partial = partial.begin_some().map_err(re)?;
        let partial = deserialize_into(partial, input, offset)?;
        partial.end().map_err(re)
    }
}

fn deserialize_list<'facet>(
    partial: Partial<'facet, false>,
    input: &[u8],
    offset: &mut usize,
) -> Result<Partial<'facet, false>, CborError> {
    let re = |e: facet_reflect::ReflectError| CborError::ReflectError(e.to_string());
    let len = decode::read_array_header(input, offset)? as usize;
    let mut partial = partial.init_list_with_capacity(len).map_err(re)?;
    for _ in 0..len {
        partial = partial.begin_list_item().map_err(re)?;
        partial = deserialize_into(partial, input, offset)?;
        partial = partial.end().map_err(re)?;
    }
    Ok(partial)
}

fn deserialize_byte_list<'facet>(
    partial: Partial<'facet, false>,
    input: &[u8],
    offset: &mut usize,
) -> Result<Partial<'facet, false>, CborError> {
    let re = |e: facet_reflect::ReflectError| CborError::ReflectError(e.to_string());
    let bytes = decode::read_bytes(input, offset)?;
    // Build Vec<u8> from the byte string and set it directly
    let vec: Vec<u8> = bytes.to_vec();
    partial.set(vec).map_err(re)
}

fn deserialize_array<'facet>(
    partial: Partial<'facet, false>,
    expected_len: usize,
    input: &[u8],
    offset: &mut usize,
) -> Result<Partial<'facet, false>, CborError> {
    let re = |e: facet_reflect::ReflectError| CborError::ReflectError(e.to_string());
    let len = decode::read_array_header(input, offset)? as usize;
    if len != expected_len {
        return Err(CborError::TypeMismatch {
            expected: format!("array of length {expected_len}"),
            got: format!("array of length {len}"),
        });
    }
    // Fixed-size arrays use begin_nth_field like tuples
    let mut partial = partial;
    for i in 0..len {
        partial = partial.begin_nth_field(i).map_err(re)?;
        partial = deserialize_into(partial, input, offset)?;
        partial = partial.end().map_err(re)?;
    }
    Ok(partial)
}

fn deserialize_map<'facet>(
    partial: Partial<'facet, false>,
    input: &[u8],
    offset: &mut usize,
) -> Result<Partial<'facet, false>, CborError> {
    let re = |e: facet_reflect::ReflectError| CborError::ReflectError(e.to_string());
    let len = decode::read_map_header(input, offset)? as usize;
    let mut partial = partial.init_map().map_err(re)?;
    for _ in 0..len {
        // key
        partial = partial.begin_key().map_err(re)?;
        partial = deserialize_into(partial, input, offset)?;
        partial = partial.end().map_err(re)?;
        // value
        partial = partial.begin_value().map_err(re)?;
        partial = deserialize_into(partial, input, offset)?;
        partial = partial.end().map_err(re)?;
    }
    Ok(partial)
}

fn deserialize_struct<'facet>(
    partial: Partial<'facet, false>,
    input: &[u8],
    offset: &mut usize,
) -> Result<Partial<'facet, false>, CborError> {
    let re = |e: facet_reflect::ReflectError| CborError::ReflectError(e.to_string());
    let len = decode::read_map_header(input, offset)? as usize;
    let mut partial = partial;
    for _ in 0..len {
        let key = decode::read_text(input, offset)?;
        // Try to find the field; if unknown, skip the value
        if partial.field_index(key).is_some() {
            partial = partial.begin_field(key).map_err(re)?;
            partial = deserialize_into(partial, input, offset)?;
            partial = partial.end().map_err(re)?;
        } else {
            decode::skip_value(input, offset)?;
        }
    }
    Ok(partial)
}

fn deserialize_tuple<'facet>(
    partial: Partial<'facet, false>,
    field_count: usize,
    input: &[u8],
    offset: &mut usize,
) -> Result<Partial<'facet, false>, CborError> {
    let re = |e: facet_reflect::ReflectError| CborError::ReflectError(e.to_string());
    let len = decode::read_array_header(input, offset)? as usize;
    if len != field_count {
        return Err(CborError::TypeMismatch {
            expected: format!("array of length {field_count}"),
            got: format!("array of length {len}"),
        });
    }
    let mut partial = partial;
    for i in 0..len {
        partial = partial.begin_nth_field(i).map_err(re)?;
        partial = deserialize_into(partial, input, offset)?;
        partial = partial.end().map_err(re)?;
    }
    Ok(partial)
}

fn deserialize_enum<'facet>(
    partial: Partial<'facet, false>,
    input: &[u8],
    offset: &mut usize,
) -> Result<Partial<'facet, false>, CborError> {
    let re = |e: facet_reflect::ReflectError| CborError::ReflectError(e.to_string());

    // Encoded as a map with 1 entry: variant_name → payload
    let map_len = decode::read_map_header(input, offset)?;
    if map_len != 1 {
        return Err(CborError::InvalidCbor(format!(
            "expected map with 1 entry for enum, got {map_len}"
        )));
    }

    let variant_name = decode::read_text(input, offset)?;

    // Find the variant by name and get its info before selecting
    let (_, variant) = partial
        .find_variant(variant_name)
        .ok_or_else(|| CborError::InvalidCbor(format!("unknown enum variant: {variant_name}")))?;
    let kind = variant.data.kind;
    let field_count = variant.data.fields.len();

    let mut partial = partial.select_variant_named(variant_name).map_err(re)?;

    match kind {
        StructKind::Unit => {
            // Unit variant: payload is null
            decode::read_null(input, offset)?;
        }
        StructKind::TupleStruct => {
            if field_count == 1 {
                // Newtype variant: payload is the single value directly
                partial = partial.begin_nth_field(0).map_err(re)?;
                partial = deserialize_into(partial, input, offset)?;
                partial = partial.end().map_err(re)?;
            } else {
                // Tuple variant: payload is an array
                let arr_len = decode::read_array_header(input, offset)? as usize;
                if arr_len != field_count {
                    return Err(CborError::TypeMismatch {
                        expected: format!("array of length {field_count}"),
                        got: format!("array of length {arr_len}"),
                    });
                }
                for i in 0..field_count {
                    partial = partial.begin_nth_field(i).map_err(re)?;
                    partial = deserialize_into(partial, input, offset)?;
                    partial = partial.end().map_err(re)?;
                }
            }
        }
        StructKind::Tuple => {
            // Tuple variant: payload is an array
            let arr_len = decode::read_array_header(input, offset)? as usize;
            if arr_len != field_count {
                return Err(CborError::TypeMismatch {
                    expected: format!("array of length {field_count}"),
                    got: format!("array of length {arr_len}"),
                });
            }
            for i in 0..field_count {
                partial = partial.begin_nth_field(i).map_err(re)?;
                partial = deserialize_into(partial, input, offset)?;
                partial = partial.end().map_err(re)?;
            }
        }
        StructKind::Struct => {
            // Struct variant: payload is a map
            let map_len = decode::read_map_header(input, offset)? as usize;
            for _ in 0..map_len {
                let field_name = decode::read_text(input, offset)?;
                if partial.field_index(field_name).is_some() {
                    partial = partial.begin_field(field_name).map_err(re)?;
                    partial = deserialize_into(partial, input, offset)?;
                    partial = partial.end().map_err(re)?;
                } else {
                    decode::skip_value(input, offset)?;
                }
            }
        }
    }

    Ok(partial)
}

/// Deserialize an internally-tagged enum.
///
/// When `#[facet(tag = "...")]` is set:
/// - If the CBOR value is a text string → unit variant (the string is the variant name)
/// - If the CBOR value is a map → read the tag field to find variant name, rest are struct fields
fn deserialize_enum_internally_tagged<'facet>(
    partial: Partial<'facet, false>,
    input: &[u8],
    offset: &mut usize,
) -> Result<Partial<'facet, false>, CborError> {
    let re = |e: facet_reflect::ReflectError| CborError::ReflectError(e.to_string());
    let tag_key = partial
        .shape()
        .tag
        .expect("internally-tagged enum must have tag");

    if *offset >= input.len() {
        return Err(CborError::InvalidCbor("unexpected end of input".into()));
    }

    let major = input[*offset] >> 5;
    if major == 3 {
        // Text string → unit variant
        let variant_name = decode::read_text(input, offset)?;
        let partial = partial.select_variant_named(variant_name).map_err(re)?;
        Ok(partial)
    } else if major == 5 {
        // Map → struct variant with tag field
        let map_len = decode::read_map_header(input, offset)? as usize;

        // First, find the tag field to determine which variant we're deserializing.
        // We need to scan through map entries to find the tag key.
        // For efficiency, we expect the tag to be the first entry.
        let mut variant_name: Option<&str> = None;
        let mut saved_offset = *offset;

        // Read the first key — it should be the tag
        let first_key = decode::read_text(input, offset)?;
        if first_key == tag_key {
            variant_name = Some(decode::read_text(input, offset)?);
        } else {
            // Tag wasn't first; scan the whole map from the start
            *offset = saved_offset;
            let scan_offset = &mut saved_offset;
            *scan_offset = *offset;
            for _ in 0..map_len {
                let key = decode::read_text(input, scan_offset)?;
                if key == tag_key {
                    variant_name = Some(decode::read_text(input, scan_offset)?);
                    break;
                }
                decode::skip_value(input, scan_offset)?;
            }
            // Reset to after the first key we already read
        }

        let variant_name = variant_name.ok_or_else(|| {
            CborError::InvalidCbor(format!(
                "internally-tagged enum map missing '{}' field",
                tag_key
            ))
        })?;

        // Find variant info before selecting
        let (_, variant) = partial.find_variant(variant_name).ok_or_else(|| {
            CborError::InvalidCbor(format!("unknown enum variant: {variant_name}"))
        })?;
        let kind = variant.data.kind;

        if kind != StructKind::Struct {
            return Err(CborError::InvalidCbor(format!(
                "internally-tagged enum variant '{}' must be a struct variant",
                variant_name
            )));
        }

        let mut partial = partial.select_variant_named(variant_name).map_err(re)?;

        // Now re-read the map from the beginning, skipping the tag field,
        // and deserializing all other fields as struct fields.
        // We need to re-parse from after the map header.
        // Actually, we've already consumed the first key. Let's handle this properly.
        // Reset offset to after map header and re-read all entries.
        // But we already consumed some bytes... Let me restructure.

        // We consumed: map_header + first_key("tag") + first_value(variant_name)
        // Now read remaining map_len - 1 entries as struct fields
        for _ in 1..map_len {
            let field_name = decode::read_text(input, offset)?;
            if field_name == tag_key {
                // Skip duplicate tag field
                decode::skip_value(input, offset)?;
            } else if partial.field_index(field_name).is_some() {
                partial = partial.begin_field(field_name).map_err(re)?;
                partial = deserialize_into(partial, input, offset)?;
                partial = partial.end().map_err(re)?;
            } else {
                decode::skip_value(input, offset)?;
            }
        }

        Ok(partial)
    } else {
        Err(CborError::InvalidCbor(format!(
            "internally-tagged enum expected text string or map, got major type {}",
            major
        )))
    }
}

fn deserialize_pointer<'facet>(
    partial: Partial<'facet, false>,
    input: &[u8],
    offset: &mut usize,
) -> Result<Partial<'facet, false>, CborError> {
    let re = |e: facet_reflect::ReflectError| CborError::ReflectError(e.to_string());
    if decode::is_null(input, *offset) {
        *offset += 1;
        Ok(partial)
    } else {
        let partial = partial.begin_smart_ptr().map_err(re)?;
        let partial = deserialize_into(partial, input, offset)?;
        partial.end().map_err(re)
    }
}
