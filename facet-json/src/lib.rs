#![cfg_attr(not(feature = "std"), no_std)]
#![warn(missing_docs)]
#![warn(clippy::std_instead_of_core)]
#![warn(clippy::std_instead_of_alloc)]
#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]

use std::io::{self, Write};

use facet_core::Facet;
use facet_deserialize::DeserError;
use facet_reflect::Peek;

extern crate alloc;

#[cfg(feature = "std")]
mod iterative;
#[cfg(feature = "std")]
mod recursive;
mod tokenizer;

const MAX_RECURSION_DEPTH: usize = 100;

/// Deserialize JSON from a given byte slice
pub fn from_slice<'input: 'facet, 'facet, T: Facet<'facet>>(
    input: &'input [u8],
) -> Result<T, DeserError<'input>> {
    recursive::from_slice(input, 0)
}

/// Deserialize JSON from a given string
pub fn from_str<'input: 'facet, 'facet, T: Facet<'facet>>(
    input: &'input str,
) -> Result<T, DeserError<'input>> {
    recursive::from_str(input, 0)
}

/// Deserialize JSON from a given string, converting any dynamic error into a static one.
///
/// This function attempts to deserialize a type `T` implementing `Facet` from the input string slice.
/// If deserialization fails, the error is converted into an owned, static error type to avoid lifetime issues.
pub fn from_str_static_error<'input: 'facet, 'facet, T: Facet<'facet>>(
    input: &'input str,
) -> Result<T, DeserError<'input>> {
    recursive::from_str_static_error(input, 0)
}

/// Serializes a value to JSON
pub fn to_string<'a, T: Facet<'a>>(value: &'a T) -> String {
    recursive::to_string(value, 0)
}

/// Serializes a Peek instance to JSON
pub fn peek_to_string<'a>(peek: Peek<'a, 'a>) -> String {
    recursive::peek_to_string(peek, 0)
}

/// Serializes a value to a writer in JSON format
pub fn to_writer<'mem: 'facet, 'facet, T: Facet<'facet>, W: Write>(
    value: &'mem T,
    writer: &mut W,
) -> io::Result<()> {
    recursive::to_writer(value, writer)
}

/// Serializes a Peek instance to a writer in JSON format
pub fn peek_to_writer<'mem: 'facet, 'facet, W: Write>(
    peek: Peek<'mem, 'facet>,
    writer: &mut W,
) -> io::Result<()> {
    recursive::peek_to_writer(peek, None, 0, writer)
}

/// The JSON format
struct Json;

/// Properly escapes and writes a JSON string
fn write_json_string<W: Write>(writer: &mut W, s: &str) -> io::Result<()> {
    writer.write_all(b"\"")?;

    for c in s.chars() {
        write_json_escaped_char(writer, c)?;
    }

    writer.write_all(b"\"")
}

/// Writes a single JSON escaped character
fn write_json_escaped_char<W: Write>(writer: &mut W, c: char) -> io::Result<()> {
    match c {
        '"' => writer.write_all(b"\\\""),
        '\\' => writer.write_all(b"\\\\"),
        '\n' => writer.write_all(b"\\n"),
        '\r' => writer.write_all(b"\\r"),
        '\t' => writer.write_all(b"\\t"),
        '\u{08}' => writer.write_all(b"\\b"),
        '\u{0C}' => writer.write_all(b"\\f"),
        c if c.is_control() => {
            let mut buf = [0; 6];
            let s = format!("{:04x}", c as u32);
            buf[0] = b'\\';
            buf[1] = b'u';
            buf[2] = s.as_bytes()[0];
            buf[3] = s.as_bytes()[1];
            buf[4] = s.as_bytes()[2];
            buf[5] = s.as_bytes()[3];
            writer.write_all(&buf)
        }
        c => {
            let mut buf = [0; 4];
            let len = c.encode_utf8(&mut buf).len();
            writer.write_all(&buf[..len])
        }
    }
}

fn variant_is_newtype_like(variant: &facet_core::Variant) -> bool {
    variant.data.kind == facet_core::StructKind::Tuple && variant.data.fields.len() == 1
}
