#![cfg_attr(not(feature = "std"), no_std)]
#![warn(missing_docs)]
#![warn(clippy::std_instead_of_core)]
#![warn(clippy::std_instead_of_alloc)]
#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]

#[cfg(feature = "std")]
use std::io::{self, Write};

pub use facet_deserialize::{DeserError, DeserErrorKind, DeserErrorMessage};

extern crate alloc;

mod deserialize;
pub use deserialize::*;

mod serialize;
pub use serialize::*;

mod tokenizer;

/// The JSON format
struct Json;

/// Properly escapes and writes a JSON string
#[cfg(feature = "std")]
#[inline]
fn write_json_string<W: Write>(writer: &mut W, s: &str) -> io::Result<()> {
    writer.write_all(b"\"")?;

    let mut idx = 0;
    while idx + 8 < s.len() {
        let chunk = &s[idx..idx + 8];
        let window = <[u8; 8]>::try_from(chunk.as_bytes()).unwrap();
        let bignum = u64::from_le_bytes(window);
        let completely_ascii = bignum & 0x8080808080808080u64 == 0;
        if completely_ascii {
            writer.write_all(chunk.as_bytes())?;
            idx += 8;
        } else {
            let mut chars = chunk[idx..].chars();
            for c in (&mut chars).take(8) {
                write_json_escaped_char(writer, c)?;
            }
            let bytes_consumed = chars.as_str().as_ptr() as usize - s.as_ptr() as usize;
            idx += bytes_consumed;
        }
    }

    for c in s[idx..].chars() {
        write_json_escaped_char(writer, c)?;
    }

    writer.write_all(b"\"")
}

/// Writes a single JSON escaped character
#[cfg(feature = "std")]
#[inline]
fn write_json_escaped_char<W: Write>(writer: &mut W, c: char) -> io::Result<()> {
    match c {
        '"' => writer.write_all(b"\\\""),
        '\\' => writer.write_all(b"\\\\"),
        '\n' => writer.write_all(b"\\n"),
        '\r' => writer.write_all(b"\\r"),
        '\t' => writer.write_all(b"\\t"),
        '\u{08}' => writer.write_all(b"\\b"),
        '\u{0C}' => writer.write_all(b"\\f"),
        c if c.is_ascii_control() => {
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
        c if c.is_ascii() => {
            writer.write(&[c as u8])?;
            Ok(())
        }
        c => {
            let mut buf = [0; 4];
            let len = c.encode_utf8(&mut buf).len();
            writer.write_all(&buf[..len])
        }
    }
}
