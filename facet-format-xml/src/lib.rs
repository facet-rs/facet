#![forbid(unsafe_code)]

//! XML parser that implements `FormatParser` for the codex prototype.
//!
//! This uses quick-xml for the underlying XML parsing and translates its
//! events into the format-agnostic ParseEvent stream.

mod parser;
mod serializer;

pub use parser::{XmlError, XmlParser};
pub use serializer::{XmlSerializeError, XmlSerializer, to_vec};
