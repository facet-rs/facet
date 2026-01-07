#![deny(unsafe_code)]

//! Hashing and method identity per `docs/content/rust-spec/_index.md`.

use roam_schema::{FieldDetail, MethodDetail, TypeDetail, VariantDetail, VariantPayload};

/// Signature encoding tags for type serialization.
/// These match the `#[repr(u8)]` discriminants in `TypeDetail`.
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
    pub const PUSH: u8 = 0x26;
    pub const PULL: u8 = 0x27;

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

/// Encode a `TypeDetail` into its canonical signature byte representation.
// rs[impl signature.primitive] - encode primitive types
// rs[impl signature.container] - encode container types (List, Option, Array, Map, Set, Tuple)
// rs[impl signature.struct] - encode struct types
// rs[impl signature.enum] - encode enum types
// rs[impl signature.stream] - encode Push/Pull stream types
pub fn encode_type(ty: &TypeDetail, out: &mut Vec<u8>) {
    match ty {
        // Primitives
        TypeDetail::Bool => out.push(sig::BOOL),
        TypeDetail::U8 => out.push(sig::U8),
        TypeDetail::U16 => out.push(sig::U16),
        TypeDetail::U32 => out.push(sig::U32),
        TypeDetail::U64 => out.push(sig::U64),
        TypeDetail::U128 => out.push(sig::U128),
        TypeDetail::I8 => out.push(sig::I8),
        TypeDetail::I16 => out.push(sig::I16),
        TypeDetail::I32 => out.push(sig::I32),
        TypeDetail::I64 => out.push(sig::I64),
        TypeDetail::I128 => out.push(sig::I128),
        TypeDetail::F32 => out.push(sig::F32),
        TypeDetail::F64 => out.push(sig::F64),
        TypeDetail::Char => out.push(sig::CHAR),
        TypeDetail::String => out.push(sig::STRING),
        TypeDetail::Unit => out.push(sig::UNIT),
        TypeDetail::Bytes => out.push(sig::BYTES),

        // Containers
        TypeDetail::List(inner) => {
            // rs[impl signature.bytes.equivalence] - bytes and List<u8> encode identically
            if matches!(inner.as_ref(), TypeDetail::U8) {
                out.push(sig::BYTES);
            } else {
                out.push(sig::LIST);
                encode_type(inner, out);
            }
        }
        TypeDetail::Option(inner) => {
            out.push(sig::OPTION);
            encode_type(inner, out);
        }
        TypeDetail::Array { element, len } => {
            out.push(sig::ARRAY);
            encode_varint_u64(*len as u64, out);
            encode_type(element, out);
        }
        TypeDetail::Map { key, value } => {
            out.push(sig::MAP);
            encode_type(key, out);
            encode_type(value, out);
        }
        TypeDetail::Set(inner) => {
            out.push(sig::SET);
            encode_type(inner, out);
        }
        TypeDetail::Tuple(items) => {
            out.push(sig::TUPLE);
            encode_varint_u64(items.len() as u64, out);
            for item in items {
                encode_type(item, out);
            }
        }
        TypeDetail::Push(inner) => {
            out.push(sig::PUSH);
            encode_type(inner, out);
        }
        TypeDetail::Pull(inner) => {
            out.push(sig::PULL);
            encode_type(inner, out);
        }

        // Composite (name is not included in signature hash - only structure matters)
        TypeDetail::Struct { fields, .. } => encode_struct(fields, out),
        TypeDetail::Enum { variants, .. } => encode_enum(variants, out),
    }
}

fn encode_struct(fields: &[FieldDetail], out: &mut Vec<u8>) {
    out.push(sig::STRUCT);
    encode_struct_body(fields, out);
}

fn encode_struct_body(fields: &[FieldDetail], out: &mut Vec<u8>) {
    encode_varint_u64(fields.len() as u64, out);
    for field in fields {
        encode_string(&field.name, out);
        encode_type(&field.type_info, out);
    }
}

fn encode_enum(variants: &[VariantDetail], out: &mut Vec<u8>) {
    out.push(sig::ENUM);
    encode_varint_u64(variants.len() as u64, out);
    for v in variants {
        encode_string(&v.name, out);
        match &v.payload {
            VariantPayload::Unit => out.push(sig::VARIANT_UNIT),
            VariantPayload::Newtype(inner) => {
                out.push(sig::VARIANT_NEWTYPE);
                encode_type(inner, out);
            }
            VariantPayload::Struct(fields) => {
                out.push(sig::VARIANT_STRUCT);
                encode_struct_body(fields, out);
            }
        }
    }
}

/// Encode a method signature: arguments followed by return type.
// rs[impl signature.method] - encode as tuple of args + return type
pub fn encode_method_signature(args: &[TypeDetail], return_type: &TypeDetail, out: &mut Vec<u8>) {
    out.push(sig::TUPLE);
    encode_varint_u64(args.len() as u64, out);
    for arg in args {
        encode_type(arg, out);
    }
    encode_type(return_type, out);
}

/// Compute `sig_bytes`: the BLAKE3 hash of the canonical signature bytes.
// rs[impl signature.hash.algorithm] - hash signature using BLAKE3
pub fn signature_hash(args: &[TypeDetail], return_type: &TypeDetail) -> blake3::Hash {
    let mut bytes = Vec::new();
    encode_method_signature(args, return_type, &mut bytes);
    blake3::hash(&bytes)
}

/// Convert an identifier to kebab-case, matching the rust-spec's `kebab()`.
pub fn kebab(input: &str) -> String {
    // Normalize separators to spaces first, then split into word tokens.
    let mut out = String::with_capacity(input.len() + 4);
    let mut prev: Option<char> = None;
    let mut prev_was_sep = false;

    let chars: Vec<char> = input.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        let is_sep = matches!(c, '_' | '-' | ' ' | '\t' | '\n' | '\r');
        if is_sep {
            if !prev_was_sep && !out.is_empty() {
                out.push('-');
            }
            prev_was_sep = true;
            prev = Some(c);
            continue;
        }

        let next = chars.get(i + 1).copied();
        let boundary = match (prev, c, next) {
            (Some('-'), _, _) => false,
            (Some(p), cur, _) if p.is_ascii_lowercase() && cur.is_ascii_uppercase() => true,
            (Some(p), cur, Some(n))
                if p.is_ascii_uppercase() && cur.is_ascii_uppercase() && n.is_ascii_lowercase() =>
            {
                true
            }
            (Some(p), cur, _) if p.is_ascii_digit() && cur.is_ascii_alphabetic() => true,
            _ => false,
        };

        if boundary && !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }

        out.push(c.to_ascii_lowercase());
        prev_was_sep = false;
        prev = Some(c);
    }

    while out.ends_with('-') {
        out.pop();
    }
    while out.starts_with('-') {
        out.remove(0);
    }
    out
}

/// Compute the final 64-bit method id.
// rs[impl method.identity.computation] - blake3(kebab(service).kebab(method).sig_bytes)[0..8]
// rs[impl signature.endianness] - method ID bytes interpreted as little-endian u64
pub fn method_id(service_name: &str, method_name: &str, sig_hash: blake3::Hash) -> u64 {
    let mut input = Vec::new();
    input.extend_from_slice(kebab(service_name).as_bytes());
    input.push(b'.');
    input.extend_from_slice(kebab(method_name).as_bytes());
    input.extend_from_slice(sig_hash.as_bytes());

    let h = blake3::hash(&input);
    let first8: [u8; 8] = h.as_bytes()[0..8].try_into().expect("slice len");
    u64::from_le_bytes(first8)
}

pub fn method_id_from_detail(detail: &MethodDetail) -> u64 {
    let args = detail
        .args
        .iter()
        .map(|a| a.type_info.clone())
        .collect::<Vec<_>>();
    let sig = signature_hash(&args, &detail.return_type);
    method_id(&detail.service_name, &detail.method_name, sig)
}

#[cfg(test)]
mod tests {
    use super::*;
    use roam_schema::{ArgDetail, MethodDetail, TypeDetail};

    #[test]
    fn kebab_normalizes_case_and_separators() {
        assert_eq!(kebab("TemplateHost"), "template-host");
        assert_eq!(kebab("loadTemplate"), "load-template");
        assert_eq!(kebab("load_template"), "load-template");
        assert_eq!(kebab("load-template"), "load-template");
        assert_eq!(kebab("HTTPServerV1"), "http-server-v1");
    }

    #[test]
    fn signature_encoding_example_add() {
        let mut out = Vec::new();
        encode_method_signature(
            &[TypeDetail::I32, TypeDetail::I32],
            &TypeDetail::I64,
            &mut out,
        );
        assert_eq!(out, vec![0x25, 0x02, 0x09, 0x09, 0x0A]);
    }

    #[test]
    fn bytes_and_list_u8_have_same_encoding() {
        let mut a = Vec::new();
        encode_type(&TypeDetail::Bytes, &mut a);

        let mut b = Vec::new();
        encode_type(&TypeDetail::List(Box::new(TypeDetail::U8)), &mut b);

        assert_eq!(a, b);
        assert_eq!(a, vec![0x11]);
    }

    #[test]
    fn method_id_is_deterministic() {
        let detail = MethodDetail {
            service_name: "TemplateHost".into(),
            method_name: "load_template".into(),
            args: vec![ArgDetail {
                name: "a".into(),
                type_info: TypeDetail::I32,
            }],
            return_type: TypeDetail::Unit,
            doc: None,
        };
        let a = method_id_from_detail(&detail);
        let b = method_id_from_detail(&detail);
        assert_eq!(a, b);
    }
}
