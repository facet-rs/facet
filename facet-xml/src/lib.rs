#![doc = include_str!("../README.md.in")]
#![deny(unsafe_code)]

mod dom_parser;
mod escaping;
mod serializer;

#[cfg(feature = "axum")]
mod axum;

pub use dom_parser::{XmlError, XmlParser};

#[cfg(feature = "axum")]
pub use axum::{Xml, XmlRejection};

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
