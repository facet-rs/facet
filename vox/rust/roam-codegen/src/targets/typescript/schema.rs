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
    is_bytes,
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

    // Helper: add a schema to the map if not already seen.
    let mut add_schema = |id: u64, bytes: Vec<u8>| {
        if schema_ids_seen.insert(id) {
            schema_bytes.push((id, bytes));
        }
    };

    // Helper: add dep if not already present.
    fn add_dep(deps: &mut Vec<u64>, id: u64) {
        if !deps.contains(&id) {
            deps.push(id);
        }
    }

    // Helper: extract schemas for a shape, add to map, return root id.
    let mut extract_and_add = |tracker: &mut SchemaSendTracker,
                               shape: &'static Shape,
                               deps: &mut Vec<u64>,
                               schema_bytes_local: &mut Vec<(u64, Vec<u8>)>,
                               schema_ids_seen_local: &mut std::collections::HashSet<u64>|
     -> TypeSchemaId {
        let schemas = tracker.extract_schemas(shape);
        let root = schemas.last().map(|s| s.type_id).unwrap_or(TypeSchemaId(0));
        for schema in &schemas {
            let id = schema.type_id.0;
            if schema_ids_seen_local.insert(id) {
                let bytes = facet_cbor::to_vec(schema).expect("failed to CBOR-encode schema");
                schema_bytes_local.push((id, bytes));
            }
            add_dep(deps, id);
        }
        root
    };

    struct MethodInfo {
        method_id: u64,
        args_dep_ids: Vec<u64>,
        args_root_id: u64,
        response_dep_ids: Vec<u64>,
        response_root_id: u64,
    }

    let mut method_infos: Vec<MethodInfo> = Vec::new();

    for method in service.methods {
        let method_id = crate::method_id(method);

        // --- Args ---
        let mut args_dep_ids: Vec<u64> = Vec::new();
        let mut arg_root_ids: Vec<TypeSchemaId> = Vec::new();

        for arg in method.args {
            let schemas = tracker.extract_schemas(arg.shape);
            if let Some(root) = schemas.last() {
                arg_root_ids.push(root.type_id);
            }
            for schema in &schemas {
                let id = schema.type_id.0;
                if schema_ids_seen.insert(id) {
                    let bytes = facet_cbor::to_vec(schema).expect("failed to CBOR-encode schema");
                    schema_bytes.push((id, bytes));
                }
                if !args_dep_ids.contains(&id) {
                    args_dep_ids.push(id);
                }
            }
        }

        // Always create an args schema and method binding, even for 0-arg methods.
        // Rust requires a method binding for args on every call so it can build a
        // translation plan. For 0-arg methods, use ()::SHAPE which Rust extracts as
        // Primitive { Unit } — NOT an empty Tuple, which would be a type mismatch.
        let args_root_id = if method.args.is_empty() {
            // Extract () schema — Rust sees 0-arg methods as () args type
            let unit_schemas = tracker.extract_schemas(<() as Facet<'static>>::SHAPE);
            let root = unit_schemas
                .last()
                .map(|s| s.type_id)
                .unwrap_or(TypeSchemaId(0));
            for schema in &unit_schemas {
                let id = schema.type_id.0;
                if schema_ids_seen.insert(id) {
                    let bytes =
                        facet_cbor::to_vec(schema).expect("failed to CBOR-encode unit schema");
                    schema_bytes.push((id, bytes));
                }
                if !args_dep_ids.contains(&id) {
                    args_dep_ids.push(id);
                }
            }
            root.0
        } else {
            let tuple_id = tracker.allocate_anonymous_id();
            let tuple_schema = Schema {
                type_id: tuple_id,
                kind: SchemaKind::Tuple {
                    elements: arg_root_ids,
                },
            };
            let id = tuple_id.0;
            let bytes =
                facet_cbor::to_vec(&tuple_schema).expect("failed to CBOR-encode tuple schema");
            schema_bytes.push((id, bytes));
            schema_ids_seen.insert(id);
            args_dep_ids.push(id);
            id
        };

        // --- Response ---
        // The wire encoding is ALWAYS Result<T, RoamError<E>>.
        // method.return_shape is T for infallible methods, Result<T,E> for fallible.
        // We need to construct the full Result<T, RoamError<E>> schema synthetically.
        let mut response_dep_ids: Vec<u64> = Vec::new();

        // Ensure String schema exists (needed for InvalidPayload variant of RoamError).
        let string_schemas = tracker.extract_schemas(<String as Facet<'static>>::SHAPE);
        let string_id = string_schemas
            .last()
            .map(|s| s.type_id)
            .unwrap_or(TypeSchemaId(0));
        for schema in &string_schemas {
            let id = schema.type_id.0;
            if schema_ids_seen.insert(id) {
                let bytes = facet_cbor::to_vec(schema).expect("failed to CBOR-encode schema");
                schema_bytes.push((id, bytes));
            }
            add_dep(&mut response_dep_ids, id);
        }

        // Determine ok_root_id and err_root_id.
        let (ok_root_id, err_root_id) = match classify_shape(method.return_shape) {
            ShapeKind::Result { ok, err } => {
                // Fallible method: extract ok type and user error type.
                let ok_schemas = tracker.extract_schemas(ok);
                let ok_root = ok_schemas
                    .last()
                    .map(|s| s.type_id)
                    .unwrap_or(TypeSchemaId(0));
                for schema in &ok_schemas {
                    let id = schema.type_id.0;
                    if schema_ids_seen.insert(id) {
                        let bytes =
                            facet_cbor::to_vec(schema).expect("failed to CBOR-encode schema");
                        schema_bytes.push((id, bytes));
                    }
                    add_dep(&mut response_dep_ids, id);
                }

                let err_schemas = tracker.extract_schemas(err);
                let err_root = err_schemas
                    .last()
                    .map(|s| s.type_id)
                    .unwrap_or(TypeSchemaId(0));
                for schema in &err_schemas {
                    let id = schema.type_id.0;
                    if schema_ids_seen.insert(id) {
                        let bytes =
                            facet_cbor::to_vec(schema).expect("failed to CBOR-encode schema");
                        schema_bytes.push((id, bytes));
                    }
                    add_dep(&mut response_dep_ids, id);
                }

                (ok_root, err_root)
            }
            _ => {
                // Infallible method: ok = return_shape, err = Infallible.
                let ok_schemas = tracker.extract_schemas(method.return_shape);
                let ok_root = ok_schemas
                    .last()
                    .map(|s| s.type_id)
                    .unwrap_or(TypeSchemaId(0));
                for schema in &ok_schemas {
                    let id = schema.type_id.0;
                    if schema_ids_seen.insert(id) {
                        let bytes =
                            facet_cbor::to_vec(schema).expect("failed to CBOR-encode schema");
                        schema_bytes.push((id, bytes));
                    }
                    add_dep(&mut response_dep_ids, id);
                }

                // Use actual Infallible::SHAPE so facet's extraction logic applies
                // (Infallible may be transparent to unit, not an empty enum).
                let infallible_schemas =
                    tracker.extract_schemas(<std::convert::Infallible as Facet<'static>>::SHAPE);
                let infallible_id = infallible_schemas
                    .last()
                    .map(|s| s.type_id)
                    .unwrap_or(TypeSchemaId(0));
                for schema in &infallible_schemas {
                    let id = schema.type_id.0;
                    if schema_ids_seen.insert(id) {
                        let bytes =
                            facet_cbor::to_vec(schema).expect("failed to CBOR-encode schema");
                        schema_bytes.push((id, bytes));
                    }
                    add_dep(&mut response_dep_ids, id);
                }

                (ok_root, infallible_id)
            }
        };

        // Construct RoamError<E> schema with 5 fixed variants.
        let roam_error_id = tracker.allocate_anonymous_id();
        let roam_error_schema = Schema {
            type_id: roam_error_id,
            kind: SchemaKind::Enum {
                name: "RoamError".to_string(),
                variants: vec![
                    VariantSchema {
                        name: "User".to_string(),
                        index: 0,
                        payload: VariantPayload::Newtype {
                            type_id: err_root_id,
                        },
                    },
                    VariantSchema {
                        name: "UnknownMethod".to_string(),
                        index: 1,
                        payload: VariantPayload::Unit,
                    },
                    VariantSchema {
                        name: "InvalidPayload".to_string(),
                        index: 2,
                        payload: VariantPayload::Newtype { type_id: string_id },
                    },
                    VariantSchema {
                        name: "Cancelled".to_string(),
                        index: 3,
                        payload: VariantPayload::Unit,
                    },
                    VariantSchema {
                        name: "Indeterminate".to_string(),
                        index: 4,
                        payload: VariantPayload::Unit,
                    },
                ],
            },
        };
        {
            let id = roam_error_id.0;
            let bytes = facet_cbor::to_vec(&roam_error_schema)
                .expect("failed to CBOR-encode RoamError schema");
            if schema_ids_seen.insert(id) {
                schema_bytes.push((id, bytes));
            }
            add_dep(&mut response_dep_ids, id);
        }

        // Construct Result<ok, RoamError<E>> schema.
        let result_id = tracker.allocate_anonymous_id();
        let result_schema = Schema {
            type_id: result_id,
            kind: SchemaKind::Enum {
                name: "Result".to_string(),
                variants: vec![
                    VariantSchema {
                        name: "Ok".to_string(),
                        index: 0,
                        payload: VariantPayload::Newtype {
                            type_id: ok_root_id,
                        },
                    },
                    VariantSchema {
                        name: "Err".to_string(),
                        index: 1,
                        payload: VariantPayload::Newtype {
                            type_id: roam_error_id,
                        },
                    },
                ],
            },
        };
        {
            let id = result_id.0;
            let bytes =
                facet_cbor::to_vec(&result_schema).expect("failed to CBOR-encode result schema");
            if schema_ids_seen.insert(id) {
                schema_bytes.push((id, bytes));
            }
            add_dep(&mut response_dep_ids, id);
        }

        let response_root_id = result_id.0;

        method_infos.push(MethodInfo {
            method_id,
            args_dep_ids,
            args_root_id,
            response_dep_ids,
            response_root_id,
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
