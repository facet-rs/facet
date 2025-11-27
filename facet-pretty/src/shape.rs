//! Pretty printer for Shape types as Rust-like code
//!
//! This module provides functionality to format a `Shape` as Rust source code,
//! showing the type definition with its attributes.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write;

use facet_core::{
    Def, EnumRepr, EnumType, PointerType, Shape, StructKind, StructType, Type, UserType,
};

/// Format a Shape as Rust-like source code
pub fn format_shape(shape: &Shape) -> String {
    let mut output = String::new();
    format_shape_into(shape, &mut output).expect("Formatting failed");
    output
}

/// Format a Shape into an existing String
pub fn format_shape_into(shape: &Shape, output: &mut String) -> core::fmt::Result {
    // First check def for container types (Map, List, Option, Array)
    // These have rich generic info even when ty is Opaque
    match shape.def {
        Def::Map(_) | Def::List(_) | Def::Option(_) | Def::Array(_) => {
            write_type_name(shape, output)?;
            return Ok(());
        }
        _ => {}
    }

    // Then check ty for user-defined types
    match &shape.ty {
        Type::User(user_type) => match user_type {
            UserType::Struct(struct_type) => {
                format_struct(shape, struct_type, output)?;
            }
            UserType::Enum(enum_type) => {
                format_enum(shape, enum_type, output)?;
            }
            UserType::Union(_) | UserType::Opaque => {
                // For union/opaque types, just show the type identifier
                write!(output, "{}", shape.type_identifier)?;
            }
        },
        _ => {
            // For non-user types (primitives, pointers, etc.), use write_type_name
            write_type_name(shape, output)?;
        }
    }
    Ok(())
}

fn format_struct(
    shape: &Shape,
    struct_type: &StructType,
    output: &mut String,
) -> core::fmt::Result {
    // Write #[derive(Facet)]
    writeln!(output, "#[derive(Facet)]")?;

    // Write facet attributes if any
    write_facet_attrs(shape, output)?;

    // Write struct definition
    match struct_type.kind {
        StructKind::Struct => {
            writeln!(output, "struct {} {{", shape.type_identifier)?;
            for field in struct_type.fields {
                write!(output, "    {}: ", field.name)?;
                write_type_name(field.shape(), output)?;
                writeln!(output, ",")?;
            }
            write!(output, "}}")?;
        }
        StructKind::Tuple | StructKind::TupleStruct => {
            write!(output, "struct {}(", shape.type_identifier)?;
            for (i, field) in struct_type.fields.iter().enumerate() {
                if i > 0 {
                    write!(output, ", ")?;
                }
                write_type_name(field.shape(), output)?;
            }
            write!(output, ");")?;
        }
        StructKind::Unit => {
            write!(output, "struct {};", shape.type_identifier)?;
        }
    }
    Ok(())
}

fn format_enum(shape: &Shape, enum_type: &EnumType, output: &mut String) -> core::fmt::Result {
    // Write #[derive(Facet)]
    writeln!(output, "#[derive(Facet)]")?;

    // Write repr for the discriminant type
    let repr_str = match enum_type.enum_repr {
        EnumRepr::RustNPO => None, // Don't show repr for nullable pointer optimization
        EnumRepr::U8 => Some("u8"),
        EnumRepr::U16 => Some("u16"),
        EnumRepr::U32 => Some("u32"),
        EnumRepr::U64 => Some("u64"),
        EnumRepr::USize => Some("usize"),
        EnumRepr::I8 => Some("i8"),
        EnumRepr::I16 => Some("i16"),
        EnumRepr::I32 => Some("i32"),
        EnumRepr::I64 => Some("i64"),
        EnumRepr::ISize => Some("isize"),
    };

    if let Some(repr) = repr_str {
        writeln!(output, "#[repr({repr})]")?;
    }

    // Write facet attributes if any
    write_facet_attrs(shape, output)?;

    // Write enum definition
    writeln!(output, "enum {} {{", shape.type_identifier)?;

    for variant in enum_type.variants {
        // variant.data is a StructType containing the variant's fields
        match variant.data.kind {
            StructKind::Unit => {
                writeln!(output, "    {},", variant.name)?;
            }
            StructKind::Tuple | StructKind::TupleStruct => {
                write!(output, "    {}(", variant.name)?;
                for (i, field) in variant.data.fields.iter().enumerate() {
                    if i > 0 {
                        write!(output, ", ")?;
                    }
                    write_type_name(field.shape(), output)?;
                }
                writeln!(output, "),")?;
            }
            StructKind::Struct => {
                writeln!(output, "    {} {{", variant.name)?;
                for field in variant.data.fields {
                    write!(output, "        {}: ", field.name)?;
                    write_type_name(field.shape(), output)?;
                    writeln!(output, ",")?;
                }
                writeln!(output, "    }},")?;
            }
        }
    }

    write!(output, "}}")?;
    Ok(())
}

fn write_facet_attrs(shape: &Shape, output: &mut String) -> core::fmt::Result {
    let mut attrs: Vec<String> = Vec::new();

    // Check for tag attribute (internally tagged enum)
    if let Some(tag) = shape.get_tag_attr() {
        if let Some(content) = shape.get_content_attr() {
            // Adjacently tagged
            attrs.push(alloc::format!("tag = \"{tag}\", content = \"{content}\""));
        } else {
            // Internally tagged
            attrs.push(alloc::format!("tag = \"{tag}\""));
        }
    }

    // Check for untagged
    if shape.is_untagged() {
        attrs.push("untagged".into());
    }

    // Check for deny_unknown_fields
    if shape.has_deny_unknown_fields_attr() {
        attrs.push("deny_unknown_fields".into());
    }

    if !attrs.is_empty() {
        writeln!(output, "#[facet({})]", attrs.join(", "))?;
    }

    Ok(())
}

fn write_type_name(shape: &Shape, output: &mut String) -> core::fmt::Result {
    // Handle common wrapper types
    match shape.def {
        Def::Scalar => {
            write!(output, "{}", shape.type_identifier)?;
        }
        Def::Pointer(_) => {
            // Handle references to slices/arrays
            if let Type::Pointer(PointerType::Reference(r)) = shape.ty {
                match r.target.def {
                    Def::Array(array_def) => {
                        write!(output, "&[")?;
                        write_type_name(array_def.t, output)?;
                        write!(output, "; {}]", array_def.n)?;
                        return Ok(());
                    }
                    _ => {}
                }
            }
            write!(output, "{}", shape.type_identifier)?;
        }
        Def::List(list_def) => {
            write!(output, "Vec<")?;
            write_type_name(list_def.t, output)?;
            write!(output, ">")?;
        }
        Def::Array(array_def) => {
            write!(output, "[")?;
            write_type_name(array_def.t, output)?;
            write!(output, "; {}]", array_def.n)?;
        }
        Def::Map(map_def) => {
            // Use the type_identifier which might be HashMap, BTreeMap, etc.
            let map_name = if shape.type_identifier.contains("BTreeMap") {
                "BTreeMap"
            } else {
                "HashMap"
            };
            write!(output, "{}<", map_name)?;
            write_type_name(map_def.k, output)?;
            write!(output, ", ")?;
            write_type_name(map_def.v, output)?;
            write!(output, ">")?;
        }
        Def::Option(option_def) => {
            write!(output, "Option<")?;
            write_type_name(option_def.t, output)?;
            write!(output, ">")?;
        }
        _ => {
            // Fallback to type_identifier
            write!(output, "{}", shape.type_identifier)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use facet::Facet;

    #[test]
    fn test_simple_struct() {
        #[derive(Facet)]
        struct Simple {
            name: String,
            count: u32,
        }

        let output = format_shape(Simple::SHAPE);
        assert!(output.contains("struct Simple"));
        assert!(output.contains("name: String"));
        assert!(output.contains("count: u32"));
    }

    #[test]
    fn test_enum_with_tag() {
        #[derive(Facet)]
        #[repr(C)]
        #[facet(tag = "type")]
        enum Tagged {
            A { x: i32 },
            B { y: String },
        }

        let output = format_shape(Tagged::SHAPE);
        assert!(output.contains("enum Tagged"));
        assert!(output.contains("#[facet(tag = \"type\")]"));
    }
}
