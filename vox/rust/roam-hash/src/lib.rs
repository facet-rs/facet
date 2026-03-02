#![deny(unsafe_code)]

//! Hashing and method identity for roam.
//!
//! Encodes types using `facet::Shape` for signature hashing, following
//! `docs/content/spec-sig.md`.

use facet_core::{Def, Facet, ScalarType, Shape, StructKind, Type, UserType};
use heck::ToKebabCase;
use roam_types::{ArgDescriptor, MethodDescriptor, MethodId};
use roam_types::{is_rx, is_tx};
use std::collections::HashSet;

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

    // Composite (0x30-0x32)
    pub const STRUCT: u8 = 0x30;
    pub const ENUM: u8 = 0x31;
    pub const BACKREF: u8 = 0x32;

    // Variant payloads
    pub const VARIANT_UNIT: u8 = 0x00;
    pub const VARIANT_NEWTYPE: u8 = 0x01;
    pub const VARIANT_STRUCT: u8 = 0x02;
}

// r[impl signature.varint]
fn encode_varint_u64(mut value: u64, out: &mut Vec<u8>) {
    while value >= 0x80 {
        out.push((value as u8) | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

fn encode_str(s: &str, out: &mut Vec<u8>) {
    encode_varint_u64(s.len() as u64, out);
    out.extend_from_slice(s.as_bytes());
}

/// Encode a `Shape` into its canonical signature byte representation.
// r[impl signature.primitive]
// r[impl signature.container]
// r[impl signature.struct]
// r[impl signature.enum]
// r[impl signature.recursive]
// r[impl signature.recursive.encoding]
// r[impl signature.recursive.stack]
fn encode_shape(shape: &'static Shape, out: &mut Vec<u8>) {
    let mut stack: Vec<&'static Shape> = Vec::new();
    encode_shape_inner(shape, out, &mut stack);
}

fn encode_shape_inner(shape: &'static Shape, out: &mut Vec<u8>, stack: &mut Vec<&'static Shape>) {
    // Channel types
    if is_tx(shape) {
        out.push(sig::TX);
        if let Some(inner) = shape.type_params.first() {
            encode_shape_inner(inner.shape, out, stack);
        }
        return;
    }
    if is_rx(shape) {
        out.push(sig::RX);
        if let Some(inner) = shape.type_params.first() {
            encode_shape_inner(inner.shape, out, stack);
        }
        return;
    }

    // Transparent wrappers
    if shape.is_transparent()
        && let Some(inner) = shape.inner
    {
        encode_shape_inner(inner, out, stack);
        return;
    }

    // Scalars
    if let Some(scalar) = shape.scalar_type() {
        encode_scalar(scalar, out);
        return;
    }

    // Semantic definitions
    match shape.def {
        Def::List(list_def) => {
            if let Some(ScalarType::U8) = list_def.t().scalar_type() {
                // r[impl signature.bytes.equivalence]
                out.push(sig::BYTES);
            } else {
                out.push(sig::LIST);
                encode_shape_inner(list_def.t(), out, stack);
            }
            return;
        }
        Def::Array(array_def) => {
            out.push(sig::ARRAY);
            encode_varint_u64(array_def.n as u64, out);
            encode_shape_inner(array_def.t(), out, stack);
            return;
        }
        Def::Slice(slice_def) => {
            out.push(sig::LIST);
            encode_shape_inner(slice_def.t(), out, stack);
            return;
        }
        Def::Map(map_def) => {
            out.push(sig::MAP);
            encode_shape_inner(map_def.k(), out, stack);
            encode_shape_inner(map_def.v(), out, stack);
            return;
        }
        Def::Set(set_def) => {
            out.push(sig::SET);
            encode_shape_inner(set_def.t(), out, stack);
            return;
        }
        Def::Option(opt_def) => {
            out.push(sig::OPTION);
            encode_shape_inner(opt_def.t(), out, stack);
            return;
        }
        Def::Pointer(ptr_def) => {
            if let Some(pointee) = ptr_def.pointee {
                encode_shape_inner(pointee, out, stack);
                return;
            }
        }
        _ => {}
    }

    // Cycle detection for user-defined types: check if this shape is
    // already on the encoding stack (indicates recursion).
    if let Some(pos) = stack.iter().rposition(|&s| s == shape) {
        // Depth = distance from top of stack (0 = immediate parent)
        let depth = stack.len() - 1 - pos;
        out.push(sig::BACKREF);
        encode_varint_u64(depth as u64, out);
        return;
    }

    // Push onto stack before encoding children, pop after.
    stack.push(shape);

    match shape.ty {
        Type::User(UserType::Struct(struct_type)) => match struct_type.kind {
            StructKind::Unit => {
                out.push(sig::UNIT);
            }
            StructKind::TupleStruct | StructKind::Tuple => {
                out.push(sig::TUPLE);
                encode_varint_u64(struct_type.fields.len() as u64, out);
                for field in struct_type.fields {
                    encode_shape_inner(field.shape(), out, stack);
                }
            }
            StructKind::Struct => {
                out.push(sig::STRUCT);
                encode_varint_u64(struct_type.fields.len() as u64, out);
                for field in struct_type.fields {
                    encode_str(field.name, out);
                    encode_shape_inner(field.shape(), out, stack);
                }
            }
        },
        Type::User(UserType::Enum(enum_type)) => {
            out.push(sig::ENUM);
            encode_varint_u64(enum_type.variants.len() as u64, out);
            for variant in enum_type.variants {
                encode_str(variant.name, out);
                match variant.data.kind {
                    StructKind::Unit => {
                        out.push(sig::VARIANT_UNIT);
                    }
                    StructKind::TupleStruct | StructKind::Tuple => {
                        if variant.data.fields.len() == 1 {
                            out.push(sig::VARIANT_NEWTYPE);
                            encode_shape_inner(variant.data.fields[0].shape(), out, stack);
                        } else {
                            out.push(sig::VARIANT_STRUCT);
                            encode_varint_u64(variant.data.fields.len() as u64, out);
                            for (i, field) in variant.data.fields.iter().enumerate() {
                                encode_str(&i.to_string(), out);
                                encode_shape_inner(field.shape(), out, stack);
                            }
                        }
                    }
                    StructKind::Struct => {
                        out.push(sig::VARIANT_STRUCT);
                        encode_varint_u64(variant.data.fields.len() as u64, out);
                        for field in variant.data.fields {
                            encode_str(field.name, out);
                            encode_shape_inner(field.shape(), out, stack);
                        }
                    }
                }
            }
        }
        Type::Pointer(_) => {
            if let Some(inner) = shape.type_params.first() {
                encode_shape_inner(inner.shape, out, stack);
            } else {
                out.push(sig::UNIT);
            }
        }
        _ => {
            out.push(sig::UNIT);
        }
    }

    stack.pop();
}

fn encode_scalar(scalar: ScalarType, out: &mut Vec<u8>) {
    match scalar {
        ScalarType::Unit => out.push(sig::UNIT),
        ScalarType::Bool => out.push(sig::BOOL),
        ScalarType::Char => out.push(sig::CHAR),
        ScalarType::Str | ScalarType::String | ScalarType::CowStr => out.push(sig::STRING),
        ScalarType::F32 => out.push(sig::F32),
        ScalarType::F64 => out.push(sig::F64),
        ScalarType::U8 => out.push(sig::U8),
        ScalarType::U16 => out.push(sig::U16),
        ScalarType::U32 => out.push(sig::U32),
        ScalarType::U64 => out.push(sig::U64),
        ScalarType::U128 => out.push(sig::U128),
        ScalarType::USize => out.push(sig::U64), // portable: usize → u64
        ScalarType::I8 => out.push(sig::I8),
        ScalarType::I16 => out.push(sig::I16),
        ScalarType::I32 => out.push(sig::I32),
        ScalarType::I64 => out.push(sig::I64),
        ScalarType::I128 => out.push(sig::I128),
        ScalarType::ISize => out.push(sig::I64), // portable: isize → i64
        ScalarType::ConstTypeId => out.push(sig::U64),
        _ => out.push(sig::UNIT),
    }
}

/// Encode a method signature: args tuple type followed by return type.
// r[impl rpc.schema-evolution]
// r[impl signature.method]
// r[impl signature.hash.algorithm]
fn encode_method_signature(args: &'static Shape, return_type: &'static Shape, out: &mut Vec<u8>) {
    encode_shape(args, out);
    encode_shape(return_type, out);
}

/// Compute the final method ID from type parameters.
///
/// `A` is the args tuple type (e.g. `(f64, f64)`), `R` is the return type.
// r[impl rpc.method-id]
// r[impl rpc.method-id.algorithm]
// r[impl rpc.method-id.no-collisions]
// r[impl method.identity.computation]
// r[impl signature.endianness]
pub fn method_id<'a, 'r, A: Facet<'a>, R: Facet<'r>>(
    service_name: &str,
    method_name: &str,
) -> MethodId {
    let mut sig_bytes = Vec::new();
    encode_method_signature(A::SHAPE, R::SHAPE, &mut sig_bytes);
    let sig_hash = blake3::hash(&sig_bytes);

    let mut input = Vec::new();
    input.extend_from_slice(service_name.to_kebab_case().as_bytes());
    input.push(b'.');
    input.extend_from_slice(method_name.to_kebab_case().as_bytes());
    input.extend_from_slice(sig_hash.as_bytes());
    let h = blake3::hash(&input);
    let first8: [u8; 8] = h.as_bytes()[0..8].try_into().expect("slice len");
    MethodId(u64::from_le_bytes(first8))
}

/// Build and leak a `MethodDescriptor` from type parameters and arg names.
///
/// Called once per method inside a `OnceLock::get_or_init` in macro-generated code.
/// `A` is the args tuple type, `R` is the return type.
pub fn method_descriptor<'a, 'r, A: Facet<'a>, R: Facet<'r>>(
    service_name: &'static str,
    method_name: &'static str,
    arg_names: &[&'static str],
    doc: Option<&'static str>,
) -> &'static MethodDescriptor {
    assert!(
        !shape_contains_channel(R::SHAPE),
        "channels are not allowed in return types: {service_name}.{method_name}"
    );

    let id = method_id::<A, R>(service_name, method_name);

    // Extract per-arg shapes from the tuple fields of A::SHAPE.
    let arg_shapes: &[&'static Shape] = match A::SHAPE.ty {
        Type::User(UserType::Struct(s)) => {
            let fields: Vec<&'static Shape> = s.fields.iter().map(|f| f.shape()).collect();
            Box::leak(fields.into_boxed_slice())
        }
        _ => &[],
    };

    assert_eq!(
        arg_names.len(),
        arg_shapes.len(),
        "arg_names length mismatch for {service_name}.{method_name}"
    );

    let args: &'static [ArgDescriptor] = Box::leak(
        arg_names
            .iter()
            .zip(arg_shapes.iter())
            .map(|(&name, &shape)| ArgDescriptor { name, shape })
            .collect::<Vec<_>>()
            .into_boxed_slice(),
    );

    Box::leak(Box::new(MethodDescriptor {
        id,
        service_name,
        method_name,
        args,
        return_shape: R::SHAPE,
        doc,
    }))
}

fn shape_contains_channel(shape: &'static Shape) -> bool {
    fn visit(shape: &'static Shape, seen: &mut HashSet<usize>) -> bool {
        if is_tx(shape) || is_rx(shape) {
            return true;
        }

        let key = shape as *const Shape as usize;
        if !seen.insert(key) {
            return false;
        }

        if let Some(inner) = shape.inner
            && visit(inner, seen)
        {
            return true;
        }

        if shape.type_params.iter().any(|t| visit(t.shape, seen)) {
            return true;
        }

        match shape.def {
            Def::List(list_def) => visit(list_def.t(), seen),
            Def::Array(array_def) => visit(array_def.t(), seen),
            Def::Slice(slice_def) => visit(slice_def.t(), seen),
            Def::Map(map_def) => visit(map_def.k(), seen) || visit(map_def.v(), seen),
            Def::Set(set_def) => visit(set_def.t(), seen),
            Def::Option(opt_def) => visit(opt_def.t(), seen),
            Def::Result(result_def) => visit(result_def.t(), seen) || visit(result_def.e(), seen),
            Def::Pointer(ptr_def) => ptr_def.pointee.is_some_and(|p| visit(p, seen)),
            _ => match shape.ty {
                Type::User(UserType::Struct(s)) => s.fields.iter().any(|f| visit(f.shape(), seen)),
                Type::User(UserType::Enum(e)) => e
                    .variants
                    .iter()
                    .any(|v| v.data.fields.iter().any(|f| visit(f.shape(), seen))),
                _ => false,
            },
        }
    }

    let mut seen = HashSet::new();
    visit(shape, &mut seen)
}

#[cfg(test)]
mod tests {
    use super::*;
    use facet::Facet;
    use roam_types::{Rx, Tx};

    #[derive(Facet)]
    struct PlainRet {
        value: u64,
    }

    #[derive(Facet)]
    struct NestedRet {
        nested: Option<Result<Rx<u8>, u32>>,
    }

    #[test]
    fn allows_non_channel_return_types() {
        let _ = method_descriptor::<(), PlainRet>("TestSvc", "plain", &[], None);
    }

    #[test]
    #[should_panic(expected = "channels are not allowed in return types: TestSvc.nested")]
    fn rejects_nested_channel_in_return_types() {
        let _ = method_descriptor::<(Tx<u8>,), NestedRet>("TestSvc", "nested", &["input"], None);
    }

    #[test]
    fn encode_varint_encodes_expected_boundaries() {
        let mut out = Vec::new();
        encode_varint_u64(0, &mut out);
        assert_eq!(out, vec![0x00]);

        out.clear();
        encode_varint_u64(127, &mut out);
        assert_eq!(out, vec![0x7F]);

        out.clear();
        encode_varint_u64(128, &mut out);
        assert_eq!(out, vec![0x80, 0x01]);

        out.clear();
        encode_varint_u64(300, &mut out);
        assert_eq!(out, vec![0xAC, 0x02]);
    }

    #[test]
    fn method_id_is_stable_and_uses_kebab_case_names() {
        let a = method_id::<(u32,), u64>("MyService", "DoThingFast");
        let b = method_id::<(u32,), u64>("my-service", "do-thing-fast");
        let c = method_id::<(u32,), u64>("MY_SERVICE", "DO_THING_FAST");
        assert_eq!(a, b);
        assert_eq!(b, c);
    }

    #[test]
    fn method_id_changes_when_signature_changes() {
        let a = method_id::<(u32,), u64>("svc", "m");
        let b = method_id::<(u64,), u64>("svc", "m");
        let c = method_id::<(u32,), u32>("svc", "m");
        assert_ne!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn method_descriptor_populates_args_and_doc() {
        let descriptor = method_descriptor::<(u32, String), PlainRet>(
            "Svc",
            "do_it",
            &["count", "name"],
            Some("doc"),
        );
        assert_eq!(descriptor.service_name, "Svc");
        assert_eq!(descriptor.method_name, "do_it");
        assert_eq!(descriptor.args.len(), 2);
        assert_eq!(descriptor.args[0].name, "count");
        assert_eq!(descriptor.args[1].name, "name");
        assert_eq!(descriptor.doc, Some("doc"));
    }

    #[test]
    #[should_panic(expected = "arg_names length mismatch for Svc.bad")]
    fn method_descriptor_panics_when_arg_names_length_mismatches_shape() {
        let _ = method_descriptor::<(u32, u64), PlainRet>("Svc", "bad", &["only_one"], None);
    }

    #[test]
    fn list_of_u8_uses_bytes_tag_while_other_lists_do_not() {
        let mut vec_u8_sig = Vec::new();
        encode_shape(<Vec<u8> as Facet>::SHAPE, &mut vec_u8_sig);
        assert_eq!(vec_u8_sig, vec![sig::BYTES]);

        let mut vec_u16_sig = Vec::new();
        encode_shape(<Vec<u16> as Facet>::SHAPE, &mut vec_u16_sig);

        assert_ne!(vec_u8_sig, vec_u16_sig);
        assert_eq!(vec_u16_sig[0], sig::LIST);
    }

    #[test]
    fn shape_contains_channel_handles_recursive_and_non_recursive_shapes() {
        #[derive(Facet)]
        struct Recursive {
            next: Option<Box<Recursive>>,
        }

        #[derive(Facet)]
        struct ChannelNested {
            inner: Option<Result<Tx<u16>, u8>>,
        }

        assert!(!shape_contains_channel(Recursive::SHAPE));
        assert!(shape_contains_channel(ChannelNested::SHAPE));
    }

    #[test]
    fn encode_shape_emits_expected_scalar_and_container_tags() {
        fn head(shape: &'static facet_core::Shape) -> u8 {
            let mut out = Vec::new();
            encode_shape(shape, &mut out);
            out[0]
        }

        assert_eq!(head(<bool as Facet>::SHAPE), sig::BOOL);
        assert_eq!(head(<u64 as Facet>::SHAPE), sig::U64);
        assert_eq!(head(<i32 as Facet>::SHAPE), sig::I32);
        assert_eq!(head(<String as Facet>::SHAPE), sig::STRING);
        assert_eq!(head(<Option<u8> as Facet>::SHAPE), sig::OPTION);
        assert_eq!(head(<Vec<u16> as Facet>::SHAPE), sig::LIST);
        assert_eq!(head(<[u16; 4] as Facet>::SHAPE), sig::ARRAY);
        assert_eq!(
            head(<std::collections::BTreeMap<u8, u16> as Facet>::SHAPE),
            sig::MAP
        );
        assert_eq!(
            head(<std::collections::BTreeSet<u8> as Facet>::SHAPE),
            sig::SET
        );
        assert_eq!(head(<(u8, u16) as Facet>::SHAPE), sig::TUPLE);
    }

    #[test]
    fn encode_shape_marks_recursive_types_with_backref() {
        #[derive(Facet)]
        struct Node {
            next: Option<Box<Node>>,
        }

        let mut out = Vec::new();
        encode_shape(Node::SHAPE, &mut out);
        assert!(
            out.contains(&sig::BACKREF),
            "recursive encoding should contain BACKREF marker"
        );
    }
}
