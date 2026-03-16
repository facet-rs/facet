use facet::Facet;
use facet_core::{Def, ScalarType, StructKind, Type, UserType};
use facet_reflect::Peek;

use crate::encode;
use crate::error::SerializeError;

/// Serialize any `Facet` type to postcard bytes.
pub fn to_vec<'a, T: Facet<'a>>(value: &T) -> Result<Vec<u8>, SerializeError> {
    let peek = Peek::new(value);
    let mut out = Vec::new();
    serialize_peek(peek, &mut out)?;
    Ok(out)
}

/// Serialize a `Peek` value to postcard, appending to `out`.
pub fn serialize_peek(peek: Peek<'_, '_>, out: &mut Vec<u8>) -> Result<(), SerializeError> {
    let peek = peek.innermost_peek();

    if let Some(scalar_type) = peek.scalar_type() {
        return serialize_scalar(peek, scalar_type, out);
    }

    // Def-based types before user types (Option<T> is both Def::Option and UserType::Enum)
    match peek.shape().def {
        Def::Option(_) => {
            let opt = peek
                .into_option()
                .map_err(|e| SerializeError::ReflectError(e.to_string()))?;
            return match opt.value() {
                Some(inner) => {
                    out.push(0x01);
                    serialize_peek(inner, out)
                }
                None => {
                    out.push(0x00);
                    Ok(())
                }
            };
        }
        Def::List(list_def) => {
            if list_def.t().is_type::<u8>() {
                // Vec<u8> → varint len + raw bytes
                let list = peek
                    .into_list()
                    .map_err(|e| SerializeError::ReflectError(e.to_string()))?;
                let len = list.len();
                let mut bytes = Vec::with_capacity(len);
                for i in 0..len {
                    let elem = list
                        .get(i)
                        .ok_or_else(|| SerializeError::ReflectError("list index OOB".into()))?;
                    let byte = elem
                        .get::<u8>()
                        .map_err(|e| SerializeError::ReflectError(e.to_string()))?;
                    bytes.push(*byte);
                }
                encode::write_varint(out, len as u64);
                out.extend_from_slice(&bytes);
            } else {
                let list = peek
                    .into_list()
                    .map_err(|e| SerializeError::ReflectError(e.to_string()))?;
                let len = list.len();
                encode::write_varint(out, len as u64);
                for elem in list.iter() {
                    serialize_peek(elem, out)?;
                }
            }
            return Ok(());
        }
        Def::Array(_) => {
            // Fixed-size array: NO length prefix
            let list_like = peek
                .into_list_like()
                .map_err(|e| SerializeError::ReflectError(e.to_string()))?;
            for elem in list_like.iter() {
                serialize_peek(elem, out)?;
            }
            return Ok(());
        }
        Def::Slice(_) => {
            let list_like = peek
                .into_list_like()
                .map_err(|e| SerializeError::ReflectError(e.to_string()))?;
            let len = list_like.len();
            encode::write_varint(out, len as u64);
            for elem in list_like.iter() {
                serialize_peek(elem, out)?;
            }
            return Ok(());
        }
        Def::Map(_) => {
            let map = peek
                .into_map()
                .map_err(|e| SerializeError::ReflectError(e.to_string()))?;
            encode::write_varint(out, map.len() as u64);
            for (key, value) in map.iter() {
                serialize_peek(key, out)?;
                serialize_peek(value, out)?;
            }
            return Ok(());
        }
        Def::Set(_) => {
            let set = peek
                .into_set()
                .map_err(|e| SerializeError::ReflectError(e.to_string()))?;
            encode::write_varint(out, set.len() as u64);
            for elem in set.iter() {
                serialize_peek(elem, out)?;
            }
            return Ok(());
        }
        Def::Pointer(_) => {
            let ptr = peek
                .into_pointer()
                .map_err(|e| SerializeError::ReflectError(e.to_string()))?;
            return match ptr.borrow_inner() {
                Some(inner) => serialize_peek(inner, out),
                None => Err(SerializeError::UnsupportedType("null pointer".into())),
            };
        }
        _ => {}
    }

    // User types: struct/enum
    match peek.shape().ty {
        Type::User(UserType::Struct(struct_type)) => match struct_type.kind {
            StructKind::Struct | StructKind::TupleStruct | StructKind::Tuple => {
                // All struct kinds: fields in order, no delimiters, no count prefix
                let ps = peek
                    .into_struct()
                    .map_err(|e| SerializeError::ReflectError(e.to_string()))?;
                for i in 0..ps.field_count() {
                    let field_peek = ps
                        .field(i)
                        .map_err(|e| SerializeError::ReflectError(e.to_string()))?;
                    serialize_peek(field_peek, out)?;
                }
                Ok(())
            }
            StructKind::Unit => Ok(()),
        },
        Type::User(UserType::Enum(_)) => {
            let pe = peek
                .into_enum()
                .map_err(|e| SerializeError::ReflectError(e.to_string()))?;
            let variant_index = pe
                .variant_index()
                .map_err(|e| SerializeError::ReflectError(e.to_string()))?;
            let variant = pe
                .active_variant()
                .map_err(|e| SerializeError::ReflectError(e.to_string()))?;

            encode::write_varint(out, variant_index as u64);

            match variant.data.kind {
                StructKind::Unit => {}
                StructKind::TupleStruct | StructKind::Tuple | StructKind::Struct => {
                    for i in 0..variant.data.fields.len() {
                        let field_peek = pe
                            .field(i)
                            .map_err(|e| SerializeError::ReflectError(e.to_string()))?
                            .ok_or_else(|| {
                                SerializeError::ReflectError("missing variant field".into())
                            })?;
                        serialize_peek(field_peek, out)?;
                    }
                }
            }
            Ok(())
        }
        _ => Err(SerializeError::UnsupportedType(format!("{}", peek.shape()))),
    }
}

fn serialize_scalar(
    peek: Peek<'_, '_>,
    scalar_type: ScalarType,
    out: &mut Vec<u8>,
) -> Result<(), SerializeError> {
    let re = |e: facet_reflect::ReflectError| SerializeError::ReflectError(e.to_string());
    match scalar_type {
        ScalarType::Unit => {}
        ScalarType::Bool => {
            let v = *peek.get::<bool>().map_err(re)?;
            out.push(if v { 0x01 } else { 0x00 });
        }
        ScalarType::Char => {
            let v = *peek.get::<char>().map_err(re)?;
            let mut buf = [0u8; 4];
            let s = v.encode_utf8(&mut buf);
            encode::write_varint(out, s.len() as u64);
            out.extend_from_slice(s.as_bytes());
        }
        ScalarType::U8 => {
            let v = *peek.get::<u8>().map_err(re)?;
            out.push(v);
        }
        ScalarType::U16 => {
            let v = *peek.get::<u16>().map_err(re)?;
            encode::write_varint(out, v as u64);
        }
        ScalarType::U32 => {
            let v = *peek.get::<u32>().map_err(re)?;
            encode::write_varint(out, v as u64);
        }
        ScalarType::U64 => {
            let v = *peek.get::<u64>().map_err(re)?;
            encode::write_varint(out, v);
        }
        ScalarType::U128 => {
            let v = *peek.get::<u128>().map_err(re)?;
            encode::write_varint_u128(out, v);
        }
        ScalarType::USize => {
            let v = *peek.get::<usize>().map_err(re)?;
            encode::write_varint(out, v as u64);
        }
        ScalarType::I8 => {
            let v = *peek.get::<i8>().map_err(re)?;
            out.push(v as u8);
        }
        ScalarType::I16 => {
            let v = *peek.get::<i16>().map_err(re)?;
            encode::write_varint_signed(out, v as i64);
        }
        ScalarType::I32 => {
            let v = *peek.get::<i32>().map_err(re)?;
            encode::write_varint_signed(out, v as i64);
        }
        ScalarType::I64 => {
            let v = *peek.get::<i64>().map_err(re)?;
            encode::write_varint_signed(out, v);
        }
        ScalarType::I128 => {
            let v = *peek.get::<i128>().map_err(re)?;
            encode::write_varint_signed_i128(out, v);
        }
        ScalarType::ISize => {
            let v = *peek.get::<isize>().map_err(re)?;
            encode::write_varint_signed(out, v as i64);
        }
        ScalarType::F32 => {
            let v = *peek.get::<f32>().map_err(re)?;
            out.extend_from_slice(&v.to_le_bytes());
        }
        ScalarType::F64 => {
            let v = *peek.get::<f64>().map_err(re)?;
            out.extend_from_slice(&v.to_le_bytes());
        }
        ScalarType::Str | ScalarType::String | ScalarType::CowStr => {
            let s = peek
                .as_str()
                .ok_or_else(|| SerializeError::ReflectError("failed to extract string".into()))?;
            encode::write_varint(out, s.len() as u64);
            out.extend_from_slice(s.as_bytes());
        }
        _ => {
            return Err(SerializeError::UnsupportedType(format!(
                "scalar type {scalar_type:?}"
            )));
        }
    }
    Ok(())
}
