//! Swift decoding statement generation.
//!
//! Generates Swift code that decodes byte arrays into Rust types.

use facet_core::{ScalarType, Shape};
use heck::ToLowerCamelCase;
use roam_types::{
    EnumInfo, ShapeKind, StructInfo, VariantKind, classify_shape, classify_variant, is_bytes,
};

/// Generate a Swift decode statement for a given shape.
/// Returns code that decodes from `payload` at `cursor` into a variable named `var_name`.
pub fn generate_decode_stmt(shape: &'static Shape, var_name: &str, indent: &str) -> String {
    generate_decode_stmt_with_cursor(shape, var_name, indent, "cursor")
}

/// Generate a Swift decode statement for a given shape using a custom cursor variable name.
pub fn generate_decode_stmt_with_cursor(
    shape: &'static Shape,
    var_name: &str,
    indent: &str,
    cursor_var: &str,
) -> String {
    generate_decode_stmt_from_with_cursor(shape, var_name, indent, "payload", cursor_var)
}

/// Generate a Swift decode statement for a given shape from a specific data variable.
/// Returns code that decodes from `data_var` at `cursor` into a variable named `var_name`.
pub fn generate_decode_stmt_from(
    shape: &'static Shape,
    var_name: &str,
    indent: &str,
    data_var: &str,
) -> String {
    generate_decode_stmt_from_with_cursor(shape, var_name, indent, data_var, "cursor")
}

/// Generate a Swift decode statement for a given shape from a specific data variable
/// and using a custom cursor variable name.
pub fn generate_decode_stmt_from_with_cursor(
    shape: &'static Shape,
    var_name: &str,
    indent: &str,
    data_var: &str,
    cursor_var: &str,
) -> String {
    // Check for bytes first
    if is_bytes(shape) {
        return format!(
            "{indent}let {var_name} = try decodeBytes(from: {data_var}, offset: &{cursor_var})\n"
        );
    }

    match classify_shape(shape) {
        ShapeKind::Scalar(scalar) => {
            let decode_fn = swift_decode_fn(scalar);
            format!(
                "{indent}let {var_name} = try {decode_fn}(from: {data_var}, offset: &{cursor_var})\n"
            )
        }
        ShapeKind::List { element }
        | ShapeKind::Slice { element }
        | ShapeKind::Array { element, .. } => {
            let inner_decode = generate_decode_closure(element);
            format!(
                "{indent}let {var_name} = try decodeVec(from: {data_var}, offset: &{cursor_var}, decoder: {inner_decode})\n"
            )
        }
        ShapeKind::Option { inner } => {
            let inner_decode = generate_decode_closure(inner);
            format!(
                "{indent}let {var_name} = try decodeOption(from: {data_var}, offset: &{cursor_var}, decoder: {inner_decode})\n"
            )
        }
        ShapeKind::Tuple { elements } if elements.len() == 2 => {
            let a_decode = generate_decode_closure(elements[0].shape);
            let b_decode = generate_decode_closure(elements[1].shape);
            format!(
                "{indent}let {var_name} = try decodeTuple2(from: {data_var}, offset: &{cursor_var}, decoderA: {a_decode}, decoderB: {b_decode})\n"
            )
        }
        ShapeKind::TupleStruct { fields } if fields.len() == 2 => {
            let a_decode = generate_decode_closure(fields[0].shape());
            let b_decode = generate_decode_closure(fields[1].shape());
            format!(
                "{indent}let {var_name} = try decodeTuple2(from: {data_var}, offset: &{cursor_var}, decoderA: {a_decode}, decoderB: {b_decode})\n"
            )
        }
        ShapeKind::Struct(StructInfo {
            name: Some(name),
            fields,
            ..
        }) => {
            // Named struct - decode fields inline and construct
            let mut out = String::new();
            for f in fields.iter() {
                let field_name = f.name.to_lower_camel_case();
                out.push_str(&generate_decode_stmt_from_with_cursor(
                    f.shape(),
                    &format!("_{var_name}_{field_name}"),
                    indent,
                    data_var,
                    cursor_var,
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
            // Named enum - decode discriminant then decode variant
            let mut out = String::new();
            out.push_str(&format!(
                "{indent}let _{var_name}_disc = try decodeVarint(from: {data_var}, offset: &{cursor_var})\n"
            ));
            out.push_str(&format!("{indent}let {var_name}: {name}\n"));
            out.push_str(&format!("{indent}switch _{var_name}_disc {{\n"));
            for (i, v) in variants.iter().enumerate() {
                out.push_str(&format!("{indent}case {i}:\n"));
                match classify_variant(v) {
                    VariantKind::Unit => {
                        out.push_str(&format!(
                            "{indent}    {var_name} = .{}\n",
                            v.name.to_lower_camel_case()
                        ));
                    }
                    VariantKind::Newtype { inner } => {
                        out.push_str(&generate_decode_stmt_from_with_cursor(
                            inner,
                            &format!("_{var_name}_val"),
                            &format!("{indent}    "),
                            data_var,
                            cursor_var,
                        ));
                        out.push_str(&format!(
                            "{indent}    {var_name} = .{}(_{var_name}_val)\n",
                            v.name.to_lower_camel_case()
                        ));
                    }
                    VariantKind::Tuple { fields } => {
                        for (j, f) in fields.iter().enumerate() {
                            out.push_str(&generate_decode_stmt_from_with_cursor(
                                f.shape(),
                                &format!("_{var_name}_f{j}"),
                                &format!("{indent}    "),
                                data_var,
                                cursor_var,
                            ));
                        }
                        let args: Vec<String> = (0..fields.len())
                            .map(|j| format!("_{var_name}_f{j}"))
                            .collect();
                        out.push_str(&format!(
                            "{indent}    {var_name} = .{}({})\n",
                            v.name.to_lower_camel_case(),
                            args.join(", ")
                        ));
                    }
                    VariantKind::Struct { fields } => {
                        for f in fields.iter() {
                            let field_name = f.name.to_lower_camel_case();
                            out.push_str(&generate_decode_stmt_from_with_cursor(
                                f.shape(),
                                &format!("_{var_name}_{field_name}"),
                                &format!("{indent}    "),
                                data_var,
                                cursor_var,
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
                            "{indent}    {var_name} = .{}({})\n",
                            v.name.to_lower_camel_case(),
                            args.join(", ")
                        ));
                    }
                }
            }
            out.push_str(&format!("{indent}default:\n"));
            out.push_str(&format!(
                "{indent}    throw RoamError.decodeError(\"unknown enum variant\")\n"
            ));
            out.push_str(&format!("{indent}}}\n"));
            out
        }
        ShapeKind::Pointer { pointee } => {
            generate_decode_stmt_with_cursor(pointee, var_name, indent, cursor_var)
        }
        ShapeKind::Result { ok, err } => {
            // Decode Result<T, E> - discriminant 0 = Ok, 1 = Err
            let ok_type = super::types::swift_type_base(ok);
            let err_type = super::types::swift_type_base(err);
            let mut out = String::new();
            out.push_str(&format!(
                "{indent}let _{var_name}_disc = try decodeVarint(from: {data_var}, offset: &{cursor_var})\n"
            ));
            out.push_str(&format!(
                "{indent}let {var_name}: Result<{ok_type}, {err_type}>\n"
            ));
            out.push_str(&format!("{indent}switch _{var_name}_disc {{\n"));
            out.push_str(&format!("{indent}case 0:\n"));
            out.push_str(&generate_decode_stmt_from_with_cursor(
                ok,
                &format!("_{var_name}_ok"),
                &format!("{indent}    "),
                data_var,
                cursor_var,
            ));
            out.push_str(&format!(
                "{indent}    {var_name} = .success(_{var_name}_ok)\n"
            ));
            out.push_str(&format!("{indent}case 1:\n"));
            out.push_str(&generate_decode_stmt_from_with_cursor(
                err,
                &format!("_{var_name}_err"),
                &format!("{indent}    "),
                data_var,
                cursor_var,
            ));
            out.push_str(&format!(
                "{indent}    {var_name} = .failure(_{var_name}_err)\n"
            ));
            out.push_str(&format!("{indent}default:\n"));
            out.push_str(&format!(
                "{indent}    throw RoamError.decodeError(\"invalid Result discriminant\")\n"
            ));
            out.push_str(&format!("{indent}}}\n"));
            out
        }
        _ => {
            // Fallback for unsupported types
            format!("{indent}let {var_name}: Any = () // unsupported type\n")
        }
    }
}

/// Generate a Swift decode closure for use with decodeVec, decodeOption, etc.
pub fn generate_decode_closure(shape: &'static Shape) -> String {
    if is_bytes(shape) {
        return "{ data, off in try decodeBytes(from: data, offset: &off) }".into();
    }

    match classify_shape(shape) {
        ShapeKind::Scalar(scalar) => {
            let decode_fn = swift_decode_fn(scalar);
            format!("{{ data, off in try {decode_fn}(from: data, offset: &off) }}")
        }
        ShapeKind::List { element } | ShapeKind::Slice { element } => {
            let inner = generate_decode_closure(element);
            format!("{{ data, off in try decodeVec(from: data, offset: &off, decoder: {inner}) }}")
        }
        ShapeKind::Option { inner } => {
            let inner_closure = generate_decode_closure(inner);
            format!(
                "{{ data, off in try decodeOption(from: data, offset: &off, decoder: {inner_closure}) }}"
            )
        }
        ShapeKind::Tuple { elements } if elements.len() == 2 => {
            let a_decode = generate_decode_closure(elements[0].shape);
            let b_decode = generate_decode_closure(elements[1].shape);
            format!(
                "{{ data, off in try decodeTuple2(from: data, offset: &off, decoderA: {a_decode}, decoderB: {b_decode}) }}"
            )
        }
        ShapeKind::TupleStruct { fields } if fields.len() == 2 => {
            let a_decode = generate_decode_closure(fields[0].shape());
            let b_decode = generate_decode_closure(fields[1].shape());
            format!(
                "{{ data, off in try decodeTuple2(from: data, offset: &off, decoderA: {a_decode}, decoderB: {b_decode}) }}"
            )
        }
        ShapeKind::Struct(StructInfo {
            name: Some(name),
            fields,
            ..
        }) => {
            // Generate inline struct decode closure
            let mut code = "{ data, off in\n".to_string();
            for f in fields.iter() {
                let field_name = f.name.to_lower_camel_case();
                let decode_call = generate_inline_decode(f.shape(), "data", "off");
                code.push_str(&format!("    let _{field_name} = try {decode_call}\n"));
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
            // Generate inline enum decode closure
            let mut code = format!(
                "{{ data, off in\n    let disc = try decodeVarint(from: data, offset: &off)\n    let result: {name}\n    switch disc {{\n"
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
                        let inner_decode = generate_inline_decode(inner, "data", "off");
                        code.push_str(&format!(
                            "        let val = try {inner_decode}\n        result = .{}(val)\n",
                            v.name.to_lower_camel_case()
                        ));
                    }
                    VariantKind::Tuple { fields } => {
                        for (j, f) in fields.iter().enumerate() {
                            let inner_decode = generate_inline_decode(f.shape(), "data", "off");
                            code.push_str(&format!("        let f{j} = try {inner_decode}\n"));
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
                            let inner_decode = generate_inline_decode(f.shape(), "data", "off");
                            code.push_str(&format!(
                                "        let _{field_name} = try {inner_decode}\n"
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
            code.push_str("    default:\n        throw RoamError.decodeError(\"unknown enum variant\")\n    }\n    return result\n}");
            code
        }
        ShapeKind::Pointer { pointee } => generate_decode_closure(pointee),
        _ => "{ _, _ in throw RoamError.decodeError(\"unsupported type\") }".into(),
    }
}

/// Generate inline decode expression (for use in closures).
pub fn generate_inline_decode(shape: &'static Shape, data_var: &str, offset_var: &str) -> String {
    if is_bytes(shape) {
        return format!("decodeBytes(from: {data_var}, offset: &{offset_var})");
    }

    match classify_shape(shape) {
        ShapeKind::Scalar(scalar) => {
            let decode_fn = swift_decode_fn(scalar);
            format!("{decode_fn}(from: {data_var}, offset: &{offset_var})")
        }
        ShapeKind::List { element } | ShapeKind::Slice { element } => {
            let inner = generate_decode_closure(element);
            format!("decodeVec(from: {data_var}, offset: &{offset_var}, decoder: {inner})")
        }
        ShapeKind::Option { inner } => {
            let inner_closure = generate_decode_closure(inner);
            format!(
                "decodeOption(from: {data_var}, offset: &{offset_var}, decoder: {inner_closure})"
            )
        }
        ShapeKind::Pointer { pointee } => generate_inline_decode(pointee, data_var, offset_var),
        _ => {
            let decode_closure = generate_decode_closure(shape);
            format!("({decode_closure})({data_var}, &{offset_var})")
        }
    }
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
        ScalarType::Unit => "{ _, _ in () }",
        _ => "decodeBytes", // fallback
    }
}
