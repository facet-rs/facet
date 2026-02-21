//! Swift type generation and collection.
//!
//! This module handles:
//! - Collecting named types (structs and enums) from service definitions
//! - Generating Swift type definitions (structs, enums)
//! - Converting Rust types to Swift type strings

use std::collections::HashSet;

use facet_core::{ScalarType, Shape};
use heck::ToLowerCamelCase;
use roam_schema::{
    EnumInfo, ServiceDetail, ShapeKind, StructInfo, VariantKind, classify_shape, classify_variant,
    is_bytes, is_rx, is_tx,
};

/// Collect all named types (structs and enums with a name) from a service.
/// Returns a vector of (name, Shape) pairs in dependency order.
pub fn collect_named_types(service: &ServiceDetail) -> Vec<(String, &'static Shape)> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut types = Vec::new();

    fn visit(
        shape: &'static Shape,
        seen: &mut HashSet<String>,
        types: &mut Vec<(String, &'static Shape)>,
    ) {
        match classify_shape(shape) {
            ShapeKind::Struct(StructInfo {
                name: Some(name),
                fields,
                ..
            }) => {
                if !seen.contains(name) {
                    seen.insert(name.to_string());
                    // Visit nested types first (dependencies before dependents)
                    for field in fields {
                        visit(field.shape(), seen, types);
                    }
                    types.push((name.to_string(), shape));
                }
            }
            ShapeKind::Enum(EnumInfo {
                name: Some(name),
                variants,
            }) => {
                if !seen.contains(name) {
                    seen.insert(name.to_string());
                    // Visit nested types in variants
                    for variant in variants {
                        match classify_variant(variant) {
                            VariantKind::Newtype { inner } => visit(inner, seen, types),
                            VariantKind::Struct { fields } | VariantKind::Tuple { fields } => {
                                for field in fields {
                                    visit(field.shape(), seen, types);
                                }
                            }
                            VariantKind::Unit => {}
                        }
                    }
                    types.push((name.to_string(), shape));
                }
            }
            ShapeKind::List { element }
            | ShapeKind::Slice { element }
            | ShapeKind::Option { inner: element }
            | ShapeKind::Array { element, .. }
            | ShapeKind::Set { element } => visit(element, seen, types),
            ShapeKind::Map { key, value } => {
                visit(key, seen, types);
                visit(value, seen, types);
            }
            ShapeKind::Tuple { elements } => {
                for param in elements {
                    visit(param.shape, seen, types);
                }
            }
            ShapeKind::Tx { inner } | ShapeKind::Rx { inner } => visit(inner, seen, types),
            ShapeKind::Pointer { pointee } => visit(pointee, seen, types),
            ShapeKind::Result { ok, err } => {
                visit(ok, seen, types);
                visit(err, seen, types);
            }
            _ => {}
        }
    }

    for method in &service.methods {
        for arg in &method.args {
            visit(arg.ty, &mut seen, &mut types);
        }
        visit(method.return_type, &mut seen, &mut types);
    }

    types
}

/// Generate Swift type definitions for all named types.
pub fn generate_named_types(named_types: &[(String, &'static Shape)]) -> String {
    let mut out = String::new();

    for (name, shape) in named_types {
        match classify_shape(shape) {
            ShapeKind::Struct(StructInfo { fields, .. }) => {
                out.push_str(&format!("public struct {name}: Codable, Sendable {{\n"));
                for field in fields {
                    let field_name = field.name.to_lower_camel_case();
                    let field_type = swift_type_base(field.shape());
                    out.push_str(&format!("    public var {field_name}: {field_type}\n"));
                }
                out.push('\n');
                // Generate initializer
                out.push_str("    public init(");
                for (i, field) in fields.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    let field_name = field.name.to_lower_camel_case();
                    let field_type = swift_type_base(field.shape());
                    out.push_str(&format!("{field_name}: {field_type}"));
                }
                out.push_str(") {\n");
                for field in fields {
                    let field_name = field.name.to_lower_camel_case();
                    out.push_str(&format!("        self.{field_name} = {field_name}\n"));
                }
                out.push_str("    }\n");
                out.push_str("}\n\n");
            }
            ShapeKind::Enum(EnumInfo { variants, .. }) => {
                // Add Error conformance if the enum name ends with "Error"
                let protocols = if name.ends_with("Error") {
                    "Codable, Sendable, Error"
                } else {
                    "Codable, Sendable"
                };
                out.push_str(&format!("public enum {name}: {protocols} {{\n"));
                for variant in variants {
                    let variant_name = variant.name.to_lower_camel_case();
                    match classify_variant(variant) {
                        VariantKind::Unit => {
                            out.push_str(&format!("    case {variant_name}\n"));
                        }
                        VariantKind::Newtype { inner } => {
                            let inner_type = swift_type_base(inner);
                            out.push_str(&format!("    case {variant_name}({inner_type})\n"));
                        }
                        VariantKind::Tuple { fields } => {
                            let field_types: Vec<_> =
                                fields.iter().map(|f| swift_type_base(f.shape())).collect();
                            out.push_str(&format!(
                                "    case {variant_name}({})\n",
                                field_types.join(", ")
                            ));
                        }
                        VariantKind::Struct { fields } => {
                            let field_decls: Vec<_> = fields
                                .iter()
                                .map(|f| {
                                    format!(
                                        "{}: {}",
                                        f.name.to_lower_camel_case(),
                                        swift_type_base(f.shape())
                                    )
                                })
                                .collect();
                            out.push_str(&format!(
                                "    case {variant_name}({})\n",
                                field_decls.join(", ")
                            ));
                        }
                    }
                }
                out.push_str("}\n\n");
            }
            _ => {}
        }
    }

    out
}

/// Convert ScalarType to Swift type string.
pub fn swift_scalar_type(scalar: ScalarType) -> String {
    match scalar {
        ScalarType::Bool => "Bool".into(),
        ScalarType::U8 => "UInt8".into(),
        ScalarType::U16 => "UInt16".into(),
        ScalarType::U32 => "UInt32".into(),
        ScalarType::U64 => "UInt64".into(),
        ScalarType::U128 => "UInt128".into(),
        ScalarType::USize => "UInt".into(),
        ScalarType::I8 => "Int8".into(),
        ScalarType::I16 => "Int16".into(),
        ScalarType::I32 => "Int32".into(),
        ScalarType::I64 => "Int64".into(),
        ScalarType::I128 => "Int128".into(),
        ScalarType::ISize => "Int".into(),
        ScalarType::F32 => "Float".into(),
        ScalarType::F64 => "Double".into(),
        ScalarType::Char | ScalarType::Str | ScalarType::String | ScalarType::CowStr => {
            "String".into()
        }
        ScalarType::Unit => "Void".into(),
        _ => "Data".into(),
    }
}

/// Convert Shape to Swift type string.
pub fn swift_type_base(shape: &'static Shape) -> String {
    // Check for bytes first
    if is_bytes(shape) {
        return "Data".into();
    }

    match classify_shape(shape) {
        ShapeKind::Scalar(scalar) => swift_scalar_type(scalar),
        ShapeKind::List { element } => format!("[{}]", swift_type_base(element)),
        ShapeKind::Slice { element } => format!("[{}]", swift_type_base(element)),
        ShapeKind::Option { inner } => format!("{}?", swift_type_base(inner)),
        ShapeKind::Array { element, .. } => format!("[{}]", swift_type_base(element)),
        ShapeKind::Map { key, value } => {
            format!("[{}: {}]", swift_type_base(key), swift_type_base(value))
        }
        ShapeKind::Set { element } => format!("Set<{}>", swift_type_base(element)),
        ShapeKind::Tuple { elements } => {
            if elements.is_empty() {
                "Void".into()
            } else {
                let types: Vec<_> = elements.iter().map(|p| swift_type_base(p.shape)).collect();
                format!("({})", types.join(", "))
            }
        }
        ShapeKind::Tx { inner } => format!("Tx<{}>", swift_type_base(inner)),
        ShapeKind::Rx { inner } => format!("Rx<{}>", swift_type_base(inner)),
        ShapeKind::Struct(StructInfo {
            name: Some(name), ..
        }) => name.to_string(),
        ShapeKind::Enum(EnumInfo {
            name: Some(name), ..
        }) => name.to_string(),
        ShapeKind::Struct(StructInfo {
            name: None, fields, ..
        }) => {
            // Anonymous struct - use tuple-like representation
            let types: Vec<_> = fields.iter().map(|f| swift_type_base(f.shape())).collect();
            format!("({})", types.join(", "))
        }
        ShapeKind::Enum(EnumInfo {
            name: None,
            variants,
        }) => {
            // Anonymous enum - not well supported in Swift, use Any
            let _ = variants; // suppress warning
            "Any".into()
        }
        ShapeKind::Pointer { pointee } => swift_type_base(pointee),
        ShapeKind::Result { ok, err } => {
            format!("Result<{}, {}>", swift_type_base(ok), swift_type_base(err))
        }
        ShapeKind::TupleStruct { fields } => {
            let types: Vec<_> = fields.iter().map(|f| swift_type_base(f.shape())).collect();
            format!("({})", types.join(", "))
        }
        ShapeKind::Opaque => "Data".into(),
    }
}

/// Convert Shape to Swift type string for client arguments.
pub fn swift_type_client_arg(shape: &'static Shape) -> String {
    match classify_shape(shape) {
        ShapeKind::Tx { inner } => format!("UnboundTx<{}>", swift_type_base(inner)),
        ShapeKind::Rx { inner } => format!("UnboundRx<{}>", swift_type_base(inner)),
        _ => swift_type_base(shape),
    }
}

/// Convert Shape to Swift type string for client returns.
pub fn swift_type_client_return(shape: &'static Shape) -> String {
    match classify_shape(shape) {
        ShapeKind::Tx { inner } => format!("UnboundTx<{}>", swift_type_base(inner)),
        ShapeKind::Rx { inner } => format!("UnboundRx<{}>", swift_type_base(inner)),
        ShapeKind::Scalar(ScalarType::Unit) => "Void".into(),
        ShapeKind::Tuple { elements: [] } => "Void".into(),
        _ => swift_type_base(shape),
    }
}

/// Convert Shape to Swift type string for server/handler arguments.
pub fn swift_type_server_arg(shape: &'static Shape) -> String {
    match classify_shape(shape) {
        ShapeKind::Tx { inner } => format!("Tx<{}>", swift_type_base(inner)),
        ShapeKind::Rx { inner } => format!("Rx<{}>", swift_type_base(inner)),
        _ => swift_type_base(shape),
    }
}

/// Convert Shape to Swift type string for server returns.
pub fn swift_type_server_return(shape: &'static Shape) -> String {
    match classify_shape(shape) {
        ShapeKind::Tx { inner } => format!("Tx<{}>", swift_type_base(inner)),
        ShapeKind::Rx { inner } => format!("Rx<{}>", swift_type_base(inner)),
        ShapeKind::Scalar(ScalarType::Unit) => "Void".into(),
        ShapeKind::Tuple { elements: [] } => "Void".into(),
        _ => swift_type_base(shape),
    }
}

/// Check if a shape represents a channel type (Tx or Rx).
pub fn is_channel(shape: &'static Shape) -> bool {
    is_tx(shape) || is_rx(shape)
}

/// Format documentation comments for Swift.
pub fn format_doc(doc: &str, indent: &str) -> String {
    doc.lines()
        .map(|line| format!("{indent}/// {line}\n"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use facet::Facet;

    #[test]
    fn test_swift_type_base_primitives() {
        assert_eq!(swift_type_base(<bool as Facet>::SHAPE), "Bool");
        assert_eq!(swift_type_base(<u32 as Facet>::SHAPE), "UInt32");
        assert_eq!(swift_type_base(<i64 as Facet>::SHAPE), "Int64");
        assert_eq!(swift_type_base(<f32 as Facet>::SHAPE), "Float");
        assert_eq!(swift_type_base(<f64 as Facet>::SHAPE), "Double");
        assert_eq!(swift_type_base(<String as Facet>::SHAPE), "String");
        assert_eq!(swift_type_base(<Vec<u8> as Facet>::SHAPE), "Data");
        assert_eq!(swift_type_base(<() as Facet>::SHAPE), "Void");
    }

    #[test]
    fn test_swift_type_base_containers() {
        assert_eq!(swift_type_base(<Vec<i32> as Facet>::SHAPE), "[Int32]");
        assert_eq!(swift_type_base(<Option<String> as Facet>::SHAPE), "String?");
    }
}
