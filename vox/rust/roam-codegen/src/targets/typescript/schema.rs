//! TypeScript schema generation for runtime service descriptors.
//!
//! Generates the `ServiceDescriptor` constant used by the runtime to perform
//! schema-driven decode/encode for all methods. The generated code has zero
//! serialization logic — everything is driven by the descriptor at runtime.
//!
//! Each method descriptor has:
//! - `args`: a tuple schema covering all arguments (decoded once before dispatch)
//! - `result`: the full `Result<T, RoamError<E>>` enum schema for encoding responses
//!
//! The `RoamError<E>` schema always has four variants at fixed indices:
//! - 0: User(E)        — user-defined error (null fields for infallible methods)
//! - 1: UnknownMethod  — unit
//! - 2: InvalidPayload — unit
//! - 3: Cancelled      — unit

use std::collections::HashSet;

use facet_core::{Facet, Field, ScalarType, Shape};
use heck::ToLowerCamelCase;
use roam_types::{
    EnumInfo, Schema, SchemaKind, SchemaSendTracker, ServiceDescriptor, ShapeKind, StructInfo,
    TypeSchemaId, VariantKind, VariantPayload, VariantSchema, classify_shape, classify_variant,
    compute_content_hash, is_bytes, schema_child_ids,
};

/// Generate a TypeScript Schema object literal for a type.
pub fn generate_schema(shape: &'static Shape) -> String {
    let mut state = SchemaGenState::default();
    generate_schema_with_field(shape, None, &mut state)
}

#[derive(Default)]
struct SchemaGenState {
    /// When true, named structs/enums are emitted as `{ kind: 'ref', name: 'T' }`
    /// unless we're currently generating that type's registry entry (`root_name`).
    use_named_refs: bool,
    /// Name of the registry entry currently being generated, if any.
    root_name: Option<String>,
    /// Active shapes for recursion detection.
    active: HashSet<&'static Shape>,
}

fn named_shape_name(shape: &'static Shape) -> Option<&'static str> {
    match classify_shape(shape) {
        ShapeKind::Struct(StructInfo {
            name: Some(name), ..
        })
        | ShapeKind::Enum(EnumInfo {
            name: Some(name), ..
        }) => Some(name),
        _ => None,
    }
}

fn extract_initial_credit(shape: &'static Shape) -> u32 {
    shape
        .const_params
        .iter()
        .find(|cp| cp.name == "N")
        .map(|cp| cp.value as u32)
        .unwrap_or(16)
}

fn generate_schema_with_field(
    shape: &'static Shape,
    field: Option<&Field>,
    state: &mut SchemaGenState,
) -> String {
    if state.use_named_refs
        && let Some(name) = named_shape_name(shape)
        && state.root_name.as_deref() != Some(name)
    {
        return format!("{{ kind: 'ref', name: '{name}' }}");
    }

    if !state.active.insert(shape) {
        if let Some(name) = named_shape_name(shape) {
            return format!("{{ kind: 'ref', name: '{name}' }}");
        }
        panic!(
            "encountered recursive anonymous shape in TypeScript schema generation; \
             recursive shapes must be named to generate refs"
        );
    }

    let bytes_schema = if field.is_some_and(|f| f.has_builtin_attr("trailing")) {
        "{ kind: 'bytes', trailing: true }"
    } else {
        "{ kind: 'bytes' }"
    };

    // Check for bytes first (Vec<u8>)
    if is_bytes(shape) {
        state.active.remove(shape);
        return bytes_schema.into();
    }

    let rendered = match classify_shape(shape) {
        ShapeKind::Scalar(scalar) => generate_scalar_schema(scalar),
        ShapeKind::Tx { inner } => {
            format!(
                "{{ kind: 'tx', initial_credit: {}, element: {} }}",
                extract_initial_credit(shape),
                generate_schema_with_field(inner, None, state)
            )
        }
        ShapeKind::Rx { inner } => {
            format!(
                "{{ kind: 'rx', initial_credit: {}, element: {} }}",
                extract_initial_credit(shape),
                generate_schema_with_field(inner, None, state)
            )
        }
        ShapeKind::List { element } => {
            format!(
                "{{ kind: 'vec', element: {} }}",
                generate_schema_with_field(element, None, state)
            )
        }
        ShapeKind::Option { inner } => {
            format!(
                "{{ kind: 'option', inner: {} }}",
                generate_schema_with_field(inner, None, state)
            )
        }
        ShapeKind::Array { element, .. } | ShapeKind::Slice { element } => {
            format!(
                "{{ kind: 'vec', element: {} }}",
                generate_schema_with_field(element, None, state)
            )
        }
        ShapeKind::Map { key, value } => {
            format!(
                "{{ kind: 'map', key: {}, value: {} }}",
                generate_schema_with_field(key, None, state),
                generate_schema_with_field(value, None, state)
            )
        }
        ShapeKind::Set { element } => {
            format!(
                "{{ kind: 'vec', element: {} }}",
                generate_schema_with_field(element, None, state)
            )
        }
        ShapeKind::Tuple { elements } => {
            let element_schemas: Vec<_> = elements
                .iter()
                .map(|p| generate_schema_with_field(p.shape, None, state))
                .collect();
            format!(
                "{{ kind: 'tuple', elements: [{}] }}",
                element_schemas.join(", ")
            )
        }
        ShapeKind::Struct(StructInfo { fields, .. }) => {
            let field_schemas: Vec<_> = fields
                .iter()
                .map(|f| {
                    format!(
                        "'{}': {}",
                        f.name,
                        generate_schema_with_field(f.shape(), Some(f), state)
                    )
                })
                .collect();
            format!(
                "{{ kind: 'struct', fields: {{ {} }} }}",
                field_schemas.join(", ")
            )
        }
        ShapeKind::Enum(EnumInfo { variants, .. }) => {
            let variant_schemas: Vec<_> = variants
                .iter()
                .map(|variant| generate_enum_variant(variant, state))
                .collect();
            format!(
                "{{ kind: 'enum', variants: [{}] }}",
                variant_schemas.join(", ")
            )
        }
        ShapeKind::Pointer { pointee } => generate_schema_with_field(pointee, None, state),
        ShapeKind::Result { ok, err } => {
            // Represent Result as enum with Ok/Err variants
            format!(
                "{{ kind: 'enum', variants: [{{ name: 'Ok', fields: {} }}, {{ name: 'Err', fields: {} }}] }}",
                generate_schema_with_field(ok, None, state),
                generate_schema_with_field(err, None, state)
            )
        }
        ShapeKind::TupleStruct { fields } => {
            let inner: Vec<String> = fields
                .iter()
                .map(|f| generate_schema_with_field(f.shape(), Some(f), state))
                .collect();
            format!("{{ kind: 'tuple', elements: [{}] }}", inner.join(", "))
        }
        ShapeKind::Opaque => "{ kind: 'bytes', opaque: true }".into(),
    };

    state.active.remove(shape);
    rendered
}

/// Generate an EnumVariant object literal.
fn generate_enum_variant(variant: &facet_core::Variant, state: &mut SchemaGenState) -> String {
    match classify_variant(variant) {
        VariantKind::Unit => {
            format!("{{ name: '{}', fields: null }}", variant.name)
        }
        VariantKind::Newtype { inner } => {
            let field = variant.data.fields.first();
            format!(
                "{{ name: '{}', fields: {} }}",
                variant.name,
                generate_schema_with_field(inner, field, state)
            )
        }
        VariantKind::Tuple { fields } => {
            let field_schemas: Vec<_> = fields
                .iter()
                .map(|f| generate_schema_with_field(f.shape(), Some(f), state))
                .collect();
            format!(
                "{{ name: '{}', fields: [{}] }}",
                variant.name,
                field_schemas.join(", ")
            )
        }
        VariantKind::Struct { fields } => {
            let field_schemas: Vec<_> = fields
                .iter()
                .map(|f| {
                    format!(
                        "'{}': {}",
                        f.name,
                        generate_schema_with_field(f.shape(), Some(f), state)
                    )
                })
                .collect();
            format!(
                "{{ name: '{}', fields: {{ {} }} }}",
                variant.name,
                field_schemas.join(", ")
            )
        }
    }
}

/// Generate schema for scalar types.
fn generate_scalar_schema(scalar: ScalarType) -> String {
    match scalar {
        ScalarType::Bool => "{ kind: 'bool' }".into(),
        ScalarType::U8 => "{ kind: 'u8' }".into(),
        ScalarType::U16 => "{ kind: 'u16' }".into(),
        ScalarType::U32 => "{ kind: 'u32' }".into(),
        ScalarType::U64 | ScalarType::USize => "{ kind: 'u64' }".into(),
        ScalarType::I8 => "{ kind: 'i8' }".into(),
        ScalarType::I16 => "{ kind: 'i16' }".into(),
        ScalarType::I32 => "{ kind: 'i32' }".into(),
        ScalarType::I64 | ScalarType::ISize => "{ kind: 'i64' }".into(),
        ScalarType::U128 | ScalarType::I128 => {
            panic!(
                "u128/i128 types are not supported in TypeScript codegen - use smaller integer types or encode as bytes"
            )
        }
        ScalarType::F32 => "{ kind: 'f32' }".into(),
        ScalarType::F64 => "{ kind: 'f64' }".into(),
        ScalarType::Char | ScalarType::Str | ScalarType::String | ScalarType::CowStr => {
            "{ kind: 'string' }".into()
        }
        ScalarType::Unit => "{ kind: 'struct', fields: {} }".into(),
        _ => "{ kind: 'bytes' }".into(),
    }
}

/// Generate the `RoamError<E>` enum schema using Rust reflection.
///
/// Instead of hardcoding variants, uses `classify_shape` on `RoamError<Infallible>` to
/// get the exact variant list (names, indices, payloads) matching Rust's `#[repr(u8)]`.
/// Only the `User` variant's inner type is replaced with `err_schema`.
fn generate_roam_error_schema(err_schema: &str) -> String {
    use roam_types::VariantKind;

    let roam_error_shape =
        <roam_types::RoamError<std::convert::Infallible> as Facet<'static>>::SHAPE;
    let ShapeKind::Enum(EnumInfo { variants, .. }) = classify_shape(roam_error_shape) else {
        panic!("RoamError must be an enum");
    };

    let mut state = SchemaGenState::default();
    let variant_schemas: Vec<String> = variants
        .iter()
        .map(|variant| match classify_variant(variant) {
            VariantKind::Unit => {
                format!("{{ name: '{}', fields: null }}", variant.name)
            }
            VariantKind::Newtype { inner } => {
                if variant.name == "User" {
                    // Replace Infallible with the actual user error schema.
                    format!("{{ name: 'User', fields: {} }}", err_schema)
                } else {
                    let inner_schema = generate_schema_with_field(inner, None, &mut state);
                    format!("{{ name: '{}', fields: {} }}", variant.name, inner_schema)
                }
            }
            VariantKind::Struct { .. } | VariantKind::Tuple { .. } => {
                // RoamError has no struct/tuple variants; panic if Rust ever adds one.
                panic!(
                    "unexpected struct/tuple variant in RoamError: {}",
                    variant.name
                )
            }
        })
        .collect();

    format!(
        "{{ kind: 'enum', variants: [{}] }}",
        variant_schemas.join(", ")
    )
}

/// Generate the full `Result<T, RoamError<E>>` enum schema for a method.
///
/// - Ok (index 0): T
/// - Err (index 1): RoamError<E>
fn generate_result_schema(ok_schema: &str, err_schema: &str) -> String {
    let roam_error = generate_roam_error_schema(err_schema);
    format!(
        "{{ kind: 'enum', variants: [{{ name: 'Ok', fields: {ok_schema} }}, {{ name: 'Err', fields: {roam_error} }}] }}"
    )
}

/// Generate TypeScript constants for pre-computed CBOR schemas.
///
/// Produces the `{service}_send_schemas` export with a `schemas` map
/// (typeId → pre-encoded CBOR bytes) and a `methods` map (methodId → schema info).
/// The CBOR bytes are generated from Rust's own Facet shapes via `facet_cbor::to_vec`,
/// so they are guaranteed to match what Rust expects on the wire.
///
/// The response schema is ALWAYS wrapped as `Result<T, RoamError<E>>` to match
/// the actual wire encoding. For infallible methods, E = Infallible (empty enum).
pub fn generate_send_schema_table(service: &ServiceDescriptor) -> String {
    use crate::render::hex_u64;

    let mut tracker = SchemaSendTracker::new();
    let service_name_lower = service.service_name.to_lower_camel_case();

    // Global schema map: (id, cbor_bytes), in stable insertion order.
    let mut schema_bytes: Vec<(u64, Vec<u8>)> = Vec::new();
    let mut schema_ids_seen: std::collections::HashSet<u64> = std::collections::HashSet::new();

    struct MethodInfo {
        method_id: u64,
        args_dep_ids: Vec<u64>,
        args_root_id: u64,
        response_dep_ids: Vec<u64>,
        response_root_id: u64,
    }

    let mut method_infos: Vec<MethodInfo> = Vec::new();

    // Collect all schemas (extracted + constructed) with temporary IDs.
    // We'll finalize content hashes and CBOR-encode at the end.
    let mut all_schemas: Vec<Schema> = Vec::new();

    /// Extract schemas for a shape, append to all_schemas, return root TypeSchemaId.
    fn extract_into(
        tracker: &mut SchemaSendTracker,
        shape: &'static Shape,
        all_schemas: &mut Vec<Schema>,
    ) -> TypeSchemaId {
        let schemas = tracker.extract_schemas(shape);
        let root = schemas.last().map(|s| s.type_id).unwrap_or(TypeSchemaId(0));
        all_schemas.extend(schemas);
        root
    }

    // Track per-method info using TypeSchemaId (content hashes from extraction).
    struct MethodSchemaInfo {
        method_id: u64,
        args_root: TypeSchemaId,
        response_root: TypeSchemaId,
    }

    let mut method_schema_infos: Vec<MethodSchemaInfo> = Vec::new();

    for method in service.methods {
        let method_id = crate::method_id(method);

        // --- Args ---
        // Extract each arg's schemas, then wrap in a Tuple (or Unit for 0 args).
        let args_root = if method.args.is_empty() {
            extract_into(
                &mut tracker,
                <() as Facet<'static>>::SHAPE,
                &mut all_schemas,
            )
        } else {
            let arg_root_ids: Vec<TypeSchemaId> = method
                .args
                .iter()
                .map(|arg| extract_into(&mut tracker, arg.shape, &mut all_schemas))
                .collect();
            let kind = SchemaKind::Tuple {
                elements: arg_root_ids,
            };
            let type_id = compute_content_hash(&kind, &|id| id.0);
            all_schemas.push(Schema { type_id, kind });
            type_id
        };

        // --- Response ---
        // The wire encoding is ALWAYS Result<T, RoamError<E>>.
        let string_id = extract_into(
            &mut tracker,
            <String as Facet<'static>>::SHAPE,
            &mut all_schemas,
        );

        let (ok_root_id, err_root_id) = match classify_shape(method.return_shape) {
            ShapeKind::Result { ok, err } => (
                extract_into(&mut tracker, ok, &mut all_schemas),
                extract_into(&mut tracker, err, &mut all_schemas),
            ),
            _ => {
                let ok = extract_into(&mut tracker, method.return_shape, &mut all_schemas);
                let err = extract_into(
                    &mut tracker,
                    <std::convert::Infallible as Facet<'static>>::SHAPE,
                    &mut all_schemas,
                );
                (ok, err)
            }
        };

        // Construct RoamError<E> schema.
        let roam_error_kind = SchemaKind::Enum {
            name: "RoamError".to_string(),
            variants: vec![
                VariantSchema {
                    name: "User".into(),
                    index: 0,
                    payload: VariantPayload::Newtype {
                        type_id: err_root_id,
                    },
                },
                VariantSchema {
                    name: "UnknownMethod".into(),
                    index: 1,
                    payload: VariantPayload::Unit,
                },
                VariantSchema {
                    name: "InvalidPayload".into(),
                    index: 2,
                    payload: VariantPayload::Newtype { type_id: string_id },
                },
                VariantSchema {
                    name: "Cancelled".into(),
                    index: 3,
                    payload: VariantPayload::Unit,
                },
                VariantSchema {
                    name: "Indeterminate".into(),
                    index: 4,
                    payload: VariantPayload::Unit,
                },
            ],
        };
        let roam_error_id = compute_content_hash(&roam_error_kind, &|id| id.0);
        all_schemas.push(Schema {
            type_id: roam_error_id,
            kind: roam_error_kind,
        });

        // Construct Result<T, RoamError<E>> schema.
        let result_kind = SchemaKind::Enum {
            name: "Result".to_string(),
            variants: vec![
                VariantSchema {
                    name: "Ok".into(),
                    index: 0,
                    payload: VariantPayload::Newtype {
                        type_id: ok_root_id,
                    },
                },
                VariantSchema {
                    name: "Err".into(),
                    index: 1,
                    payload: VariantPayload::Newtype {
                        type_id: roam_error_id,
                    },
                },
            ],
        };
        let result_id = compute_content_hash(&result_kind, &|id| id.0);
        all_schemas.push(Schema {
            type_id: result_id,
            kind: result_kind,
        });

        method_schema_infos.push(MethodSchemaInfo {
            method_id,
            args_root: args_root,
            response_root: result_id,
        });
    }

    // Dedup and CBOR-encode.
    for schema in &all_schemas {
        let id = schema.type_id.0;
        if schema_ids_seen.insert(id) {
            let bytes = facet_cbor::to_vec(schema).expect("failed to CBOR-encode schema");
            schema_bytes.push((id, bytes));
        }
    }

    // Build the schema ID → index map for dep tracking.
    let schema_id_set: std::collections::HashSet<u64> =
        all_schemas.iter().map(|s| s.type_id.0).collect();

    // Build method infos with final content-hashed IDs.
    for info in &method_schema_infos {
        // Collect all dep IDs reachable from a root by walking the schema graph.
        let collect_deps = |root: TypeSchemaId| -> Vec<u64> {
            let mut deps = Vec::new();
            let mut visited = std::collections::HashSet::new();
            let mut queue = vec![root];
            while let Some(id) = queue.pop() {
                if !visited.insert(id) {
                    continue;
                }
                if !schema_id_set.contains(&id.0) {
                    continue;
                }
                deps.push(id.0);
                // Find the schema and add its children.
                if let Some(schema) = all_schemas.iter().find(|s| s.type_id == id) {
                    for child in schema_child_ids(&schema.kind) {
                        queue.push(child);
                    }
                }
            }
            deps
        };

        let args_dep_ids = collect_deps(info.args_root);
        let response_dep_ids = collect_deps(info.response_root);

        method_infos.push(MethodInfo {
            method_id: info.method_id,
            args_dep_ids,
            args_root_id: info.args_root.0,
            response_dep_ids,
            response_root_id: info.response_root.0,
        });
    }

    // Generate TypeScript output.
    let mut out = String::new();

    out.push_str(
        "// Pre-computed CBOR schema bytes for wire schema exchange (TypeScript \u{2192} Rust)\n",
    );
    out.push_str("// Generated from Rust Facet shapes \u{2014} do not modify.\n");
    out.push_str(&format!(
        "export const {service_name_lower}_send_schemas: import(\"@bearcove/roam-core\").ServiceSendSchemas = {{\n"
    ));
    out.push_str("  schemas: new Map<bigint, Uint8Array>([\n");
    for (id, bytes) in &schema_bytes {
        let hex_bytes: Vec<String> = bytes.iter().map(|b| format!("0x{b:02x}")).collect();
        let id_hex = hex_u64(*id);
        out.push_str(&format!(
            "    [{id_hex}n, new Uint8Array([{}])],\n",
            hex_bytes.join(", ")
        ));
    }
    out.push_str("  ]),\n");
    out.push_str(
        "  methods: new Map<bigint, import(\"@bearcove/roam-core\").MethodSendSchemas>([\n",
    );
    for info in &method_infos {
        let id_hex = hex_u64(info.method_id);
        let args_dep_ids_str: Vec<String> = info
            .args_dep_ids
            .iter()
            .map(|id| format!("0x{:016x}n", id))
            .collect();
        let response_dep_ids_str: Vec<String> = info
            .response_dep_ids
            .iter()
            .map(|id| format!("0x{:016x}n", id))
            .collect();
        let args_root_hex = hex_u64(info.args_root_id);
        let response_root_hex = hex_u64(info.response_root_id);
        out.push_str(&format!(
            "    [{}n, {{ argsDepIds: [{}], argsRootId: {}n, responseDepIds: [{}], responseRootId: {}n }}],\n",
            id_hex,
            args_dep_ids_str.join(", "),
            args_root_hex,
            response_dep_ids_str.join(", "),
            response_root_hex,
        ));
    }
    out.push_str("  ]),\n");
    out.push_str("};\n\n");
    out
}

/// Generate the service descriptor constant.
///
/// The descriptor contains all method descriptors with their args tuple schemas
/// and full result schemas. The runtime uses this for schema-driven dispatch.
pub fn generate_descriptor(service: &ServiceDescriptor) -> String {
    use super::types::collect_named_types;
    use crate::render::hex_u64;

    let mut out = String::new();
    let service_name_lower = service.service_name.to_lower_camel_case();
    let named_types = collect_named_types(service);

    out.push_str("// Named schema registry (for recursive / shared named types)\n");
    out.push_str(&format!(
        "const {service_name_lower}_schema_registry: SchemaRegistry = new Map<string, Schema>([\n"
    ));
    for (name, shape) in &named_types {
        let mut state = SchemaGenState {
            use_named_refs: true,
            root_name: Some(name.clone()),
            active: HashSet::new(),
        };
        let schema = generate_schema_with_field(shape, None, &mut state);
        out.push_str(&format!("  [\"{name}\", {schema}],\n"));
    }
    out.push_str("]);\n\n");

    out.push_str("// Service descriptor for runtime schema-driven dispatch\n");
    out.push_str(&format!(
        "export const {service_name_lower}_descriptor: ServiceDescriptor = {{\n"
    ));
    out.push_str(&format!("  service_name: '{}',\n", service.service_name));
    out.push_str(&format!(
        "  schema_registry: {service_name_lower}_schema_registry,\n"
    ));
    out.push_str(&format!(
        "  send_schemas: {service_name_lower}_send_schemas,\n"
    ));
    out.push_str("  methods: [\n");

    for method in service.methods {
        let method_name = method.method_name.to_lower_camel_case();
        let id = crate::method_id(method);

        // Args as a tuple schema
        let mut args_state = SchemaGenState {
            use_named_refs: true,
            root_name: None,
            active: HashSet::new(),
        };
        let arg_schemas: Vec<_> = method
            .args
            .iter()
            .map(|a| generate_schema_with_field(a.shape, None, &mut args_state))
            .collect();
        let args_schema = format!(
            "{{ kind: 'tuple', elements: [{}] }}",
            arg_schemas.join(", ")
        );

        // Result schema: Result<T, RoamError<E>>
        let result_schema = match classify_shape(method.return_shape) {
            ShapeKind::Result { ok, err } => {
                let mut ok_state = SchemaGenState {
                    use_named_refs: true,
                    root_name: None,
                    active: HashSet::new(),
                };
                let mut err_state = SchemaGenState {
                    use_named_refs: true,
                    root_name: None,
                    active: HashSet::new(),
                };
                let ok_schema = generate_schema_with_field(ok, None, &mut ok_state);
                let err_schema = generate_schema_with_field(err, None, &mut err_state);
                generate_result_schema(&ok_schema, &err_schema)
            }
            _ => {
                // Infallible: ok = return type, err = null (User variant never sent)
                let mut ok_state = SchemaGenState {
                    use_named_refs: true,
                    root_name: None,
                    active: HashSet::new(),
                };
                let ok_schema =
                    generate_schema_with_field(method.return_shape, None, &mut ok_state);
                generate_result_schema(&ok_schema, "null")
            }
        };

        out.push_str("    {\n");
        out.push_str(&format!("      name: '{method_name}',\n"));
        out.push_str(&format!("      id: {}n,\n", hex_u64(id)));
        out.push_str(&format!(
            "      retry: {{ persist: {}, idem: {} }},\n",
            method.retry.persist, method.retry.idem
        ));
        out.push_str(&format!("      args: {args_schema},\n"));
        out.push_str(&format!("      result: {result_schema},\n"));
        out.push_str("    },\n");
    }

    out.push_str("  ],\n");
    out.push_str("};\n\n");
    out
}
