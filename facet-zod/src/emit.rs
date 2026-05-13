//! Emit Zod TypeScript source text from the intermediate [`ZodType`](crate::mapping::ZodType) tree.

use crate::config::{Config, ExportStyle};
use crate::mapping::{NamedSchema, ZodField, ZodType};

/// Emit a top-level named schema declaration.
pub fn emit_schema(schema: &NamedSchema, config: &Config) -> String {
    let mut out = String::new();

    if let Some(doc) = &schema.doc {
        for line in doc.lines() {
            out.push_str(&format!("/** {} */\n", line.trim()));
        }
    }

    let type_expr = emit_type(&schema.ty);
    let const_name = format!("{}Schema", schema.name);

    match config.export_style {
        ExportStyle::ConstAndType => {
            out.push_str(&format!("export const {const_name} = {type_expr};\n"));
            out.push_str(&format!(
                "export type {} = z.infer<typeof {const_name}>;\n",
                schema.name
            ));
        }
        ExportStyle::ConstOnly => {
            out.push_str(&format!("export const {const_name} = {type_expr};\n"));
        }
        ExportStyle::TypeOnly => {
            out.push_str(&format!(
                "export type {} = z.infer<typeof {type_expr}>;\n",
                schema.name
            ));
        }
    }

    out
}

/// Emit a Zod expression for a single [`ZodType`] node.
pub fn emit_type(ty: &ZodType) -> String {
    match ty {
        ZodType::String => "z.string()".into(),
        ZodType::Number { int: true } => "z.number().int()".into(),
        ZodType::Number { int: false } => "z.number()".into(),
        ZodType::BigInt => "z.bigint()".into(),
        ZodType::Boolean => "z.boolean()".into(),
        ZodType::Unknown => "z.unknown()".into(),
        ZodType::Never => "z.never()".into(),
        ZodType::Undefined => "z.undefined()".into(),

        ZodType::Literal(val) => {
            if val == "true" || val == "false" {
                format!("z.literal({val})")
            } else {
                format!("z.literal(\"{val}\")")
            }
        }

        ZodType::Optional(inner) => format!("{}.optional()", emit_type(inner)),
        ZodType::Nullable(inner) => format!("{}.nullable()", emit_type(inner)),
        ZodType::Nullish(inner) => format!("{}.nullish()", emit_type(inner)),

        ZodType::Array(elem) => format!("z.array({})", emit_type(elem)),

        ZodType::Tuple(elems) => {
            let items: Vec<String> = elems.iter().map(emit_type).collect();
            format!("z.tuple([{}])", items.join(", "))
        }

        ZodType::Record(k, v) => format!("z.record({}, {})", emit_type(k), emit_type(v)),

        ZodType::Object(fields) => emit_object(fields),

        ZodType::Union(members) => {
            let items: Vec<String> = members.iter().map(emit_type).collect();
            format!("z.union([{}])", items.join(", "))
        }

        ZodType::Enum(variants) => {
            let items: Vec<String> = variants.iter().map(|v| format!("\"{v}\"")).collect();
            format!("z.enum([{}])", items.join(", "))
        }

        ZodType::Ref(name) => format!("{name}Schema"),
        ZodType::Lazy(name) => format!("z.lazy(() => {name}Schema)"),
    }
}

fn emit_object(fields: &[ZodField]) -> String {
    if fields.is_empty() {
        return "z.object({})".into();
    }

    let mut lines = Vec::new();
    for field in fields {
        let type_expr = emit_type(&field.ty);
        let line = if let Some(doc) = &field.doc {
            format!("  /** {} */\n  {}: {}", doc, field.name, type_expr)
        } else {
            format!("  {}: {}", field.name, type_expr)
        };
        lines.push(line);
    }

    format!("z.object({{\n{},\n}})", lines.join(",\n"))
}
