use facet_reflect::Peek;
use facet_serialize::{Serializer, serialize_iterative};
use log::debug;
use std::io::{self, Write};

/// Serializes a Peek instance to a writer in JSON format
pub(crate) fn peek_to_writer<'mem, 'facet, 'shape, W: Write>(
    peek: Peek<'mem, 'facet, 'shape>,
    writer: &mut W,
) -> io::Result<()> {
    let mut serializer = JsonSerializer::new(writer);
    serialize_iterative(peek, &mut serializer)
}

#[derive(Debug)]
enum StackItem {
    ArrayItem { first: bool },
    ObjectItem { object_state: ObjectItemState },
}

#[derive(Debug)]
enum ObjectItemState {
    FirstKey,
    Key,
    Value,
}

/// A serializer for JSON format that implements the `facet_serialize::Serializer` trait.
pub struct JsonSerializer<W> {
    writer: W,
    stack: Vec<StackItem>,
}

impl<W> JsonSerializer<W>
where
    W: Write,
{
    /// Creates a new JSON serializer with the given writer.
    pub fn new(writer: W) -> Self {
        Self {
            writer,
            stack: Vec::new(),
        }
    }

    fn start_value(&mut self) -> Result<(), io::Error> {
        debug!("start_value, stack = {:?}", self.stack);

        match self.stack.last_mut() {
            Some(StackItem::ArrayItem { first }) => {
                if *first {
                    *first = false;
                } else {
                    write!(self.writer, ",")?;
                }
            }
            Some(StackItem::ObjectItem { object_state }) => {
                debug!("ObjectItem: object_state = {:?}", object_state);
                match object_state {
                    ObjectItemState::FirstKey => {
                        *object_state = ObjectItemState::Value;
                    }
                    ObjectItemState::Key => {
                        write!(self.writer, ",")?;
                        *object_state = ObjectItemState::Value;
                    }
                    ObjectItemState::Value => {
                        write!(self.writer, ":")?;
                        *object_state = ObjectItemState::Key;
                    }
                }
            }
            None => {
                debug!("No stack frame (top-level value)");
            }
        }

        Ok(())
    }

    fn end_value(&mut self) -> Result<(), io::Error> {
        Ok(())
    }
}

impl<'shape, W> Serializer<'shape> for JsonSerializer<W>
where
    W: Write,
{
    type Error = io::Error;

    fn serialize_u8(&mut self, value: u8) -> Result<(), Self::Error> {
        self.start_value()?;
        write!(self.writer, "{}", value)?;
        self.end_value()
    }

    fn serialize_u16(&mut self, value: u16) -> Result<(), Self::Error> {
        self.start_value()?;
        write!(self.writer, "{}", value)?;
        self.end_value()
    }

    fn serialize_u32(&mut self, value: u32) -> Result<(), Self::Error> {
        self.start_value()?;
        write!(self.writer, "{}", value)?;
        self.end_value()
    }

    fn serialize_u64(&mut self, value: u64) -> Result<(), Self::Error> {
        self.start_value()?;
        write!(self.writer, "{}", value)?;
        self.end_value()
    }

    fn serialize_u128(&mut self, value: u128) -> Result<(), Self::Error> {
        self.start_value()?;
        write!(self.writer, "{}", value)?;
        self.end_value()
    }

    fn serialize_usize(&mut self, value: usize) -> Result<(), Self::Error> {
        self.start_value()?;
        write!(self.writer, "{}", value)?;
        self.end_value()
    }

    fn serialize_i8(&mut self, value: i8) -> Result<(), Self::Error> {
        self.start_value()?;
        write!(self.writer, "{}", value)?;
        self.end_value()
    }

    fn serialize_i16(&mut self, value: i16) -> Result<(), Self::Error> {
        self.start_value()?;
        write!(self.writer, "{}", value)?;
        self.end_value()
    }

    fn serialize_i32(&mut self, value: i32) -> Result<(), Self::Error> {
        self.start_value()?;
        write!(self.writer, "{}", value)?;
        self.end_value()
    }

    fn serialize_i64(&mut self, value: i64) -> Result<(), Self::Error> {
        self.start_value()?;
        write!(self.writer, "{}", value)?;
        self.end_value()
    }

    fn serialize_i128(&mut self, value: i128) -> Result<(), Self::Error> {
        self.start_value()?;
        write!(self.writer, "{}", value)?;
        self.end_value()
    }

    fn serialize_isize(&mut self, value: isize) -> Result<(), Self::Error> {
        self.start_value()?;
        write!(self.writer, "{}", value)?;
        self.end_value()
    }

    fn serialize_f32(&mut self, value: f32) -> Result<(), Self::Error> {
        self.start_value()?;
        write!(self.writer, "{}", value)?;
        self.end_value()
    }

    fn serialize_f64(&mut self, value: f64) -> Result<(), Self::Error> {
        self.start_value()?;
        write!(self.writer, "{}", value)?;
        self.end_value()
    }

    fn serialize_bool(&mut self, value: bool) -> Result<(), Self::Error> {
        self.start_value()?;
        write!(self.writer, "{}", if value { "true" } else { "false" })?;
        self.end_value()
    }

    fn serialize_char(&mut self, value: char) -> Result<(), Self::Error> {
        self.start_value()?;
        self.writer.write_all(b"\"")?;
        crate::write_json_escaped_char(&mut self.writer, value)?;
        self.writer.write_all(b"\"")?;
        self.end_value()
    }

    fn serialize_str(&mut self, value: &str) -> Result<(), Self::Error> {
        self.start_value()?;
        crate::write_json_string(&mut self.writer, value)?;
        self.end_value()
    }

    fn serialize_bytes(&mut self, _value: &[u8]) -> Result<(), Self::Error> {
        panic!("JSON does not support byte arrays")
    }

    fn serialize_none(&mut self) -> Result<(), Self::Error> {
        self.start_value()?;
        self.writer.write_all(b"null")?;
        self.end_value()
    }

    fn serialize_unit(&mut self) -> Result<(), Self::Error> {
        self.start_value()?;
        self.writer.write_all(b"null")?;
        self.end_value()
    }

    fn serialize_unit_variant(
        &mut self,
        _variant_index: usize,
        variant_name: &'shape str,
    ) -> Result<(), Self::Error> {
        self.start_value()?;
        crate::write_json_string(&mut self.writer, variant_name)?;
        self.end_value()
    }

    fn start_object(&mut self, _len: Option<usize>) -> Result<(), Self::Error> {
        self.start_value()?;
        self.writer.write_all(b"{")?;
        self.stack.push(StackItem::ObjectItem {
            object_state: ObjectItemState::FirstKey,
        });
        Ok(())
    }

    fn end_object(&mut self) -> Result<(), Self::Error> {
        let object = self.stack.pop().unwrap();
        match object {
            StackItem::ArrayItem { .. } => unreachable!(),
            StackItem::ObjectItem { object_state } => match object_state {
                ObjectItemState::FirstKey | ObjectItemState::Key => {
                    // good
                }
                ObjectItemState::Value => unreachable!(),
            },
        }
        self.writer.write_all(b"}")?;
        self.end_value()?;
        Ok(())
    }

    fn start_array(&mut self, _len: Option<usize>) -> Result<(), Self::Error> {
        self.start_value()?;
        self.writer.write_all(b"[")?;
        self.stack.push(StackItem::ArrayItem { first: true });
        Ok(())
    }

    fn end_array(&mut self) -> Result<(), Self::Error> {
        let item = self.stack.pop().unwrap();
        match item {
            StackItem::ArrayItem { .. } => {
                // good
            }
            StackItem::ObjectItem { .. } => unreachable!(),
        }
        self.writer.write_all(b"]")?;
        self.end_value()?;
        Ok(())
    }

    fn start_map(&mut self, _len: Option<usize>) -> Result<(), Self::Error> {
        self.start_object(_len)
    }

    fn end_map(&mut self) -> Result<(), Self::Error> {
        self.end_object()
    }

    fn serialize_field_name(&mut self, name: &'shape str) -> Result<(), Self::Error> {
        // Handle object key comma logic
        if let Some(StackItem::ObjectItem { object_state }) = self.stack.last_mut() {
            match object_state {
                ObjectItemState::FirstKey => {
                    *object_state = ObjectItemState::Key;
                }
                ObjectItemState::Key => {
                    self.writer.write_all(b",")?;
                }
                ObjectItemState::Value => unreachable!(),
            }
        }
        crate::write_json_string(&mut self.writer, name)?;
        if let Some(StackItem::ObjectItem { object_state }) = self.stack.last_mut() {
            *object_state = ObjectItemState::Value;
        }
        Ok(())
    }
}
