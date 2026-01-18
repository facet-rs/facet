//! TypeScript schema generation for runtime channel binding.
//!
//! Generates runtime schema information that allows the TypeScript runtime
//! to discover and bind streaming channels (Tx/Rx) in method arguments.
//!
//! The generated schemas use the new EnumVariant[] format:
//! ```typescript
//! { kind: 'enum', variants: [
//!   { name: 'Circle', fields: [{ kind: 'f64' }] },
//!   { name: 'Point', fields: null },
//! ] }
//! ```

use facet_core::{ScalarType, Shape};
use heck::ToLowerCamelCase;
use roam_schema::{
    EnumInfo, ServiceDetail, ShapeKind, StructInfo, VariantKind, classify_shape, classify_variant,
    is_bytes, is_rx, is_tx,
};

/// Generate a TypeScript Schema object literal for a type.
/// Used by the runtime binder to find and bind Tx/Rx channels.
pub fn generate_schema(shape: &'static Shape) -> String {
    // Check for bytes first (Vec<u8>)
    if is_bytes(shape) {
        return "{ kind: 'bytes' }".into();
    }

    match classify_shape(shape) {
        ShapeKind::Scalar(scalar) => generate_scalar_schema(scalar),
        ShapeKind::Tx { inner } => {
            format!("{{ kind: 'tx', element: {} }}", generate_schema(inner))
        }
        ShapeKind::Rx { inner } => {
            format!("{{ kind: 'rx', element: {} }}", generate_schema(inner))
        }
        ShapeKind::List { element } => {
            format!("{{ kind: 'vec', element: {} }}", generate_schema(element))
        }
        ShapeKind::Option { inner } => {
            format!("{{ kind: 'option', inner: {} }}", generate_schema(inner))
        }
        ShapeKind::Array { element, .. } | ShapeKind::Slice { element } => {
            format!("{{ kind: 'vec', element: {} }}", generate_schema(element))
        }
        ShapeKind::Map { key, value } => {
            format!(
                "{{ kind: 'map', key: {}, value: {} }}",
                generate_schema(key),
                generate_schema(value)
            )
        }
        ShapeKind::Set { element } => {
            format!("{{ kind: 'vec', element: {} }}", generate_schema(element))
        }
        ShapeKind::Tuple { elements } => {
            // Generate as TupleSchema
            let element_schemas: Vec<_> =
                elements.iter().map(|p| generate_schema(p.shape)).collect();
            format!(
                "{{ kind: 'tuple', elements: [{}] }}",
                element_schemas.join(", ")
            )
        }
        ShapeKind::Struct(StructInfo { fields, .. }) => {
            let field_schemas: Vec<_> = fields
                .iter()
                .map(|f| format!("'{}': {}", f.name, generate_schema(f.shape())))
                .collect();
            format!(
                "{{ kind: 'struct', fields: {{ {} }} }}",
                field_schemas.join(", ")
            )
        }
        ShapeKind::Enum(EnumInfo { variants, .. }) => {
            // Generate new EnumSchema format with EnumVariant[]
            let variant_schemas: Vec<_> = variants.iter().map(generate_enum_variant).collect();
            format!(
                "{{ kind: 'enum', variants: [{}] }}",
                variant_schemas.join(", ")
            )
        }
        ShapeKind::Pointer { pointee } => generate_schema(pointee),
        ShapeKind::Result { ok, err } => {
            // Represent Result as enum with Ok/Err variants using new format
            format!(
                "{{ kind: 'enum', variants: [{{ name: 'Ok', fields: {} }}, {{ name: 'Err', fields: {} }}] }}",
                generate_schema(ok),
                generate_schema(err)
            )
        }
        ShapeKind::TupleStruct { fields } => {
            let inner: Vec<String> = fields.iter().map(|f| generate_schema(f.shape())).collect();
            format!("{{ kind: 'tuple', elements: [{}] }}", inner.join(", "))
        }
        ShapeKind::Opaque => "{ kind: 'bytes' }".into(),
    }
}

/// Generate an EnumVariant object literal.
fn generate_enum_variant(variant: &facet_core::Variant) -> String {
    match classify_variant(variant) {
        VariantKind::Unit => {
            format!("{{ name: '{}', fields: null }}", variant.name)
        }
        VariantKind::Newtype { inner } => {
            // Newtype variant: fields is a single Schema
            format!(
                "{{ name: '{}', fields: {} }}",
                variant.name,
                generate_schema(inner)
            )
        }
        VariantKind::Tuple { fields } => {
            // Tuple variant: fields is Schema[]
            let field_schemas: Vec<_> = fields.iter().map(|f| generate_schema(f.shape())).collect();
            format!(
                "{{ name: '{}', fields: [{}] }}",
                variant.name,
                field_schemas.join(", ")
            )
        }
        VariantKind::Struct { fields } => {
            // Struct variant: fields is Record<string, Schema>
            let field_schemas: Vec<_> = fields
                .iter()
                .map(|f| format!("'{}': {}", f.name, generate_schema(f.shape())))
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
        ScalarType::U64 | ScalarType::USize | ScalarType::U128 => "{ kind: 'u64' }".into(),
        ScalarType::I8 => "{ kind: 'i8' }".into(),
        ScalarType::I16 => "{ kind: 'i16' }".into(),
        ScalarType::I32 => "{ kind: 'i32' }".into(),
        ScalarType::I64 | ScalarType::ISize | ScalarType::I128 => "{ kind: 'i64' }".into(),
        ScalarType::F32 => "{ kind: 'f32' }".into(),
        ScalarType::F64 => "{ kind: 'f64' }".into(),
        ScalarType::Char | ScalarType::Str | ScalarType::String | ScalarType::CowStr => {
            "{ kind: 'string' }".into()
        }
        ScalarType::Unit => "{ kind: 'struct', fields: {} }".into(),
        _ => "{ kind: 'bytes' }".into(),
    }
}

/// Generate method schemas for runtime channel binding and encoding/decoding.
pub fn generate_method_schemas(service: &ServiceDetail) -> String {
    let mut out = String::new();
    let service_name_lower = service.name.to_lower_camel_case();

    out.push_str("// Method schemas for runtime encoding/decoding and channel binding\n");
    out.push_str(&format!(
        "export const {service_name_lower}_schemas: Record<string, MethodSchema> = {{\n"
    ));

    for method in &service.methods {
        let method_name = method.method_name.to_lower_camel_case();
        let arg_schemas: Vec<_> = method.args.iter().map(|a| generate_schema(a.ty)).collect();
        let return_schema = generate_schema(method.return_type);

        out.push_str(&format!(
            "  {method_name}: {{ args: [{}], returns: {} }},\n",
            arg_schemas.join(", "),
            return_schema
        ));
    }

    out.push_str("};\n\n");
    out
}

/// Generate BindingSerializers for runtime channel binding.
/// These provide encode/decode functions based on schema element types.
pub fn generate_binding_serializers(service: &ServiceDetail) -> String {
    let mut out = String::new();
    let service_name_lower = service.name.to_lower_camel_case();

    out.push_str("// Serializers for runtime channel binding\n");
    out.push_str(&format!(
        "export const {service_name_lower}_serializers: BindingSerializers = {{\n"
    ));

    // getTxSerializer: given element schema, return a serializer function
    out.push_str("  getTxSerializer(schema: Schema): (value: unknown) => Uint8Array {\n");
    out.push_str("    switch (schema.kind) {\n");
    out.push_str("      case 'bool': return (v) => encodeBool(v as boolean);\n");
    out.push_str("      case 'u8': return (v) => encodeU8(v as number);\n");
    out.push_str("      case 'i8': return (v) => encodeI8(v as number);\n");
    out.push_str("      case 'u16': return (v) => encodeU16(v as number);\n");
    out.push_str("      case 'i16': return (v) => encodeI16(v as number);\n");
    out.push_str("      case 'u32': return (v) => encodeU32(v as number);\n");
    out.push_str("      case 'i32': return (v) => encodeI32(v as number);\n");
    out.push_str("      case 'u64': return (v) => encodeU64(v as bigint);\n");
    out.push_str("      case 'i64': return (v) => encodeI64(v as bigint);\n");
    out.push_str("      case 'f32': return (v) => encodeF32(v as number);\n");
    out.push_str("      case 'f64': return (v) => encodeF64(v as number);\n");
    out.push_str("      case 'string': return (v) => encodeString(v as string);\n");
    out.push_str("      case 'bytes': return (v) => encodeBytes(v as Uint8Array);\n");
    out.push_str(
        "      default: throw new Error(`Unsupported schema kind for Tx: ${schema.kind}`);\n",
    );
    out.push_str("    }\n");
    out.push_str("  },\n");

    // getRxDeserializer: given element schema, return a deserializer function
    out.push_str("  getRxDeserializer(schema: Schema): (bytes: Uint8Array) => unknown {\n");
    out.push_str("    switch (schema.kind) {\n");
    out.push_str("      case 'bool': return (b) => decodeBool(b, 0).value;\n");
    out.push_str("      case 'u8': return (b) => decodeU8(b, 0).value;\n");
    out.push_str("      case 'i8': return (b) => decodeI8(b, 0).value;\n");
    out.push_str("      case 'u16': return (b) => decodeU16(b, 0).value;\n");
    out.push_str("      case 'i16': return (b) => decodeI16(b, 0).value;\n");
    out.push_str("      case 'u32': return (b) => decodeU32(b, 0).value;\n");
    out.push_str("      case 'i32': return (b) => decodeI32(b, 0).value;\n");
    out.push_str("      case 'u64': return (b) => decodeU64(b, 0).value;\n");
    out.push_str("      case 'i64': return (b) => decodeI64(b, 0).value;\n");
    out.push_str("      case 'f32': return (b) => decodeF32(b, 0).value;\n");
    out.push_str("      case 'f64': return (b) => decodeF64(b, 0).value;\n");
    out.push_str("      case 'string': return (b) => decodeString(b, 0).value;\n");
    out.push_str("      case 'bytes': return (b) => decodeBytes(b, 0).value;\n");
    out.push_str(
        "      default: throw new Error(`Unsupported schema kind for Rx: ${schema.kind}`);\n",
    );
    out.push_str("    }\n");
    out.push_str("  },\n");

    out.push_str("};\n\n");
    out
}

/// Generate complete schema exports (method schemas + serializers).
pub fn generate_schemas(service: &ServiceDetail) -> String {
    let mut out = String::new();

    // Generate method schemas
    out.push_str(&generate_method_schemas(service));

    // Check if any method uses streaming
    let has_streaming = service.methods.iter().any(|m| {
        m.args.iter().any(|a| is_tx(a.ty) || is_rx(a.ty))
            || is_tx(m.return_type)
            || is_rx(m.return_type)
    });

    // Generate serializers only if streaming is used
    if has_streaming {
        out.push_str(&generate_binding_serializers(service));
    }

    out
}
