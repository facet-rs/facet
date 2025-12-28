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

// XML extension attributes for use with #[facet(xml::attr)] syntax.
//
// After importing `use facet_format_xml as xml;`, users can write:
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
    crate_path ::facet_format_xml;

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
