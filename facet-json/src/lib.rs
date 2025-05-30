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
    // Just a little bit of text on how it works. There are two main steps:
    // 1. Check if the string is completely ASCII and doesn't contain any quotes or backslashes or
    //    control characters. This is the fast path, because it means that the bytes can be written
    //    as they are, without any escaping needed. In this case we go over the string in windows
    //    of 16 bytes (which is completely arbitrary, maybe find some real world data to tune this
    //    with? I don't know and you don't have to do this dear reader.) and we just feed them into
    //    the writer.
    // 2. If the string is not completely ASCII or contains quotes or backslashes or control
    //    characters, we need to escape them. This is the slow path, because it means that we need
    //    to write the bytes one by one, and we need to figure out where to put the escapes. So we
    //    just call `write_json_escaped_char` for each character.

    const STEP_SIZE: usize = 16;
    type BigNum = u128;
    type Chunk = [u8; STEP_SIZE];

    writer.write_all(b"\"")?;

    let mut idx = 0;
    while idx + STEP_SIZE < s.len() {
        let chunk = &s[idx..idx + STEP_SIZE];
        // Unwrap here is fine because the chunk is guaranteed to be exactly `CHUNK_SIZE` bytes long
        // by construction.
        let window = Chunk::try_from(chunk.as_bytes()).unwrap();
        #[cfg(target_endian = "little")]
        let bignum = BigNum::from_le_bytes(window);
        #[cfg(target_endian = "big")]
        let bignum = BigNum::from_be_bytes(window);
        // Our bignum is a concatenation of u8 values. For each value, we need to make sure that:
        // 1. It is ASCII (i.e. the first bit of the u8 is 0, so u8 & 0x80 == 0)
        // 2. It does not contain quotes (i.e. u8 & 0x22 != 0)
        // 3. It does not contain backslashes (i.e. u8 & 0x5c != 0)
        // 4. It does not contain control characters (i.e. characters below 32, including 0)
        //    This means the bit above the 1st, 2nd or 3rd bit must be set, so u8 & 0xe0 != 0
        let completely_ascii = bignum & 0x80808080808080808080808080808080u128 == 0;
        let quote_free = bignum & 0x22222222222222222222222222222222u128 == 0;
        let backslash_free = bignum & 0x5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5cu128 == 0;
        let control_char_free = bignum & 0xe0e0e0e0e0e0e0e0e0e0e0e0e0e0e0eu128 != 0;
        if completely_ascii && quote_free && backslash_free && control_char_free {
            // Yay! Whack it into the writer!
            writer.write_all(chunk.as_bytes())?;
            idx += STEP_SIZE;
        } else {
            // Ahw one of the conditions not met. Let's take our time and artisanally handle each
            // character.
            let mut chars = s[idx..].chars();
            for c in (&mut chars).take(STEP_SIZE) {
                write_json_escaped_char(writer, c)?;
            }
            let bits_consumed = chars.as_str().as_ptr() as usize - s.as_ptr() as usize;
            idx += bits_consumed / 8;
        }
    }

    // In our loop we checked that we were able to consume at least `STEP_SIZE` bytes every
    // iteration. That means there might be a small remnant at the end that we can handle in the
    // slow method.
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
