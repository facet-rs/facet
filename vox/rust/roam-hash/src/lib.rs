#![deny(unsafe_code)]

//! Hashing and method identity per `docs/content/rust-spec/_index.md`.

use rapace_schema::{FieldDetail, MethodDetail, TypeDetail, VariantDetail, VariantPayload};

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
// rs[impl signature.stream] - encode Stream type
pub fn encode_type(ty: &TypeDetail, out: &mut Vec<u8>) {
    match ty {
        // Primitives
        TypeDetail::Bool => out.push(0x01),
        TypeDetail::U8 => out.push(0x02),
        TypeDetail::U16 => out.push(0x03),
        TypeDetail::U32 => out.push(0x04),
        TypeDetail::U64 => out.push(0x05),
        TypeDetail::U128 => out.push(0x06),
        TypeDetail::I8 => out.push(0x07),
        TypeDetail::I16 => out.push(0x08),
        TypeDetail::I32 => out.push(0x09),
        TypeDetail::I64 => out.push(0x0A),
        TypeDetail::I128 => out.push(0x0B),
        TypeDetail::F32 => out.push(0x0C),
        TypeDetail::F64 => out.push(0x0D),
        TypeDetail::Char => out.push(0x0E),
        TypeDetail::String => out.push(0x0F),
        TypeDetail::Unit => out.push(0x10),
        TypeDetail::Bytes => out.push(0x11),

        // Containers
        TypeDetail::List(inner) => {
            // rs[impl signature.bytes.equivalence] - bytes and List<u8> encode identically
            if matches!(inner.as_ref(), TypeDetail::U8) {
                out.push(0x11);
            } else {
                out.push(0x20);
                encode_type(inner, out);
            }
        }
        TypeDetail::Option(inner) => {
            out.push(0x21);
            encode_type(inner, out);
        }
        TypeDetail::Array { element, len } => {
            out.push(0x22);
            encode_varint_u64(*len as u64, out);
            encode_type(element, out);
        }
        TypeDetail::Map { key, value } => {
            out.push(0x23);
            encode_type(key, out);
            encode_type(value, out);
        }
        TypeDetail::Set(inner) => {
            out.push(0x24);
            encode_type(inner, out);
        }
        TypeDetail::Tuple(items) => {
            out.push(0x25);
            encode_varint_u64(items.len() as u64, out);
            for item in items {
                encode_type(item, out);
            }
        }
        TypeDetail::Stream(inner) => {
            out.push(0x26);
            encode_type(inner, out);
        }

        // Composite
        TypeDetail::Struct { fields } => encode_struct(fields, out),
        TypeDetail::Enum { variants } => encode_enum(variants, out),
    }
}

fn encode_struct(fields: &[FieldDetail], out: &mut Vec<u8>) {
    out.push(0x30);
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
    out.push(0x31);
    encode_varint_u64(variants.len() as u64, out);
    for v in variants {
        encode_string(&v.name, out);
        match &v.payload {
            VariantPayload::Unit => out.push(0x00),
            VariantPayload::Newtype(inner) => {
                out.push(0x01);
                encode_type(inner, out);
            }
            VariantPayload::Struct(fields) => {
                out.push(0x02);
                // struct encoding without the 0x30 tag
                encode_struct_body(fields, out);
            }
        }
    }
}

/// Encode a method signature: arguments followed by return type.
// rs[impl signature.method] - encode as tuple of args + return type
pub fn encode_method_signature(args: &[TypeDetail], return_type: &TypeDetail, out: &mut Vec<u8>) {
    out.push(0x25);
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
    use rapace_schema::{ArgDetail, MethodDetail, TypeDetail};

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
