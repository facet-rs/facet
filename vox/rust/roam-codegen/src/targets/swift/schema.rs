//! Swift schema generation for runtime channel binding.
//!
//! Generates runtime schema information for channel discovery.

use facet_core::{ScalarType, Shape};
use heck::{ToLowerCamelCase, ToUpperCamelCase};
use roam_types::{
    EnumInfo, ServiceDescriptor, ShapeKind, StructInfo, VariantKind, classify_shape,
    classify_variant, is_bytes,
};

use crate::code_writer::CodeWriter;
use crate::cw_writeln;

/// Generate complete schema code (method schemas + serializers).
pub fn generate_schemas(service: &ServiceDescriptor) -> String {
    let mut out = String::new();
    out.push_str(&generate_method_schemas(service));
    out.push_str(&generate_serializers(service));
    out
}

/// Generate method schemas for runtime channel binding.
fn generate_method_schemas(service: &ServiceDescriptor) -> String {
    let mut out = String::new();
    let service_name = service.service_name.to_lower_camel_case();

    out.push_str(&format!(
        "public let {service_name}_schemas: [String: MethodSchema] = [\n"
    ));

    for method in service.methods {
        let method_name = method.method_name.to_lower_camel_case();
        out.push_str(&format!("    \"{method_name}\": MethodSchema(args: ["));

        let schemas: Vec<String> = method
            .args
            .iter()
            .map(|a| shape_to_schema(a.shape))
            .collect();
        out.push_str(&schemas.join(", "));

        out.push_str("]),\n");
    }

    out.push_str("]\n\n");
    out
}

/// Convert a Shape to its Swift Schema representation.
fn shape_to_schema(shape: &'static Shape) -> String {
    if is_bytes(shape) {
        return ".bytes".into();
    }

    match classify_shape(shape) {
        ShapeKind::Scalar(scalar) => match scalar {
            ScalarType::Bool => ".bool".into(),
            ScalarType::U8 => ".u8".into(),
            ScalarType::U16 => ".u16".into(),
            ScalarType::U32 => ".u32".into(),
            ScalarType::U64 => ".u64".into(),
            ScalarType::I8 => ".i8".into(),
            ScalarType::I16 => ".i16".into(),
            ScalarType::I32 => ".i32".into(),
            ScalarType::I64 => ".i64".into(),
            ScalarType::F32 => ".f32".into(),
            ScalarType::F64 => ".f64".into(),
            ScalarType::Str | ScalarType::CowStr | ScalarType::String => ".string".into(),
            ScalarType::Unit => ".tuple(elements: [])".into(),
            _ => ".bytes".into(), // fallback
        },
        ShapeKind::List { element } | ShapeKind::Slice { element } => {
            format!(".vec(element: {})", shape_to_schema(element))
        }
        ShapeKind::Option { inner } => {
            format!(".option(inner: {})", shape_to_schema(inner))
        }
        ShapeKind::Map { key, value } => {
            format!(
                ".map(key: {}, value: {})",
                shape_to_schema(key),
                shape_to_schema(value)
            )
        }
        ShapeKind::Tx { inner } => {
            format!(".tx(element: {})", shape_to_schema(inner))
        }
        ShapeKind::Rx { inner } => {
            format!(".rx(element: {})", shape_to_schema(inner))
        }
        ShapeKind::Tuple { elements } => {
            let inner: Vec<String> = elements.iter().map(|p| shape_to_schema(p.shape)).collect();
            format!(".tuple(elements: [{}])", inner.join(", "))
        }
        ShapeKind::Struct(StructInfo { fields, .. }) => {
            let field_strs: Vec<String> = fields
                .iter()
                .map(|f| format!("(\"{}\", {})", f.name, shape_to_schema(f.shape())))
                .collect();
            format!(".struct(fields: [{}])", field_strs.join(", "))
        }
        ShapeKind::Enum(EnumInfo { variants, .. }) => {
            let variant_strs: Vec<String> = variants
                .iter()
                .map(|v| {
                    let fields: Vec<String> = match classify_variant(v) {
                        VariantKind::Unit => vec![],
                        VariantKind::Newtype { inner } => vec![shape_to_schema(inner)],
                        VariantKind::Tuple { fields } | VariantKind::Struct { fields } => {
                            fields.iter().map(|f| shape_to_schema(f.shape())).collect()
                        }
                    };
                    format!("(\"{}\", [{}])", v.name, fields.join(", "))
                })
                .collect();
            format!(".enum(variants: [{}])", variant_strs.join(", "))
        }
        _ => ".bytes".into(), // fallback for unknown types
    }
}

/// Generate serializers for runtime channel binding.
fn generate_serializers(service: &ServiceDescriptor) -> String {
    let mut out = String::new();
    let mut w = CodeWriter::with_indent_spaces(&mut out, 4);
    let service_name_upper = service.service_name.to_upper_camel_case();

    cw_writeln!(
        w,
        "public struct {service_name_upper}Serializers: BindingSerializers {{"
    )
    .unwrap();
    {
        let _indent = w.indent();
        w.writeln("public init() {}").unwrap();
        w.blank_line().unwrap();

        // txSerializer
        w.writeln("public func txSerializer(for schema: Schema) -> @Sendable (Any) -> [UInt8] {")
            .unwrap();
        {
            let _indent = w.indent();
            w.writeln("switch schema {").unwrap();
            w.writeln("case .bool: return { encodeBool($0 as! Bool) }")
                .unwrap();
            w.writeln("case .u8: return { encodeU8($0 as! UInt8) }")
                .unwrap();
            w.writeln("case .i8: return { encodeI8($0 as! Int8) }")
                .unwrap();
            w.writeln("case .u16: return { encodeU16($0 as! UInt16) }")
                .unwrap();
            w.writeln("case .i16: return { encodeI16($0 as! Int16) }")
                .unwrap();
            w.writeln("case .u32: return { encodeU32($0 as! UInt32) }")
                .unwrap();
            w.writeln("case .i32: return { encodeI32($0 as! Int32) }")
                .unwrap();
            w.writeln("case .u64: return { encodeVarint($0 as! UInt64) }")
                .unwrap();
            w.writeln("case .i64: return { encodeI64($0 as! Int64) }")
                .unwrap();
            w.writeln("case .f32: return { encodeF32($0 as! Float) }")
                .unwrap();
            w.writeln("case .f64: return { encodeF64($0 as! Double) }")
                .unwrap();
            w.writeln("case .string: return { encodeString($0 as! String) }")
                .unwrap();
            w.writeln("case .bytes: return { [UInt8]($0 as! Data) }")
                .unwrap();
            w.writeln(
                "default: fatalError(\"Unsupported schema for Tx serialization: \\(schema)\")",
            )
            .unwrap();
            w.writeln("}").unwrap();
        }
        w.writeln("}").unwrap();
        w.blank_line().unwrap();

        // rxDeserializer
        w.writeln(
            "public func rxDeserializer(for schema: Schema) -> @Sendable ([UInt8]) throws -> Any {",
        )
        .unwrap();
        {
            let _indent = w.indent();
            w.writeln("switch schema {").unwrap();
            w.writeln("case .bool: return { var o = 0; return try decodeBool(from: Data($0), offset: &o) }").unwrap();
            w.writeln(
                "case .u8: return { var o = 0; return try decodeU8(from: Data($0), offset: &o) }",
            )
            .unwrap();
            w.writeln(
                "case .i8: return { var o = 0; return try decodeI8(from: Data($0), offset: &o) }",
            )
            .unwrap();
            w.writeln(
                "case .u16: return { var o = 0; return try decodeU16(from: Data($0), offset: &o) }",
            )
            .unwrap();
            w.writeln(
                "case .i16: return { var o = 0; return try decodeI16(from: Data($0), offset: &o) }",
            )
            .unwrap();
            w.writeln(
                "case .u32: return { var o = 0; return try decodeU32(from: Data($0), offset: &o) }",
            )
            .unwrap();
            w.writeln(
                "case .i32: return { var o = 0; return try decodeI32(from: Data($0), offset: &o) }",
            )
            .unwrap();
            w.writeln("case .u64: return { var o = 0; return try decodeVarint(from: Data($0), offset: &o) }").unwrap();
            w.writeln(
                "case .i64: return { var o = 0; return try decodeI64(from: Data($0), offset: &o) }",
            )
            .unwrap();
            w.writeln(
                "case .f32: return { var o = 0; return try decodeF32(from: Data($0), offset: &o) }",
            )
            .unwrap();
            w.writeln(
                "case .f64: return { var o = 0; return try decodeF64(from: Data($0), offset: &o) }",
            )
            .unwrap();
            w.writeln("case .string: return { var o = 0; return try decodeString(from: Data($0), offset: &o) }").unwrap();
            w.writeln("case .bytes: return { Data($0) }").unwrap();
            w.writeln(
                "default: fatalError(\"Unsupported schema for Rx deserialization: \\(schema)\")",
            )
            .unwrap();
            w.writeln("}").unwrap();
        }
        w.writeln("}").unwrap();
    }
    w.writeln("}").unwrap();
    w.blank_line().unwrap();

    out
}
