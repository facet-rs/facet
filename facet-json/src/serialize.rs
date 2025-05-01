use facet_core::Facet;
use facet_reflect::Peek;
use facet_serialize::serialize_iterative;
use std::io::{self, Write};

use crate::json_serializer::JsonSerializer;

/// Serializes a value to JSON
pub fn to_string<'a, T: Facet<'a>>(value: &T) -> String {
    let peek = Peek::new(value);
    let mut output = Vec::new();
    let mut serializer = JsonSerializer::new(&mut output);
    serialize_iterative(peek, &mut serializer).unwrap();
    String::from_utf8(output).unwrap()
}

/// Serializes a Peek instance to JSON
pub fn peek_to_string(peek: &Peek<'_, '_>) -> String {
    let mut output = Vec::new();
    let mut serializer = JsonSerializer::new(&mut output);
    serialize_iterative(*peek, &mut serializer).unwrap();
    String::from_utf8(output).unwrap()
}

/// Serializes a value to a writer in JSON format
pub fn to_writer<'a, T: Facet<'a>, W: Write>(value: &T, writer: &mut W) -> io::Result<()> {
    let peek = Peek::new(value);
    let mut serializer = JsonSerializer::new(writer);
    serialize_iterative(peek, &mut serializer)
}

/// Serializes a Peek instance to a writer in JSON format
pub fn peek_to_writer<W: Write>(peek: &Peek<'_, '_>, writer: &mut W) -> io::Result<()> {
    let mut serializer = JsonSerializer::new(writer);
    serialize_iterative(*peek, &mut serializer)
}
