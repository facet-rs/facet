#![warn(missing_docs)]
#![warn(clippy::std_instead_of_core)]
#![warn(clippy::std_instead_of_alloc)]
#![deny(unsafe_code)]
#![doc = include_str!("../README.md")]

use facet::{Def, Facet, ScalarDef, Shape};

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
        facet::ShapeAttribute::Arbitrary(attr_str) => {
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

    match shape.def {
        Def::Scalar(ref scalar_def) => serialize_scalar(scalar_def, writer)?,
        Def::Struct(ref struct_def) => serialize_struct(struct_def, writer)?,
        Def::Map(_map_def) => todo!("Map"),
        Def::List(list_def) => serialize_list(list_def, writer)?,
        Def::Slice(slice_def) => serialize_slice(slice_def, writer)?,
        Def::Array(array_def) => serialize_array(array_def, writer)?,
        Def::Enum(_enum_def) => todo!("Enum"),
        Def::Option(option_def) => serialize_option(option_def, writer)?,
        Def::SmartPointer(_smart_pointer_def) => todo!("SmartPointer"),
        _ => todo!("{:#?}", shape.def),
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
        facet::ScalarAffinity::Number(number_affinity) => {
            match number_affinity.bits {
                facet::NumberBits::Integer { bits, sign } => {
                    write!(writer, "\"type\": \"integer\"")?;
                    match sign {
                        facet::Signedness::Unsigned => {
                            write!(writer, ", \"format\": \"uint{bits}\"")?;
                            write!(writer, ", \"minimum\": 0")?;
                        }
                        facet::Signedness::Signed => {
                            write!(writer, ", \"format\": \"int{bits}\"")?;
                        }
                    }
                }
                facet::NumberBits::Float { .. } => {
                    write!(writer, "\"type\": \"number\"")?;
                    write!(writer, ", \"format\": \"double\"")?;
                }
                _ => unimplemented!(),
            }
            Ok(())
        }
        facet::ScalarAffinity::String(_) => {
            write!(writer, "\"type\": \"string\"")?;
            Ok(())
        }
        facet::ScalarAffinity::Boolean(_) => {
            write!(writer, "\"type\": \"boolean\"")?;
            Ok(())
        }
        _ => Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Unsupported scalar type: {scalar_def:#?}"),
        )),
    }
}

fn serialize_struct<W: Write>(struct_def: &facet::Struct, writer: &mut W) -> std::io::Result<()> {
    write!(writer, "\"type\": \"object\",")?;
    let required = struct_def
        .fields
        .iter()
        .map(|f| format!("\"{}\"", f.name))
        .collect::<Vec<_>>()
        .join(",");
    write!(writer, "\"required\": [{required}],")?;
    write!(writer, "\"properties\": {{")?;
    let mut first = true;
    for field in struct_def.fields {
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
fn serialize_list<W: Write>(list_def: facet::ListDef, writer: &mut W) -> std::io::Result<()> {
    write!(writer, "\"type\": \"array\",")?;
    write!(writer, "\"items\": {{")?;
    serialize(list_def.t(), &[], writer)?;
    write!(writer, "}}")?;
    Ok(())
}

/// Serialize a slice definition to JSON schema format.
fn serialize_slice<W: Write>(slice_def: facet::SliceDef, writer: &mut W) -> std::io::Result<()> {
    write!(writer, "\"type\": \"array\",")?;
    write!(writer, "\"items\": {{")?;
    serialize(slice_def.t(), &[], writer)?;
    write!(writer, "}}")?;
    Ok(())
}

/// Serialize an array definition to JSON schema format.
fn serialize_array<W: Write>(array_def: facet::ArrayDef, writer: &mut W) -> std::io::Result<()> {
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
    _option_def: facet::OptionDef,
    writer: &mut W,
) -> std::io::Result<()> {
    write!(writer, "\"type\": \"[]\",")?;
    unimplemented!("serialize_option");
}

#[cfg(test)]
mod tests {
    use super::*;

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

        let round_trip: schemars::schema::RootSchema = serde_json::from_str(&schema).unwrap();
        assert_eq!(
            round_trip.meta_schema,
            Some("https://json-schema.org/draft/2020-12/schema".to_string())
        );
        assert_eq!(
            round_trip.schema.metadata.as_deref(),
            Some(&schemars::schema::Metadata {
                //title: Some("TestStruct".to_string()),
                id: Some("http://example.com/schema".to_string()),
                description: Some("Test documentation".to_string()),
                ..Default::default()
            })
        );
        assert_eq!(
            round_trip.schema.instance_type,
            Some(schemars::schema::SingleOrVec::from(
                schemars::schema::InstanceType::Object,
            ))
        );
    }
}
