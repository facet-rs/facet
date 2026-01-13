#![deny(unsafe_code)]

//! XML parser and serializer using the facet-dom architecture.
//!
//! This crate provides XML parsing and serialization using quick-xml under the hood,
//! with a DOM-based deserializer from facet-dom.
//!
//! # Data Model: XML vs HTML
//!
//! `facet-xml` and `facet-html` use different data models that reflect the semantic
//! differences between the two formats:
//!
//! **XML is data-centric**: Elements with only text content are treated as scalar values.
//! This enables natural mappings like `<age>25</age>` → `age: u32`.
//!
//! ```rust
//! use facet::Facet;
//!
//! #[derive(Debug, Facet, PartialEq)]
//! struct Person {
//!     name: String,
//!     age: u32,
//! }
//!
//! let xml = "<Person><name>Alice</name><age>30</age></Person>";
//! let person: Person = facet_xml::from_str(xml).unwrap();
//! assert_eq!(person, Person { name: "Alice".into(), age: 30 });
//! ```
//!
//! **HTML is structure-centric**: Every element is a structural node with a tag name,
//! attributes, and children. Text is always a child node, never the element itself.
//! This preserves the DOM structure and enables tag name capture via `#[facet(html::tag)]`.
//!
//! This difference affects how unknown/dynamic content is captured. A flattened `HashMap`
//! can capture both unknown attributes and unknown child elements:
//!
//! ```rust
//! use facet::Facet;
//! use std::collections::HashMap;
//!
//! #[derive(Debug, Facet)]
//! #[facet(rename = "config")]
//! struct Config {
//!     #[facet(flatten, default)]
//!     settings: HashMap<String, String>,
//! }
//!
//! // In XML, text-only child elements can be captured in a HashMap
//! let xml = r#"<config><timeout>30</timeout><host>localhost</host></config>"#;
//! let config: Config = facet_xml::from_str(xml).unwrap();
//! assert_eq!(config.settings.get("timeout"), Some(&"30".to_string()));
//! assert_eq!(config.settings.get("host"), Some(&"localhost".to_string()));
//! ```
//!
//! Unknown attributes are also captured:
//!
//! ```rust
//! use facet::Facet;
//! use facet_xml as xml;
//! use std::collections::HashMap;
//!
//! #[derive(Debug, Facet)]
//! #[facet(rename = "div")]
//! struct Div {
//!     #[facet(xml::attribute)]
//!     id: Option<String>,
//!
//!     #[facet(flatten, default)]
//!     extra_attrs: HashMap<String, String>,
//! }
//!
//! let xml = r#"<div id="main" data-value="123"></div>"#;
//! let div: Div = facet_xml::from_str(xml).unwrap();
//! assert_eq!(div.id, Some("main".to_string()));
//! assert_eq!(div.extra_attrs.get("data-value"), Some(&"123".to_string()));
//! ```
//!
//! The same pattern would not work with `facet-html` because HTML elements are always
//! structures (with `_tag` and `_text` fields), not scalars. For HTML, use typed
//! element structs or `Vec<FlowContent>` with custom elements instead.

mod dom_parser;
mod escaping;
mod serializer;

#[cfg(feature = "axum")]
mod axum;

#[cfg(feature = "diff")]
mod diff_serialize;

pub use dom_parser::{XmlError, XmlParser};

#[cfg(feature = "axum")]
pub use axum::{Xml, XmlRejection};

#[cfg(feature = "diff")]
pub use diff_serialize::{
    DiffSerializeOptions, DiffSymbols, DiffTheme, diff_to_string, diff_to_string_with_options,
    diff_to_writer, diff_to_writer_with_options,
};
pub use serializer::{
    FloatFormatter, SerializeOptions, XmlSerializeError, XmlSerializer, to_string,
    to_string_pretty, to_string_with_options, to_vec, to_vec_with_options,
};

// Re-export error types for convenience
pub use facet_dom::DomDeserializeError as DeserializeError;
pub use facet_dom::DomSerializeError as SerializeError;

/// Deserialize a value from an XML string into an owned type.
///
/// This is the recommended default for most use cases. The input does not need
/// to outlive the result, making it suitable for deserializing from temporary
/// buffers (e.g., HTTP request bodies).
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_xml::from_str;
///
/// #[derive(Facet, Debug, PartialEq)]
/// struct Person {
///     name: String,
///     age: u32,
/// }
///
/// let xml = r#"<Person><name>Alice</name><age>30</age></Person>"#;
/// let person: Person = from_str(xml).unwrap();
/// assert_eq!(person.name, "Alice");
/// assert_eq!(person.age, 30);
/// ```
pub fn from_str<T>(input: &str) -> Result<T, DeserializeError<XmlError>>
where
    T: facet_core::Facet<'static>,
{
    from_slice(input.as_bytes())
}

/// Deserialize a value from XML bytes into an owned type.
///
/// This is the recommended default for most use cases. The input does not need
/// to outlive the result, making it suitable for deserializing from temporary
/// buffers (e.g., HTTP request bodies).
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_xml::from_slice;
///
/// #[derive(Facet, Debug, PartialEq)]
/// struct Person {
///     name: String,
///     age: u32,
/// }
///
/// let xml = b"<Person><name>Alice</name><age>30</age></Person>";
/// let person: Person = from_slice(xml).unwrap();
/// assert_eq!(person.name, "Alice");
/// assert_eq!(person.age, 30);
/// ```
pub fn from_slice<T>(input: &[u8]) -> Result<T, DeserializeError<XmlError>>
where
    T: facet_core::Facet<'static>,
{
    let parser = XmlParser::new(input);
    let mut de = facet_dom::DomDeserializer::new_owned(parser);
    de.deserialize()
}

/// Deserialize a value from an XML string, allowing borrowing from the input.
///
/// Use this when the deserialized type can borrow from the input string
/// (e.g., contains `&'a str` fields). The input must outlive the result.
///
/// For most use cases, prefer [`from_str`] which produces owned types.
pub fn from_str_borrowed<'input, T>(input: &'input str) -> Result<T, DeserializeError<XmlError>>
where
    T: facet_core::Facet<'input>,
{
    from_slice_borrowed(input.as_bytes())
}

/// Deserialize a value from XML bytes, allowing borrowing from the input.
///
/// Use this when the deserialized type can borrow from the input bytes
/// (e.g., contains `&'a str` fields). The input must outlive the result.
///
/// For most use cases, prefer [`from_slice`] which produces owned types.
pub fn from_slice_borrowed<'input, T>(input: &'input [u8]) -> Result<T, DeserializeError<XmlError>>
where
    T: facet_core::Facet<'input>,
{
    let parser = XmlParser::new(input);
    let mut de = facet_dom::DomDeserializer::new(parser);
    de.deserialize()
}

// XML extension attributes for use with #[facet(xml::attr)] syntax.
//
// After importing `use facet_xml as xml;`, users can write:
//   #[facet(xml::element)]
//   #[facet(xml::elements)]
//   #[facet(xml::attribute)]
//   #[facet(xml::text)]
//   #[facet(xml::tag)]

// Generate XML attribute grammar using the grammar DSL.
// This generates:
// - `Attr` enum with all XML attribute variants
// - `__attr!` macro that dispatches to attribute handlers and returns ExtensionAttr
// - `__parse_attr!` macro for parsing (internal use)
facet::define_attr_grammar! {
    ns "xml";
    crate_path ::facet_xml;

    /// XML attribute types for field and container configuration.
    pub enum Attr {
        /// Marks a field as a single XML child element
        Element,
        /// Marks a field as collecting multiple XML child elements
        Elements,
        /// Marks a field as an XML attribute (on the element tag)
        Attribute,
        /// Marks a field as the text content of the element
        Text,
        /// Marks a field as storing the XML element tag name dynamically.
        ///
        /// Used on a `String` field to capture the tag name of an element
        /// during deserialization. When serializing, this value becomes the element's tag.
        Tag,
        /// Specifies the XML namespace URI for this field.
        ///
        /// Usage: `#[facet(xml::ns = "http://example.com/ns")]`
        ///
        /// When deserializing, the field will only match elements/attributes
        /// in the specified namespace. When serializing, the element/attribute
        /// will be emitted with the appropriate namespace prefix.
        Ns(&'static str),
        /// Specifies the default XML namespace URI for all fields in this container.
        ///
        /// Usage: `#[facet(xml::ns_all = "http://example.com/ns")]`
        ///
        /// This sets the default namespace for all fields that don't have their own
        /// `xml::ns` attribute. Individual fields can override this with `xml::ns`.
        NsAll(&'static str),
    }
}
