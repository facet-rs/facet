#![deny(unsafe_code)]
#![deny(missing_docs, rustdoc::broken_intra_doc_links)]

//! HTML parser and serializer using the facet-dom architecture.
//!
//! This crate provides:
//! - **Parsing**: WHATWG-compliant HTML tokenization via html5gum
//! - **Serialization**: Configurable HTML output (minified or pretty-printed)
//!
//! # Data Model: HTML vs XML
//!
//! `facet-html` and `facet-xml` use different data models that reflect the semantic
//! differences between the two formats:
//!
//! **HTML is structure-centric**: Every element is a structural node with a tag name,
//! attributes, and children. Text is always a child node, never the element itself.
//! This preserves the DOM structure and enables:
//!
//! - Tag name capture via `#[facet(html::tag)]` for custom elements
//! - Proper mixed content handling (interleaved text and elements)
//! - Faithful DOM roundtripping
//!
//! **XML is data-centric**: Elements with only text content are treated as scalar values.
//! `<age>25</age>` naturally maps to `age: u32`. This is more convenient for configuration
//! files and data interchange, but loses structural information.
//!
//! This difference affects how unknown/dynamic children are handled:
//!
//! ```rust
//! use facet::Facet;
//! use facet_html as html;
//!
//! // For HTML, use typed element structs or custom element capture
//! #[derive(Debug, Facet)]
//! struct CustomElement {
//!     #[facet(html::tag, default)]
//!     tag: String,
//!     #[facet(html::text, default)]
//!     text: String,
//! }
//!
//! // Unknown child elements preserve their tag name
//! // (unlike XML where <child>text</child> becomes just "text")
//! ```
//!
//! If you need to capture unknown children with `HashMap<String, String>` (where element
//! names become keys and text content becomes values), use `facet-xml` instead.
//! HTML's DOM-preserving model requires the full element structure.
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
//! use facet_html as html;
//!
//! #[derive(Debug, Facet, PartialEq)]
//! #[facet(rename = "html")]
//! struct Document {
//!     #[facet(html::element, default)]
//!     head: Option<Head>,
//!     #[facet(html::element, default)]
//!     body: Option<Body>,
//! }
//!
//! #[derive(Debug, Facet, PartialEq)]
//! #[facet(rename = "head")]
//! struct Head {
//!     #[facet(html::element, default)]
//!     title: Option<Title>,
//! }
//!
//! #[derive(Debug, Facet, PartialEq)]
//! #[facet(rename = "title")]
//! struct Title {
//!     #[facet(html::text, default)]
//!     text: String,
//! }
//!
//! #[derive(Debug, Facet, PartialEq)]
//! #[facet(rename = "body")]
//! struct Body {
//!     #[facet(html::attribute, default)]
//!     class: Option<String>,
//!     #[facet(html::text, default)]
//!     content: String,
//! }
//!
//! let html_input = r#"<html><head><title>Hello</title></head><body class="main">World</body></html>"#;
//! let doc: Document = html::from_str(html_input).unwrap();
//!
//! assert_eq!(doc.head.unwrap().title.unwrap().text, "Hello");
//! assert_eq!(doc.body.as_ref().unwrap().class, Some("main".to_string()));
//! assert_eq!(doc.body.unwrap().content, "World");
//! ```
//!
//! # Serialization Example
//!
//! ```rust
//! use facet::Facet;
//! use facet_html as html;
//!
//! #[derive(Debug, Facet)]
//! #[facet(rename = "div")]
//! struct MyDiv {
//!     #[facet(html::attribute, default)]
//!     class: Option<String>,
//!     #[facet(html::text, default)]
//!     content: String,
//! }
//!
//! let div = MyDiv {
//!     class: Some("container".into()),
//!     content: "Hello!".into(),
//! };
//!
//! // Minified output (default)
//! let output = html::to_string(&div).unwrap();
//! assert_eq!(output, r#"<div class="container">Hello!</div>"#);
//!
//! // Pretty-printed output
//! let output_pretty = html::to_string_pretty(&div).unwrap();
//! ```
//!
//! # Pre-defined HTML Element Types
//!
//! For typed definitions of all standard HTML5 elements, use the `facet-html-dom` crate:
//!
//! ```rust,ignore
//! use facet_html_dom::{Html, Body, Div, P, A, FlowContent};
//!
//! // Parse a complete HTML document
//! let doc: Html = facet_html::from_str(html_source)?;
//!
//! // Access typed elements
//! if let Some(body) = &doc.body {
//!     for child in &body.children {
//!         match child {
//!             FlowContent::P(p) => println!("Paragraph: {:?}", p),
//!             FlowContent::Div(div) => println!("Div: {:?}", div),
//!             _ => {}
//!         }
//!     }
//! }
//! ```
//!
//! The DOM crate provides typed structs for all HTML5 elements with proper nesting
//! via content model enums (`FlowContent`, `PhrasingContent`). Unknown elements
//! and attributes (like `data-*`, `aria-*`) are captured in `extra` fields.

mod parser;
mod serializer;

pub use parser::{HtmlError, HtmlParser};
pub use serializer::{
    HtmlSerializeError, HtmlSerializer, SerializeOptions, to_string, to_string_pretty,
    to_string_with_options, to_vec, to_vec_with_options,
};

// HTML extension attributes for use with #[facet(html::attr)] syntax.
//
// After importing `use facet_html as html;`, users can write:
//   #[facet(html::element)]
//   #[facet(html::elements)]
//   #[facet(html::attribute)]
//   #[facet(html::text)]
//   #[facet(html::tag)]
//   #[facet(html::custom_element)]

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
        /// Marks a field as storing the element's tag name (for custom elements).
        ///
        /// Used on a `String` field to capture the tag name of an unknown element
        /// during deserialization. When serializing, this value becomes the element's tag.
        Tag,
        /// Marks an enum variant as a catch-all for unknown elements.
        ///
        /// When deserializing, if no other variant matches the element name,
        /// this variant is selected. The variant's struct must have a field
        /// marked with `#[facet(html::tag)]` to capture the element name.
        CustomElement,
    }
}

// Re-export error types for convenience
pub use facet_dom::DomDeserializeError as DeserializeError;
pub use facet_dom::DomSerializeError as SerializeError;

/// Deserialize an HTML document from a string into an owned type.
///
/// This is the recommended default for most use cases. The input does not need
/// to outlive the result, making it suitable for deserializing from temporary
/// buffers (e.g., HTTP request bodies).
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
pub fn from_str<T>(s: &str) -> Result<T, DeserializeError<HtmlError>>
where
    T: facet_core::Facet<'static>,
{
    from_slice(s.as_bytes())
}

/// Deserialize an HTML document from bytes into an owned type.
///
/// This is the recommended default for most use cases. The input does not need
/// to outlive the result, making it suitable for deserializing from temporary
/// buffers (e.g., HTTP request bodies).
pub fn from_slice<T>(bytes: &[u8]) -> Result<T, DeserializeError<HtmlError>>
where
    T: facet_core::Facet<'static>,
{
    let parser = HtmlParser::new(bytes);
    let mut deserializer = facet_dom::DomDeserializer::new_owned(parser);
    deserializer.deserialize()
}

/// Deserialize an HTML document from a string, allowing borrowing from the input.
///
/// Use this when the deserialized type can borrow from the input string
/// (e.g., contains `&'a str` fields). The input must outlive the result.
///
/// For most use cases, prefer [`from_str`] which produces owned types.
pub fn from_str_borrowed<'input, T>(input: &'input str) -> Result<T, DeserializeError<HtmlError>>
where
    T: facet_core::Facet<'input>,
{
    from_slice_borrowed(input.as_bytes())
}

/// Deserialize an HTML document from bytes, allowing borrowing from the input.
///
/// Use this when the deserialized type can borrow from the input bytes
/// (e.g., contains `&'a str` fields). The input must outlive the result.
///
/// For most use cases, prefer [`from_slice`] which produces owned types.
pub fn from_slice_borrowed<'input, T>(input: &'input [u8]) -> Result<T, DeserializeError<HtmlError>>
where
    T: facet_core::Facet<'input>,
{
    let parser = HtmlParser::new(input);
    let mut deserializer = facet_dom::DomDeserializer::new(parser);
    deserializer.deserialize()
}
