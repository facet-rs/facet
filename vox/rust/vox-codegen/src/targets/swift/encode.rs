//! Swift encoding expression/statement generation.
//!
//! Generates Swift code that encodes values into an `inout ByteBuffer`.
//! All encode functions are void — they append into the buffer rather than
//! returning a new `[UInt8]`.

use facet_core::{ScalarType, Shape};
use heck::ToLowerCamelCase;
use vox_types::{
    EnumInfo, ShapeKind, StructInfo, VariantKind, classify_shape, classify_variant, is_bytes,
};

/// Generate a Swift encode statement for a given shape and value.
/// The statement appends into the implicit `buffer: inout ByteBuffer` in scope.
pub fn generate_encode_stmt(shape: &'static Shape, value: &str) -> String {
    if is_bytes(shape) {
        return format!("encodeByteSeq({value}, into: &buffer)");
    }

    match classify_shape(shape) {
        ShapeKind::Scalar(scalar) => {
            let fn_name = swift_encode_fn(scalar);
            format!("{fn_name}({value}, into: &buffer)")
        }
        ShapeKind::List { element }
        | ShapeKind::Slice { element }
        | ShapeKind::Array { element, .. } => {
            let inner = generate_encode_closure(element);
            format!("encodeVec({value}, into: &buffer, encoder: {inner})")
        }
        ShapeKind::Option { inner } => {
            let inner = generate_encode_closure(inner);
            format!("encodeOption({value}, into: &buffer, encoder: {inner})")
        }
        ShapeKind::Tx { .. } | ShapeKind::Rx { .. } => {
            format!("encodeVarint({value}.channelId, into: &buffer)")
        }
        ShapeKind::Tuple { elements } if elements.len() == 2 => {
            let a = generate_encode_stmt(elements[0].shape, &format!("{value}.0"));
            let b = generate_encode_stmt(elements[1].shape, &format!("{value}.1"));
            format!("{a}\n{b}")
        }
        ShapeKind::TupleStruct { fields } if fields.len() == 2 => {
            let a = generate_encode_stmt(fields[0].shape(), &format!("{value}.0"));
            let b = generate_encode_stmt(fields[1].shape(), &format!("{value}.1"));
            format!("{a}\n{b}")
        }
        ShapeKind::Struct(StructInfo {
            name: Some(name), ..
        }) => {
            let fn_name = named_type_encode_fn_name(name);
            format!("{fn_name}({value}, into: &buffer)")
        }
        ShapeKind::Struct(StructInfo {
            name: None, fields, ..
        }) => {
            // Anonymous struct — encode each field inline
            let stmts: Vec<String> = fields
                .iter()
                .map(|f| {
                    let field_name = f.name.to_lower_camel_case();
                    generate_encode_stmt(f.shape(), &format!("{value}.{field_name}"))
                })
                .collect();
            stmts.join("\n")
        }
        ShapeKind::Enum(EnumInfo {
            name: Some(name), ..
        }) => {
            let fn_name = named_type_encode_fn_name(name);
            format!("{fn_name}({value}, into: &buffer)")
        }
        ShapeKind::Enum(EnumInfo { name: None, .. }) => {
            // Anonymous enum — inline switch
            let closure = generate_encode_closure(shape);
            format!("{closure}({value}, &buffer)")
        }
        ShapeKind::Pointer { pointee } => generate_encode_stmt(pointee, value),
        ShapeKind::Result { ok, err } => {
            let ok_stmt = generate_encode_stmt(ok, "v");
            let err_stmt = generate_encode_stmt(err, "e");
            format!(
                "switch {value} {{\ncase .success(let v):\n    encodeVarint(UInt64(0), into: &buffer)\n    {ok_stmt}\ncase .failure(let e):\n    encodeVarint(UInt64(1), into: &buffer)\n    {err_stmt}\n}}"
            )
        }
        _ => format!("/* unsupported encode for {value} */"),
    }
}

/// Generate a Swift encode closure `(T, inout ByteBuffer) -> Void` for use with
/// `encodeVec`, `encodeOption`, etc.
pub fn generate_encode_closure(shape: &'static Shape) -> String {
    if is_bytes(shape) {
        return "{ val, buf in encodeByteSeq(val, into: &buf) }".into();
    }

    match classify_shape(shape) {
        ShapeKind::Scalar(scalar) => {
            let fn_name = swift_encode_fn(scalar);
            format!("{{ val, buf in {fn_name}(val, into: &buf) }}")
        }
        ShapeKind::List { element } | ShapeKind::Slice { element } => {
            let inner = generate_encode_closure(element);
            format!("{{ val, buf in encodeVec(val, into: &buf, encoder: {inner}) }}")
        }
        ShapeKind::Option { inner } => {
            let inner = generate_encode_closure(inner);
            format!("{{ val, buf in encodeOption(val, into: &buf, encoder: {inner}) }}")
        }
        ShapeKind::Tx { .. } | ShapeKind::Rx { .. } => {
            "{ val, buf in encodeVarint(val.channelId, into: &buf) }".into()
        }
        ShapeKind::Tuple { elements } if elements.len() == 2 => {
            let a = encode_call_expr(elements[0].shape, "val.0", "buf");
            let b = encode_call_expr(elements[1].shape, "val.1", "buf");
            format!("{{ val, buf in {a}; {b} }}")
        }
        ShapeKind::TupleStruct { fields } if fields.len() == 2 => {
            let a = encode_call_expr(fields[0].shape(), "val.0", "buf");
            let b = encode_call_expr(fields[1].shape(), "val.1", "buf");
            format!("{{ val, buf in {a}; {b} }}")
        }
        ShapeKind::Struct(StructInfo {
            name: Some(name), ..
        }) => {
            let fn_name = named_type_encode_fn_name(name);
            format!("{{ val, buf in {fn_name}(val, into: &buf) }}")
        }
        ShapeKind::Struct(StructInfo {
            name: None, fields, ..
        }) => {
            // Anonymous struct — inline all field encodes
            let stmts: Vec<String> = fields
                .iter()
                .map(|f| {
                    let field_name = f.name.to_lower_camel_case();
                    let inner = generate_encode_closure(f.shape());
                    format!("{inner}(val.{field_name}, &buf)")
                })
                .collect();
            if stmts.is_empty() {
                "{ _, _ in }".into()
            } else {
                format!("{{ val, buf in {} }}", stmts.join("; "))
            }
        }
        ShapeKind::Enum(EnumInfo {
            name: Some(name), ..
        }) => {
            let fn_name = named_type_encode_fn_name(name);
            format!("{{ val, buf in {fn_name}(val, into: &buf) }}")
        }
        ShapeKind::Enum(EnumInfo {
            name: None,
            variants,
        }) => {
            // Anonymous enum — inline switch
            let mut code = "{ val, buf in\nswitch val {\n".to_string();
            for (i, v) in variants.iter().enumerate() {
                let variant_name = v.name.to_lower_camel_case();
                match classify_variant(v) {
                    VariantKind::Unit => {
                        code.push_str(&format!(
                            "case .{variant_name}: encodeVarint(UInt64({i}), into: &buf)\n"
                        ));
                    }
                    VariantKind::Newtype { inner } => {
                        let inner_closure = generate_encode_closure(inner);
                        code.push_str(&format!(
                            "case .{variant_name}(let v): encodeVarint(UInt64({i}), into: &buf); {inner_closure}(v, &buf)\n"
                        ));
                    }
                    VariantKind::Tuple { fields } => {
                        let bindings: Vec<String> =
                            (0..fields.len()).map(|j| format!("f{j}")).collect();
                        let stmts: Vec<String> = fields
                            .iter()
                            .enumerate()
                            .map(|(j, f)| {
                                let c = generate_encode_closure(f.shape());
                                format!("{c}(f{j}, &buf)")
                            })
                            .collect();
                        code.push_str(&format!(
                            "case .{variant_name}({}): encodeVarint(UInt64({i}), into: &buf); {}\n",
                            bindings
                                .iter()
                                .map(|b| format!("let {b}"))
                                .collect::<Vec<_>>()
                                .join(", "),
                            stmts.join("; ")
                        ));
                    }
                    VariantKind::Struct { fields } => {
                        let bindings: Vec<String> = fields
                            .iter()
                            .map(|f| f.name.to_lower_camel_case())
                            .collect();
                        let stmts: Vec<String> = fields
                            .iter()
                            .map(|f| {
                                let field_name = f.name.to_lower_camel_case();
                                let c = generate_encode_closure(f.shape());
                                format!("{c}({field_name}, &buf)")
                            })
                            .collect();
                        code.push_str(&format!(
                            "case .{variant_name}({}): encodeVarint(UInt64({i}), into: &buf); {}\n",
                            bindings
                                .iter()
                                .map(|b| format!("let {b}"))
                                .collect::<Vec<_>>()
                                .join(", "),
                            stmts.join("; ")
                        ));
                    }
                }
            }
            code.push_str("} }");
            code
        }
        ShapeKind::Pointer { pointee } => generate_encode_closure(pointee),
        ShapeKind::Result { ok, err } => {
            let ok_closure = generate_encode_closure(ok);
            let err_closure = generate_encode_closure(err);
            format!(
                "{{ val, buf in switch val {{ case .success(let v): encodeVarint(UInt64(0), into: &buf); {ok_closure}(v, &buf); case .failure(let e): encodeVarint(UInt64(1), into: &buf); {err_closure}(e, &buf) }} }}"
            )
        }
        _ => "{ _, _ in /* unsupported */ }".into(),
    }
}

/// Generate a top-level Swift encode function for a named struct or enum.
/// e.g. `internal func encodeGnarlyPayload(_ value: GnarlyPayload, into buffer: inout ByteBuffer)`
pub fn generate_named_type_encode_fn(name: &str, shape: &'static Shape) -> String {
    let fn_name = named_type_encode_fn_name(name);
    let mut out = String::new();
    out.push_str(&format!(
        "nonisolated internal func {fn_name}(_ value: {name}, into buffer: inout ByteBuffer) {{\n"
    ));

    match classify_shape(shape) {
        ShapeKind::Struct(StructInfo { fields, .. }) => {
            for f in fields {
                let field_name = f.name.to_lower_camel_case();
                let stmt = generate_encode_stmt(f.shape(), &format!("value.{field_name}"));
                for line in stmt.lines() {
                    out.push_str(&format!("    {line}\n"));
                }
            }
        }
        ShapeKind::Enum(EnumInfo { variants, .. }) => {
            out.push_str("    switch value {\n");
            for (i, v) in variants.iter().enumerate() {
                let variant_name = v.name.to_lower_camel_case();
                match classify_variant(v) {
                    VariantKind::Unit => {
                        out.push_str(&format!(
                            "    case .{variant_name}:\n        encodeVarint(UInt64({i}), into: &buffer)\n"
                        ));
                    }
                    VariantKind::Newtype { inner } => {
                        let stmt = generate_encode_stmt(inner, "val");
                        out.push_str(&format!(
                            "    case .{variant_name}(let val):\n        encodeVarint(UInt64({i}), into: &buffer)\n"
                        ));
                        for line in stmt.lines() {
                            out.push_str(&format!("        {line}\n"));
                        }
                    }
                    VariantKind::Tuple { fields } => {
                        let bindings: Vec<String> =
                            (0..fields.len()).map(|j| format!("f{j}")).collect();
                        let binding_str = bindings
                            .iter()
                            .map(|b| format!("let {b}"))
                            .collect::<Vec<_>>()
                            .join(", ");
                        out.push_str(&format!(
                            "    case .{variant_name}({binding_str}):\n        encodeVarint(UInt64({i}), into: &buffer)\n"
                        ));
                        for (j, f) in fields.iter().enumerate() {
                            let stmt = generate_encode_stmt(f.shape(), &format!("f{j}"));
                            for line in stmt.lines() {
                                out.push_str(&format!("        {line}\n"));
                            }
                        }
                    }
                    VariantKind::Struct { fields } => {
                        let bindings: Vec<String> = fields
                            .iter()
                            .map(|f| f.name.to_lower_camel_case())
                            .collect();
                        let binding_str = bindings
                            .iter()
                            .map(|b| format!("let {b}"))
                            .collect::<Vec<_>>()
                            .join(", ");
                        out.push_str(&format!(
                            "    case .{variant_name}({binding_str}):\n        encodeVarint(UInt64({i}), into: &buffer)\n"
                        ));
                        for f in fields {
                            let field_name = f.name.to_lower_camel_case();
                            let stmt = generate_encode_stmt(f.shape(), &field_name);
                            for line in stmt.lines() {
                                out.push_str(&format!("        {line}\n"));
                            }
                        }
                    }
                }
            }
            out.push_str("    }\n");
        }
        _ => {}
    }

    out.push_str("}\n");
    out
}

/// Generate encode functions for all named types.
pub fn generate_named_type_encode_fns(named_types: &[(String, &'static Shape)]) -> String {
    named_types
        .iter()
        .map(|(name, shape)| generate_named_type_encode_fn(name, shape))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The name of the generated encode function for a named type.
pub fn named_type_encode_fn_name(name: &str) -> String {
    format!("encode{name}")
}

/// Generate a direct encode call expression (not a closure) for a value into a named buffer.
/// Used inside closure bodies where closure IIFEs would be invalid Swift.
fn encode_call_expr(shape: &'static Shape, value: &str, buf: &str) -> String {
    if is_bytes(shape) {
        return format!("encodeByteSeq({value}, into: &{buf})");
    }
    match classify_shape(shape) {
        ShapeKind::Scalar(scalar) => {
            let fn_name = swift_encode_fn(scalar);
            format!("{fn_name}({value}, into: &{buf})")
        }
        ShapeKind::Struct(StructInfo {
            name: Some(name), ..
        })
        | ShapeKind::Enum(EnumInfo {
            name: Some(name), ..
        }) => {
            let fn_name = named_type_encode_fn_name(name);
            format!("{fn_name}({value}, into: &{buf})")
        }
        ShapeKind::List { element } | ShapeKind::Slice { element } => {
            let inner = generate_encode_closure(element);
            format!("encodeVec({value}, into: &{buf}, encoder: {inner})")
        }
        ShapeKind::Option { inner } => {
            let inner = generate_encode_closure(inner);
            format!("encodeOption({value}, into: &{buf}, encoder: {inner})")
        }
        ShapeKind::Pointer { pointee } => encode_call_expr(pointee, value, buf),
        ShapeKind::Tx { .. } | ShapeKind::Rx { .. } => {
            format!("encodeVarint({value}.channelId, into: &{buf})")
        }
        _ => {
            // Fallback: wrap in a named function call via closure
            let closure = generate_encode_closure(shape);
            format!("({closure})({value}, &{buf})")
        }
    }
}

/// Get the Swift encode function name for a scalar type.
pub fn swift_encode_fn(scalar: ScalarType) -> &'static str {
    match scalar {
        ScalarType::Bool => "encodeBool",
        ScalarType::U8 => "encodeU8",
        ScalarType::I8 => "encodeI8",
        ScalarType::U16 => "encodeU16",
        ScalarType::I16 => "encodeI16",
        ScalarType::U32 => "encodeU32",
        ScalarType::I32 => "encodeI32",
        ScalarType::U64 | ScalarType::USize => "encodeVarint",
        ScalarType::I64 | ScalarType::ISize => "encodeI64",
        ScalarType::F32 => "encodeF32",
        ScalarType::F64 => "encodeF64",
        ScalarType::Char | ScalarType::Str | ScalarType::CowStr | ScalarType::String => {
            "encodeString"
        }
        ScalarType::Unit => "{ _, _ in }",
        _ => "encodeByteSeq",
    }
}
