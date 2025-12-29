//! XML serialization and deserialization for Facet types.
//!
//! This crate provides XML support for Facet types using an event-driven
//! architecture powered by [`quick-xml`](https://crates.io/crates/quick-xml).
//!
//! # Quick start
//!
//! ```rust
//! use facet::Facet;
//! use facet_xml_legacy as xml;
//!
//! #[derive(Facet, Debug, PartialEq)]
//! struct Person {
//!     #[facet(xml::attribute)]
//!     id: u32,
//!     #[facet(xml::element)]
//!     name: String,
//!     #[facet(xml::element)]
//!     age: Option<u32>,
//! }
//!
//! fn main() -> Result<(), facet_xml_legacy::XmlError> {
//!     let xml_str = r#"<Person id="42"><name>Alice</name></Person>"#;
//!     let person: Person = facet_xml_legacy::from_str(xml_str)?;
//!     assert_eq!(person.name, "Alice");
//!     assert_eq!(person.id, 42);
//!     assert_eq!(person.age, None);
//!
//!     let output = facet_xml_legacy::to_string(&person)?;
//!     // Output: <Person id="42"><name>Alice</name></Person>
//!     Ok(())
//! }
//! ```
//!
//! > **Important:** Every struct field must declare how it maps to XML. Add
//! > `#[facet(xml::attribute)]`, `#[facet(xml::element)]`,
//! > `#[facet(xml::elements)]`, `#[facet(xml::text)]`,
//! > `#[facet(xml::element_name)]`, or `#[facet(child)]` to every field that
//! > should appear in XML. Fields without an annotation now trigger an error
//! > instead of being silently skipped.
//!
//! # Attribute Guide
//!
//! ## `#[facet(xml::element)]`
//!
//! Maps a field to a child XML element:
//!
//! ```rust
//! # use facet::Facet;
//! # use facet_xml_legacy as xml;
//! #[derive(Facet)]
//! struct Book {
//!     #[facet(xml::element)]
//!     title: String,
//!     #[facet(xml::element)]
//!     author: String,
//! }
//! // Deserializes: <Book><title>1984</title><author>Orwell</author></Book>
//! ```
//!
//! ## `#[facet(xml::elements)]`
//!
//! Maps a field to multiple child elements (for Vec, HashSet, etc.):
//!
//! ```rust
//! # use facet::Facet;
//! # use facet_xml_legacy as xml;
//! #[derive(Facet)]
//! struct Library {
//!     #[facet(xml::elements)]
//!     books: Vec<Book>,
//! }
//!
//! #[derive(Facet)]
//! struct Book {
//!     #[facet(xml::attribute)]
//!     isbn: String,
//! }
//! // Deserializes: <Library><Book isbn="123"/><Book isbn="456"/></Library>
//! ```
//!
//! ## `#[facet(xml::attribute)]`
//!
//! Maps a field to an XML attribute:
//!
//! ```rust
//! # use facet::Facet;
//! # use facet_xml_legacy as xml;
//! #[derive(Facet)]
//! struct Item {
//!     #[facet(xml::attribute)]
//!     id: u32,
//!     #[facet(xml::attribute)]
//!     name: String,
//! }
//! // Deserializes: <Item id="1" name="widget"/>
//! ```
//!
//! ## `#[facet(xml::text)]`
//!
//! Maps a field to the text content of the element:
//!
//! ```rust
//! # use facet::Facet;
//! # use facet_xml_legacy as xml;
//! #[derive(Facet)]
//! struct Message {
//!     #[facet(xml::attribute)]
//!     from: String,
//!     #[facet(xml::text)]
//!     content: String,
//! }
//! // Deserializes: <Message from="alice">Hello, world!</Message>
//! ```
//!
//! # Error Reporting
//!
//! Errors use `miette` spans where possible, so diagnostics can point back to
//! the offending XML source.

#![warn(missing_docs)]
#![allow(clippy::result_large_err)]

mod annotation;
mod deserialize;
mod error;
mod serialize;

// Re-export span types from facet-reflect
pub use facet_reflect::{Span, Spanned};

// Re-export error types
pub use error::{XmlError, XmlErrorKind};

// Re-export deserialization
pub use deserialize::{
    DeserializeOptions, from_slice, from_slice_owned, from_slice_with_options, from_str,
    from_str_with_options,
};

// Re-export serialization
pub use serialize::{
    FloatFormatter, SerializeOptions, to_string, to_string_pretty, to_string_with_options,
    to_writer, to_writer_pretty, to_writer_with_options,
};

mod xml;
pub use xml::Xml;

#[cfg(feature = "axum")]
mod axum;
#[cfg(feature = "axum")]
pub use self::axum::XmlRejection;

#[cfg(feature = "diff")]
mod diff_serialize;
#[cfg(feature = "diff")]
pub use diff_serialize::*;

// XML extension attributes for use with #[facet(xml::attr)] syntax.
//
// After importing `use facet_xml_legacy as xml;`, users can write:
//   #[facet(xml::element)]
//   #[facet(xml::elements)]
//   #[facet(xml::attribute)]
//   #[facet(xml::text)]
//   #[facet(xml::element_name)]

// Generate XML attribute grammar using the grammar DSL.
// This generates:
// - `Attr` enum with all XML attribute variants
// - `__attr!` macro that dispatches to attribute handlers and returns ExtensionAttr
// - `__parse_attr!` macro for parsing (internal use)
facet::define_attr_grammar! {
    ns "xml";
    crate_path ::facet_xml_legacy;

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
        /// Marks a field as storing the XML element name dynamically
        ElementName,
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
