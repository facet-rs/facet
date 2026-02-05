//! Swift encoding expression generation.
//!
//! Generates Swift code that encodes Rust types into byte arrays.

use facet_core::{ScalarType, Shape};
use heck::ToLowerCamelCase;
use roam_schema::{
    EnumInfo, ShapeKind, StructInfo, VariantKind, classify_shape, classify_variant, is_bytes,
};

/// Generate a Swift encode expression for a given shape and value.
pub fn generate_encode_expr(shape: &'static Shape, value: &str) -> String {
    if is_bytes(shape) {
        return format!("encodeBytes(Array({value}))");
    }

    match classify_shape(shape) {
        ShapeKind::Scalar(scalar) => {
            let encode_fn = swift_encode_fn(scalar);
            format!("{encode_fn}({value})")
        }
        ShapeKind::List { element }
        | ShapeKind::Slice { element }
        | ShapeKind::Array { element, .. } => {
            let inner_encode = generate_encode_closure(element);
            format!("encodeVec({value}, encoder: {inner_encode})")
        }
        ShapeKind::Option { inner } => {
            let inner_encode = generate_encode_closure(inner);
            format!("encodeOption({value}, encoder: {inner_encode})")
        }
        ShapeKind::Tuple { elements } if elements.len() == 2 => {
            let a_encode = generate_encode_closure(elements[0].shape);
            let b_encode = generate_encode_closure(elements[1].shape);
            format!("{a_encode}({value}.0) + {b_encode}({value}.1)")
        }
        ShapeKind::TupleStruct { fields } if fields.len() == 2 => {
            let a_encode = generate_encode_closure(fields[0].shape());
            let b_encode = generate_encode_closure(fields[1].shape());
            format!("{a_encode}({value}.0) + {b_encode}({value}.1)")
        }
        ShapeKind::Struct(StructInfo { fields, .. }) => {
            // Encode each field and concatenate
            let field_encodes: Vec<String> = fields
                .iter()
                .map(|f| {
                    let field_name = f.name.to_lower_camel_case();
                    generate_encode_expr(f.shape(), &format!("{value}.{field_name}"))
                })
                .collect();
            if field_encodes.is_empty() {
                "[]".into()
            } else {
                field_encodes.join(" + ")
            }
        }
        ShapeKind::Enum(EnumInfo { .. }) => {
            let encode_closure = generate_encode_closure(shape);
            format!("{encode_closure}({value})")
        }
        ShapeKind::Pointer { pointee } => generate_encode_expr(pointee, value),
        ShapeKind::Result { ok, err } => {
            // Encode Result<T, E> - discriminant 0 = Ok, 1 = Err
            let ok_encode = generate_encode_closure(ok);
            let err_encode = generate_encode_closure(err);
            format!(
                "{{ switch {value} {{ case .success(let v): return [UInt8(0)] + {ok_encode}(v); case .failure(let e): return [UInt8(1)] + {err_encode}(e) }} }}()"
            )
        }
        _ => "[]".into(), // fallback
    }
}

/// Generate a Swift encode closure for use with encodeVec, encodeOption, etc.
pub fn generate_encode_closure(shape: &'static Shape) -> String {
    if is_bytes(shape) {
        return "{ encodeBytes(Array($0)) }".into();
    }

    match classify_shape(shape) {
        ShapeKind::Scalar(scalar) => {
            let encode_fn = swift_encode_fn(scalar);
            format!("{{ {encode_fn}($0) }}")
        }
        ShapeKind::List { element } | ShapeKind::Slice { element } => {
            let inner = generate_encode_closure(element);
            format!("{{ encodeVec($0, encoder: {inner}) }}")
        }
        ShapeKind::Option { inner } => {
            let inner_closure = generate_encode_closure(inner);
            format!("{{ encodeOption($0, encoder: {inner_closure}) }}")
        }
        ShapeKind::Tuple { elements } if elements.len() == 2 => {
            let a_encode = generate_encode_closure(elements[0].shape);
            let b_encode = generate_encode_closure(elements[1].shape);
            format!("{{ {a_encode}($0.0) + {b_encode}($0.1) }}")
        }
        ShapeKind::TupleStruct { fields } if fields.len() == 2 => {
            let a_encode = generate_encode_closure(fields[0].shape());
            let b_encode = generate_encode_closure(fields[1].shape());
            format!("{{ {a_encode}($0.0) + {b_encode}($0.1) }}")
        }
        ShapeKind::Struct(StructInfo { fields, .. }) => {
            // Generate inline struct encode closure
            let field_encodes: Vec<String> = fields
                .iter()
                .map(|f| {
                    let field_name = f.name.to_lower_camel_case();
                    generate_encode_expr(f.shape(), &format!("$0.{field_name}"))
                })
                .collect();
            if field_encodes.is_empty() {
                "{ _ in [] }".into()
            } else {
                format!("{{ {} }}", field_encodes.join(" + "))
            }
        }
        ShapeKind::Enum(EnumInfo {
            name: Some(_name),
            variants,
            ..
        }) => {
            // Generate inline enum encode closure with switch
            let mut code = "{ v in\n    switch v {\n".to_string();
            for (i, v) in variants.iter().enumerate() {
                let variant_name = v.name.to_lower_camel_case();
                match classify_variant(v) {
                    VariantKind::Unit => {
                        code.push_str(&format!(
                            "    case .{variant_name}:\n        return [UInt8({i})]\n"
                        ));
                    }
                    VariantKind::Newtype { inner } => {
                        let inner_encode = generate_encode_expr(inner, "val");
                        code.push_str(&format!(
                            "    case .{variant_name}(let val):\n        return [UInt8({i})] + {inner_encode}\n"
                        ));
                    }
                    VariantKind::Tuple { fields } => {
                        let bindings: Vec<String> =
                            (0..fields.len()).map(|j| format!("f{j}")).collect();
                        let field_encodes: Vec<String> = fields
                            .iter()
                            .enumerate()
                            .map(|(j, f)| generate_encode_expr(f.shape(), &format!("f{j}")))
                            .collect();
                        code.push_str(&format!(
                            "    case .{variant_name}({}):\n        return [UInt8({i})] + {}\n",
                            bindings
                                .iter()
                                .map(|b| format!("let {b}"))
                                .collect::<Vec<_>>()
                                .join(", "),
                            field_encodes.join(" + ")
                        ));
                    }
                    VariantKind::Struct { fields } => {
                        let bindings: Vec<String> = fields
                            .iter()
                            .map(|f| f.name.to_lower_camel_case())
                            .collect();
                        let field_encodes: Vec<String> = fields
                            .iter()
                            .map(|f| {
                                let field_name = f.name.to_lower_camel_case();
                                generate_encode_expr(f.shape(), &field_name)
                            })
                            .collect();
                        code.push_str(&format!(
                            "    case .{variant_name}({}):\n        return [UInt8({i})] + {}\n",
                            bindings
                                .iter()
                                .map(|b| format!("let {b}"))
                                .collect::<Vec<_>>()
                                .join(", "),
                            field_encodes.join(" + ")
                        ));
                    }
                }
            }
            code.push_str("    }\n}");
            code
        }
        ShapeKind::Pointer { pointee } => generate_encode_closure(pointee),
        ShapeKind::Result { ok, err } => {
            let ok_encode = generate_encode_closure(ok);
            let err_encode = generate_encode_closure(err);
            format!(
                "{{ switch $0 {{ case .success(let v): return [UInt8(0)] + {ok_encode}(v); case .failure(let e): return [UInt8(1)] + {err_encode}(e) }} }}"
            )
        }
        _ => "{ _ in [] }".into(), // fallback
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
        ScalarType::Unit => "{ _ in [] }",
        _ => "encodeBytes", // fallback
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use facet::Facet;

    #[repr(u8)]
    #[derive(Facet)]
    enum Color {
        Red,
        Green,
    }

    #[test]
    fn test_encode_primitives() {
        assert_eq!(
            generate_encode_expr(<bool as Facet>::SHAPE, "x"),
            "encodeBool(x)"
        );
        assert_eq!(
            generate_encode_expr(<u32 as Facet>::SHAPE, "x"),
            "encodeU32(x)"
        );
        assert_eq!(
            generate_encode_expr(<String as Facet>::SHAPE, "x"),
            "encodeString(x)"
        );
    }

    #[test]
    fn test_encode_vec() {
        let result = generate_encode_expr(<Vec<i32> as Facet>::SHAPE, "items");
        assert!(result.contains("encodeVec"));
        assert!(result.contains("encodeI32"));
    }

    #[test]
    fn test_encode_option() {
        let result = generate_encode_expr(<Option<String> as Facet>::SHAPE, "val");
        assert!(result.contains("encodeOption"));
        assert!(result.contains("encodeString"));
    }

    #[test]
    fn test_encode_bytes() {
        let result = generate_encode_expr(<Vec<u8> as Facet>::SHAPE, "data");
        assert_eq!(result, "encodeBytes(Array(data))");
    }

    #[test]
    fn test_encode_enum_expr() {
        let result = generate_encode_expr(<Color as Facet>::SHAPE, "color");
        assert!(result.contains("switch"));
        assert!(result.contains("(color)"));
        assert_ne!(result, "[]");
    }
}
