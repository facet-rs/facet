//! TypeScript type generation and collection.
//!
//! This module handles:
//! - Collecting named types (structs and enums) from service definitions
//! - Generating TypeScript type definitions (interfaces, type unions)
//! - Converting Rust types to TypeScript type strings

use std::collections::HashSet;

use facet_core::{ScalarType, Shape};
use roam_schema::{
    EnumInfo, ServiceDetail, ShapeKind, StructInfo, VariantKind, classify_shape, classify_variant,
    is_bytes,
};

/// Generate TypeScript field access expression.
/// Uses bracket notation for numeric field names (tuple fields), dot notation otherwise.
pub fn ts_field_access(expr: &str, field_name: &str) -> String {
    if field_name
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_digit())
    {
        format!("{expr}[{field_name}]")
    } else {
        format!("{expr}.{field_name}")
    }
}

/// Collect all named types (structs and enums with a name) from a service.
/// Returns a vector of (name, Shape) pairs in dependency order.
pub fn collect_named_types(service: &ServiceDetail) -> Vec<(String, &'static Shape)> {
    let mut seen = HashSet::new();
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
            ShapeKind::List { element } => visit(element, seen, types),
            ShapeKind::Option { inner } => visit(inner, seen, types),
            ShapeKind::Array { element, .. } => visit(element, seen, types),
            ShapeKind::Map { key, value } => {
                visit(key, seen, types);
                visit(value, seen, types);
            }
            ShapeKind::Set { element } => visit(element, seen, types),
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
            // Scalars, slices, opaque - no named types to collect
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

/// Generate TypeScript type definitions for all named types.
pub fn generate_named_types(named_types: &[(String, &'static Shape)]) -> String {
    let mut out = String::new();

    if named_types.is_empty() {
        return out;
    }

    out.push_str("// Named type definitions\n");

    for (name, shape) in named_types {
        match classify_shape(shape) {
            ShapeKind::Struct(StructInfo { fields, .. }) => {
                out.push_str(&format!("export interface {} {{\n", name));
                for field in fields {
                    out.push_str(&format!(
                        "  {}: {};\n",
                        field.name,
                        ts_type_base_named(field.shape())
                    ));
                }
                out.push_str("}\n\n");
            }
            ShapeKind::Enum(EnumInfo { variants, .. }) => {
                out.push_str(&format!("export type {} =\n", name));
                for (i, variant) in variants.iter().enumerate() {
                    let variant_type = match classify_variant(variant) {
                        VariantKind::Unit => format!("{{ tag: '{}' }}", variant.name),
                        VariantKind::Newtype { inner } => {
                            format!(
                                "{{ tag: '{}'; value: {} }}",
                                variant.name,
                                ts_type_base_named(inner)
                            )
                        }
                        VariantKind::Tuple { fields } | VariantKind::Struct { fields } => {
                            let field_strs = fields
                                .iter()
                                .map(|f| format!("{}: {}", f.name, ts_type_base_named(f.shape())))
                                .collect::<Vec<_>>()
                                .join("; ");
                            format!("{{ tag: '{}'; {} }}", variant.name, field_strs)
                        }
                    };
                    let sep = if i < variants.len() - 1 { "" } else { ";" };
                    out.push_str(&format!("  | {}{}\n", variant_type, sep));
                }
                out.push('\n');
            }
            _ => {}
        }
    }

    out
}

/// Convert Shape to TypeScript type string, using named types when available.
/// This handles container types recursively, using named types at every level.
pub fn ts_type_base_named(shape: &'static Shape) -> String {
    match classify_shape(shape) {
        // Named types - use the name directly
        ShapeKind::Struct(StructInfo {
            name: Some(name), ..
        }) => name.to_string(),
        ShapeKind::Enum(EnumInfo {
            name: Some(name), ..
        }) => name.to_string(),

        // Container types - recurse with ts_type_base_named
        ShapeKind::List { element } => {
            // Check for bytes first
            if is_bytes(shape) {
                return "Uint8Array".into();
            }
            // Wrap in parens if inner is an anonymous enum to avoid precedence issues
            if matches!(
                classify_shape(element),
                ShapeKind::Enum(EnumInfo { name: None, .. })
            ) {
                format!("({})[]", ts_type_base_named(element))
            } else {
                format!("{}[]", ts_type_base_named(element))
            }
        }
        ShapeKind::Option { inner } => format!("{} | null", ts_type_base_named(inner)),
        ShapeKind::Array { element, len } => format!("[{}; {}]", ts_type_base_named(element), len),
        ShapeKind::Map { key, value } => {
            format!(
                "Map<{}, {}>",
                ts_type_base_named(key),
                ts_type_base_named(value)
            )
        }
        ShapeKind::Set { element } => format!("Set<{}>", ts_type_base_named(element)),
        ShapeKind::Tuple { elements } => {
            let inner = elements
                .iter()
                .map(|p| ts_type_base_named(p.shape))
                .collect::<Vec<_>>()
                .join(", ");
            format!("[{inner}]")
        }
        ShapeKind::Tx { inner } => format!("Tx<{}>", ts_type_base_named(inner)),
        ShapeKind::Rx { inner } => format!("Rx<{}>", ts_type_base_named(inner)),

        // Anonymous structs - inline as object type
        ShapeKind::Struct(StructInfo {
            name: None, fields, ..
        }) => {
            let inner = fields
                .iter()
                .map(|f| format!("{}: {}", f.name, ts_type_base_named(f.shape())))
                .collect::<Vec<_>>()
                .join("; ");
            format!("{{ {inner} }}")
        }

        // Anonymous enums - inline as union type
        ShapeKind::Enum(EnumInfo {
            name: None,
            variants,
        }) => variants
            .iter()
            .map(|v| match classify_variant(v) {
                VariantKind::Unit => format!("{{ tag: '{}' }}", v.name),
                VariantKind::Newtype { inner } => {
                    format!(
                        "{{ tag: '{}'; value: {} }}",
                        v.name,
                        ts_type_base_named(inner)
                    )
                }
                VariantKind::Tuple { fields } | VariantKind::Struct { fields } => {
                    let field_strs = fields
                        .iter()
                        .map(|f| format!("{}: {}", f.name, ts_type_base_named(f.shape())))
                        .collect::<Vec<_>>()
                        .join("; ");
                    format!("{{ tag: '{}'; {} }}", v.name, field_strs)
                }
            })
            .collect::<Vec<_>>()
            .join(" | "),

        // Scalars and other types
        ShapeKind::Scalar(scalar) => ts_scalar_type(scalar),
        ShapeKind::Slice { element } => format!("{}[]", ts_type_base_named(element)),
        ShapeKind::Pointer { pointee } => ts_type_base_named(pointee),
        ShapeKind::Result { ok, err } => {
            format!(
                "{{ ok: true; value: {} }} | {{ ok: false; error: {} }}",
                ts_type_base_named(ok),
                ts_type_base_named(err)
            )
        }
        ShapeKind::TupleStruct { fields } => {
            let inner = fields
                .iter()
                .map(|f| ts_type_base_named(f.shape()))
                .collect::<Vec<_>>()
                .join(", ");
            format!("[{inner}]")
        }
        ShapeKind::Opaque => "unknown".into(),
    }
}

/// Convert ScalarType to TypeScript type string.
pub fn ts_scalar_type(scalar: ScalarType) -> String {
    match scalar {
        ScalarType::Bool => "boolean".into(),
        ScalarType::U8
        | ScalarType::U16
        | ScalarType::U32
        | ScalarType::I8
        | ScalarType::I16
        | ScalarType::I32
        | ScalarType::F32
        | ScalarType::F64 => "number".into(),
        ScalarType::U64
        | ScalarType::U128
        | ScalarType::I64
        | ScalarType::I128
        | ScalarType::USize
        | ScalarType::ISize => "bigint".into(),
        ScalarType::Char | ScalarType::Str | ScalarType::String | ScalarType::CowStr => {
            "string".into()
        }
        ScalarType::Unit => "void".into(),
        _ => "unknown".into(),
    }
}

/// Convert Shape to TypeScript type string for client arguments.
/// Schema is from server's perspective - no inversion needed.
/// Client passes the same types that server receives.
pub fn ts_type_client_arg(shape: &'static Shape) -> String {
    match classify_shape(shape) {
        ShapeKind::Tx { inner } => format!("Tx<{}>", ts_type_client_arg(inner)),
        ShapeKind::Rx { inner } => format!("Rx<{}>", ts_type_client_arg(inner)),
        _ => ts_type_base_named(shape),
    }
}

/// Convert Shape to TypeScript type string for client returns.
/// Schema is from server's perspective - no inversion needed.
pub fn ts_type_client_return(shape: &'static Shape) -> String {
    match classify_shape(shape) {
        ShapeKind::Tx { inner } => format!("Tx<{}>", ts_type_client_return(inner)),
        ShapeKind::Rx { inner } => format!("Rx<{}>", ts_type_client_return(inner)),
        _ => ts_type_base_named(shape),
    }
}

/// Convert Shape to TypeScript type string for server/handler arguments.
/// Schema is from server's perspective - no inversion needed.
/// Rx means server receives, Tx means server sends.
pub fn ts_type_server_arg(shape: &'static Shape) -> String {
    match classify_shape(shape) {
        ShapeKind::Tx { inner } => format!("Tx<{}>", ts_type_server_arg(inner)),
        ShapeKind::Rx { inner } => format!("Rx<{}>", ts_type_server_arg(inner)),
        _ => ts_type_base_named(shape),
    }
}

/// Schema is from server's perspective - no inversion needed.
pub fn ts_type_server_return(shape: &'static Shape) -> String {
    match classify_shape(shape) {
        ShapeKind::Tx { inner } => format!("Tx<{}>", ts_type_server_return(inner)),
        ShapeKind::Rx { inner } => format!("Rx<{}>", ts_type_server_return(inner)),
        _ => ts_type_base_named(shape),
    }
}

/// TypeScript type for user-facing type definitions.
/// Uses named types when available.
pub fn ts_type(shape: &'static Shape) -> String {
    ts_type_base_named(shape)
}

/// Check if a type can be fully encoded/decoded.
/// Streaming types (Tx/Rx) are supported - they encode as stream IDs.
pub fn is_fully_supported(shape: &'static Shape) -> bool {
    match classify_shape(shape) {
        // Streaming types are supported - they encode/decode as stream IDs
        ShapeKind::Tx { inner } | ShapeKind::Rx { inner } => is_fully_supported(inner),
        ShapeKind::List { element }
        | ShapeKind::Option { inner: element }
        | ShapeKind::Set { element }
        | ShapeKind::Array { element, .. }
        | ShapeKind::Slice { element } => is_fully_supported(element),
        ShapeKind::Map { key, value } => is_fully_supported(key) && is_fully_supported(value),
        ShapeKind::Tuple { elements } => elements.iter().all(|p| is_fully_supported(p.shape)),
        ShapeKind::TupleStruct { fields } => fields.iter().all(|f| is_fully_supported(f.shape())),
        ShapeKind::Struct(StructInfo { fields, .. }) => {
            fields.iter().all(|f| is_fully_supported(f.shape()))
        }
        ShapeKind::Enum(EnumInfo { variants, .. }) => {
            variants.iter().all(|v| match classify_variant(v) {
                VariantKind::Unit => true,
                VariantKind::Newtype { inner } => is_fully_supported(inner),
                VariantKind::Tuple { fields } | VariantKind::Struct { fields } => {
                    fields.iter().all(|f| is_fully_supported(f.shape()))
                }
            })
        }
        ShapeKind::Pointer { pointee } => is_fully_supported(pointee),
        ShapeKind::Scalar(_) => true,
        ShapeKind::Result { .. } => false,
        ShapeKind::Opaque => false,
    }
}
