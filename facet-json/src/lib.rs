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
    let input = input.as_bytes();
    facet_deserialize::deserialize(input, Json).map_err(|e| e.into_owned())
}

/// Serializes a value to JSON
pub fn to_string<'a, T: Facet<'a>>(value: &'a T) -> String {
    recursive::to_string(value, 0)
}

/// Serializes a Peek instance to JSON
pub fn peek_to_string<'a>(peek: &'a Peek<'a, 'a>) -> String {
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
    peek: &Peek<'mem, 'facet>,
    writer: &mut W,
) -> io::Result<()> {
    recursive::peek_to_writer(peek, writer)
}

/// The JSON format
struct Json;
