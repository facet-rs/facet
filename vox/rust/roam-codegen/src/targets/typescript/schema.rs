//! TypeScript generation for canonical service schema tables and descriptors.
//!
//! The generated descriptor carries method metadata plus a precomputed
//! canonical schema table for method args and responses. Encoding and decoding
//! are driven entirely by that canonical schema data at runtime.

use facet_core::{Facet, Shape};
use heck::ToLowerCamelCase;
use roam_types::{
    RoamError, Schema, SchemaHash, SchemaKind, ServiceDescriptor, ShapeKind, TypeRef,
    VariantPayload, VariantSchema, classify_shape,
};

/// Generate TypeScript constants for canonical service schemas.
///
/// Produces the `{service}_send_schemas` export with a `schemas` map
/// (typeId → Schema object) and a `methods` map (methodId → root refs).
///
/// The response schema is ALWAYS wrapped as `Result<T, RoamError<E>>` to match
/// the actual wire encoding. For infallible methods, E = Infallible, which
/// extracts to the canonical `never` primitive.
pub fn generate_send_schema_table(service: &ServiceDescriptor) -> String {
    use crate::render::hex_u64;

    let service_name_lower = service.service_name.to_lower_camel_case();

    let mut schema_ids_seen: std::collections::HashSet<u64> = std::collections::HashSet::new();

    // Collect all schemas (extracted + constructed) with temporary IDs.
    // We'll finalize content hashes and CBOR-encode at the end.
    let mut all_schemas: Vec<Schema> = Vec::new();

    /// Extract schemas for a shape, append to all_schemas, return root TypeRef.
    fn extract_into(shape: &'static Shape, all_schemas: &mut Vec<Schema>) -> TypeRef<SchemaHash> {
        let extracted = roam_types::extract_schemas(shape).expect("schema extraction");
        let root = extracted.root.clone();
        all_schemas.extend(extracted.schemas);
        root
    }

    fn type_id_of(type_ref: &TypeRef<SchemaHash>) -> SchemaHash {
        match type_ref {
            TypeRef::Concrete { type_id, .. } => *type_id,
            TypeRef::Var { .. } => panic!("schema root cannot be a type variable"),
        }
    }

    // Track per-method info with full TypeRefs (preserving generic args).
    struct MethodSchemaInfo {
        method_id: u64,
        args_root: TypeRef<SchemaHash>,
        response_root: TypeRef<SchemaHash>,
    }

    let mut method_schema_infos: Vec<MethodSchemaInfo> = Vec::new();

    let result_template_root = extract_into(
        <Result<bool, u32> as Facet<'static>>::SHAPE,
        &mut all_schemas,
    );
    let result_type_id = type_id_of(&result_template_root);
    let roam_error_template_root = extract_into(
        <RoamError<std::convert::Infallible> as Facet<'static>>::SHAPE,
        &mut all_schemas,
    );
    let roam_error_type_id = type_id_of(&roam_error_template_root);

    for method in service.methods {
        let method_id = crate::method_id(method);

        // --- Args ---
        // Use the macro-provided canonical args tuple shape directly.
        let args_root = extract_into(method.args_shape, &mut all_schemas);

        // --- Response ---
        // The wire encoding is ALWAYS Result<T, RoamError<E>>.
        let (ok_ref, err_ref) = match classify_shape(method.return_shape) {
            ShapeKind::Result { ok, err } => (
                extract_into(ok, &mut all_schemas),
                extract_into(err, &mut all_schemas),
            ),
            _ => {
                let ok = extract_into(method.return_shape, &mut all_schemas);
                let err = extract_into(
                    <std::convert::Infallible as Facet<'static>>::SHAPE,
                    &mut all_schemas,
                );
                (ok, err)
            }
        };

        let roam_error_ref = TypeRef::generic(roam_error_type_id, vec![err_ref]);

        method_schema_infos.push(MethodSchemaInfo {
            method_id,
            args_root,
            response_root: TypeRef::generic(result_type_id, vec![ok_ref, roam_error_ref]),
        });
    }

    // Dedup schemas by ID.
    let mut deduped_schemas: Vec<&Schema> = Vec::new();
    for schema in &all_schemas {
        let id = schema.id.0;
        if schema_ids_seen.insert(id) {
            deduped_schemas.push(schema);
        }
    }

    // Generate TypeScript output — Schema objects as typed literals, not CBOR bytes.
    let mut out = String::new();

    out.push_str("// Schema objects for wire schema exchange (TypeScript \u{2192} Rust)\n");
    out.push_str("// Generated from Rust Facet shapes \u{2014} do not modify.\n");
    out.push_str(&format!(
        "export const {service_name_lower}_send_schemas: import(\"@bearcove/roam-core\").ServiceSendSchemas = {{\n"
    ));

    // schemas: Map<bigint, Schema>
    out.push_str("  schemas: new Map<bigint, import(\"@bearcove/roam-postcard\").Schema>([\n");
    for schema in &deduped_schemas {
        let id_hex = hex_u64(schema.id.0);
        let schema_ts = render_schema(schema);
        out.push_str(&format!("    [{id_hex}n, {schema_ts}],\n"));
    }
    out.push_str("  ]),\n");

    // methods: Map<bigint, MethodSendSchemas>
    out.push_str(
        "  methods: new Map<bigint, import(\"@bearcove/roam-core\").MethodSendSchemas>([\n",
    );
    for info in &method_schema_infos {
        let id_hex = hex_u64(info.method_id);
        let args_root_ref_ts = render_type_ref(&info.args_root);
        let response_root_ref_ts = render_type_ref(&info.response_root);
        out.push_str(&format!(
            "    [{}n, {{ argsRootRef: {}, responseRootRef: {} }}],\n",
            id_hex, args_root_ref_ts, response_root_ref_ts,
        ));
    }
    out.push_str("  ]),\n");
    out.push_str("};\n\n");
    out
}

/// Generate the service descriptor constant.
///
/// The descriptor carries method metadata plus the canonical service schema
/// table. Legacy TS-only args/result schemas are no longer emitted here.
pub fn generate_descriptor(service: &ServiceDescriptor) -> String {
    use crate::render::hex_u64;

    let mut out = String::new();
    let service_name_lower = service.service_name.to_lower_camel_case();
    out.push_str("// Service descriptor for runtime dispatch metadata\n");
    out.push_str(&format!(
        "export const {service_name_lower}_descriptor: ServiceDescriptor = {{\n"
    ));
    out.push_str(&format!("  service_name: '{}',\n", service.service_name));
    out.push_str(&format!(
        "  send_schemas: {service_name_lower}_send_schemas,\n"
    ));
    out.push_str("  methods: [\n");

    for method in service.methods {
        let method_name = method.method_name.to_lower_camel_case();
        let id = crate::method_id(method);

        out.push_str("    {\n");
        out.push_str(&format!("      name: '{method_name}',\n"));
        out.push_str(&format!("      id: {}n,\n", hex_u64(id)));
        out.push_str(&format!(
            "      retry: {{ persist: {}, idem: {} }},\n",
            method.retry.persist, method.retry.idem
        ));
        out.push_str("    },\n");
    }

    out.push_str("  ],\n");
    out.push_str("};\n\n");
    out
}

// ============================================================================
// Rendering helpers: Rust Schema → TypeScript object literal strings
// ============================================================================

use roam_types::{ChannelDirection, FieldSchema, PrimitiveType};

pub(crate) fn render_schema(schema: &Schema) -> String {
    use crate::render::hex_u64;

    let id_hex = hex_u64(schema.id.0);
    let type_params = if schema.type_params.is_empty() {
        "[]".to_string()
    } else {
        let params: Vec<String> = schema
            .type_params
            .iter()
            .map(|p| format!("'{}'", p.as_str()))
            .collect();
        format!("[{}]", params.join(", "))
    };
    let kind = render_schema_kind(&schema.kind);
    format!("{{ id: {id_hex}n, type_params: {type_params}, kind: {kind} }}")
}

fn render_schema_kind(kind: &SchemaKind) -> String {
    match kind {
        SchemaKind::Struct { name, fields } => {
            let fields_ts: Vec<String> = fields.iter().map(render_field_schema).collect();
            format!(
                "{{ tag: 'struct', name: '{}', fields: [{}] }}",
                name,
                fields_ts.join(", ")
            )
        }
        SchemaKind::Enum { name, variants } => {
            let variants_ts: Vec<String> = variants.iter().map(render_variant_schema).collect();
            format!(
                "{{ tag: 'enum', name: '{}', variants: [{}] }}",
                name,
                variants_ts.join(", ")
            )
        }
        SchemaKind::Tuple { elements } => {
            let elems: Vec<String> = elements.iter().map(render_type_ref).collect();
            format!("{{ tag: 'tuple', elements: [{}] }}", elems.join(", "))
        }
        SchemaKind::List { element } => {
            format!("{{ tag: 'list', element: {} }}", render_type_ref(element))
        }
        SchemaKind::Map { key, value } => {
            format!(
                "{{ tag: 'map', key: {}, value: {} }}",
                render_type_ref(key),
                render_type_ref(value)
            )
        }
        SchemaKind::Array { element, length } => {
            format!(
                "{{ tag: 'array', element: {}, length: {} }}",
                render_type_ref(element),
                length
            )
        }
        SchemaKind::Option { element } => {
            format!("{{ tag: 'option', element: {} }}", render_type_ref(element))
        }
        SchemaKind::Channel { direction, element } => {
            let dir = match direction {
                ChannelDirection::Tx => "tx",
                ChannelDirection::Rx => "rx",
            };
            format!(
                "{{ tag: 'channel', direction: '{}', element: {} }}",
                dir,
                render_type_ref(element)
            )
        }
        SchemaKind::Primitive { primitive_type } => {
            format!(
                "{{ tag: 'primitive', primitive_type: '{}' }}",
                render_primitive_type(primitive_type)
            )
        }
    }
}

pub(crate) fn render_type_ref(type_ref: &TypeRef) -> String {
    use crate::render::hex_u64;

    match type_ref {
        TypeRef::Concrete { type_id, args } => {
            let id_hex = hex_u64(type_id.0);
            let args_ts: Vec<String> = args.iter().map(render_type_ref).collect();
            format!(
                "{{ tag: 'concrete', type_id: {id_hex}n, args: [{}] }}",
                args_ts.join(", ")
            )
        }
        TypeRef::Var { name } => {
            format!("{{ tag: 'var', name: '{}' }}", name.as_str())
        }
    }
}

fn render_field_schema(field: &FieldSchema) -> String {
    format!(
        "{{ name: '{}', type_ref: {}, required: {} }}",
        field.name,
        render_type_ref(&field.type_ref),
        field.required
    )
}

fn render_variant_schema(variant: &VariantSchema) -> String {
    format!(
        "{{ name: '{}', index: {}, payload: {} }}",
        variant.name,
        variant.index,
        render_variant_payload(&variant.payload)
    )
}

fn render_variant_payload(payload: &VariantPayload) -> String {
    match payload {
        VariantPayload::Unit => "{ tag: 'unit' }".to_string(),
        VariantPayload::Newtype { type_ref } => {
            format!(
                "{{ tag: 'newtype', type_ref: {} }}",
                render_type_ref(type_ref)
            )
        }
        VariantPayload::Tuple { types } => {
            let types_ts: Vec<String> = types.iter().map(render_type_ref).collect();
            format!("{{ tag: 'tuple', types: [{}] }}", types_ts.join(", "))
        }
        VariantPayload::Struct { fields } => {
            let fields_ts: Vec<String> = fields.iter().map(render_field_schema).collect();
            format!("{{ tag: 'struct', fields: [{}] }}", fields_ts.join(", "))
        }
    }
}

fn render_primitive_type(pt: &PrimitiveType) -> &'static str {
    match pt {
        PrimitiveType::Bool => "bool",
        PrimitiveType::U8 => "u8",
        PrimitiveType::U16 => "u16",
        PrimitiveType::U32 => "u32",
        PrimitiveType::U64 => "u64",
        PrimitiveType::U128 => "u128",
        PrimitiveType::I8 => "i8",
        PrimitiveType::I16 => "i16",
        PrimitiveType::I32 => "i32",
        PrimitiveType::I64 => "i64",
        PrimitiveType::I128 => "i128",
        PrimitiveType::F32 => "f32",
        PrimitiveType::F64 => "f64",
        PrimitiveType::Char => "char",
        PrimitiveType::String => "string",
        PrimitiveType::Unit => "unit",
        PrimitiveType::Never => "never",
        PrimitiveType::Bytes => "bytes",
        PrimitiveType::Payload => "payload",
    }
}
