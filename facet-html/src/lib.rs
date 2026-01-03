#![deny(unsafe_code)]
#![deny(missing_docs, rustdoc::broken_intra_doc_links)]

//! HTML parser and serializer implementing the facet format architecture.
//!
//! This crate provides:
//! - **Parsing**: WHATWG-compliant HTML tokenization via html5gum
//! - **Serialization**: Configurable HTML output (minified or pretty-printed)
//!
//! # Attributes
//!
//! After importing `use facet_html as html;`, you can use these attributes:
//!
//! - `#[facet(html::element)]` - Marks a field as a single HTML child element
//! - `#[facet(html::elements)]` - Marks a field as collecting multiple HTML child elements  
//! - `#[facet(html::attribute)]` - Marks a field as an HTML attribute (on the element tag)
//! - `#[facet(html::text)]` - Marks a field as the text content of the element
//!
//! # Parsing Example
//!
//! ```rust
//! use facet::Facet;
//! use facet_format::FormatDeserializer;
//! use facet_html::HtmlParser;
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
//! # Serialization Example
//!
//! ```rust
//! use facet::Facet;
//! use facet_xml as xml;
//! use facet_html::{to_string, to_string_pretty};
//!
//! #[derive(Debug, Facet)]
//! #[facet(rename = "div")]
//! struct MyDiv {
//!     #[facet(xml::attribute, default)]
//!     class: Option<String>,
//!     #[facet(xml::text, default)]
//!     content: String,
//! }
//!
//! let div = MyDiv {
//!     class: Some("container".into()),
//!     content: "Hello!".into(),
//! };
//!
//! // Minified output (default)
//! let html = to_string(&div).unwrap();
//!
//! // Pretty-printed output
//! let html_pretty = to_string_pretty(&div).unwrap();
//! ```
//!
//! # Pre-defined HTML Element Types
//!
//! For typed definitions of all standard HTML5 elements, use the `facet-html-dom` crate:
//!
//! ```rust,ignore
//! use facet_html_dom::{Html, Div, P, A};
//! ```
//!
//! The DOM types are in a separate crate so they can use `#[facet(html::elements)]`
//! instead of falling back to `#[facet(xml::elements)]`.

mod parser;
mod serializer;

pub use parser::{HtmlError, HtmlParser};
pub use serializer::{
    HtmlSerializeError, HtmlSerializer, SerializeOptions, to_string, to_string_pretty,
    to_string_with_options, to_vec, to_vec_with_options,
};

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
    crate_path ::facet_html;

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
/// use facet_html as html;
///
/// #[derive(Debug, Facet)]
/// struct Div {
///     #[facet(html::text, default)]
///     text: String,
/// }
///
/// let doc: Div = facet_html::from_str("<div>hello</div>").unwrap();
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
