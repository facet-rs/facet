//! Create and/or write TOML strings from Rust values.

use core::fmt::Result;

#[cfg(feature = "alloc")]
use alloc::string::String;

use facet_core::{Def, Facet};
use facet_reflect::{Peek, ScalarType};
use toml_write::TomlWrite;

/// Implement writing TOML values to types that allow being written to.
pub trait TomlSerialize: TomlWrite {
    /// Serialize and write the TOML representation of the type referenced.
    fn toml<'a, T: Facet<'a>>(&mut self, value: &T) -> Result {
        let peek = Peek::new(value);
        self.toml_peek(&peek)
    }

    /// Serialize and write the TOML representation of the type held by the [`facet_reflect::Peek`] instance.
    fn toml_peek(&mut self, peek: &Peek<'_, '_>) -> Result {
        serialize(peek, self)
    }
}

impl<W> TomlSerialize for W where W: TomlWrite {}

/// Serializes a value to TOML.
#[cfg(feature = "alloc")]
pub fn to_string<'a, T: Facet<'a>>(value: &T) -> String {
    let mut output = String::new();

    output.toml(value).unwrap();

    output
}

/// Serializes a [`facet_reflect::Peek`] instance to TOML.
#[cfg(feature = "alloc")]
pub fn peek_to_string(peek: &Peek<'_, '_>) -> String {
    let mut output = String::new();

    output.toml_peek(peek).unwrap();

    output
}

/// Core serialization, can be called recursively.
fn serialize<W>(peek: &Peek<'_, '_>, writer: &mut W) -> Result
where
    W: TomlWrite + ?Sized,
{
    match peek.shape().def {
        Def::Scalar(_) => serialize_scalar(peek, writer),
        Def::Struct(_) => serialize_struct(peek, writer),
        Def::Enum(_) => todo!(),
        Def::Map(_) => todo!(),
        Def::List(_) => todo!(),
        Def::Array(_) => todo!(),
        Def::Slice(_) => todo!(),
        Def::Option(_) => todo!(),
        Def::SmartPointer(_) => todo!(),
        Def::FunctionPointer(_) => todo!(),
        _ => todo!(),
    }
}

/// Serialize a single scalar value.
fn serialize_scalar<W>(peek: &Peek<'_, '_>, writer: &mut W) -> Result
where
    W: TomlWrite + ?Sized,
{
    match peek.scalar_type() {
        Some(ScalarType::Bool) => {
            let value = peek.get::<bool>().unwrap();
            write!(writer, "{}", if *value { "true" } else { "false" })
        }
        #[cfg(feature = "alloc")]
        Some(ScalarType::String) => {
            let value = peek.get::<String>().unwrap();
            write!(writer, "\"{}\"", value)
        }
        Some(ScalarType::U64) => {
            let value = peek.get::<u64>().unwrap();
            write!(writer, "{}", value)
        }
        Some(other) => todo!("Unimplemented scalar type {other:?}"),
        None => unreachable!(),
    }
}

/// Serialize a Rust struct.
fn serialize_struct<W>(peek: &Peek<'_, '_>, writer: &mut W) -> Result
where
    W: TomlWrite + ?Sized,
{
    let struct_peek = peek.into_struct().unwrap();

    for (field, field_peek) in struct_peek.fields_for_serialize() {
        writer.key(field.name)?;
        writer.space()?;
        writer.keyval_sep()?;
        writer.space()?;
        serialize(&field_peek, writer)?;
        writer.newline()?;
    }

    Ok(())
}
