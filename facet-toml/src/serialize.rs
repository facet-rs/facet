//! Create and/or write TOML strings from Rust values.

use alloc::format;
#[cfg(feature = "alloc")]
use alloc::string::String;
use core::fmt::{Display, Error, Write};

#[cfg(feature = "alloc")]
use facet_serialize::Serialize;
use facet_serialize::Serializer;
use toml_write::TomlWrite;

/// Serializer for TOML values.
pub struct TomlSerializer<'a, W: Write> {
    /// Where to write the output to.
    writer: &'a mut W,
    /// What we are writing right now.
    writing: Writing,
}

impl<'a, W: TomlWrite> TomlSerializer<'a, W> {
    /// Create a new serialzer.
    pub fn new(writer: &'a mut W) -> Self {
        let writing = Writing::Root;

        Self { writer, writing }
    }

    /// Write a value depending on the context.
    fn write_value(&mut self, value: impl Display) -> Result<(), Error> {
        write!(self.writer, "{value}")?;

        match self.writing {
            Writing::Root | Writing::Table => self.writer.newline(),
        }
    }
}

impl<W: TomlWrite> Serializer for TomlSerializer<'_, W> {
    type Error = Error;

    fn serialize_u8(&mut self, value: u8) -> Result<(), Self::Error> {
        self.write_value(value)
    }

    fn serialize_u16(&mut self, value: u16) -> Result<(), Self::Error> {
        self.write_value(value)
    }

    fn serialize_u32(&mut self, value: u32) -> Result<(), Self::Error> {
        self.write_value(value)
    }

    fn serialize_u64(&mut self, value: u64) -> Result<(), Self::Error> {
        self.write_value(value)
    }

    fn serialize_u128(&mut self, value: u128) -> Result<(), Self::Error> {
        self.write_value(value)
    }

    fn serialize_usize(&mut self, value: usize) -> Result<(), Self::Error> {
        self.write_value(value)
    }

    fn serialize_i8(&mut self, value: i8) -> Result<(), Self::Error> {
        self.write_value(value)
    }

    fn serialize_i16(&mut self, value: i16) -> Result<(), Self::Error> {
        self.write_value(value)
    }

    fn serialize_i32(&mut self, value: i32) -> Result<(), Self::Error> {
        self.write_value(value)
    }

    fn serialize_i64(&mut self, value: i64) -> Result<(), Self::Error> {
        self.write_value(value)
    }

    fn serialize_i128(&mut self, value: i128) -> Result<(), Self::Error> {
        self.write_value(value)
    }

    fn serialize_isize(&mut self, value: isize) -> Result<(), Self::Error> {
        self.write_value(value)
    }

    fn serialize_f32(&mut self, value: f32) -> Result<(), Self::Error> {
        self.write_value(value)
    }

    fn serialize_f64(&mut self, value: f64) -> Result<(), Self::Error> {
        self.write_value(value)
    }

    fn serialize_bool(&mut self, value: bool) -> Result<(), Self::Error> {
        self.write_value(if value { "true" } else { "false" })
    }

    fn serialize_char(&mut self, value: char) -> Result<(), Self::Error> {
        // TODO: improve performance of this
        self.write_value(format!("\"{value}\""))
    }

    fn serialize_str(&mut self, value: &str) -> Result<(), Self::Error> {
        // TODO: improve performance of this
        self.write_value(format!("\"{value}\""))
    }

    fn serialize_bytes(&mut self, _value: &[u8]) -> Result<(), Self::Error> {
        todo!()
    }

    fn serialize_none(&mut self) -> Result<(), Self::Error> {
        todo!()
    }

    fn serialize_unit(&mut self) -> Result<(), Self::Error> {
        todo!()
    }

    fn serialize_unit_variant(
        &mut self,
        _variant_index: usize,
        _variant_name: &'static str,
    ) -> Result<(), Self::Error> {
        todo!()
    }

    fn start_object(&mut self, _len: Option<usize>) -> Result<(), Self::Error> {
        // TODO: define this based on something, attributes?
        self.writing = Writing::Table;

        Ok(())
    }

    fn end_object(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn start_array(&mut self, _len: Option<usize>) -> Result<(), Self::Error> {
        todo!()
    }

    fn end_array(&mut self) -> Result<(), Self::Error> {
        todo!()
    }

    fn start_map(&mut self, _len: Option<usize>) -> Result<(), Self::Error> {
        todo!()
    }

    fn end_map(&mut self) -> Result<(), Self::Error> {
        todo!()
    }

    fn serialize_field_name(&mut self, name: &'static str) -> Result<(), Self::Error> {
        self.writer.key(name)?;
        self.writer.space()?;
        self.writer.keyval_sep()?;
        self.writer.space()
    }
}

/// What we are writing right now.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Writing {
    /// Root of the document.
    Root,
    /// Regular table.
    Table,
}

/// Serialize any [`facet::Facet`] type to a TOML string.
#[cfg(feature = "alloc")]
pub fn to_string<'a, T: facet_core::Facet<'a>>(value: &'a T) -> Result<String, Error> {
    let mut output = String::new();

    let mut serializer = TomlSerializer::new(&mut output);
    value.serialize(&mut serializer)?;

    Ok(output)
}
