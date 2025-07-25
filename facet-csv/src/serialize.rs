use facet_core::Facet;
use facet_reflect::Peek;
use facet_serialize::{Serializer, serialize_iterative};
use std::io::{self, Write};

/// Serializes a value to CSV
pub fn to_string<'facet, T: Facet<'facet>>(value: &'facet T) -> String {
    let peek = Peek::new(value);
    let mut output = Vec::new();
    let mut serializer = CsvSerializer::new(&mut output);
    serialize_iterative(peek, &mut serializer).unwrap();
    String::from_utf8(output).unwrap()
}

/// Serializes a Peek instance to CSV
pub fn peek_to_string<'facet>(peek: Peek<'_, 'facet>) -> String {
    let mut output = Vec::new();
    let mut serializer = CsvSerializer::new(&mut output);
    serialize_iterative(peek, &mut serializer).unwrap();
    String::from_utf8(output).unwrap()
}

/// Serializes a value to a writer in CSV format
pub fn to_writer<'a, T: Facet<'a>, W: Write>(value: &'a T, writer: &mut W) -> io::Result<()> {
    let peek = Peek::new(value);
    let mut serializer = CsvSerializer::new(writer);
    serialize_iterative(peek, &mut serializer)
}

/// Serializes a Peek instance to a writer in CSV format
pub fn peek_to_writer<'facet, W: Write>(peek: Peek<'_, 'facet>, writer: &mut W) -> io::Result<()> {
    let mut serializer = CsvSerializer::new(writer);
    serialize_iterative(peek, &mut serializer)
}

/// A struct to handle the CSV serializer logic
pub struct CsvSerializer<W> {
    /// Owned writer
    writer: W,

    /// The current position in a row
    pos: usize,

    /// Initialized by `start_object`
    n_fields: usize,

    /// Delimeter used to separate values
    delim: &'static [u8],

    /// Newline encoding
    newline: &'static [u8],
}
impl<W> CsvSerializer<W>
where
    W: Write,
{
    /// Initializes a new CSV Serializer
    pub fn new(writer: W) -> Self {
        Self {
            writer,
            pos: 0,
            n_fields: 0,
            delim: b",",
            newline: b"\n",
        }
    }

    fn set_n_fields(&mut self, n_fields: usize) {
        self.n_fields = n_fields;
    }

    /// Conditionally prefix the value with the required delimeter
    fn start_value(&mut self) -> Result<(), io::Error> {
        if self.pos == 0 {
            // no prefix
            Ok(())
        } else {
            self.writer.write_all(self.delim)
        }
    }

    /// Conditionally suffix the value with the required newline
    fn end_value(&mut self) -> Result<(), io::Error> {
        if self.pos == self.n_fields - 1 {
            // Reset the position to zero
            self.pos = 0;
            self.writer.write_all(self.newline)
        } else {
            // Increment the position
            self.pos += 1;
            // no suffix
            Ok(())
        }
    }
}

impl<W> Serializer for CsvSerializer<W>
where
    W: Write,
{
    type Error = io::Error;

    fn start_object(&mut self, len: Option<usize>) -> Result<(), Self::Error> {
        self.set_n_fields(len.expect("Must know the length of the object for CSV"));
        Ok(())
    }

    fn end_object(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn start_array(&mut self, _len: Option<usize>) -> Result<(), Self::Error> {
        unimplemented!("Arrays are not implemented yet in CSV")
    }

    fn end_array(&mut self) -> Result<(), Self::Error> {
        unimplemented!("Arrays are not implemented yet in CSV")
    }

    fn start_map(&mut self, _len: Option<usize>) -> Result<(), Self::Error> {
        unimplemented!("Maps are not implemented yet in CSV")
    }

    fn end_map(&mut self) -> Result<(), Self::Error> {
        unimplemented!("Maps are not implemented yet in CSV")
    }

    fn serialize_field_name(&mut self, _name: &str) -> Result<(), Self::Error> {
        // field names are not serialized in CSV
        Ok(())
    }

    fn serialize_unit_variant(
        &mut self,
        _variant_index: usize,
        _variant_name: &str,
    ) -> Result<(), Self::Error> {
        // unit variants should not serialize to anything
        Ok(())
    }

    fn serialize_u8(&mut self, value: u8) -> Result<(), Self::Error> {
        self.start_value()?;
        write!(self.writer, "{value}")?;
        self.end_value()
    }

    fn serialize_u16(&mut self, value: u16) -> Result<(), Self::Error> {
        self.start_value()?;
        write!(self.writer, "{value}")?;
        self.end_value()
    }

    fn serialize_u32(&mut self, value: u32) -> Result<(), Self::Error> {
        self.start_value()?;
        write!(self.writer, "{value}")?;
        self.end_value()
    }

    fn serialize_u64(&mut self, value: u64) -> Result<(), Self::Error> {
        self.start_value()?;
        write!(self.writer, "{value}")?;
        self.end_value()
    }

    fn serialize_u128(&mut self, value: u128) -> Result<(), Self::Error> {
        self.start_value()?;
        write!(self.writer, "{value}")?;
        self.end_value()
    }

    fn serialize_usize(&mut self, value: usize) -> Result<(), Self::Error> {
        self.start_value()?;
        write!(self.writer, "{value}")?;
        self.end_value()
    }

    fn serialize_i8(&mut self, value: i8) -> Result<(), Self::Error> {
        self.start_value()?;
        write!(self.writer, "{value}")?;
        self.end_value()
    }

    fn serialize_i16(&mut self, value: i16) -> Result<(), Self::Error> {
        self.start_value()?;
        write!(self.writer, "{value}")?;
        self.end_value()
    }

    fn serialize_i32(&mut self, value: i32) -> Result<(), Self::Error> {
        self.start_value()?;
        write!(self.writer, "{value}")?;
        self.end_value()
    }

    fn serialize_i64(&mut self, value: i64) -> Result<(), Self::Error> {
        self.start_value()?;
        write!(self.writer, "{value}")?;
        self.end_value()
    }

    fn serialize_i128(&mut self, value: i128) -> Result<(), Self::Error> {
        self.start_value()?;
        write!(self.writer, "{value}")?;
        self.end_value()
    }

    fn serialize_isize(&mut self, value: isize) -> Result<(), Self::Error> {
        self.start_value()?;
        write!(self.writer, "{value}")?;
        self.end_value()
    }

    fn serialize_f32(&mut self, value: f32) -> Result<(), Self::Error> {
        self.start_value()?;
        write!(self.writer, "{value}")?;
        self.end_value()
    }

    fn serialize_f64(&mut self, value: f64) -> Result<(), Self::Error> {
        self.start_value()?;
        write!(self.writer, "{value}")?;
        self.end_value()
    }

    fn serialize_bool(&mut self, value: bool) -> Result<(), Self::Error> {
        self.start_value()?;
        write!(self.writer, "{}", if value { "true" } else { "false" })?;
        self.end_value()
    }

    fn serialize_char(&mut self, value: char) -> Result<(), Self::Error> {
        self.start_value()?;
        write!(self.writer, "{value}")?;
        self.end_value()
    }

    fn serialize_str(&mut self, value: &str) -> Result<(), Self::Error> {
        self.start_value()?;
        write!(self.writer, "{value}")?;
        self.end_value()
    }

    fn serialize_bytes(&mut self, _value: &[u8]) -> Result<(), Self::Error> {
        panic!("CSV does not support byte arrays")
    }

    fn serialize_none(&mut self) -> Result<(), Self::Error> {
        self.start_value()?;
        // skip empty columns
        self.end_value()
    }

    fn serialize_unit(&mut self) -> Result<(), Self::Error> {
        self.start_value()?;
        // skip empty columns
        self.end_value()
    }
}
