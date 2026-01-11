#![deny(unsafe_code)]

//! Hashing and method identity per `docs/content/rust-spec/_index.md`.
//!
//! This crate encodes types using `facet::Shape` for signature hashing.

use facet_core::{Def, ScalarType, Shape, StructKind, Type, UserType};
use heck::ToKebabCase;
use roam_schema::{MethodDetail, is_rx, is_tx};

/// Signature encoding tags for type serialization.
mod sig {
    // Primitives (0x01-0x11)
    pub const BOOL: u8 = 0x01;
    pub const U8: u8 = 0x02;
    pub const U16: u8 = 0x03;
    pub const U32: u8 = 0x04;
    pub const U64: u8 = 0x05;
    pub const U128: u8 = 0x06;
    pub const I8: u8 = 0x07;
    pub const I16: u8 = 0x08;
    pub const I32: u8 = 0x09;
    pub const I64: u8 = 0x0A;
    pub const I128: u8 = 0x0B;
    pub const F32: u8 = 0x0C;
    pub const F64: u8 = 0x0D;
    pub const CHAR: u8 = 0x0E;
    pub const STRING: u8 = 0x0F;
    pub const UNIT: u8 = 0x10;
    pub const BYTES: u8 = 0x11;

    // Containers (0x20-0x27)
    pub const LIST: u8 = 0x20;
    pub const OPTION: u8 = 0x21;
    pub const ARRAY: u8 = 0x22;
    pub const MAP: u8 = 0x23;
    pub const SET: u8 = 0x24;
    pub const TUPLE: u8 = 0x25;
    pub const TX: u8 = 0x26;
    pub const RX: u8 = 0x27;

    // Composite (0x30-0x31)
    pub const STRUCT: u8 = 0x30;
    pub const ENUM: u8 = 0x31;

    // Variant payloads
    pub const VARIANT_UNIT: u8 = 0x00;
    pub const VARIANT_NEWTYPE: u8 = 0x01;
    pub const VARIANT_STRUCT: u8 = 0x02;
}

// rs[impl signature.varint] - encode unsigned integers as varints
pub fn encode_varint_u64(mut value: u64, out: &mut Vec<u8>) {
    while value >= 0x80 {
        out.push((value as u8) | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

fn encode_string(s: &str, out: &mut Vec<u8>) {
    encode_varint_u64(s.len() as u64, out);
    out.extend_from_slice(s.as_bytes());
}

/// Encode a `Shape` into its canonical signature byte representation.
// rs[impl signature.primitive] - encode primitive types
// rs[impl signature.container] - encode container types (List, Option, Array, Map, Set, Tuple)
// rs[impl signature.struct] - encode struct types
// rs[impl signature.enum] - encode enum types
// rs[impl signature.stream] - encode Tx/Rx stream types
pub fn encode_shape(shape: &'static Shape, out: &mut Vec<u8>) {
    // Check for roam streaming types first (marked with #[facet(roam::tx)] or #[facet(roam::rx)])
    if is_tx(shape) {
        out.push(sig::TX);
        if let Some(inner) = shape.type_params.first() {
            encode_shape(inner.shape, out);
        }
        return;
    }

    if is_rx(shape) {
        out.push(sig::RX);
        if let Some(inner) = shape.type_params.first() {
            encode_shape(inner.shape, out);
        }
        return;
    }

    // Handle transparent wrappers - encode as inner type
    // Only if marked with #[repr(transparent)] or #[facet(transparent)]
    if shape.is_transparent()
        && let Some(inner) = shape.inner
    {
        encode_shape(inner, out);
        return;
    }

    // Try scalar types first - this handles primitives, String, etc.
    if let Some(scalar) = shape.scalar_type() {
        encode_scalar(scalar, out);
        return;
    }

    // Handle semantic definitions (List, Map, Option, etc.)
    match shape.def {
        Def::List(list_def) => {
            // Check for Vec<u8> -> Bytes equivalence
            if let Some(ScalarType::U8) = list_def.t().scalar_type() {
                out.push(sig::BYTES);
            } else {
                out.push(sig::LIST);
                encode_shape(list_def.t(), out);
            }
            return;
        }
        Def::Array(array_def) => {
            out.push(sig::ARRAY);
            encode_varint_u64(array_def.n as u64, out);
            encode_shape(array_def.t(), out);
            return;
        }
        Def::Slice(slice_def) => {
            // Slices encode like lists
            out.push(sig::LIST);
            encode_shape(slice_def.t(), out);
            return;
        }
        Def::Map(map_def) => {
            out.push(sig::MAP);
            encode_shape(map_def.k(), out);
            encode_shape(map_def.v(), out);
            return;
        }
        Def::Set(set_def) => {
            out.push(sig::SET);
            encode_shape(set_def.t(), out);
            return;
        }
        Def::Option(opt_def) => {
            out.push(sig::OPTION);
            encode_shape(opt_def.t(), out);
            return;
        }
        Def::Pointer(ptr_def) => {
            // Smart pointers are transparent - encode inner type
            if let Some(pointee) = ptr_def.pointee {
                encode_shape(pointee, out);
                return;
            }
        }
        _ => {}
    }

    // Handle user-defined types (structs, enums)
    match shape.ty {
        Type::User(UserType::Struct(struct_type)) => {
            match struct_type.kind {
                StructKind::Unit => {
                    out.push(sig::UNIT);
                }
                StructKind::TupleStruct | StructKind::Tuple => {
                    // Tuple structs and tuples encode as tuples
                    out.push(sig::TUPLE);
                    encode_varint_u64(struct_type.fields.len() as u64, out);
                    for field in struct_type.fields {
                        encode_shape(field.shape(), out);
                    }
                }
                StructKind::Struct => {
                    out.push(sig::STRUCT);
                    encode_varint_u64(struct_type.fields.len() as u64, out);
                    for field in struct_type.fields {
                        encode_string(field.name, out);
                        encode_shape(field.shape(), out);
                    }
                }
            }
        }
        Type::User(UserType::Enum(enum_type)) => {
            out.push(sig::ENUM);
            encode_varint_u64(enum_type.variants.len() as u64, out);
            for variant in enum_type.variants {
                encode_string(variant.name, out);
                match variant.data.kind {
                    StructKind::Unit => {
                        out.push(sig::VARIANT_UNIT);
                    }
                    StructKind::TupleStruct | StructKind::Tuple => {
                        if variant.data.fields.len() == 1 {
                            // Single-field tuple variant = newtype
                            out.push(sig::VARIANT_NEWTYPE);
                            encode_shape(variant.data.fields[0].shape(), out);
                        } else {
                            // Multi-field tuple variant encodes like struct with numeric keys
                            out.push(sig::VARIANT_STRUCT);
                            encode_varint_u64(variant.data.fields.len() as u64, out);
                            for (i, field) in variant.data.fields.iter().enumerate() {
                                encode_string(&i.to_string(), out);
                                encode_shape(field.shape(), out);
                            }
                        }
                    }
                    StructKind::Struct => {
                        out.push(sig::VARIANT_STRUCT);
                        encode_varint_u64(variant.data.fields.len() as u64, out);
                        for field in variant.data.fields {
                            encode_string(field.name, out);
                            encode_shape(field.shape(), out);
                        }
                    }
                }
            }
        }
        Type::Pointer(_) => {
            // References are transparent - encode the inner type via type_params
            if let Some(inner) = shape.type_params.first() {
                encode_shape(inner.shape, out);
            } else {
                out.push(sig::UNIT); // Fallback
            }
        }
        _ => {
            // Unknown type - encode as unit
            out.push(sig::UNIT);
        }
    }
}

fn encode_scalar(scalar: ScalarType, out: &mut Vec<u8>) {
    match scalar {
        ScalarType::Unit => out.push(sig::UNIT),
        ScalarType::Bool => out.push(sig::BOOL),
        ScalarType::Char => out.push(sig::CHAR),
        ScalarType::Str => out.push(sig::STRING),
        ScalarType::String => out.push(sig::STRING),
        ScalarType::CowStr => out.push(sig::STRING),
        ScalarType::F32 => out.push(sig::F32),
        ScalarType::F64 => out.push(sig::F64),
        ScalarType::U8 => out.push(sig::U8),
        ScalarType::U16 => out.push(sig::U16),
        ScalarType::U32 => out.push(sig::U32),
        ScalarType::U64 => out.push(sig::U64),
        ScalarType::U128 => out.push(sig::U128),
        ScalarType::USize => out.push(sig::U64), // Treat usize as u64 for portability
        ScalarType::I8 => out.push(sig::I8),
        ScalarType::I16 => out.push(sig::I16),
        ScalarType::I32 => out.push(sig::I32),
        ScalarType::I64 => out.push(sig::I64),
        ScalarType::I128 => out.push(sig::I128),
        ScalarType::ISize => out.push(sig::I64), // Treat isize as i64 for portability
        ScalarType::ConstTypeId => out.push(sig::U64), // TypeId encodes as u64
        _ => out.push(sig::UNIT),                // Unknown scalar - fallback
    }
}

/// Encode a method signature: arguments followed by return type.
// rs[impl signature.method] - encode as tuple of args + return type
pub fn encode_method_signature(
    args: &[&'static Shape],
    return_type: &'static Shape,
    out: &mut Vec<u8>,
) {
    out.push(sig::TUPLE);
    encode_varint_u64(args.len() as u64, out);
    for arg in args {
        encode_shape(arg, out);
    }
    encode_shape(return_type, out);
}

/// Compute `sig_bytes`: the BLAKE3 hash of the canonical signature bytes.
// rs[impl signature.hash.algorithm] - hash signature using BLAKE3
pub fn signature_hash(args: &[&'static Shape], return_type: &'static Shape) -> blake3::Hash {
    let mut bytes = Vec::new();
    encode_method_signature(args, return_type, &mut bytes);
    blake3::hash(&bytes)
}

/// Compute the final 64-bit method id.
// rs[impl method.identity.computation] - blake3(kebab(service).kebab(method).sig_bytes)[0..8]
// rs[impl signature.endianness] - method ID bytes interpreted as little-endian u64
pub fn method_id(service_name: &str, method_name: &str, sig_hash: blake3::Hash) -> u64 {
    let mut input = Vec::new();
    input.extend_from_slice(service_name.to_kebab_case().as_bytes());
    input.push(b'.');
    input.extend_from_slice(method_name.to_kebab_case().as_bytes());
    input.extend_from_slice(sig_hash.as_bytes());

    let h = blake3::hash(&input);
    let first8: [u8; 8] = h.as_bytes()[0..8].try_into().expect("slice len");
    u64::from_le_bytes(first8)
}

/// Compute method ID from a MethodDetail.
pub fn method_id_from_detail(detail: &MethodDetail) -> u64 {
    let args: Vec<&'static Shape> = detail.args.iter().map(|a| a.ty).collect();
    let sig = signature_hash(&args, detail.return_type);
    method_id(&detail.service_name, &detail.method_name, sig)
}

/// Compute method ID from shapes at runtime.
///
/// This is the runtime equivalent of the codegen-time method ID computation.
/// Can be used to compute method IDs lazily and cache them.
///
/// # Example
///
/// ```ignore
/// use std::sync::LazyLock;
/// use facet::Facet;
///
/// static ECHO_METHOD_ID: LazyLock<u64> = LazyLock::new(|| {
///     method_id_from_shapes(
///         "Testbed",
///         "echo",
///         &[<String as Facet>::SHAPE],
///         <String as Facet>::SHAPE,
///     )
/// });
/// ```
pub fn method_id_from_shapes(
    service_name: &str,
    method_name: &str,
    args: &[&'static Shape],
    return_type: &'static Shape,
) -> u64 {
    let sig = signature_hash(args, return_type);
    method_id(service_name, method_name, sig)
}

#[cfg(test)]
mod tests {
    use super::*;
    use facet::Facet;

    #[test]
    fn signature_encoding_primitives() {
        let mut out = Vec::new();
        encode_shape(<i32 as Facet>::SHAPE, &mut out);
        assert_eq!(out, vec![sig::I32]);

        out.clear();
        encode_shape(<bool as Facet>::SHAPE, &mut out);
        assert_eq!(out, vec![sig::BOOL]);
    }

    #[test]
    fn signature_encoding_string() {
        let mut out = Vec::new();
        encode_shape(<String as Facet>::SHAPE, &mut out);
        assert_eq!(out, vec![sig::STRING]);
    }

    #[test]
    fn bytes_and_vec_u8_have_same_encoding() {
        let mut a = Vec::new();
        encode_shape(<Vec<u8> as Facet>::SHAPE, &mut a);
        assert_eq!(a, vec![sig::BYTES]);
    }

    #[test]
    fn method_id_is_deterministic() {
        let args = vec![<i32 as Facet>::SHAPE];
        let return_type = <() as Facet>::SHAPE;
        let sig = signature_hash(&args, return_type);

        let a = method_id("TemplateHost", "load_template", sig);
        let b = method_id("TemplateHost", "load_template", sig);
        assert_eq!(a, b);
    }
}
