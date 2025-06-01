#![no_std]
#![warn(missing_docs)]
#![warn(clippy::std_instead_of_core)]
#![warn(clippy::std_instead_of_alloc)]
#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]

extern crate alloc;

use alloc::vec::Vec;
pub use facet_deserialize::{DeserError, DeserErrorKind, DeserErrorMessage};

mod deserialize;
pub use deserialize::*;

mod serialize;
pub use serialize::*;

mod tokenizer;

/// The JSON format
struct Json;

/// `no_std` compatible Write trait used by the json serializer.
pub trait JsonWrite {
    /// Write all these bytes to the writer.
    fn write(&mut self, buf: &[u8]);

    /// If the writer supports it, reserve space for `len` additional bytes.
    fn reserve(&mut self, additional: usize);
}

impl JsonWrite for &mut Vec<u8> {
    fn write(&mut self, buf: &[u8]) {
        self.extend(buf);
    }

    fn reserve(&mut self, additional: usize) {
        Vec::reserve(self, additional)
    }
}

impl JsonWrite for Vec<u8> {
    fn write(&mut self, buf: &[u8]) {
        self.extend(buf);
    }

    fn reserve(&mut self, additional: usize) {
        Vec::reserve(self, additional)
    }
}

/// Properly escapes and writes a JSON string
#[inline]
fn write_json_string<W: JsonWrite>(writer: &mut W, s: &str) {
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

    const STEP_SIZE: usize = Window::BITS as usize / 8;
    type Window = u128;
    type Chunk = [u8; STEP_SIZE];

    writer.write(b"\"");

    let mut s = s;
    while let Some(Ok(chunk)) = s.as_bytes().get(..STEP_SIZE).map(Chunk::try_from) {
        let window = Window::from_ne_bytes(chunk);
        // Our window is a concatenation of u8 values. For each value, we need to make sure that:
        // 1. It is ASCII (i.e. the first bit of the u8 is 0, so u8 & 0x80 == 0)
        // 2. It does not contain quotes (i.e. 0x22)
        // 3. It does not contain backslashes (i.e. 0x5c)
        // 4. It does not contain control characters (i.e. characters below 32, including 0)
        //    This means the bit above the 1st, 2nd or 3rd bit must be set, so u8 & 0xe0 != 0
        let completely_ascii = window & 0x80808080808080808080808080808080 == 0;
        let quote_free = !contains_0x22(window);
        let backslash_free = !contains_0x5c(window);
        let control_char_free = top_three_bits_set(window);
        if completely_ascii && quote_free && backslash_free && control_char_free {
            // Yay! Whack it into the writer!
            writer.write(&chunk);
            s = &s[STEP_SIZE..];
        } else {
            // Ahw one of the conditions not met. Let's take our time and artisanally handle each
            // character.
            let mut chars = s.chars();
            let mut count = STEP_SIZE;
            for c in &mut chars {
                write_json_escaped_char(writer, c);
                count = count.saturating_sub(c.len_utf8());
                if count == 0 {
                    // Done with our chunk
                    break;
                }
            }
            s = chars.as_str();
        }
    }

    // In our loop we checked that we were able to consume at least `STEP_SIZE` bytes every
    // iteration. That means there might be a small remnant at the end that we can handle in the
    // slow method.
    for c in s.chars() {
        write_json_escaped_char(writer, c);
    }

    writer.write(b"\"")
}

/// Writes a single JSON escaped character
#[inline]
fn write_json_escaped_char<W: JsonWrite>(writer: &mut W, c: char) {
    match c {
        '"' => writer.write(b"\\\""),
        '\\' => writer.write(b"\\\\"),
        '\n' => writer.write(b"\\n"),
        '\r' => writer.write(b"\\r"),
        '\t' => writer.write(b"\\t"),
        '\u{08}' => writer.write(b"\\b"),
        '\u{0C}' => writer.write(b"\\f"),
        c if c.is_ascii_control() => {
            let bytes = (c as u32).to_be_bytes();
            // A radix 16 number (famously) fits in a u8, so unwrap here is safe.
            let to_hex = |d: u8| char::from_digit(d as u32, 16).unwrap() as u8;
            let buf = [
                b'\\',
                b'u',
                to_hex(bytes[0]),
                to_hex(bytes[1]),
                to_hex(bytes[2]),
                to_hex(bytes[3]),
            ];
            writer.write(&buf);
        }
        c if c.is_ascii() => {
            writer.write(&[c as u8]);
        }
        c => {
            let mut buf = [0; 4];
            let len = c.encode_utf8(&mut buf).len();
            writer.write(&buf[..len])
        }
    }
}

#[inline]
fn contains_0x22(val: u128) -> bool {
    let xor_result = val ^ 0x22222222222222222222222222222222;
    let has_zero = (xor_result.wrapping_sub(0x01010101010101010101010101010101))
        & !xor_result
        & 0x80808080808080808080808080808080;
    has_zero != 0
}

#[inline]
fn contains_0x5c(val: u128) -> bool {
    let xor_result = val ^ 0x5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c;
    let has_zero = (xor_result.wrapping_sub(0x01010101010101010101010101010101))
        & !xor_result
        & 0x80808080808080808080808080808080;
    has_zero != 0
}

/// For each of the 16 u8s that make up a u128, check if the top three bits are set.
#[inline]
fn top_three_bits_set(value: u128) -> bool {
    let xor_result = value & 0xe0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0;
    let has_zero = (xor_result.wrapping_sub(0x01010101010101010101010101010101))
        & !xor_result
        & 0x80808080808080808080808080808080;
    has_zero == 0
}
