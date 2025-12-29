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

#[cfg(feature = "axum")]
mod axum;

#[cfg(feature = "diff")]
mod diff_serialize;

pub use parser::{XmlError, XmlParser};

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

// Re-export DeserializeError for convenience
pub use facet_format::DeserializeError;

#[cfg(all(feature = "streaming", feature = "std"))]
pub use streaming::from_reader;

#[cfg(feature = "tokio")]
pub use streaming::from_async_reader_tokio;

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
/// use facet_format_xml::from_str;
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
/// use facet_format_xml::from_slice;
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
    use facet_format::FormatDeserializer;
    let parser = XmlParser::new(input);
    let mut de = FormatDeserializer::new_owned(parser);
    de.deserialize()
}

/// Deserialize a value from an XML string, allowing zero-copy borrowing.
///
/// This variant requires the input to outlive the result (`'input: 'facet`),
/// enabling zero-copy deserialization of string fields as `&str` or `Cow<str>`.
///
/// Use this when you need maximum performance and can guarantee the input
/// buffer outlives the deserialized value. For most use cases, prefer
/// [`from_str`] which doesn't have lifetime requirements.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_format_xml::from_str_borrowed;
///
/// #[derive(Facet, Debug, PartialEq)]
/// struct Person {
///     name: String,
///     age: u32,
/// }
///
/// let xml = r#"<Person><name>Alice</name><age>30</age></Person>"#;
/// let person: Person = from_str_borrowed(xml).unwrap();
/// assert_eq!(person.name, "Alice");
/// assert_eq!(person.age, 30);
/// ```
pub fn from_str_borrowed<'input, 'facet, T>(
    input: &'input str,
) -> Result<T, DeserializeError<XmlError>>
where
    T: facet_core::Facet<'facet>,
    'input: 'facet,
{
    from_slice_borrowed(input.as_bytes())
}

/// Deserialize a value from XML bytes, allowing zero-copy borrowing.
///
/// This variant requires the input to outlive the result (`'input: 'facet`),
/// enabling zero-copy deserialization of string fields as `&str` or `Cow<str>`.
///
/// Use this when you need maximum performance and can guarantee the input
/// buffer outlives the deserialized value. For most use cases, prefer
/// [`from_slice`] which doesn't have lifetime requirements.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_format_xml::from_slice_borrowed;
///
/// #[derive(Facet, Debug, PartialEq)]
/// struct Person {
///     name: String,
///     age: u32,
/// }
///
/// let xml = b"<Person><name>Alice</name><age>30</age></Person>";
/// let person: Person = from_slice_borrowed(xml).unwrap();
/// assert_eq!(person.name, "Alice");
/// assert_eq!(person.age, 30);
/// ```
pub fn from_slice_borrowed<'input, 'facet, T>(
    input: &'input [u8],
) -> Result<T, DeserializeError<XmlError>>
where
    T: facet_core::Facet<'facet>,
    'input: 'facet,
{
    use facet_format::FormatDeserializer;
    let parser = XmlParser::new(input);
    let mut de = FormatDeserializer::new(parser);
    de.deserialize()
}

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
