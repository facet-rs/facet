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

use facet_core::{Field, ScalarType, Shape};
use heck::ToLowerCamelCase;
use roam_types::{
    EnumInfo, ServiceDescriptor, ShapeKind, StructInfo, VariantKind, classify_shape,
    classify_variant, is_bytes,
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
    /// Active shape pointers for recursion detection.
    active: HashSet<usize>,
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

    let shape_ptr = shape as *const Shape as usize;
    if !state.active.insert(shape_ptr) {
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
        state.active.remove(&shape_ptr);
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
        ShapeKind::Opaque => bytes_schema.into(),
    };

    state.active.remove(&shape_ptr);
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

/// Generate the `RoamError<E>` enum schema.
///
/// Always has four variants at fixed indices:
/// - 0: User(E)        — user-defined error (null for infallible)
/// - 1: UnknownMethod  — unit
/// - 2: InvalidPayload — unit
/// - 3: Cancelled      — unit
fn generate_roam_error_schema(err_schema: &str) -> String {
    format!(
        "{{ kind: 'enum', variants: [\
          {{ name: 'User', fields: {err_schema} }}, \
          {{ name: 'UnknownMethod', fields: null }}, \
          {{ name: 'InvalidPayload', fields: null }}, \
          {{ name: 'Cancelled', fields: null }}\
        ] }}"
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
        out.push_str(&format!("      args: {args_schema},\n"));
        out.push_str(&format!("      result: {result_schema},\n"));
        out.push_str("    },\n");
    }

    out.push_str("  ],\n");
    out.push_str("};\n\n");
    out
}
