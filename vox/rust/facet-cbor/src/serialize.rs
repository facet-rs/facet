use facet::Facet;
use facet_core::{Def, ScalarType, StructKind, Type, UserType};
use facet_reflect::Peek;

use crate::encode;
use crate::error::CborError;

/// Serialize any `Facet` type to CBOR bytes.
pub fn to_vec<'a, T: Facet<'a>>(value: &T) -> Result<Vec<u8>, CborError> {
    let peek = Peek::new(value);
    let mut out = Vec::new();
    serialize_peek(peek, &mut out)?;
    Ok(out)
}

/// Serialize a `Peek` value to CBOR, appending to `out`.
pub fn serialize_peek(peek: Peek<'_, '_>, out: &mut Vec<u8>) -> Result<(), CborError> {
    // Unwrap transparent wrappers (NonZero, newtypes, smart pointers, etc.)
    let peek = peek.innermost_peek();

    // Try scalar first
    if let Some(scalar_type) = peek.scalar_type() {
        return serialize_scalar(peek, scalar_type, out);
    }

    // Try struct/enum (user types)
    match peek.shape().ty {
        Type::User(UserType::Struct(struct_type)) => match struct_type.kind {
            StructKind::Struct => {
                let ps = peek
                    .into_struct()
                    .map_err(|e| CborError::ReflectError(e.to_string()))?;
                encode::write_map_header(out, ps.field_count() as u64);
                for i in 0..ps.field_count() {
                    let field = &struct_type.fields[i];
                    encode::write_text(out, field.name);
                    let field_peek = ps
                        .field(i)
                        .map_err(|e| CborError::ReflectError(e.to_string()))?;
                    serialize_peek(field_peek, out)?;
                }
                return Ok(());
            }
            StructKind::TupleStruct | StructKind::Tuple => {
                let ps = peek
                    .into_struct()
                    .map_err(|e| CborError::ReflectError(e.to_string()))?;
                encode::write_array_header(out, ps.field_count() as u64);
                for i in 0..ps.field_count() {
                    let field_peek = ps
                        .field(i)
                        .map_err(|e| CborError::ReflectError(e.to_string()))?;
                    serialize_peek(field_peek, out)?;
                }
                return Ok(());
            }
            StructKind::Unit => {
                encode::write_null(out);
                return Ok(());
            }
        },
        Type::User(UserType::Enum(_)) => {
            let pe = peek
                .into_enum()
                .map_err(|e| CborError::ReflectError(e.to_string()))?;
            let variant = pe
                .active_variant()
                .map_err(|e| CborError::ReflectError(e.to_string()))?;

            // Map with one entry: variant_name → payload
            encode::write_map_header(out, 1);
            encode::write_text(out, variant.name);

            match variant.data.kind {
                StructKind::Unit => {
                    encode::write_null(out);
                }
                StructKind::TupleStruct => {
                    // Newtype variant if exactly one field, otherwise tuple
                    if variant.data.fields.len() == 1 {
                        let field_peek = pe
                            .field(0)
                            .map_err(|e| CborError::ReflectError(e.to_string()))?
                            .ok_or_else(|| {
                                CborError::ReflectError("missing newtype variant field".into())
                            })?;
                        serialize_peek(field_peek, out)?;
                    } else {
                        encode::write_array_header(out, variant.data.fields.len() as u64);
                        for i in 0..variant.data.fields.len() {
                            let field_peek = pe
                                .field(i)
                                .map_err(|e| CborError::ReflectError(e.to_string()))?
                                .ok_or_else(|| {
                                    CborError::ReflectError("missing tuple variant field".into())
                                })?;
                            serialize_peek(field_peek, out)?;
                        }
                    }
                }
                StructKind::Tuple => {
                    encode::write_array_header(out, variant.data.fields.len() as u64);
                    for i in 0..variant.data.fields.len() {
                        let field_peek = pe
                            .field(i)
                            .map_err(|e| CborError::ReflectError(e.to_string()))?
                            .ok_or_else(|| {
                                CborError::ReflectError("missing tuple variant field".into())
                            })?;
                        serialize_peek(field_peek, out)?;
                    }
                }
                StructKind::Struct => {
                    encode::write_map_header(out, variant.data.fields.len() as u64);
                    for i in 0..variant.data.fields.len() {
                        let field = &variant.data.fields[i];
                        encode::write_text(out, field.name);
                        let field_peek = pe
                            .field(i)
                            .map_err(|e| CborError::ReflectError(e.to_string()))?
                            .ok_or_else(|| {
                                CborError::ReflectError("missing struct variant field".into())
                            })?;
                        serialize_peek(field_peek, out)?;
                    }
                }
            }
            return Ok(());
        }
        _ => {}
    }

    // Try def-based types
    match peek.shape().def {
        Def::List(list_def) => {
            // Special case: Vec<u8> → byte string
            if list_def.t().is_type::<u8>() {
                let list = peek
                    .into_list()
                    .map_err(|e| CborError::ReflectError(e.to_string()))?;
                let len = list.len();
                let mut bytes = Vec::with_capacity(len);
                for i in 0..len {
                    let elem = list.get(i).ok_or_else(|| {
                        CborError::ReflectError("list index out of bounds".into())
                    })?;
                    let byte = elem
                        .get::<u8>()
                        .map_err(|e| CborError::ReflectError(e.to_string()))?;
                    bytes.push(*byte);
                }
                encode::write_bytes(out, &bytes);
            } else {
                let list = peek
                    .into_list()
                    .map_err(|e| CborError::ReflectError(e.to_string()))?;
                let len = list.len();
                encode::write_array_header(out, len as u64);
                for elem in list.iter() {
                    serialize_peek(elem, out)?;
                }
            }
            Ok(())
        }
        Def::Array(_) | Def::Slice(_) => {
            let list_like = peek
                .into_list_like()
                .map_err(|e| CborError::ReflectError(e.to_string()))?;
            let len = list_like.len();
            encode::write_array_header(out, len as u64);
            for elem in list_like.iter() {
                serialize_peek(elem, out)?;
            }
            Ok(())
        }
        Def::Map(_) => {
            let map = peek
                .into_map()
                .map_err(|e| CborError::ReflectError(e.to_string()))?;
            encode::write_map_header(out, map.len() as u64);
            for (key, value) in map.iter() {
                serialize_peek(key, out)?;
                serialize_peek(value, out)?;
            }
            Ok(())
        }
        Def::Set(_) => {
            let set = peek
                .into_set()
                .map_err(|e| CborError::ReflectError(e.to_string()))?;
            encode::write_array_header(out, set.len() as u64);
            for elem in set.iter() {
                serialize_peek(elem, out)?;
            }
            Ok(())
        }
        Def::Option(_) => {
            let opt = peek
                .into_option()
                .map_err(|e| CborError::ReflectError(e.to_string()))?;
            match opt.value() {
                Some(inner) => serialize_peek(inner, out),
                None => {
                    encode::write_null(out);
                    Ok(())
                }
            }
        }
        Def::Pointer(_) => {
            let ptr = peek
                .into_pointer()
                .map_err(|e| CborError::ReflectError(e.to_string()))?;
            match ptr.borrow_inner() {
                Some(inner) => serialize_peek(inner, out),
                None => {
                    encode::write_null(out);
                    Ok(())
                }
            }
        }
        _ => Err(CborError::UnsupportedType(format!("{}", peek.shape()))),
    }
}

fn serialize_scalar(
    peek: Peek<'_, '_>,
    scalar_type: ScalarType,
    out: &mut Vec<u8>,
) -> Result<(), CborError> {
    match scalar_type {
        ScalarType::Unit => {
            encode::write_null(out);
        }
        ScalarType::Bool => {
            let v = peek
                .get::<bool>()
                .map_err(|e| CborError::ReflectError(e.to_string()))?;
            encode::write_bool(out, *v);
        }
        ScalarType::Char => {
            let v = peek
                .get::<char>()
                .map_err(|e| CborError::ReflectError(e.to_string()))?;
            let mut buf = [0u8; 4];
            let s = v.encode_utf8(&mut buf);
            encode::write_text(out, s);
        }
        ScalarType::U8 => {
            let v = peek
                .get::<u8>()
                .map_err(|e| CborError::ReflectError(e.to_string()))?;
            encode::write_uint(out, *v as u64);
        }
        ScalarType::U16 => {
            let v = peek
                .get::<u16>()
                .map_err(|e| CborError::ReflectError(e.to_string()))?;
            encode::write_uint(out, *v as u64);
        }
        ScalarType::U32 => {
            let v = peek
                .get::<u32>()
                .map_err(|e| CborError::ReflectError(e.to_string()))?;
            encode::write_uint(out, *v as u64);
        }
        ScalarType::U64 => {
            let v = peek
                .get::<u64>()
                .map_err(|e| CborError::ReflectError(e.to_string()))?;
            encode::write_uint(out, *v);
        }
        ScalarType::U128 => {
            let v = peek
                .get::<u128>()
                .map_err(|e| CborError::ReflectError(e.to_string()))?;
            // CBOR only supports up to u64; encode as byte string for u128
            if *v <= u64::MAX as u128 {
                encode::write_uint(out, *v as u64);
            } else {
                // Tag 2 (positive bignum) + 16-byte big-endian
                out.push(0xc2);
                let bytes = v.to_be_bytes();
                // Strip leading zeros
                let start = bytes.iter().position(|&b| b != 0).unwrap_or(15);
                encode::write_bytes(out, &bytes[start..]);
            }
        }
        ScalarType::USize => {
            let v = peek
                .get::<usize>()
                .map_err(|e| CborError::ReflectError(e.to_string()))?;
            encode::write_uint(out, *v as u64);
        }
        ScalarType::I8 => {
            let v = peek
                .get::<i8>()
                .map_err(|e| CborError::ReflectError(e.to_string()))?;
            write_signed(out, *v as i64);
        }
        ScalarType::I16 => {
            let v = peek
                .get::<i16>()
                .map_err(|e| CborError::ReflectError(e.to_string()))?;
            write_signed(out, *v as i64);
        }
        ScalarType::I32 => {
            let v = peek
                .get::<i32>()
                .map_err(|e| CborError::ReflectError(e.to_string()))?;
            write_signed(out, *v as i64);
        }
        ScalarType::I64 => {
            let v = peek
                .get::<i64>()
                .map_err(|e| CborError::ReflectError(e.to_string()))?;
            write_signed(out, *v);
        }
        ScalarType::I128 => {
            let v = peek
                .get::<i128>()
                .map_err(|e| CborError::ReflectError(e.to_string()))?;
            if *v >= 0 {
                if *v <= u64::MAX as i128 {
                    encode::write_uint(out, *v as u64);
                } else {
                    // Tag 2 (positive bignum)
                    out.push(0xc2);
                    let bytes = (*v as u128).to_be_bytes();
                    let start = bytes.iter().position(|&b| b != 0).unwrap_or(15);
                    encode::write_bytes(out, &bytes[start..]);
                }
            } else {
                // For negative: CBOR major 1 encodes -1-n, so n = -1 - v
                let abs_minus_one = (-1i128 - *v) as u128;
                if abs_minus_one <= u64::MAX as u128 {
                    encode::write_neg(out, abs_minus_one as u64);
                } else {
                    // Tag 3 (negative bignum)
                    out.push(0xc3);
                    let bytes = abs_minus_one.to_be_bytes();
                    let start = bytes.iter().position(|&b| b != 0).unwrap_or(15);
                    encode::write_bytes(out, &bytes[start..]);
                }
            }
        }
        ScalarType::ISize => {
            let v = peek
                .get::<isize>()
                .map_err(|e| CborError::ReflectError(e.to_string()))?;
            write_signed(out, *v as i64);
        }
        ScalarType::F32 => {
            let v = peek
                .get::<f32>()
                .map_err(|e| CborError::ReflectError(e.to_string()))?;
            encode::write_f32(out, *v);
        }
        ScalarType::F64 => {
            let v = peek
                .get::<f64>()
                .map_err(|e| CborError::ReflectError(e.to_string()))?;
            encode::write_f64(out, *v);
        }
        ScalarType::Str | ScalarType::String | ScalarType::CowStr => {
            let s = peek
                .as_str()
                .ok_or_else(|| CborError::ReflectError("failed to extract string value".into()))?;
            encode::write_text(out, s);
        }
        _ => {
            return Err(CborError::UnsupportedType(format!(
                "scalar type {scalar_type:?}"
            )));
        }
    }
    Ok(())
}

fn write_signed(out: &mut Vec<u8>, v: i64) {
    if v >= 0 {
        encode::write_uint(out, v as u64);
    } else {
        // CBOR major 1: encode -1-n, so for value v, n = -1 - v
        encode::write_neg(out, (-1i64 - v) as u64);
    }
}
