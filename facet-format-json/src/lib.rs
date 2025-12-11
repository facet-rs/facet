#![forbid(unsafe_code)]

//! Minimal JSON parser that implements `FormatParser` for the codex prototype.

mod parser;
mod serializer;

pub use parser::{JsonError, JsonParser};
pub use serializer::{JsonSerializeError, JsonSerializer, to_vec};
