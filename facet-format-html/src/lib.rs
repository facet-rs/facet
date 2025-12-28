#![deny(unsafe_code)]
#![deny(missing_docs, rustdoc::broken_intra_doc_links)]

//! HTML parser that implements `FormatParser` for the facet format architecture.
//!
//! This uses html5gum for WHATWG-compliant HTML tokenization and translates its
//! tokens into the format-agnostic ParseEvent stream.
//!
//! # Example
//!
//! ```rust
//! use facet::Facet;
//! use facet_format::FormatDeserializer;
//! use facet_format_html::HtmlParser;
//!
//! #[derive(Debug, Facet, PartialEq)]
//! struct Document {
//!     #[facet(default)]
//!     head: Option<Head>,
//!     #[facet(default)]
//!     body: Option<Body>,
//! }
//!
//! #[derive(Debug, Facet, PartialEq)]
//! struct Head {
//!     #[facet(default)]
//!     title: Option<String>,
//! }
//!
//! #[derive(Debug, Facet, PartialEq)]
//! struct Body {
//!     #[facet(default)]
//!     text: String,
//! }
//! ```
//!
//! # Pre-defined HTML Element Types
//!
//! This crate provides typed definitions for all standard HTML5 elements in the
//! [`elements`] module. You can use these to deserialize HTML into strongly-typed
//! Rust structures:
//!
//! ```rust
//! use facet_format_html::elements::{Html, Div, P, A};
//! ```

pub mod elements;
mod parser;

pub use parser::{HtmlError, HtmlParser};

/// Deserialize an HTML document from a string.
///
/// # Example
///
/// ```rust
/// use facet::Facet;
///
/// #[derive(Debug, Facet)]
/// struct Simple {
///     #[facet(default)]
///     text: String,
/// }
///
/// let doc: Simple = facet_format_html::from_str("<div>hello</div>").unwrap();
/// ```
pub fn from_str<'de, T: facet_core::Facet<'de>>(
    s: &'de str,
) -> Result<T, facet_format::DeserializeError<HtmlError>> {
    let parser = HtmlParser::new(s.as_bytes());
    let mut deserializer = facet_format::FormatDeserializer::new(parser);
    deserializer.deserialize()
}

/// Deserialize an HTML document from bytes.
pub fn from_slice<'de, T: facet_core::Facet<'de>>(
    bytes: &'de [u8],
) -> Result<T, facet_format::DeserializeError<HtmlError>> {
    let parser = HtmlParser::new(bytes);
    let mut deserializer = facet_format::FormatDeserializer::new(parser);
    deserializer.deserialize()
}
