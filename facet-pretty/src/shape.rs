//! Pretty printer for Shape types as Rust-like code
//!
//! This module provides functionality to format a `Shape` as Rust source code,
//! showing the type definition with its attributes.

use alloc::collections::BTreeSet;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write;

use facet_core::{
    Def, EnumRepr, EnumType, PointerType, Shape, StructKind, StructType, Type, UserType,
};

/// Format a Shape as Rust-like source code, recursively expanding nested types
pub fn format_shape(shape: &Shape) -> String {
    let mut output = String::new();
    format_shape_into(shape, &mut output).expect("Formatting failed");
    output
}

/// Format a Shape into an existing String, recursively expanding nested types
pub fn format_shape_into(shape: &Shape, output: &mut String) -> core::fmt::Result {
    // Track which types we've already printed to avoid duplicates
    let mut printed: BTreeSet<&'static str> = BTreeSet::new();
    // Queue of types to print
    let mut queue: Vec<&Shape> = Vec::new();

    // Start with the root shape
    queue.push(shape);

    while let Some(current) = queue.pop() {
        // Skip if we've already printed this type
        if !printed.insert(current.type_identifier) {
            continue;
        }

        // Add separator between type definitions
        if printed.len() > 1 {
            writeln!(output)?;
            writeln!(output)?;
        }

        // First check def for container types (Map, List, Option, Array)
        // These have rich generic info even when ty is Opaque
        match current.def {
            Def::Map(_) | Def::List(_) | Def::Option(_) | Def::Array(_) => {
                // Don't print container types as definitions, they're inline
                printed.remove(current.type_identifier);
                continue;
            }
            _ => {}
        }

        // Then check ty for user-defined types
        match &current.ty {
            Type::User(user_type) => match user_type {
                UserType::Struct(struct_type) => {
                    format_struct(current, struct_type, output)?;
                    // Queue nested types from fields
                    collect_nested_types(struct_type, &mut queue);
                }
                UserType::Enum(enum_type) => {
                    format_enum(current, enum_type, output)?;
                    // Queue nested types from variants
                    for variant in enum_type.variants {
                        collect_nested_types(&variant.data, &mut queue);
                    }
                }
                UserType::Union(_) | UserType::Opaque => {
                    // For union/opaque types, just show the type identifier
                    // Don't actually print anything since we can't expand them
                    printed.remove(current.type_identifier);
                }
            },
            _ => {
                // For non-user types (primitives, pointers, etc.), don't print
                printed.remove(current.type_identifier);
            }
        }
    }
    Ok(())
}

/// Collect nested user-defined types from struct fields
fn collect_nested_types<'a>(struct_type: &'a StructType, queue: &mut Vec<&'a Shape>) {
    for field in struct_type.fields {
        collect_from_shape(field.shape(), queue);
    }
}

/// Recursively collect user-defined types from a shape (handles containers)
fn collect_from_shape<'a>(shape: &'a Shape, queue: &mut Vec<&'a Shape>) {
    match shape.def {
        Def::List(list_def) => collect_from_shape(list_def.t, queue),
        Def::Array(array_def) => collect_from_shape(array_def.t, queue),
        Def::Map(map_def) => {
            collect_from_shape(map_def.k, queue);
            collect_from_shape(map_def.v, queue);
        }
        Def::Option(option_def) => collect_from_shape(option_def.t, queue),
        _ => {
            // Check if it's a user-defined type worth expanding
            if let Type::User(UserType::Struct(_) | UserType::Enum(_)) = &shape.ty {
                queue.push(shape);
            }
        }
    }
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
                if let Def::Array(array_def) = r.target.def {
                    write!(output, "&[")?;
                    write_type_name(array_def.t, output)?;
                    write!(output, "; {}]", array_def.n)?;
                    return Ok(());
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
            write!(output, "{map_name}<")?;
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
        #[allow(dead_code)]
        enum Tagged {
            A { x: i32 },
            B { y: String },
        }

        let output = format_shape(Tagged::SHAPE);
        assert!(output.contains("enum Tagged"));
        assert!(output.contains("#[facet(tag = \"type\")]"));
    }

    #[test]
    fn test_nested_types() {
        #[derive(Facet)]
        #[allow(dead_code)]
        struct Inner {
            value: i32,
        }

        #[derive(Facet)]
        #[allow(dead_code)]
        struct Outer {
            inner: Inner,
            name: String,
        }

        let output = format_shape(Outer::SHAPE);
        // Should contain both Outer and Inner definitions
        assert!(output.contains("struct Outer"), "Missing Outer: {output}");
        assert!(
            output.contains("inner: Inner"),
            "Missing inner field: {output}"
        );
        assert!(
            output.contains("struct Inner"),
            "Missing Inner definition: {output}"
        );
        assert!(
            output.contains("value: i32"),
            "Missing value field: {output}"
        );
    }

    #[test]
    fn test_nested_in_vec() {
        #[derive(Facet)]
        #[allow(dead_code)]
        struct Item {
            id: u32,
        }

        #[derive(Facet)]
        #[allow(dead_code)]
        struct Container {
            items: Vec<Item>,
        }

        let output = format_shape(Container::SHAPE);
        // Should contain both Container and Item definitions
        assert!(
            output.contains("struct Container"),
            "Missing Container: {output}"
        );
        assert!(
            output.contains("items: Vec<Item>"),
            "Missing items field: {output}"
        );
        assert!(
            output.contains("struct Item"),
            "Missing Item definition: {output}"
        );
    }
}
