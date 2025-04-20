#![warn(missing_docs)]
#![warn(clippy::std_instead_of_core)]
#![warn(clippy::std_instead_of_alloc)]
#![deny(unsafe_code)]
#![doc = include_str!("../README.md")]

use facet::{Def, Facet, ScalarDef, Shape};

use std::io::Write;

/// Convert a `Facet` type to a JSON schema string.
pub fn to_string<T: Facet>() -> String {
    let mut buffer = Vec::new();
    write!(buffer, "{{").unwrap();
    write!(
        buffer,
        "\"$schema\": \"https://json-schema.org/draft/2020-12/schema\","
    )
    .unwrap();
    //TODO: if `Shape` also allowed arbitrary attributes, we could optionally output an `id` field here.
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
        #[derive(
            Facet,
            //schemars::JsonSchema
        )]
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
        println!("JSON Schema: {schema}");

        //let x: RootSchema = serde_json::from_str(&schema).unwrap();
        //dbg!(x);
        //let x = schemars::schema_for!(TestStruct);
        //println!("{}", serde_json::to_string_pretty(&x).unwrap());
    }

    #[test]
    fn test_option() {
        #[derive(
            Facet,
            //schemars::JsonSchema
        )]
        struct Foo {
            bar: u32,
        }

        #[derive(
            Facet,
            //schemars::JsonSchema
        )]
        struct TestOption {
            string_field: Option<String>,
            struct_field: Option<Foo>,
        }

        //let schema = to_string::<TestOption>();
        //println!("JSON Schema: {schema}");

        //let x: RootSchema = serde_json::from_str(&schema).unwrap();
        //dbg!(x);
        //let x = schemars::schema_for!(TestOption);
        //println!("{}", serde_json::to_string_pretty(&x).unwrap());
    }

    /*
    #[test]
    fn test_enum() {
        #[derive(
            Facet,
            //schemars::JsonSchema
        )]
        #[repr(u8)]
        enum TestEnum {
            /// Test doc1
            StringField(String),
            /// Test doc2
            IntField(u32),
            VecField(Vec<bool>),
            SliceField(&'static [f64]),
            ArrayField([f64; 3]),
        }

        //let schema = to_string::<TestEnum>();
        //println!("JSON Schema: {schema}");
        //let x: RootSchema = serde_json::from_str(&schema).unwrap();
        //dbg!(x);
        //let x = schemars::schema_for!(TestEnum);
        //println!("{}", serde_json::to_string_pretty(&x).unwrap());
    }
    */
}
