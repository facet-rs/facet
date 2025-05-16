#![warn(missing_docs)]
#![warn(clippy::std_instead_of_core)]
#![warn(clippy::std_instead_of_alloc)]
#![deny(unsafe_code)]
#![doc = include_str!("../README.md")]

extern crate facet_core as facet;
use facet::{PointerType, SmartPointerDef};
use facet_core::{Def, Facet, ScalarDef, Shape, Type, UserType};

use std::io::Write;

/// Convert a `Facet` type to a JSON schema string.
pub fn to_string<'a, T: Facet<'a>>() -> String {
    let mut buffer = Vec::new();
    write!(buffer, "{{").unwrap();
    write!(
        buffer,
        "\"$schema\": \"https://json-schema.org/draft/2020-12/schema\","
    )
    .unwrap();

    // Find the first attribute that starts with "id=", if it exists more than once is an error
    let mut id = T::SHAPE.attributes.iter().filter_map(|attr| match attr {
        facet_core::ShapeAttribute::Arbitrary(attr_str) => {
            if attr_str.starts_with("id") {
                let id = attr_str
                    .split('=')
                    .nth(1)
                    .unwrap_or_default()
                    .trim()
                    .trim_matches('"');
                Some(id)
            } else {
                None
            }
        }
        _ => None,
    });
    match (id.next(), id.next()) {
        (Some(_), Some(_)) => panic!("More than one id attribute found"),
        (Some(id), None) => {
            write!(buffer, "\"$id\": \"{id}\",").unwrap();
        }
        _ => {
            // No id attribute found, do nothing
        }
    }

    serialize(T::SHAPE, &[], &mut buffer).unwrap();
    write!(buffer, "}}").unwrap();
    String::from_utf8(buffer).unwrap()
}

fn serialize<W: Write>(shape: &'static Shape, doc: &[&str], writer: &mut W) -> std::io::Result<()> {
    serialize_doc(&[shape.doc, doc].concat(), writer)?;

    // First check the type system (Type)
    match &shape.ty {
        Type::User(UserType::Struct(struct_def)) => {
            serialize_struct(struct_def, writer)?;
            return Ok(());
        }
        Type::User(UserType::Enum(_enum_def)) => {
            todo!("Enum");
        }
        Type::Sequence(sequence_type) => {
            use facet_core::SequenceType;
            match sequence_type {
                SequenceType::Slice(_slice_type) => {
                    // For slices, use the Def::Slice if available
                    if let Def::Slice(slice_def) = shape.def {
                        serialize_slice(slice_def, writer)?;
                        return Ok(());
                    }
                }
                SequenceType::Array(_array_type) => {
                    // For arrays, use the Def::Array if available
                    if let Def::Array(array_def) = shape.def {
                        serialize_array(array_def, writer)?;
                        return Ok(());
                    }
                }
                _ => {} // Handle other sequence types
            }
        }
        _ => {} // Continue to check the def system
    }

    // Then check the def system (Def)
    match shape.def {
        Def::Scalar(ref scalar_def) => serialize_scalar(scalar_def, writer)?,
        Def::Map(_map_def) => todo!("Map"),
        Def::List(list_def) => serialize_list(list_def, writer)?,
        Def::Slice(slice_def) => serialize_slice(slice_def, writer)?,
        Def::Array(array_def) => serialize_array(array_def, writer)?,
        Def::Option(option_def) => serialize_option(option_def, writer)?,
        Def::SmartPointer(SmartPointerDef {
            pointee: Some(inner_shape),
            ..
        }) => serialize(inner_shape(), &[], writer)?,
        Def::Undefined => {
            // Handle the case when not yet migrated to the Type enum
            // For primitives, we can try to infer the type
            match &shape.ty {
                Type::Primitive(primitive) => {
                    use facet_core::{NumericType, PrimitiveType, TextualType};
                    match primitive {
                        PrimitiveType::Numeric(NumericType::Float) => {
                            write!(writer, "\"type\": \"number\", \"format\": \"double\"")?;
                        }
                        PrimitiveType::Boolean => {
                            write!(writer, "\"type\": \"boolean\"")?;
                        }
                        PrimitiveType::Textual(TextualType::Str) => {
                            write!(writer, "\"type\": \"string\"")?;
                        }
                        _ => {
                            write!(writer, "\"type\": \"unknown\"")?;
                        }
                    }
                }
                Type::Pointer(PointerType::Reference(pt) | PointerType::Raw(pt)) => {
                    serialize((pt.target)(), &[], writer)?
                }
                _ => {
                    write!(writer, "\"type\": \"unknown\"")?;
                }
            }
        }
        _ => {
            write!(writer, "\"type\": \"unknown\"")?;
        }
    }

    Ok(())
}

fn serialize_doc<W: Write>(doc: &[&str], writer: &mut W) -> Result<(), std::io::Error> {
    if !doc.is_empty() {
        let doc = doc.join("\n");
        write!(writer, "\"description\": \"{}\",", doc.trim())?;
    }
    Ok(())
}

/// Serialize a scalar definition to JSON schema format.
fn serialize_scalar<W: Write>(scalar_def: &ScalarDef, writer: &mut W) -> std::io::Result<()> {
    match scalar_def.affinity {
        facet_core::ScalarAffinity::Number(number_affinity) => {
            match number_affinity.bits {
                facet_core::NumberBits::Integer { bits, sign } => {
                    write!(writer, "\"type\": \"integer\"")?;
                    match sign {
                        facet_core::Signedness::Unsigned => {
                            write!(writer, ", \"format\": \"uint{bits}\"")?;
                            write!(writer, ", \"minimum\": 0")?;
                        }
                        facet_core::Signedness::Signed => {
                            write!(writer, ", \"format\": \"int{bits}\"")?;
                        }
                    }
                }
                facet_core::NumberBits::Float { .. } => {
                    write!(writer, "\"type\": \"number\"")?;
                    write!(writer, ", \"format\": \"double\"")?;
                }
                _ => unimplemented!(),
            }
            Ok(())
        }
        facet_core::ScalarAffinity::String(_) => {
            write!(writer, "\"type\": \"string\"")?;
            Ok(())
        }
        facet_core::ScalarAffinity::Boolean(_) => {
            write!(writer, "\"type\": \"boolean\"")?;
            Ok(())
        }
        _ => Err(std::io::Error::other(format!(
            "facet-jsonschema: nsupported scalar type: {scalar_def:#?}"
        ))),
    }
}

fn serialize_struct<W: Write>(
    struct_type: &facet_core::StructType,
    writer: &mut W,
) -> std::io::Result<()> {
    write!(writer, "\"type\": \"object\",")?;
    let required = struct_type
        .fields
        .iter()
        .map(|f| format!("\"{}\"", f.name))
        .collect::<Vec<_>>()
        .join(",");
    write!(writer, "\"required\": [{required}],")?;
    write!(writer, "\"properties\": {{")?;
    let mut first = true;
    for field in struct_type.fields {
        if !first {
            write!(writer, ",")?;
        }
        first = false;
        write!(writer, "\"{}\": {{", field.name)?;
        serialize(field.shape(), field.doc, writer)?;
        write!(writer, "}}")?;
    }
    write!(writer, "}}")?;
    Ok(())
}

/// Serialize a list definition to JSON schema format.
fn serialize_list<W: Write>(list_def: facet_core::ListDef, writer: &mut W) -> std::io::Result<()> {
    write!(writer, "\"type\": \"array\",")?;
    write!(writer, "\"items\": {{")?;
    serialize(list_def.t(), &[], writer)?;
    write!(writer, "}}")?;
    Ok(())
}

/// Serialize a slice definition to JSON schema format.
fn serialize_slice<W: Write>(
    slice_def: facet_core::SliceDef,
    writer: &mut W,
) -> std::io::Result<()> {
    write!(writer, "\"type\": \"array\",")?;
    write!(writer, "\"items\": {{")?;
    serialize(slice_def.t(), &[], writer)?;
    write!(writer, "}}")?;
    Ok(())
}

/// Serialize an array definition to JSON schema format.
fn serialize_array<W: Write>(
    array_def: facet_core::ArrayDef,
    writer: &mut W,
) -> std::io::Result<()> {
    write!(writer, "\"type\": \"array\",")?;
    write!(writer, "\"minItems\": {},", array_def.n)?;
    write!(writer, "\"maxItems\": {},", array_def.n)?;
    write!(writer, "\"items\": {{")?;
    serialize(array_def.t(), &[], writer)?;
    write!(writer, "}}")?;
    Ok(())
}

/// Serialize an option definition to JSON schema format.
fn serialize_option<W: Write>(
    _option_def: facet_core::OptionDef,
    writer: &mut W,
) -> std::io::Result<()> {
    write!(writer, "\"type\": \"[]\",")?;
    unimplemented!("serialize_option");
}

#[cfg(test)]
mod tests {
    extern crate alloc;
    use alloc::{rc::Rc, sync::Arc};

    use super::*;
    use facet_derive::Facet;
    use insta::assert_snapshot;

    #[test]
    fn test_basic() {
        /// Test documentation
        #[derive(Facet)]
        #[facet(id = "http://example.com/schema")]
        struct TestStruct {
            /// Test doc1
            string_field: String,
            /// Test doc2
            int_field: u32,
            vec_field: Vec<bool>,
            slice_field: &'static [f64],
            array_field: [f64; 3],
        }

        let schema = to_string::<TestStruct>();
        assert_snapshot!(schema);
    }

    #[test]
    fn test_pointers() {
        /// Test documentation
        #[derive(Facet)]
        #[facet(id = "http://example.com/schema")]
        struct TestStruct<'a> {
            normal_pointer: &'a str,
            box_pointer: Box<u32>,
            arc: Arc<u32>,
            rc: Rc<u32>,
            #[allow(clippy::redundant_allocation)]
            nested: Rc<&'a Arc<&'a *const u32>>,
        }

        let schema = to_string::<TestStruct>();
        assert_snapshot!(schema);
    }
}
