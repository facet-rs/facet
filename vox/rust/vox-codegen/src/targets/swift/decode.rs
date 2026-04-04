//! Swift decoding statement generation.
//!
//! Generates Swift code that decodes values from an `inout ByteBuffer`.
//! All decode functions take `inout ByteBuffer` and advance its reader index.

use facet_core::{ScalarType, Shape};
use heck::ToLowerCamelCase;
use vox_types::{
    EnumInfo, ShapeKind, StructInfo, VariantKind, classify_shape, classify_variant, is_bytes,
};

/// Generate a Swift decode statement for a given shape.
/// Returns code that decodes from `buffer` into a variable named `var_name`.
pub fn generate_decode_stmt(shape: &'static Shape, var_name: &str, indent: &str) -> String {
    generate_decode_stmt_impl(shape, var_name, indent, "buffer")
}

/// Generate a Swift decode statement for a given shape using a custom cursor variable name.
/// (cursor_var is now ignored — kept for call-site compatibility, buffer is always `buffer`)
pub fn generate_decode_stmt_with_cursor(
    shape: &'static Shape,
    var_name: &str,
    indent: &str,
    _cursor_var: &str,
) -> String {
    generate_decode_stmt_impl(shape, var_name, indent, "buffer")
}

/// Generate a Swift decode statement from a specific data variable.
/// (data_var is now ignored — kept for call-site compatibility, buffer is always `buffer`)
pub fn generate_decode_stmt_from(
    shape: &'static Shape,
    var_name: &str,
    indent: &str,
    _data_var: &str,
) -> String {
    generate_decode_stmt_impl(shape, var_name, indent, "buffer")
}

/// Generate a Swift decode statement from a specific data variable and cursor.
/// (data_var and cursor_var are now ignored — kept for call-site compatibility)
pub fn generate_decode_stmt_from_with_cursor(
    shape: &'static Shape,
    var_name: &str,
    indent: &str,
    _data_var: &str,
    _cursor_var: &str,
) -> String {
    generate_decode_stmt_impl(shape, var_name, indent, "buffer")
}

/// Core implementation: generate a decode statement that reads from the named buffer variable.
pub fn generate_decode_stmt_with_buf(
    shape: &'static Shape,
    var_name: &str,
    indent: &str,
    buf_name: &str,
) -> String {
    generate_decode_stmt_impl(shape, var_name, indent, buf_name)
}

fn generate_decode_stmt_impl(
    shape: &'static Shape,
    var_name: &str,
    indent: &str,
    buf_name: &str,
) -> String {
    // bytes → ByteBuffer slice, presented as Data for the user-facing type
    if is_bytes(shape) {
        return format!(
            "{indent}var _{var_name}_buf = try decodeBytes(from: &{buf_name})\n{indent}let {var_name} = Data(_{var_name}_buf.readBytes(length: _{var_name}_buf.readableBytes) ?? [])\n"
        );
    }

    match classify_shape(shape) {
        ShapeKind::Scalar(scalar) => {
            let decode_fn = swift_decode_fn(scalar);
            format!("{indent}let {var_name} = try {decode_fn}(from: &{buf_name})\n")
        }
        ShapeKind::List { element }
        | ShapeKind::Slice { element }
        | ShapeKind::Array { element, .. } => {
            let inner = generate_decode_closure(element);
            format!("{indent}let {var_name} = try decodeVec(from: &{buf_name}, decoder: {inner})\n")
        }
        ShapeKind::Option { inner } => {
            let inner = generate_decode_closure(inner);
            format!(
                "{indent}let {var_name} = try decodeOption(from: &{buf_name}, decoder: {inner})\n"
            )
        }
        ShapeKind::Tuple { elements } if elements.len() == 2 => {
            let a = generate_decode_closure(elements[0].shape);
            let b = generate_decode_closure(elements[1].shape);
            format!(
                "{indent}let {var_name} = try decodeTuple2(from: &{buf_name}, decoderA: {a}, decoderB: {b})\n"
            )
        }
        ShapeKind::TupleStruct { fields } if fields.len() == 2 => {
            let a = generate_decode_closure(fields[0].shape());
            let b = generate_decode_closure(fields[1].shape());
            format!(
                "{indent}let {var_name} = try decodeTuple2(from: &{buf_name}, decoderA: {a}, decoderB: {b})\n"
            )
        }
        ShapeKind::Struct(StructInfo {
            name: Some(name),
            fields,
            ..
        }) => {
            // Named struct — decode each field then construct
            let mut out = String::new();
            for f in fields.iter() {
                let field_name = f.name.to_lower_camel_case();
                out.push_str(&generate_decode_stmt_impl(
                    f.shape(),
                    &format!("_{var_name}_{field_name}"),
                    indent,
                    buf_name,
                ));
            }
            let field_inits: Vec<String> = fields
                .iter()
                .map(|f| {
                    let field_name = f.name.to_lower_camel_case();
                    format!("{field_name}: _{var_name}_{field_name}")
                })
                .collect();
            out.push_str(&format!(
                "{indent}let {var_name} = {name}({})\n",
                field_inits.join(", ")
            ));
            out
        }
        ShapeKind::Enum(EnumInfo {
            name: Some(name),
            variants,
            ..
        }) => {
            let mut out = String::new();
            out.push_str(&format!(
                "{indent}let _{var_name}_disc = try decodeVarint(from: &{buf_name})\n"
            ));
            out.push_str(&format!("{indent}let {var_name}: {name}\n"));
            out.push_str(&format!("{indent}switch _{var_name}_disc {{\n"));
            for (i, v) in variants.iter().enumerate() {
                out.push_str(&format!("{indent}case {i}:\n"));
                let inner_indent = format!("{indent}    ");
                match classify_variant(v) {
                    VariantKind::Unit => {
                        out.push_str(&format!(
                            "{inner_indent}{var_name} = .{}\n",
                            v.name.to_lower_camel_case()
                        ));
                    }
                    VariantKind::Newtype { inner } => {
                        out.push_str(&generate_decode_stmt_impl(
                            inner,
                            &format!("_{var_name}_val"),
                            &inner_indent,
                            buf_name,
                        ));
                        out.push_str(&format!(
                            "{inner_indent}{var_name} = .{}(_{var_name}_val)\n",
                            v.name.to_lower_camel_case()
                        ));
                    }
                    VariantKind::Tuple { fields } => {
                        for (j, f) in fields.iter().enumerate() {
                            out.push_str(&generate_decode_stmt_impl(
                                f.shape(),
                                &format!("_{var_name}_f{j}"),
                                &inner_indent,
                                buf_name,
                            ));
                        }
                        let args: Vec<String> = (0..fields.len())
                            .map(|j| format!("_{var_name}_f{j}"))
                            .collect();
                        out.push_str(&format!(
                            "{inner_indent}{var_name} = .{}({})\n",
                            v.name.to_lower_camel_case(),
                            args.join(", ")
                        ));
                    }
                    VariantKind::Struct { fields } => {
                        for f in fields.iter() {
                            let field_name = f.name.to_lower_camel_case();
                            out.push_str(&generate_decode_stmt_impl(
                                f.shape(),
                                &format!("_{var_name}_{field_name}"),
                                &inner_indent,
                                buf_name,
                            ));
                        }
                        let args: Vec<String> = fields
                            .iter()
                            .map(|f| {
                                let field_name = f.name.to_lower_camel_case();
                                format!("{field_name}: _{var_name}_{field_name}")
                            })
                            .collect();
                        out.push_str(&format!(
                            "{inner_indent}{var_name} = .{}({})\n",
                            v.name.to_lower_camel_case(),
                            args.join(", ")
                        ));
                    }
                }
            }
            out.push_str(&format!("{indent}default:\n"));
            out.push_str(&format!(
                "{indent}    throw VoxError.decodeError(\"unknown enum variant\")\n"
            ));
            out.push_str(&format!("{indent}}}\n"));
            out
        }
        ShapeKind::Pointer { pointee } => {
            generate_decode_stmt_impl(pointee, var_name, indent, buf_name)
        }
        ShapeKind::Result { ok, err } => {
            let ok_type = super::types::swift_type_base(ok);
            let err_type = super::types::swift_type_base(err);
            let mut out = String::new();
            out.push_str(&format!(
                "{indent}let _{var_name}_disc = try decodeVarint(from: &{buf_name})\n"
            ));
            out.push_str(&format!(
                "{indent}let {var_name}: Result<{ok_type}, {err_type}>\n"
            ));
            out.push_str(&format!("{indent}switch _{var_name}_disc {{\n"));
            out.push_str(&format!("{indent}case 0:\n"));
            let inner_indent = format!("{indent}    ");
            out.push_str(&generate_decode_stmt_impl(
                ok,
                &format!("_{var_name}_ok"),
                &inner_indent,
                buf_name,
            ));
            out.push_str(&format!(
                "{inner_indent}{var_name} = .success(_{var_name}_ok)\n"
            ));
            out.push_str(&format!("{indent}case 1:\n"));
            out.push_str(&generate_decode_stmt_impl(
                err,
                &format!("_{var_name}_err"),
                &inner_indent,
                buf_name,
            ));
            out.push_str(&format!(
                "{inner_indent}{var_name} = .failure(_{var_name}_err)\n"
            ));
            out.push_str(&format!("{indent}default:\n"));
            out.push_str(&format!(
                "{indent}    throw VoxError.decodeError(\"invalid Result discriminant\")\n"
            ));
            out.push_str(&format!("{indent}}}\n"));
            out
        }
        _ => {
            format!("{indent}let {var_name}: Any = () // unsupported type\n")
        }
    }
}

/// Generate a Swift decode closure `(inout ByteBuffer) throws -> T` for use with
/// `decodeVec`, `decodeOption`, etc.
pub fn generate_decode_closure(shape: &'static Shape) -> String {
    if is_bytes(shape) {
        // decodeBytes returns ByteBuffer; convert to Data for user-facing type
        return "{ buf in var _b = try decodeBytes(from: &buf); return Data(_b.readBytes(length: _b.readableBytes) ?? []) }".into();
    }

    match classify_shape(shape) {
        ShapeKind::Scalar(scalar) => {
            let decode_fn = swift_decode_fn(scalar);
            format!("{{ buf in try {decode_fn}(from: &buf) }}")
        }
        ShapeKind::List { element } | ShapeKind::Slice { element } => {
            let inner = generate_decode_closure(element);
            format!("{{ buf in try decodeVec(from: &buf, decoder: {inner}) }}")
        }
        ShapeKind::Option { inner } => {
            let inner = generate_decode_closure(inner);
            format!("{{ buf in try decodeOption(from: &buf, decoder: {inner}) }}")
        }
        ShapeKind::Tuple { elements } if elements.len() == 2 => {
            let a = generate_decode_closure(elements[0].shape);
            let b = generate_decode_closure(elements[1].shape);
            format!("{{ buf in try decodeTuple2(from: &buf, decoderA: {a}, decoderB: {b}) }}")
        }
        ShapeKind::TupleStruct { fields } if fields.len() == 2 => {
            let a = generate_decode_closure(fields[0].shape());
            let b = generate_decode_closure(fields[1].shape());
            format!("{{ buf in try decodeTuple2(from: &buf, decoderA: {a}, decoderB: {b}) }}")
        }
        ShapeKind::Struct(StructInfo {
            name: Some(name),
            fields,
            ..
        }) => {
            // Named struct — inline decode all fields then construct
            let mut code = "{ buf in\n".to_string();
            for f in fields.iter() {
                let field_name = f.name.to_lower_camel_case();
                let inner = generate_decode_closure(f.shape());
                code.push_str(&format!("    let _{field_name} = try ({inner})(&buf)\n"));
            }
            let field_inits: Vec<String> = fields
                .iter()
                .map(|f| {
                    let field_name = f.name.to_lower_camel_case();
                    format!("{field_name}: _{field_name}")
                })
                .collect();
            code.push_str(&format!(
                "    return {name}({})\n}}",
                field_inits.join(", ")
            ));
            code
        }
        ShapeKind::Enum(EnumInfo {
            name: Some(name),
            variants,
            ..
        }) => {
            let mut code = format!(
                "{{ buf in\n    let disc = try decodeVarint(from: &buf)\n    let result: {name}\n    switch disc {{\n"
            );
            for (i, v) in variants.iter().enumerate() {
                code.push_str(&format!("    case {i}:\n"));
                match classify_variant(v) {
                    VariantKind::Unit => {
                        code.push_str(&format!(
                            "        result = .{}\n",
                            v.name.to_lower_camel_case()
                        ));
                    }
                    VariantKind::Newtype { inner } => {
                        let inner_closure = generate_decode_closure(inner);
                        code.push_str(&format!(
                            "        let val = try ({inner_closure})(&buf)\n        result = .{}(val)\n",
                            v.name.to_lower_camel_case()
                        ));
                    }
                    VariantKind::Tuple { fields } => {
                        for (j, f) in fields.iter().enumerate() {
                            let inner = generate_decode_closure(f.shape());
                            code.push_str(&format!("        let f{j} = try ({inner})(&buf)\n"));
                        }
                        let args: Vec<String> =
                            (0..fields.len()).map(|j| format!("f{j}")).collect();
                        code.push_str(&format!(
                            "        result = .{}({})\n",
                            v.name.to_lower_camel_case(),
                            args.join(", ")
                        ));
                    }
                    VariantKind::Struct { fields } => {
                        for f in fields.iter() {
                            let field_name = f.name.to_lower_camel_case();
                            let inner = generate_decode_closure(f.shape());
                            code.push_str(&format!(
                                "        let _{field_name} = try ({inner})(&buf)\n"
                            ));
                        }
                        let args: Vec<String> = fields
                            .iter()
                            .map(|f| {
                                let field_name = f.name.to_lower_camel_case();
                                format!("{field_name}: _{field_name}")
                            })
                            .collect();
                        code.push_str(&format!(
                            "        result = .{}({})\n",
                            v.name.to_lower_camel_case(),
                            args.join(", ")
                        ));
                    }
                }
            }
            code.push_str(
                "    default:\n        throw VoxError.decodeError(\"unknown enum variant\")\n    }\n    return result\n}",
            );
            code
        }
        ShapeKind::Pointer { pointee } => generate_decode_closure(pointee),
        _ => "{ _ in throw VoxError.decodeError(\"unsupported type\") }".into(),
    }
}

/// Generate inline decode expression — just the expression part, no `let x =`.
/// Used internally where a closure calls another closure.
pub fn generate_inline_decode(shape: &'static Shape, _data_var: &str, _offset_var: &str) -> String {
    // data_var and offset_var are ignored — we always use `buf` (the closure parameter)
    let closure = generate_decode_closure(shape);
    format!("({closure})(&buf)")
}

/// Get the Swift decode function name for a scalar type.
pub fn swift_decode_fn(scalar: ScalarType) -> &'static str {
    match scalar {
        ScalarType::Bool => "decodeBool",
        ScalarType::U8 => "decodeU8",
        ScalarType::I8 => "decodeI8",
        ScalarType::U16 => "decodeU16",
        ScalarType::I16 => "decodeI16",
        ScalarType::U32 => "decodeU32",
        ScalarType::I32 => "decodeI32",
        ScalarType::U64 | ScalarType::USize => "decodeVarint",
        ScalarType::I64 | ScalarType::ISize => "decodeI64",
        ScalarType::F32 => "decodeF32",
        ScalarType::F64 => "decodeF64",
        ScalarType::Char | ScalarType::Str | ScalarType::CowStr | ScalarType::String => {
            "decodeString"
        }
        ScalarType::Unit => "{ _ in () }",
        _ => "decodeBytes",
    }
}
