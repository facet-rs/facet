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
    const STEP_SIZE: usize = 16;
    type BigNum = u128;
    type Chunk = [u8; STEP_SIZE];

    writer.write_all(b"\"")?;

    let mut idx = 0;
    while idx + STEP_SIZE < s.len() {
        let chunk = &s[idx..idx + STEP_SIZE];
        let window = Chunk::try_from(chunk.as_bytes()).unwrap();
        let bignum = BigNum::from_le_bytes(window);
        let completely_ascii = bignum & 0x80808080808080808080808080808080u128 == 0;
        let contains_quotes = bignum & 0x22222222222222222222222222222222u128 != 0;
        let contains_backslash = bignum & 0x5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5cu128 != 0;
        // Here we check if any of the u8 comprising bignum consist of numbers below 32.
        let contains_control_chars = bignum & 0xe0e0e0e0e0e0e0e0e0e0e0e0e0e0e0eu128 != 0;
        if completely_ascii && !contains_quotes && !contains_backslash && !contains_control_chars {
            writer.write_all(chunk.as_bytes())?;
            idx += STEP_SIZE;
        } else {
            let mut chars = s[idx..].chars();
            for c in (&mut chars).take(STEP_SIZE) {
                write_json_escaped_char(writer, c)?;
            }
            let bits_consumed = chars.as_str().as_ptr() as usize - s.as_ptr() as usize;
            idx += bits_consumed / 8;
        }
    }

    if idx < s.len() {
        for c in s[idx..].chars() {
            write_json_escaped_char(writer, c)?;
        }
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
