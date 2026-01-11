//! TypeScript decoding statement generation.
//!
//! Generates TypeScript code that decodes byte arrays into Rust types.

use facet_core::{ScalarType, Shape, StructKind};
use roam_schema::{
    EnumInfo, ShapeKind, StructInfo, VariantKind, classify_shape, classify_variant, is_bytes,
};

use super::types::{ts_type_base_named, ts_type_client_return, ts_type_server_arg};

/// Generate TypeScript code that decodes a value from a buffer for CLIENT context.
/// Schema is from server's perspective - types match on both sides.
pub fn generate_decode_stmt_client(
    shape: &'static Shape,
    var_name: &str,
    offset_var: &str,
) -> String {
    match classify_shape(shape) {
        ShapeKind::Tx { inner } => {
            // Caller's Tx (caller sends) - decode channel_id and create Tx handle
            // r[impl channeling.type] - Channel types decode as channel_id on wire.
            // TODO: Need Connection access to create proper Tx handle
            let inner_type = ts_type_client_return(inner);
            format!(
                "const _{var_name}_r = decodeU64(buf, {offset_var}); const {var_name} = {{ channelId: _{var_name}_r.value }} as Tx<{inner_type}>; {offset_var} = _{var_name}_r.next; /* TODO: create real Tx handle */"
            )
        }
        ShapeKind::Rx { inner } => {
            // Caller's Rx (caller receives) - decode channel_id and create Rx handle
            // r[impl channeling.type] - Channel types decode as channel_id on wire.
            // TODO: Need Connection access to create proper Rx handle
            let inner_type = ts_type_client_return(inner);
            format!(
                "const _{var_name}_r = decodeU64(buf, {offset_var}); const {var_name} = {{ channelId: _{var_name}_r.value }} as Rx<{inner_type}>; {offset_var} = _{var_name}_r.next; /* TODO: create real Rx handle */"
            )
        }
        // For non-streaming types, use the regular decode
        _ => generate_decode_stmt(shape, var_name, offset_var),
    }
}

/// Generate TypeScript code that decodes a value from a buffer for SERVER context.
/// Schema is from server's perspective - no inversion needed.
/// - Schema Tx → server sends via Tx
/// - Schema Rx → server receives via Rx
pub fn generate_decode_stmt_server(
    shape: &'static Shape,
    var_name: &str,
    offset_var: &str,
) -> String {
    match classify_shape(shape) {
        ShapeKind::Tx { inner } => {
            // Schema Tx → server sends via Tx
            // r[impl channeling.type] - Channel types decode as channel_id on wire.
            let inner_type = ts_type_server_arg(inner);
            format!(
                "const _{var_name}_r = decodeU64(buf, {offset_var}); const {var_name} = {{ channelId: _{var_name}_r.value }} as Tx<{inner_type}>; {offset_var} = _{var_name}_r.next; /* TODO: create real Tx handle */"
            )
        }
        ShapeKind::Rx { inner } => {
            // Schema Rx → server receives via Rx
            // r[impl channeling.type] - Channel types decode as channel_id on wire.
            let inner_type = ts_type_server_arg(inner);
            format!(
                "const _{var_name}_r = decodeU64(buf, {offset_var}); const {var_name} = {{ channelId: _{var_name}_r.value }} as Rx<{inner_type}>; {offset_var} = _{var_name}_r.next; /* TODO: create real Rx handle */"
            )
        }
        // For non-streaming types, use the regular decode
        _ => generate_decode_stmt(shape, var_name, offset_var),
    }
}

/// Generate decode statement for server-side streaming context.
/// Creates real Tx/Rx handles using the registry and taskSender.
pub fn generate_decode_stmt_server_streaming(
    shape: &'static Shape,
    var_name: &str,
    offset_var: &str,
    registry_var: &str,
    task_sender_var: &str,
) -> String {
    match classify_shape(shape) {
        ShapeKind::Tx { inner } => {
            // Server sends data to client via Tx
            // Decode channel_id, create server-side Tx with taskSender
            let inner_type = ts_type_server_arg(inner);
            let encode_fn = super::encode::generate_encode_fn_inline(inner);
            format!(
                "const _{var_name}_r = decodeU64(buf, {offset_var}); \
                 const {var_name} = createServerTx<{inner_type}>(_{var_name}_r.value, {task_sender_var}, {encode_fn}); \
                 {offset_var} = _{var_name}_r.next;"
            )
        }
        ShapeKind::Rx { inner } => {
            // Server receives data from client via Rx
            // Decode channel_id, register for incoming (creates channel), create Rx with receiver
            let inner_type = ts_type_server_arg(inner);
            let decode_fn = generate_decode_fn_inline(inner);
            format!(
                "const _{var_name}_r = decodeU64(buf, {offset_var}); \
                 const _{var_name}_receiver = {registry_var}.registerIncoming(_{var_name}_r.value); \
                 const {var_name} = createServerRx<{inner_type}>(_{var_name}_r.value, _{var_name}_receiver, {decode_fn}); \
                 {offset_var} = _{var_name}_r.next;"
            )
        }
        // For non-streaming types, use the regular decode
        _ => generate_decode_stmt(shape, var_name, offset_var),
    }
}

/// Generate TypeScript code that decodes a value from a buffer.
/// Returns the decoded value in a variable and updates offset.
/// `var_name` is the variable to assign the result to.
/// `offset_var` is the variable holding the current offset.
pub fn generate_decode_stmt(shape: &'static Shape, var_name: &str, offset_var: &str) -> String {
    // Check for bytes first
    if is_bytes(shape) {
        return format!(
            "const _{var_name}_r = decodeBytes(buf, {offset_var}); const {var_name} = _{var_name}_r.value; {offset_var} = _{var_name}_r.next;"
        );
    }

    match classify_shape(shape) {
        ShapeKind::Scalar(scalar) => decode_scalar_stmt(scalar, var_name, offset_var),
        ShapeKind::List { element } => {
            let decode_fn = generate_decode_fn(element, "item");
            format!(
                "const _{var_name}_r = decodeVec(buf, {offset_var}, {decode_fn}); const {var_name} = _{var_name}_r.value; {offset_var} = _{var_name}_r.next;"
            )
        }
        ShapeKind::Option { inner } => {
            let decode_fn = generate_decode_fn(inner, "inner");
            format!(
                "const _{var_name}_r = decodeOption(buf, {offset_var}, {decode_fn}); const {var_name} = _{var_name}_r.value; {offset_var} = _{var_name}_r.next;"
            )
        }
        ShapeKind::Array { element, .. } | ShapeKind::Slice { element } => {
            let decode_fn = generate_decode_fn(element, "item");
            format!(
                "const _{var_name}_r = decodeVec(buf, {offset_var}, {decode_fn}); const {var_name} = _{var_name}_r.value; {offset_var} = _{var_name}_r.next;"
            )
        }
        ShapeKind::Tuple { elements } if elements.len() == 2 => {
            let decode_a = generate_decode_fn(elements[0].shape, "a");
            let decode_b = generate_decode_fn(elements[1].shape, "b");
            format!(
                "const _{var_name}_r = decodeTuple2(buf, {offset_var}, {decode_a}, {decode_b}); const {var_name} = _{var_name}_r.value; {offset_var} = _{var_name}_r.next;"
            )
        }
        ShapeKind::Tuple { elements } if elements.len() == 3 => {
            let decode_a = generate_decode_fn(elements[0].shape, "a");
            let decode_b = generate_decode_fn(elements[1].shape, "b");
            let decode_c = generate_decode_fn(elements[2].shape, "c");
            format!(
                "const _{var_name}_r = decodeTuple3(buf, {offset_var}, {decode_a}, {decode_b}, {decode_c}); const {var_name} = _{var_name}_r.value; {offset_var} = _{var_name}_r.next;"
            )
        }
        ShapeKind::Tuple { elements } => {
            if elements.is_empty() {
                return format!("const {var_name} = undefined;");
            }
            // Generic tuple decoding
            let mut code = format!("const {var_name}: [");
            code.push_str(
                &elements
                    .iter()
                    .map(|p| ts_type_base_named(p.shape))
                    .collect::<Vec<_>>()
                    .join(", "),
            );
            code.push_str("] = [] as any;\n");
            for (i, param) in elements.iter().enumerate() {
                let item_var = format!("{var_name}_{i}");
                code.push_str(&generate_decode_stmt(param.shape, &item_var, offset_var));
                code.push_str(&format!(" {var_name}[{i}] = {item_var};\n"));
            }
            code
        }
        ShapeKind::Struct(StructInfo { fields, kind, .. }) => {
            if fields.is_empty() || kind == StructKind::Unit {
                return format!("const {var_name} = undefined;");
            }
            let mut code = String::new();
            for (i, field) in fields.iter().enumerate() {
                let field_var = format!("{var_name}_f{i}");
                code.push_str(&generate_decode_stmt(field.shape(), &field_var, offset_var));
                code.push('\n');
            }
            code.push_str(&format!("const {var_name} = {{ "));
            for (i, field) in fields.iter().enumerate() {
                let field_var = format!("{var_name}_f{i}");
                if i > 0 {
                    code.push_str(", ");
                }
                code.push_str(&format!("{}: {field_var}", field.name));
            }
            code.push_str(" };");
            code
        }
        ShapeKind::Enum(EnumInfo { variants, .. }) => {
            let mut code = format!(
                "const _{var_name}_disc = decodeEnumVariant(buf, {offset_var}); {offset_var} = _{var_name}_disc.next;\n"
            );
            code.push_str(&format!("let {var_name}: {};\n", ts_type_base_named(shape)));
            code.push_str(&format!("switch (_{var_name}_disc.value) {{\n"));
            for (i, v) in variants.iter().enumerate() {
                code.push_str(&format!("  case {i}: {{\n"));
                match classify_variant(v) {
                    VariantKind::Unit => {
                        code.push_str(&format!("    {var_name} = {{ tag: '{}' }};\n", v.name));
                    }
                    VariantKind::Newtype { inner } => {
                        let inner_var = format!("{var_name}_inner");
                        code.push_str(&format!(
                            "    {}\n",
                            generate_decode_stmt(inner, &inner_var, offset_var)
                        ));
                        code.push_str(&format!(
                            "    {var_name} = {{ tag: '{}', value: {inner_var} }};\n",
                            v.name
                        ));
                    }
                    VariantKind::Tuple { fields } | VariantKind::Struct { fields } => {
                        for (fi, field) in fields.iter().enumerate() {
                            let field_var = format!("{var_name}_f{fi}");
                            code.push_str(&format!(
                                "    {}\n",
                                generate_decode_stmt(field.shape(), &field_var, offset_var)
                            ));
                        }
                        code.push_str(&format!("    {var_name} = {{ tag: '{}'", v.name));
                        for (fi, field) in fields.iter().enumerate() {
                            let field_var = format!("{var_name}_f{fi}");
                            code.push_str(&format!(", {}: {field_var}", field.name));
                        }
                        code.push_str(" };\n");
                    }
                }
                code.push_str("    break;\n  }\n");
            }
            code.push_str(&format!(
                "  default: throw new Error(`unknown enum variant ${{_{var_name}_disc.value}}`);\n}}"
            ));
            code
        }
        ShapeKind::Map { key, value } => {
            let decode_k = generate_decode_fn(key, "k");
            let decode_v = generate_decode_fn(value, "v");
            format!(
                "const _{var_name}_r = decodeVec(buf, {offset_var}, (buf, off) => {{ \
                const kr = ({decode_k})(buf, off); \
                const vr = ({decode_v})(buf, kr.next); \
                return {{ value: [kr.value, vr.value] as [any, any], next: vr.next }}; \
                }}); const {var_name} = new Map(_{var_name}_r.value); {offset_var} = _{var_name}_r.next;"
            )
        }
        ShapeKind::Set { element } => {
            let decode_fn = generate_decode_fn(element, "item");
            format!(
                "const _{var_name}_r = decodeVec(buf, {offset_var}, {decode_fn}); const {var_name} = new Set(_{var_name}_r.value); {offset_var} = _{var_name}_r.next;"
            )
        }
        ShapeKind::Tx { inner } => {
            let inner_type = ts_type_base_named(inner);
            format!(
                "const _{var_name}_r = decodeU64(buf, {offset_var}); const {var_name} = {{ channelId: _{var_name}_r.value }} as Tx<{inner_type}>; {offset_var} = _{var_name}_r.next;"
            )
        }
        ShapeKind::Rx { inner } => {
            let inner_type = ts_type_base_named(inner);
            format!(
                "const _{var_name}_r = decodeU64(buf, {offset_var}); const {var_name} = {{ channelId: _{var_name}_r.value }} as Rx<{inner_type}>; {offset_var} = _{var_name}_r.next;"
            )
        }
        ShapeKind::Pointer { pointee } => generate_decode_stmt(pointee, var_name, offset_var),
        ShapeKind::Result { .. } => {
            format!("const {var_name} = undefined; /* Result type decoding not yet implemented */")
        }
        ShapeKind::TupleStruct { fields } => {
            let mut stmts = Vec::new();
            for (i, f) in fields.iter().enumerate() {
                stmts.push(generate_decode_stmt(
                    f.shape(),
                    &format!("{var_name}_{i}"),
                    offset_var,
                ));
            }
            let tuple_elements: Vec<String> = (0..fields.len())
                .map(|i| format!("{var_name}_{i}"))
                .collect();
            // Generate tuple type for assertion
            let tuple_types: Vec<String> = fields
                .iter()
                .map(|f| ts_type_base_named(f.shape()))
                .collect();
            stmts.push(format!(
                "const {var_name} = [{}] as [{}];",
                tuple_elements.join(", "),
                tuple_types.join(", ")
            ));
            stmts.join("\n")
        }
        ShapeKind::Opaque => format!("const {var_name} = undefined; /* unsupported type */"),
    }
}

/// Generate decode statement for scalar types.
fn decode_scalar_stmt(scalar: ScalarType, var_name: &str, offset_var: &str) -> String {
    match scalar {
        ScalarType::Bool => format!(
            "const _{var_name}_r = decodeBool(buf, {offset_var}); const {var_name} = _{var_name}_r.value; {offset_var} = _{var_name}_r.next;"
        ),
        ScalarType::U8 => format!(
            "const _{var_name}_r = decodeU8(buf, {offset_var}); const {var_name} = _{var_name}_r.value; {offset_var} = _{var_name}_r.next;"
        ),
        ScalarType::I8 => format!(
            "const _{var_name}_r = decodeI8(buf, {offset_var}); const {var_name} = _{var_name}_r.value; {offset_var} = _{var_name}_r.next;"
        ),
        ScalarType::U16 => format!(
            "const _{var_name}_r = decodeU16(buf, {offset_var}); const {var_name} = _{var_name}_r.value; {offset_var} = _{var_name}_r.next;"
        ),
        ScalarType::I16 => format!(
            "const _{var_name}_r = decodeI16(buf, {offset_var}); const {var_name} = _{var_name}_r.value; {offset_var} = _{var_name}_r.next;"
        ),
        ScalarType::U32 => format!(
            "const _{var_name}_r = decodeU32(buf, {offset_var}); const {var_name} = _{var_name}_r.value; {offset_var} = _{var_name}_r.next;"
        ),
        ScalarType::I32 => format!(
            "const _{var_name}_r = decodeI32(buf, {offset_var}); const {var_name} = _{var_name}_r.value; {offset_var} = _{var_name}_r.next;"
        ),
        ScalarType::U64 | ScalarType::USize => format!(
            "const _{var_name}_r = decodeU64(buf, {offset_var}); const {var_name} = _{var_name}_r.value; {offset_var} = _{var_name}_r.next;"
        ),
        ScalarType::I64 | ScalarType::ISize => format!(
            "const _{var_name}_r = decodeI64(buf, {offset_var}); const {var_name} = _{var_name}_r.value; {offset_var} = _{var_name}_r.next;"
        ),
        ScalarType::U128 => format!(
            "const _{var_name}_r = decodeU64(buf, {offset_var}); const {var_name} = _{var_name}_r.value; {offset_var} = _{var_name}_r.next;"
        ),
        ScalarType::I128 => format!(
            "const _{var_name}_r = decodeI64(buf, {offset_var}); const {var_name} = _{var_name}_r.value; {offset_var} = _{var_name}_r.next;"
        ),
        ScalarType::F32 => format!(
            "const _{var_name}_r = decodeF32(buf, {offset_var}); const {var_name} = _{var_name}_r.value; {offset_var} = _{var_name}_r.next;"
        ),
        ScalarType::F64 => format!(
            "const _{var_name}_r = decodeF64(buf, {offset_var}); const {var_name} = _{var_name}_r.value; {offset_var} = _{var_name}_r.next;"
        ),
        ScalarType::Char | ScalarType::Str | ScalarType::String | ScalarType::CowStr => format!(
            "const _{var_name}_r = decodeString(buf, {offset_var}); const {var_name} = _{var_name}_r.value; {offset_var} = _{var_name}_r.next;"
        ),
        ScalarType::Unit => format!("const {var_name} = undefined;"),
        _ => format!("const {var_name} = undefined; /* unsupported scalar */"),
    }
}

/// Generate a decode function expression for use with decodeVec, decodeOption, etc.
pub fn generate_decode_fn(shape: &'static Shape, _var_hint: &str) -> String {
    // Check for bytes first
    if is_bytes(shape) {
        return "(buf, off) => decodeBytes(buf, off)".into();
    }

    match classify_shape(shape) {
        ShapeKind::Scalar(scalar) => decode_scalar_fn(scalar),
        ShapeKind::List { element } => {
            let inner_fn = generate_decode_fn(element, "item");
            format!("(buf, off) => decodeVec(buf, off, {inner_fn})")
        }
        ShapeKind::Option { inner } => {
            let inner_fn = generate_decode_fn(inner, "inner");
            format!("(buf, off) => decodeOption(buf, off, {inner_fn})")
        }
        ShapeKind::Array { element, .. } | ShapeKind::Slice { element } => {
            let inner_fn = generate_decode_fn(element, "item");
            format!("(buf, off) => decodeVec(buf, off, {inner_fn})")
        }
        ShapeKind::Tuple { elements } if elements.len() == 2 => {
            let a_fn = generate_decode_fn(elements[0].shape, "a");
            let b_fn = generate_decode_fn(elements[1].shape, "b");
            format!("(buf, off) => decodeTuple2(buf, off, {a_fn}, {b_fn})")
        }
        ShapeKind::Tuple { elements } if elements.len() == 3 => {
            let a_fn = generate_decode_fn(elements[0].shape, "a");
            let b_fn = generate_decode_fn(elements[1].shape, "b");
            let c_fn = generate_decode_fn(elements[2].shape, "c");
            format!("(buf, off) => decodeTuple3(buf, off, {a_fn}, {b_fn}, {c_fn})")
        }
        ShapeKind::Tuple { elements: [] } => {
            "(buf, off) => ({ value: undefined, next: off })".into()
        }
        ShapeKind::Struct(StructInfo { fields, kind, .. }) => {
            if fields.is_empty() || kind == StructKind::Unit {
                return "(buf, off) => ({ value: undefined, next: off })".into();
            }
            // Generate inline struct decoder
            let mut code = "(buf: Uint8Array, off: number) => { let o = off;\n".to_string();
            for (i, f) in fields.iter().enumerate() {
                code.push_str(&format!(
                    "  {}\n",
                    generate_decode_stmt(f.shape(), &format!("f{i}"), "o")
                ));
            }
            code.push_str("  return { value: { ");
            for (i, f) in fields.iter().enumerate() {
                if i > 0 {
                    code.push_str(", ");
                }
                code.push_str(&format!("{}: f{i}", f.name));
            }
            code.push_str(" }, next: o };\n}");
            code
        }
        ShapeKind::Enum(EnumInfo { variants, .. }) => {
            // Generate inline enum decoder
            let mut code =
                "(buf: Uint8Array, off: number): DecodeResult<any> => { let o = off;\n".to_string();
            code.push_str("  const disc = decodeEnumVariant(buf, o); o = disc.next;\n");
            code.push_str("  switch (disc.value) {\n");
            for (i, v) in variants.iter().enumerate() {
                code.push_str(&format!("    case {i}: "));
                match classify_variant(v) {
                    VariantKind::Unit => {
                        code.push_str(&format!(
                            "return {{ value: {{ tag: '{}' }}, next: o }};\n",
                            v.name
                        ));
                    }
                    VariantKind::Newtype { inner } => {
                        code.push_str("{\n");
                        code.push_str(&format!(
                            "      {}\n",
                            generate_decode_stmt(inner, "val", "o")
                        ));
                        code.push_str(&format!(
                            "      return {{ value: {{ tag: '{}', value: val }}, next: o }};\n",
                            v.name
                        ));
                        code.push_str("    }\n");
                    }
                    VariantKind::Tuple { fields } | VariantKind::Struct { fields } => {
                        code.push_str("{\n");
                        for (j, f) in fields.iter().enumerate() {
                            code.push_str(&format!(
                                "      {}\n",
                                generate_decode_stmt(f.shape(), &format!("f{j}"), "o")
                            ));
                        }
                        code.push_str(&format!("      return {{ value: {{ tag: '{}', ", v.name));
                        for (j, f) in fields.iter().enumerate() {
                            if j > 0 {
                                code.push_str(", ");
                            }
                            code.push_str(&format!("{}: f{j}", f.name));
                        }
                        code.push_str(" }, next: o };\n    }\n");
                    }
                }
            }
            code.push_str(
                "    default: throw new Error(`unknown enum variant: ${disc.value}`);\n  }\n}",
            );
            code
        }
        ShapeKind::Pointer { pointee } => generate_decode_fn(pointee, _var_hint),
        _ => "(buf, off) => { throw new Error('unsupported type'); }".into(),
    }
}

/// Generate decode function for scalar types.
fn decode_scalar_fn(scalar: ScalarType) -> String {
    match scalar {
        ScalarType::Bool => "(buf, off) => decodeBool(buf, off)".into(),
        ScalarType::U8 => "(buf, off) => decodeU8(buf, off)".into(),
        ScalarType::I8 => "(buf, off) => decodeI8(buf, off)".into(),
        ScalarType::U16 => "(buf, off) => decodeU16(buf, off)".into(),
        ScalarType::I16 => "(buf, off) => decodeI16(buf, off)".into(),
        ScalarType::U32 => "(buf, off) => decodeU32(buf, off)".into(),
        ScalarType::I32 => "(buf, off) => decodeI32(buf, off)".into(),
        ScalarType::U64 | ScalarType::USize => "(buf, off) => decodeU64(buf, off)".into(),
        ScalarType::I64 | ScalarType::ISize => "(buf, off) => decodeI64(buf, off)".into(),
        ScalarType::U128 => "(buf, off) => decodeU64(buf, off)".into(),
        ScalarType::I128 => "(buf, off) => decodeI64(buf, off)".into(),
        ScalarType::F32 => "(buf, off) => decodeF32(buf, off)".into(),
        ScalarType::F64 => "(buf, off) => decodeF64(buf, off)".into(),
        ScalarType::Char | ScalarType::Str | ScalarType::String | ScalarType::CowStr => {
            "(buf, off) => decodeString(buf, off)".into()
        }
        ScalarType::Unit => "(buf, off) => ({ value: undefined, next: off })".into(),
        _ => "(buf, off) => { throw new Error('unsupported scalar'); }".into(),
    }
}

/// Generate an inline decode function for a type.
pub fn generate_decode_fn_inline(shape: &'static Shape) -> String {
    // Check for bytes first
    if is_bytes(shape) {
        return "(bytes: Uint8Array) => bytes".into();
    }

    match classify_shape(shape) {
        ShapeKind::Scalar(scalar) => decode_scalar_fn_inline(scalar),
        _ => {
            // For complex types, generate a function that decodes from offset 0
            let decode_fn = generate_decode_fn(shape, "v");
            format!("(bytes: Uint8Array) => ({decode_fn})(bytes, 0).value")
        }
    }
}

/// Generate inline decode function for scalars.
fn decode_scalar_fn_inline(scalar: ScalarType) -> String {
    match scalar {
        ScalarType::Bool => "(bytes: Uint8Array) => decodeBool(bytes, 0).value".into(),
        ScalarType::U8 => "(bytes: Uint8Array) => decodeU8(bytes, 0).value".into(),
        ScalarType::I8 => "(bytes: Uint8Array) => decodeI8(bytes, 0).value".into(),
        ScalarType::U16 => "(bytes: Uint8Array) => decodeU16(bytes, 0).value".into(),
        ScalarType::I16 => "(bytes: Uint8Array) => decodeI16(bytes, 0).value".into(),
        ScalarType::U32 => "(bytes: Uint8Array) => decodeU32(bytes, 0).value".into(),
        ScalarType::I32 => "(bytes: Uint8Array) => decodeI32(bytes, 0).value".into(),
        ScalarType::U64 | ScalarType::USize => {
            "(bytes: Uint8Array) => decodeU64(bytes, 0).value".into()
        }
        ScalarType::I64 | ScalarType::ISize => {
            "(bytes: Uint8Array) => decodeI64(bytes, 0).value".into()
        }
        ScalarType::F32 => "(bytes: Uint8Array) => decodeF32(bytes, 0).value".into(),
        ScalarType::F64 => "(bytes: Uint8Array) => decodeF64(bytes, 0).value".into(),
        ScalarType::Char | ScalarType::Str | ScalarType::String | ScalarType::CowStr => {
            "(bytes: Uint8Array) => decodeString(bytes, 0).value".into()
        }
        ScalarType::Unit => "(bytes: Uint8Array) => undefined".into(),
        _ => "(bytes: Uint8Array) => undefined".into(),
    }
}
