#![deny(unsafe_code)]
// Note: streaming.rs uses limited unsafe for lifetime extension in YieldingReader

//! XML parser that implements `FormatParser` for the codex prototype.
//!
//! This uses quick-xml for the underlying XML parsing and translates its
//! events into the format-agnostic ParseEvent stream.

mod parser;
mod serializer;

#[cfg(feature = "streaming")]
mod streaming;

pub use parser::{XmlError, XmlParser};
pub use serializer::{XmlSerializeError, XmlSerializer, to_vec};

#[cfg(all(feature = "streaming", feature = "std"))]
pub use streaming::from_reader;

#[cfg(feature = "tokio")]
pub use streaming::from_async_reader_tokio;
