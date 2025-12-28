#![deny(unsafe_code)]
#![deny(missing_docs, rustdoc::broken_intra_doc_links)]

//! HTML parser that implements `FormatParser` for the facet format architecture.
//!
//! This uses html5gum for WHATWG-compliant HTML tokenization and translates its
//! tokens into the format-agnostic ParseEvent stream.
//!
//! # Attributes
//!
//! After importing `use facet_format_html as html;`, you can use these attributes:
//!
//! - `#[facet(html::element)]` - Marks a field as a single HTML child element
//! - `#[facet(html::elements)]` - Marks a field as collecting multiple HTML child elements  
//! - `#[facet(html::attribute)]` - Marks a field as an HTML attribute (on the element tag)
//! - `#[facet(html::text)]` - Marks a field as the text content of the element
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

// HTML extension attributes for use with #[facet(html::attr)] syntax.
//
// After importing `use facet_format_html as html;`, users can write:
//   #[facet(html::element)]
//   #[facet(html::elements)]
//   #[facet(html::attribute)]
//   #[facet(html::text)]

// Generate HTML attribute grammar using the grammar DSL.
// This generates:
// - `Attr` enum with all HTML attribute variants
// - `__attr!` macro that dispatches to attribute handlers and returns ExtensionAttr
// - `__parse_attr!` macro for parsing (internal use)
facet::define_attr_grammar! {
    ns "html";
    crate_path ::facet_format_html;

    /// HTML attribute types for field and container configuration.
    pub enum Attr {
        /// Marks a field as a single HTML child element
        Element,
        /// Marks a field as collecting multiple HTML child elements
        Elements,
        /// Marks a field as an HTML attribute (on the element tag)
        Attribute,
        /// Marks a field as the text content of the element
        Text,
    }
}

/// Deserialize an HTML document from a string.
///
/// # Example
///
/// ```rust
/// use facet::Facet;
///
/// #[derive(Debug, Facet)]
/// struct Div {
///     #[facet(facet_format_xml::text, default)]
///     text: String,
/// }
///
/// let doc: Div = facet_format_html::from_str("<div>hello</div>").unwrap();
/// assert_eq!(doc.text, "hello");
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
