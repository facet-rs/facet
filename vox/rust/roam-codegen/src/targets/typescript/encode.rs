//! TypeScript encoding expression generation.
//!
//! Generates TypeScript code that encodes Rust types into byte arrays.

use facet_core::{ScalarType, Shape, StructKind};
use roam_schema::{
    EnumInfo, ShapeKind, StructInfo, VariantKind, classify_shape, classify_variant, is_bytes,
};

use super::types::ts_field_access;

/// Generate a TypeScript expression that encodes a value of the given type.
/// `expr` is the JavaScript expression to encode.
pub fn generate_encode_expr(shape: &'static Shape, expr: &str) -> String {
    // Check for bytes first (Vec<u8>)
    if is_bytes(shape) {
        return format!("pc.encodeBytes({expr})");
    }

    match classify_shape(shape) {
        ShapeKind::Scalar(scalar) => encode_scalar_expr(scalar, expr),
        ShapeKind::List { element } => {
            let item_encode = generate_encode_expr(element, "item");
            format!("pc.encodeVec({expr}, (item) => {item_encode})")
        }
        ShapeKind::Option { inner } => {
            let inner_encode = generate_encode_expr(inner, "v");
            format!("pc.encodeOption({expr}, (v) => {inner_encode})")
        }
        ShapeKind::Array { element, .. } => {
            // Encode as vec for now
            let item_encode = generate_encode_expr(element, "item");
            format!("pc.encodeVec({expr}, (item) => {item_encode})")
        }
        ShapeKind::Slice { element } => {
            let item_encode = generate_encode_expr(element, "item");
            format!("pc.encodeVec({expr}, (item) => {item_encode})")
        }
        ShapeKind::Map { key, value } => {
            // Encode as vec of tuples
            let k_enc = generate_encode_expr(key, "k");
            let v_enc = generate_encode_expr(value, "v");
            format!(
                "pc.encodeVec(Array.from({expr}.entries()), ([k, v]) => pc.concat({k_enc}, {v_enc}))"
            )
        }
        ShapeKind::Set { element } => {
            let item_encode = generate_encode_expr(element, "item");
            format!("pc.encodeVec(Array.from({expr}), (item) => {item_encode})")
        }
        ShapeKind::Tuple { elements } => {
            if elements.len() == 2 {
                let a_enc = generate_encode_expr(elements[0].shape, &format!("{expr}[0]"));
                let b_enc = generate_encode_expr(elements[1].shape, &format!("{expr}[1]"));
                format!("pc.concat({a_enc}, {b_enc})")
            } else if elements.len() == 3 {
                let a_enc = generate_encode_expr(elements[0].shape, &format!("{expr}[0]"));
                let b_enc = generate_encode_expr(elements[1].shape, &format!("{expr}[1]"));
                let c_enc = generate_encode_expr(elements[2].shape, &format!("{expr}[2]"));
                format!("pc.concat({a_enc}, {b_enc}, {c_enc})")
            } else if elements.is_empty() {
                "new Uint8Array(0)".into()
            } else {
                // Fallback: concat all
                let parts: Vec<_> = elements
                    .iter()
                    .enumerate()
                    .map(|(i, p)| generate_encode_expr(p.shape, &format!("{expr}[{i}]")))
                    .collect();
                format!("pc.concat({})", parts.join(", "))
            }
        }
        ShapeKind::Struct(StructInfo { fields, kind, .. }) => {
            if fields.is_empty() || kind == StructKind::Unit {
                "new Uint8Array(0)".into()
            } else {
                let parts: Vec<_> = fields
                    .iter()
                    .map(|f| generate_encode_expr(f.shape(), &ts_field_access(expr, f.name)))
                    .collect();
                format!("pc.concat({})", parts.join(", "))
            }
        }
        ShapeKind::Enum(EnumInfo { variants, .. }) => {
            // Generate switch on tag
            let mut cases = String::new();
            for (i, v) in variants.iter().enumerate() {
                cases.push_str(&format!("      case '{}': ", v.name));
                match classify_variant(v) {
                    VariantKind::Unit => {
                        cases.push_str(&format!("return pc.encodeEnumVariant({i});\n"));
                    }
                    VariantKind::Newtype { inner } => {
                        let inner_enc = generate_encode_expr(inner, &format!("{expr}.value"));
                        cases.push_str(&format!(
                            "return pc.concat(pc.encodeEnumVariant({i}), {inner_enc});\n"
                        ));
                    }
                    VariantKind::Tuple { fields } | VariantKind::Struct { fields } => {
                        let field_encs: Vec<_> = fields
                            .iter()
                            .map(|f| {
                                generate_encode_expr(f.shape(), &ts_field_access(expr, f.name))
                            })
                            .collect();
                        cases.push_str(&format!(
                            "return pc.concat(pc.encodeEnumVariant({i}), {});\n",
                            field_encs.join(", ")
                        ));
                    }
                }
            }
            format!(
                "(() => {{ switch ({expr}.tag) {{\n{cases}      default: throw new Error('unknown enum variant'); }} }})()"
            )
        }
        ShapeKind::Tx { .. } | ShapeKind::Rx { .. } => {
            // Streaming types encode as u64 stream ID (varint)
            // r[impl channeling.type] - Tx/Rx serialize as channel_id on wire.
            format!("pc.encodeU64({expr}.channelId)")
        }
        ShapeKind::Pointer { pointee } => generate_encode_expr(pointee, expr),
        ShapeKind::Result { .. } => {
            "/* Result type encoding not yet implemented */ new Uint8Array(0)".to_string()
        }
        ShapeKind::TupleStruct { fields } => {
            let field_encodes: Vec<String> = fields
                .iter()
                .enumerate()
                .map(|(i, f)| generate_encode_expr(f.shape(), &format!("{expr}[{i}]")))
                .collect();
            format!("pc.concat({})", field_encodes.join(", "))
        }
        ShapeKind::Opaque => "/* unsupported type */ new Uint8Array(0)".to_string(),
    }
}

/// Generate encode expression for scalar types.
fn encode_scalar_expr(scalar: ScalarType, expr: &str) -> String {
    match scalar {
        ScalarType::Bool => format!("pc.encodeBool({expr})"),
        ScalarType::U8 => format!("pc.encodeU8({expr})"),
        ScalarType::I8 => format!("pc.encodeI8({expr})"),
        ScalarType::U16 => format!("pc.encodeU16({expr})"),
        ScalarType::I16 => format!("pc.encodeI16({expr})"),
        ScalarType::U32 => format!("pc.encodeU32({expr})"),
        ScalarType::I32 => format!("pc.encodeI32({expr})"),
        ScalarType::U64 | ScalarType::USize => format!("pc.encodeU64({expr})"),
        ScalarType::I64 | ScalarType::ISize => format!("pc.encodeI64({expr})"),
        ScalarType::U128 | ScalarType::I128 => {
            panic!(
                "u128/i128 types are not supported in TypeScript codegen - use smaller integer types or encode as bytes"
            )
        }
        ScalarType::F32 => format!("pc.encodeF32({expr})"),
        ScalarType::F64 => format!("pc.encodeF64({expr})"),
        ScalarType::Char | ScalarType::Str | ScalarType::String | ScalarType::CowStr => {
            format!("pc.encodeString({expr})")
        }
        ScalarType::Unit => "new Uint8Array(0)".into(),
        _ => "/* unsupported scalar */ new Uint8Array(0)".to_string(),
    }
}

/// Generate an inline encode function for a type.
pub fn generate_encode_fn_inline(shape: &'static Shape) -> String {
    // Check for bytes first
    if is_bytes(shape) {
        return "(v: Uint8Array) => v".into();
    }

    match classify_shape(shape) {
        ShapeKind::Scalar(scalar) => encode_scalar_fn_inline(scalar),
        ShapeKind::Struct(StructInfo { fields, .. }) => {
            if fields.is_empty() {
                return "(v: any) => new Uint8Array(0)".into();
            }
            let parts: Vec<_> = fields
                .iter()
                .map(|f| generate_encode_expr(f.shape(), &format!("v.{}", f.name)))
                .collect();
            if parts.len() == 1 {
                format!("(v: any) => {}", parts[0])
            } else {
                format!("(v: any) => pc.concat({})", parts.join(", "))
            }
        }
        _ => {
            // Fallback: generate inline encode expression
            let encode_expr = generate_encode_expr(shape, "v");
            format!("(v: any) => {encode_expr}")
        }
    }
}

/// Generate inline encode function for scalars.
fn encode_scalar_fn_inline(scalar: ScalarType) -> String {
    match scalar {
        ScalarType::Bool => "(v: boolean) => pc.encodeBool(v)".into(),
        ScalarType::U8 => "(v: number) => pc.encodeU8(v)".into(),
        ScalarType::I8 => "(v: number) => pc.encodeI8(v)".into(),
        ScalarType::U16 => "(v: number) => pc.encodeU16(v)".into(),
        ScalarType::I16 => "(v: number) => pc.encodeI16(v)".into(),
        ScalarType::U32 => "(v: number) => pc.encodeU32(v)".into(),
        ScalarType::I32 => "(v: number) => pc.encodeI32(v)".into(),
        ScalarType::U64 | ScalarType::USize => "(v: bigint) => pc.encodeU64(v)".into(),
        ScalarType::I64 | ScalarType::ISize => "(v: bigint) => pc.encodeI64(v)".into(),
        ScalarType::F32 => "(v: number) => pc.encodeF32(v)".into(),
        ScalarType::F64 => "(v: number) => pc.encodeF64(v)".into(),
        ScalarType::Char | ScalarType::Str | ScalarType::String | ScalarType::CowStr => {
            "(v: string) => pc.encodeString(v)".into()
        }
        ScalarType::Unit => "(v: void) => new Uint8Array(0)".into(),
        _ => "(v: any) => new Uint8Array(0)".into(),
    }
}
