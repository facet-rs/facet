//! Swift schema generation for wire schema exchange.
//!
//! Generates runtime schema information for protocol wire schema exchange
//! using the canonical `Schema`/`SchemaKind`/`TypeRef` representation.
//! Type-level channel binding is driven by the same schema registry.

use facet_core::Facet;
use heck::ToLowerCamelCase;
use vox_types::{ServiceDescriptor, ShapeKind, TypeRef, VoxError, classify_shape, extract_schemas};

/// Generate schema code (wire schemas only — binding uses the same registry).
pub fn generate_schemas(service: &ServiceDescriptor) -> String {
    generate_wire_schemas(service)
}

/// Generate wire schema infrastructure for protocol schema exchange.
///
/// Generates:
/// 1. A global schema registry containing all schemas for all methods (deduplicated)
/// 2. Per-method schema ID lists and root TypeRefs for args and response
///
/// At runtime, the Swift code filters schemas through SchemaSendTracker before encoding.
fn generate_wire_schemas(service: &ServiceDescriptor) -> String {
    use crate::render::hex_u64;
    use std::collections::HashMap;
    use vox_types::{Schema, SchemaHash};

    let service_name = service.service_name.to_lower_camel_case();

    // Extract Result and VoxError schemas once (used for wrapping responses)
    let result_extracted =
        extract_schemas(<Result<bool, u32> as Facet<'static>>::SHAPE).expect("Result schema");
    let result_type_id = match &result_extracted.root {
        TypeRef::Concrete { type_id, .. } => *type_id,
        _ => panic!("Result root should be concrete"),
    };

    let vox_error_extracted =
        extract_schemas(<VoxError<std::convert::Infallible> as Facet<'static>>::SHAPE)
            .expect("VoxError schema");
    let vox_error_type_id = match &vox_error_extracted.root {
        TypeRef::Concrete { type_id, .. } => *type_id,
        _ => panic!("VoxError root should be concrete"),
    };

    // Collect all schemas across all methods into a global registry
    let mut global_schemas: HashMap<SchemaHash, Schema> = HashMap::new();

    // Add Result and VoxError schemas
    for schema in result_extracted.schemas.iter() {
        global_schemas.insert(schema.id, schema.clone());
    }
    for schema in vox_error_extracted.schemas.iter() {
        global_schemas.insert(schema.id, schema.clone());
    }

    // Per-method info: (args_schema_ids, args_root, response_schema_ids, response_root)
    struct MethodSchemaInfo {
        args_schema_ids: Vec<SchemaHash>,
        args_root: TypeRef,
        response_schema_ids: Vec<SchemaHash>,
        response_root: TypeRef,
    }
    let mut method_infos: Vec<(u64, MethodSchemaInfo)> = Vec::new();

    for method in service.methods {
        let method_id = crate::method_id(method);

        // Extract args schemas
        let args_extracted = extract_schemas(method.args_shape).expect("args schema extraction");
        let args_schema_ids: Vec<SchemaHash> =
            args_extracted.schemas.iter().map(|s| s.id).collect();
        for schema in args_extracted.schemas.iter().cloned() {
            global_schemas.insert(schema.id, schema);
        }

        // Extract response schemas - wrap in Result<T, VoxError<E>>
        let (ok_extracted, err_extracted) = match classify_shape(method.return_shape) {
            ShapeKind::Result { ok, err } => (
                extract_schemas(ok).expect("ok schema"),
                extract_schemas(err).expect("err schema"),
            ),
            _ => (
                extract_schemas(method.return_shape).expect("return schema"),
                extract_schemas(<std::convert::Infallible as Facet<'static>>::SHAPE)
                    .expect("Infallible schema"),
            ),
        };

        // Collect response schema IDs (including Result and VoxError)
        let mut response_schema_ids: Vec<SchemaHash> = Vec::new();
        for schema in result_extracted.schemas.iter() {
            response_schema_ids.push(schema.id);
        }
        for schema in vox_error_extracted.schemas.iter() {
            response_schema_ids.push(schema.id);
        }
        for schema in ok_extracted.schemas.iter().cloned() {
            response_schema_ids.push(schema.id);
            global_schemas.insert(schema.id, schema);
        }
        for schema in err_extracted.schemas.iter().cloned() {
            response_schema_ids.push(schema.id);
            global_schemas.insert(schema.id, schema);
        }

        // Deduplicate schema IDs (smaller codegen output)
        let mut seen = std::collections::HashSet::new();
        response_schema_ids.retain(|id| seen.insert(*id));

        // Build the response root: Result<ok_root, VoxError<err_root>>
        let vox_error_ref = TypeRef::generic(vox_error_type_id, vec![err_extracted.root.clone()]);
        let response_root = TypeRef::generic(
            result_type_id,
            vec![ok_extracted.root.clone(), vox_error_ref],
        );

        method_infos.push((
            method_id,
            MethodSchemaInfo {
                args_schema_ids,
                args_root: args_extracted.root.clone(),
                response_schema_ids,
                response_root,
            },
        ));
    }

    let mut out = String::new();

    // Generate global schema registry
    out.push_str("/// Global schema registry containing all schemas for this service.\n");
    out.push_str(&format!(
        "nonisolated(unsafe) public let {service_name}_schema_registry: [UInt64: Schema] = [\n"
    ));

    let mut sorted_schemas: Vec<_> = global_schemas.into_iter().collect();
    sorted_schemas.sort_by_key(|(id, _)| *id);

    for (schema_id, schema) in &sorted_schemas {
        out.push_str(&format!(
            "    {}: {},\n",
            hex_u64(schema_id.0),
            format_swift_schema(schema)
        ));
    }
    out.push_str("]\n\n");

    // Generate per-method schema info
    out.push_str("/// Per-method schema information for wire protocol.\n");
    out.push_str(&format!(
        "nonisolated(unsafe) public let {service_name}_method_schemas: [UInt64: MethodSchemaInfo] = [\n"
    ));

    for (method_id, info) in &method_infos {
        out.push_str(&format!("    {}: MethodSchemaInfo(\n", hex_u64(*method_id)));
        out.push_str(&format!(
            "        argsSchemaIds: [{}],\n",
            info.args_schema_ids
                .iter()
                .map(|id| hex_u64(id.0))
                .collect::<Vec<_>>()
                .join(", ")
        ));
        out.push_str(&format!(
            "        argsRoot: {},\n",
            format_swift_type_ref(&info.args_root)
        ));
        out.push_str(&format!(
            "        responseSchemaIds: [{}],\n",
            info.response_schema_ids
                .iter()
                .map(|id| hex_u64(id.0))
                .collect::<Vec<_>>()
                .join(", ")
        ));
        out.push_str(&format!(
            "        responseRoot: {}\n",
            format_swift_type_ref(&info.response_root)
        ));
        out.push_str("    ),\n");
    }
    out.push_str("]\n\n");

    out
}

/// Format a Schema as Swift code.
fn format_swift_schema(schema: &vox_types::Schema) -> String {
    use crate::render::hex_u64;

    let type_params = if schema.type_params.is_empty() {
        "[]".to_string()
    } else {
        format!(
            "[{}]",
            schema
                .type_params
                .iter()
                .map(|p| format!("\"{}\"", p.as_str()))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };

    format!(
        "Schema(id: {}, typeParams: {}, kind: {})",
        hex_u64(schema.id.0),
        type_params,
        format_swift_schema_kind(&schema.kind)
    )
}

/// Format a SchemaKind as Swift code.
fn format_swift_schema_kind(kind: &vox_types::SchemaKind) -> String {
    use vox_types::SchemaKind;

    match kind {
        SchemaKind::Struct { name, fields } => {
            let fields_str = fields
                .iter()
                .map(|f| {
                    format!(
                        "FieldSchema(name: \"{}\", typeRef: {}, required: {})",
                        f.name,
                        format_swift_type_ref(&f.type_ref),
                        f.required
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!(".struct(name: \"{}\", fields: [{}])", name, fields_str)
        }
        SchemaKind::Enum { name, variants } => {
            let variants_str = variants
                .iter()
                .map(|v| {
                    format!(
                        "VariantSchema(name: \"{}\", index: {}, payload: {})",
                        v.name,
                        v.index,
                        format_swift_variant_payload(&v.payload)
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!(".enum(name: \"{}\", variants: [{}])", name, variants_str)
        }
        SchemaKind::Tuple { elements } => {
            let elems_str = elements
                .iter()
                .map(format_swift_type_ref)
                .collect::<Vec<_>>()
                .join(", ");
            format!(".tuple(elements: [{}])", elems_str)
        }
        SchemaKind::List { element } => {
            format!(".list(element: {})", format_swift_type_ref(element))
        }
        SchemaKind::Map { key, value } => {
            format!(
                ".map(key: {}, value: {})",
                format_swift_type_ref(key),
                format_swift_type_ref(value)
            )
        }
        SchemaKind::Array { element, length } => {
            format!(
                ".array(element: {}, length: {})",
                format_swift_type_ref(element),
                length
            )
        }
        SchemaKind::Option { element } => {
            format!(".option(element: {})", format_swift_type_ref(element))
        }
        SchemaKind::Channel { direction, element } => {
            let dir = match direction {
                vox_types::ChannelDirection::Tx => ".tx",
                vox_types::ChannelDirection::Rx => ".rx",
            };
            format!(
                ".channel(direction: {}, element: {})",
                dir,
                format_swift_type_ref(element)
            )
        }
        SchemaKind::Primitive { primitive_type } => {
            format!(".primitive({})", format_swift_primitive(primitive_type))
        }
    }
}

/// Format a VariantPayload as Swift code.
fn format_swift_variant_payload(payload: &vox_types::VariantPayload) -> String {
    use vox_types::VariantPayload;

    match payload {
        VariantPayload::Unit => ".unit".to_string(),
        VariantPayload::Newtype { type_ref } => {
            format!(".newtype(typeRef: {})", format_swift_type_ref(type_ref))
        }
        VariantPayload::Tuple { types } => {
            let types_str = types
                .iter()
                .map(format_swift_type_ref)
                .collect::<Vec<_>>()
                .join(", ");
            format!(".tuple(types: [{}])", types_str)
        }
        VariantPayload::Struct { fields } => {
            let fields_str = fields
                .iter()
                .map(|f| {
                    format!(
                        "FieldSchema(name: \"{}\", typeRef: {}, required: {})",
                        f.name,
                        format_swift_type_ref(&f.type_ref),
                        f.required
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!(".struct(fields: [{}])", fields_str)
        }
    }
}

/// Format a TypeRef as Swift code.
fn format_swift_type_ref(type_ref: &TypeRef) -> String {
    use crate::render::hex_u64;

    match type_ref {
        TypeRef::Concrete { type_id, args } => {
            if args.is_empty() {
                format!(".concrete({})", hex_u64(type_id.0))
            } else {
                let args_str = args
                    .iter()
                    .map(format_swift_type_ref)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(".generic({}, args: [{}])", hex_u64(type_id.0), args_str)
            }
        }
        TypeRef::Var { name } => {
            format!(".var(name: \"{}\")", name.as_str())
        }
    }
}

/// Format a PrimitiveType as Swift code.
fn format_swift_primitive(prim: &vox_types::PrimitiveType) -> String {
    use vox_types::PrimitiveType;

    match prim {
        PrimitiveType::Bool => ".bool",
        PrimitiveType::U8 => ".u8",
        PrimitiveType::U16 => ".u16",
        PrimitiveType::U32 => ".u32",
        PrimitiveType::U64 => ".u64",
        PrimitiveType::U128 => ".u128",
        PrimitiveType::I8 => ".i8",
        PrimitiveType::I16 => ".i16",
        PrimitiveType::I32 => ".i32",
        PrimitiveType::I64 => ".i64",
        PrimitiveType::I128 => ".i128",
        PrimitiveType::F32 => ".f32",
        PrimitiveType::F64 => ".f64",
        PrimitiveType::Char => ".char",
        PrimitiveType::String => ".string",
        PrimitiveType::Unit => ".unit",
        PrimitiveType::Never => ".never",
        PrimitiveType::Bytes => ".bytes",
        PrimitiveType::Payload => ".payload",
    }
    .to_string()
}
