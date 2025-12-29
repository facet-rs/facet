//! XML deserialization using quick-xml streaming events.
//!
//! This deserializer uses quick-xml's event-based API, processing events
//! on-demand and supporting rewind via event indices for flatten deserialization.

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use facet_core::{
    Def, EnumType, Facet, Field, NumericType, PrimitiveType, ShapeLayout, StructKind, StructType,
    Type, UserType, Variant,
};
use facet_reflect::{Partial, is_spanned_shape};
use facet_solver::{PathSegment, Schema, Solver};
use miette::SourceSpan;
use quick_xml::escape::resolve_xml_entity;
use quick_xml::events::{BytesStart, Event};
use quick_xml::name::ResolveResult;
use quick_xml::reader::NsReader;

use crate::annotation::{XmlAnnotationPhase, fields_missing_xml_annotations};
use crate::error::{MissingAnnotationPhase, XmlError, XmlErrorKind};

pub(crate) type Result<T> = std::result::Result<T, XmlError>;

// ============================================================================
// Deserialize Options
// ============================================================================

/// Options for controlling XML deserialization behavior.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_xml_legacy::{self as xml, DeserializeOptions};
///
/// #[derive(Facet, Debug, PartialEq)]
/// struct Person {
///     #[facet(xml::attribute)]
///     name: String,
/// }
///
/// let xml_str = r#"<Person name="Alice" extra="unknown"/>"#;
///
/// // Without options: unknown attributes are silently ignored
/// let person: Person = xml::from_str(xml_str).unwrap();
/// assert_eq!(person.name, "Alice");
///
/// // With deny_unknown_fields: unknown attributes cause an error
/// let options = DeserializeOptions::default().deny_unknown_fields(true);
/// let result: Result<Person, _> = xml::from_str_with_options(xml_str, &options);
/// assert!(result.is_err());
/// ```
#[derive(Debug, Clone, Default)]
pub struct DeserializeOptions {
    /// If `true`, reject XML documents with unknown attributes or elements
    /// that don't correspond to any field in the target struct.
    ///
    /// When `false` (the default), unknown attributes and elements are
    /// silently ignored.
    ///
    /// This option is combined with any `#[facet(deny_unknown_fields)]`
    /// attribute on the struct - if either is set, unknown fields cause
    /// an error.
    pub deny_unknown_fields: bool,
}

impl DeserializeOptions {
    /// Create new options with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set whether to deny unknown fields.
    ///
    /// When enabled, deserialization will fail if the XML contains
    /// attributes or elements that don't match any field in the struct.
    pub fn deny_unknown_fields(mut self, deny: bool) -> Self {
        self.deny_unknown_fields = deny;
        self
    }
}

/// Get the display name for a variant (respecting `rename` attribute).
fn get_variant_display_name(variant: &Variant) -> &'static str {
    if let Some(attr) = variant.get_builtin_attr("rename")
        && let Some(&renamed) = attr.get_as::<&str>()
    {
        return renamed;
    }
    variant.name
}

/// Get the display name for a shape (respecting `rename` attribute).
pub(crate) fn get_shape_display_name(shape: &facet_core::Shape) -> &'static str {
    if let Some(renamed) = shape.get_builtin_attr_value::<&str>("rename") {
        return renamed;
    }
    shape.type_identifier
}

/// Get the display name for a field (respecting `rename` attribute).
fn get_field_display_name(field: &Field) -> &'static str {
    if let Some(attr) = field.get_builtin_attr("rename")
        && let Some(&renamed) = attr.get_as::<&str>()
    {
        return renamed;
    }
    field.name
}

/// Extract the local name from a potentially prefixed name.
///
/// For example: `"android:name"` -> `"name"`, `"name"` -> `"name"`
///
/// This handles the case where field names use `rename = "prefix:localname"`
/// to match elements/attributes with a specific prefix in the document.
fn local_name_of(name: &str) -> &str {
    // Use rsplit_once to handle names with multiple colons correctly
    // (though that's unusual in XML)
    name.rsplit_once(':')
        .map(|(_, local)| local)
        .unwrap_or(name)
}

/// Check if a shape can accept an element with the given name.
/// For structs: element name must match struct's display name.
/// For enums: element name must match one of the variant's display names.
fn shape_accepts_element(shape: &facet_core::Shape, element_name: &str) -> bool {
    match &shape.ty {
        Type::User(UserType::Enum(enum_type)) => {
            // For enums, check if element name matches any variant
            enum_type
                .variants
                .iter()
                .any(|v| get_variant_display_name(v) == element_name)
        }
        Type::User(UserType::Struct(_)) => {
            // For structs, check if element name matches struct's name
            get_shape_display_name(shape) == element_name
        }
        _ => {
            // For other types (opaque, etc.), use type identifier
            shape.type_identifier == element_name
        }
    }
}

/// Get the list item shape from a field's shape (if it's a list type).
fn get_list_item_shape(shape: &facet_core::Shape) -> Option<&'static facet_core::Shape> {
    match &shape.def {
        Def::List(list_def) => Some(list_def.t()),
        _ => None,
    }
}

/// Check if the attribute is reserved for XML namespace
fn is_xml_namespace_attribute(name: &quick_xml::name::QName<'_>) -> bool {
    match name.prefix() {
        Some(prefix) => prefix.as_ref() == b"xmlns",
        None => name.local_name().as_ref() == b"xmlns",
    }
}

// ============================================================================
// Public API
// ============================================================================

/// Deserialize an XML string into a value of type `T`.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_xml_legacy as xml;
///
/// #[derive(Facet, Debug, PartialEq)]
/// struct Person {
///     #[facet(xml::attribute)]
///     id: u32,
///     #[facet(xml::element)]
///     name: String,
/// }
///
/// let xml_str = r#"<Person id="42"><name>Alice</name></Person>"#;
/// let person: Person = facet_xml_legacy::from_str(xml_str).unwrap();
/// assert_eq!(person.name, "Alice");
/// assert_eq!(person.id, 42);
/// ```
pub fn from_str<'input, 'facet, T>(xml: &'input str) -> Result<T>
where
    T: Facet<'facet>,
    'input: 'facet,
{
    from_str_with_options(xml, &DeserializeOptions::default())
}

/// Deserialize an XML string into a value of type `T` with custom options.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_xml_legacy::{self as xml, DeserializeOptions};
///
/// #[derive(Facet, Debug, PartialEq)]
/// struct Person {
///     #[facet(xml::attribute)]
///     name: String,
/// }
///
/// // With deny_unknown_fields, unknown attributes cause an error
/// let options = DeserializeOptions::default().deny_unknown_fields(true);
/// let xml_str = r#"<Person name="Alice" extra="unknown"/>"#;
/// let result: Result<Person, _> = xml::from_str_with_options(xml_str, &options);
/// assert!(result.is_err());
///
/// // Valid XML without unknown fields works fine
/// let xml_str = r#"<Person name="Alice"/>"#;
/// let person: Person = xml::from_str_with_options(xml_str, &options).unwrap();
/// assert_eq!(person.name, "Alice");
/// ```
pub fn from_str_with_options<'input, 'facet, T>(
    xml: &'input str,
    options: &DeserializeOptions,
) -> Result<T>
where
    T: Facet<'facet>,
    'input: 'facet,
{
    log::trace!(
        "from_str_with_options: parsing XML for type {}",
        core::any::type_name::<T>()
    );

    let mut deserializer = XmlDeserializer::new(xml, options.clone())?;
    let partial = Partial::alloc::<T>()?;

    let partial = deserializer.deserialize_document(partial)?;

    let result = partial
        .build()
        .map_err(|e| XmlError::new(XmlErrorKind::Reflect(e)).with_source(xml))?
        .materialize()
        .map_err(|e| XmlError::new(XmlErrorKind::Reflect(e)).with_source(xml))?;

    Ok(result)
}

/// Deserialize an XML byte slice into a value of type `T`.
///
/// This is a convenience wrapper around [`from_str`] that first validates
/// that the input is valid UTF-8.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_xml_legacy as xml;
///
/// #[derive(Facet, Debug, PartialEq)]
/// struct Person {
///     #[facet(xml::attribute)]
///     id: u32,
///     #[facet(xml::element)]
///     name: String,
/// }
///
/// let xml_bytes = b"<Person id=\"42\"><name>Alice</name></Person>";
/// let person: Person = facet_xml_legacy::from_slice(xml_bytes).unwrap();
/// assert_eq!(person.name, "Alice");
/// assert_eq!(person.id, 42);
/// ```
pub fn from_slice<'input, 'facet, T>(xml: &'input [u8]) -> Result<T>
where
    T: Facet<'facet>,
    'input: 'facet,
{
    from_slice_with_options(xml, &DeserializeOptions::default())
}

/// Deserialize an XML byte slice into a value of type `T` with custom options.
///
/// This is a convenience wrapper around [`from_str_with_options`] that first validates
/// that the input is valid UTF-8.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_xml_legacy::{self as xml, DeserializeOptions};
///
/// #[derive(Facet, Debug, PartialEq)]
/// struct Person {
///     #[facet(xml::attribute)]
///     name: String,
/// }
///
/// let options = DeserializeOptions::default().deny_unknown_fields(true);
/// let xml_bytes = b"<Person name=\"Alice\"/>";
/// let person: Person = xml::from_slice_with_options(xml_bytes, &options).unwrap();
/// assert_eq!(person.name, "Alice");
/// ```
pub fn from_slice_with_options<'input, 'facet, T>(
    xml: &'input [u8],
    options: &DeserializeOptions,
) -> Result<T>
where
    T: Facet<'facet>,
    'input: 'facet,
{
    let xml_str = std::str::from_utf8(xml)
        .map_err(|e| XmlError::new(XmlErrorKind::InvalidUtf8(e.to_string())))?;
    from_str_with_options(xml_str, options)
}

/// Deserialize an XML byte slice into an owned type.
///
/// This variant does not require the input to outlive the result, making it
/// suitable for deserializing from temporary buffers (e.g., HTTP request bodies).
///
/// Types containing `&str` fields cannot be deserialized with this function;
/// use `String` or `Cow<str>` instead.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_xml_legacy as xml;
///
/// #[derive(Facet, Debug, PartialEq)]
/// struct Person {
///     #[facet(xml::attribute)]
///     id: u32,
///     #[facet(xml::element)]
///     name: String,
/// }
///
/// let xml_bytes = b"<Person id=\"42\"><name>Alice</name></Person>";
/// let person: Person = xml::from_slice_owned(xml_bytes).unwrap();
/// assert_eq!(person.name, "Alice");
/// assert_eq!(person.id, 42);
/// ```
pub fn from_slice_owned<T: Facet<'static>>(xml: &[u8]) -> Result<T> {
    let xml_str = std::str::from_utf8(xml)
        .map_err(|e| XmlError::new(XmlErrorKind::InvalidUtf8(e.to_string())))?;

    log::trace!(
        "from_slice_owned: parsing XML for type {}",
        core::any::type_name::<T>()
    );

    let options = DeserializeOptions::default();
    let mut deserializer = XmlDeserializer::new(xml_str, options)?;
    let partial = Partial::alloc::<T>()?;

    let partial = deserializer.deserialize_document(partial)?;

    let result = partial
        .build()
        .map_err(|e| XmlError::new(XmlErrorKind::Reflect(e)).with_source(xml_str))?
        .materialize()
        .map_err(|e| XmlError::new(XmlErrorKind::Reflect(e)).with_source(xml_str))?;

    Ok(result)
}

// ============================================================================
// Extension trait for XML-specific field attributes
// ============================================================================

/// Extension trait for Field to check XML-specific attributes.
pub(crate) trait XmlFieldExt {
    /// Returns true if this field is an element field.
    fn is_xml_element(&self) -> bool;
    /// Returns true if this field is an elements (list) field.
    fn is_xml_elements(&self) -> bool;
    /// Returns true if this field is an attribute field.
    fn is_xml_attribute(&self) -> bool;
    /// Returns true if this field is a text field.
    fn is_xml_text(&self) -> bool;
    /// Returns true if this field stores the element name.
    #[allow(dead_code)]
    fn is_xml_element_name(&self) -> bool;
    /// Returns the expected XML namespace URI for this field, if specified.
    ///
    /// Returns `Some(ns)` if the field has `#[facet(xml::ns = "...")]`,
    /// or `None` if no namespace constraint is specified (matches any namespace).
    fn xml_ns(&self) -> Option<&'static str>;
}

impl XmlFieldExt for Field {
    fn is_xml_element(&self) -> bool {
        self.is_child() || self.has_attr(Some("xml"), "element")
    }

    fn is_xml_elements(&self) -> bool {
        self.has_attr(Some("xml"), "elements")
    }

    fn is_xml_attribute(&self) -> bool {
        self.has_attr(Some("xml"), "attribute")
    }

    fn is_xml_text(&self) -> bool {
        self.has_attr(Some("xml"), "text")
    }

    fn is_xml_element_name(&self) -> bool {
        self.has_attr(Some("xml"), "element_name")
    }

    fn xml_ns(&self) -> Option<&'static str> {
        self.get_attr(Some("xml"), "ns")
            .and_then(|attr| attr.get_as::<&str>().copied())
    }
}

/// Extension trait for Shape to check XML-specific container attributes.
pub(crate) trait XmlShapeExt {
    /// Returns the default XML namespace URI for all fields in this container.
    ///
    /// Returns `Some(ns)` if the shape has `#[facet(xml::ns_all = "...")]`,
    /// or `None` if no default namespace is specified.
    fn xml_ns_all(&self) -> Option<&'static str>;
}

impl XmlShapeExt for facet_core::Shape {
    fn xml_ns_all(&self) -> Option<&'static str> {
        self.attributes
            .iter()
            .find(|attr| attr.ns == Some("xml") && attr.key == "ns_all")
            .and_then(|attr| attr.get_as::<&str>().copied())
    }
}

// ============================================================================
// Qualified Name (namespace + local name)
// ============================================================================

/// A qualified XML name with optional namespace URI.
///
/// In XML, elements and attributes can be in a namespace. The namespace is
/// identified by a URI, not the prefix used in the document. For example,
/// `android:label` and `a:label` are the same if both prefixes resolve to
/// the same namespace URI.
#[derive(Debug, Clone, PartialEq, Eq)]
struct QName {
    /// The namespace URI, or `None` for "no namespace".
    ///
    /// - Elements without a prefix and no default `xmlns` are in no namespace.
    /// - Attributes without a prefix are always in no namespace (even with default xmlns).
    /// - Elements/attributes with a prefix have their namespace resolved via xmlns declarations.
    namespace: Option<String>,
    /// The local name (without prefix).
    local_name: String,
}

impl QName {
    /// Create a qualified name with no namespace.
    fn local(name: impl Into<String>) -> Self {
        Self {
            namespace: None,
            local_name: name.into(),
        }
    }

    /// Create a qualified name with a namespace.
    fn with_ns(namespace: impl Into<String>, local_name: impl Into<String>) -> Self {
        Self {
            namespace: Some(namespace.into()),
            local_name: local_name.into(),
        }
    }

    /// Check if this name matches a local name with an optional expected namespace.
    ///
    /// If `expected_ns` is `None`, matches any name with the given local name.
    /// If `expected_ns` is `Some(ns)`, only matches if both local name and namespace match.
    fn matches(&self, local_name: &str, expected_ns: Option<&str>) -> bool {
        if self.local_name != local_name {
            return false;
        }
        match expected_ns {
            None => true, // No namespace constraint - match any namespace (or none)
            Some(ns) => self.namespace.as_deref() == Some(ns),
        }
    }

    /// Check if this name matches exactly (same local name and same namespace presence).
    ///
    /// Unlike `matches()`, this requires the namespace to match exactly:
    /// - expected_ns: None means the element must be in "no namespace"
    /// - expected_ns: Some(ns) means the element must be in that specific namespace
    #[allow(dead_code)]
    fn matches_exact(&self, local_name: &str, expected_ns: Option<&str>) -> bool {
        self.local_name == local_name && self.namespace.as_deref() == expected_ns
    }
}

impl std::fmt::Display for QName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.namespace {
            Some(ns) => write!(f, "{{{}}}{}", ns, self.local_name),
            None => write!(f, "{}", self.local_name),
        }
    }
}

/// Compare QName with str by local name only (for backward compatibility).
impl PartialEq<str> for QName {
    fn eq(&self, other: &str) -> bool {
        self.local_name == other
    }
}

impl PartialEq<&str> for QName {
    fn eq(&self, other: &&str) -> bool {
        self.local_name == *other
    }
}

impl PartialEq<QName> for str {
    fn eq(&self, other: &QName) -> bool {
        self == other.local_name
    }
}

impl PartialEq<QName> for &str {
    fn eq(&self, other: &QName) -> bool {
        *self == other.local_name
    }
}

// ============================================================================
// Entity Resolution
// ============================================================================

/// Resolve a general entity reference to its character value.
/// Handles both named entities (lt, gt, amp, etc.) and numeric entities (&#10;, &#x09;, etc.)
fn resolve_entity(raw: &str) -> Result<String> {
    // Try named entity first (e.g., "lt" -> "<")
    if let Some(resolved) = resolve_xml_entity(raw) {
        return Ok(resolved.into());
    }

    // Try numeric entity (e.g., "#10" -> "\n", "#x09" -> "\t")
    if let Some(rest) = raw.strip_prefix('#') {
        let code = if let Some(hex) = rest.strip_prefix('x').or_else(|| rest.strip_prefix('X')) {
            // Hexadecimal numeric entity
            u32::from_str_radix(hex, 16).map_err(|_| {
                XmlError::new(XmlErrorKind::Parse(format!(
                    "Invalid hex numeric entity: #{}",
                    rest
                )))
            })?
        } else {
            // Decimal numeric entity
            rest.parse::<u32>().map_err(|_| {
                XmlError::new(XmlErrorKind::Parse(format!(
                    "Invalid decimal numeric entity: #{}",
                    rest
                )))
            })?
        };

        let ch = char::from_u32(code).ok_or_else(|| {
            XmlError::new(XmlErrorKind::Parse(format!(
                "Invalid Unicode code point: {}",
                code
            )))
        })?;
        return Ok(ch.to_string());
    }

    // Unknown entity - return as-is with & and ;
    Ok(format!("&{};", raw))
}

// ============================================================================
// Event wrapper with owned strings
// ============================================================================

/// An XML event with owned string data and span information.
#[derive(Debug, Clone)]
enum OwnedEvent {
    /// Start of an element with qualified name and attributes
    Start {
        name: QName,
        attributes: Vec<(QName, String)>,
    },
    /// End of an element
    End { name: QName },
    /// Empty element (self-closing)
    Empty {
        name: QName,
        attributes: Vec<(QName, String)>,
    },
    /// Text content
    Text { content: String },
    /// CDATA content
    CData { content: String },
    /// End of file
    Eof,
}

#[derive(Debug, Clone)]
struct SpannedEvent {
    event: OwnedEvent,
    /// Byte offset in the original input where this event starts.
    offset: usize,
    /// Length of the event in bytes.
    len: usize,
}

impl SpannedEvent {
    fn span(&self) -> SourceSpan {
        SourceSpan::from((self.offset, self.len))
    }
}

// ============================================================================
// Event Collector
// ============================================================================

/// Collects all events from the parser upfront, resolving namespaces.
struct EventCollector<'input> {
    reader: NsReader<&'input [u8]>,
    input: &'input str,
}

impl<'input> EventCollector<'input> {
    fn new(input: &'input str) -> Self {
        let mut reader = NsReader::from_str(input);
        // Don't use trim_text(true) - it drops whitespace-only text events
        // which breaks entity handling (spaces between entities are lost).
        // We handle whitespace filtering at the consumption level instead.
        reader.config_mut().trim_text(false);
        Self { reader, input }
    }

    /// Convert a ResolveResult to an optional namespace string.
    fn resolve_ns(resolve: ResolveResult<'_>) -> Option<String> {
        match resolve {
            ResolveResult::Bound(ns) => Some(String::from_utf8_lossy(ns.as_ref()).into_owned()),
            ResolveResult::Unbound => None,
            ResolveResult::Unknown(prefix) => {
                // Unknown prefix - treat as unbound but log a warning
                log::warn!(
                    "Unknown namespace prefix: {}",
                    String::from_utf8_lossy(&prefix)
                );
                None
            }
        }
    }

    fn collect_all(mut self) -> Result<Vec<SpannedEvent>> {
        let mut events = Vec::new();
        let mut buf = Vec::new();

        loop {
            let offset = self.reader.buffer_position() as usize;
            let (resolve, event) = self
                .reader
                .read_resolved_event_into(&mut buf)
                .map_err(|e| {
                    XmlError::new(XmlErrorKind::Parse(e.to_string())).with_source(self.input)
                })?;

            let (owned, len) = match event {
                Event::Start(ref e) => {
                    // Convert namespace to owned before calling methods on self
                    let ns = Self::resolve_ns(resolve);
                    let local = String::from_utf8_lossy(e.local_name().as_ref()).into_owned();
                    let name = match ns {
                        Some(uri) => QName::with_ns(uri, local),
                        None => QName::local(local),
                    };
                    let attributes = self.collect_attributes(e)?;
                    let len = self.reader.buffer_position() as usize - offset;
                    (OwnedEvent::Start { name, attributes }, len)
                }
                Event::End(ref e) => {
                    // For End events, we need to resolve the element name
                    let (resolve, _) = self.reader.resolve_element(e.name());
                    let ns = Self::resolve_ns(resolve);
                    let local = String::from_utf8_lossy(e.local_name().as_ref()).into_owned();
                    let name = match ns {
                        Some(uri) => QName::with_ns(uri, local),
                        None => QName::local(local),
                    };
                    let len = self.reader.buffer_position() as usize - offset;
                    (OwnedEvent::End { name }, len)
                }
                Event::Empty(ref e) => {
                    // Convert namespace to owned before calling methods on self
                    let ns = Self::resolve_ns(resolve);
                    let local = String::from_utf8_lossy(e.local_name().as_ref()).into_owned();
                    let name = match ns {
                        Some(uri) => QName::with_ns(uri, local),
                        None => QName::local(local),
                    };
                    let attributes = self.collect_attributes(e)?;
                    let len = self.reader.buffer_position() as usize - offset;
                    (OwnedEvent::Empty { name, attributes }, len)
                }
                Event::Text(e) => {
                    let content = e.decode().map_err(|e| {
                        XmlError::new(XmlErrorKind::Parse(e.to_string())).with_source(self.input)
                    })?;
                    // Don't skip whitespace-only text at collection time.
                    // It may be significant when adjacent to entity references.
                    // Consumers filter whitespace as needed.
                    let len = self.reader.buffer_position() as usize - offset;
                    (
                        OwnedEvent::Text {
                            content: content.into_owned(),
                        },
                        len,
                    )
                }
                Event::CData(e) => {
                    let content = String::from_utf8_lossy(&e).into_owned();
                    let len = self.reader.buffer_position() as usize - offset;
                    (OwnedEvent::CData { content }, len)
                }
                Event::Eof => {
                    events.push(SpannedEvent {
                        event: OwnedEvent::Eof,
                        offset,
                        len: 0,
                    });
                    break;
                }
                Event::GeneralRef(e) => {
                    // General entity references (e.g., &lt;, &gt;, &amp;, &#10;, etc.)
                    // These are reported separately in quick-xml 0.38+ for text content.
                    // Resolve the entity and emit as a Text event.
                    let raw = e.decode().map_err(|e| {
                        XmlError::new(XmlErrorKind::Parse(e.to_string())).with_source(self.input)
                    })?;
                    let content = resolve_entity(&raw)?;
                    let len = self.reader.buffer_position() as usize - offset;
                    (OwnedEvent::Text { content }, len)
                }
                Event::Comment(_) | Event::Decl(_) | Event::PI(_) | Event::DocType(_) => {
                    // Skip comments, declarations, processing instructions, doctypes
                    buf.clear();
                    continue;
                }
            };

            log::trace!("XML event: {owned:?} at offset {offset}");
            events.push(SpannedEvent {
                event: owned,
                offset,
                len,
            });
            buf.clear();
        }

        Ok(events)
    }

    fn collect_attributes(&self, e: &BytesStart<'_>) -> Result<Vec<(QName, String)>> {
        let mut attrs = Vec::new();
        for attr in e.attributes() {
            let attr = attr.map_err(|e| {
                XmlError::new(XmlErrorKind::Parse(e.to_string())).with_source(self.input)
            })?;

            // Ignore attributes reserved for XML namespace declarations
            if is_xml_namespace_attribute(&attr.key) {
                continue;
            }

            // Resolve attribute namespace
            let (resolve, _) = self.reader.resolve_attribute(attr.key);
            let ns = Self::resolve_ns(resolve);
            let local = String::from_utf8_lossy(attr.key.local_name().as_ref()).into_owned();
            let qname = match ns {
                Some(uri) => QName::with_ns(uri, local),
                None => QName::local(local),
            };

            let value = attr
                .unescape_value()
                .map_err(|e| {
                    XmlError::new(XmlErrorKind::Parse(e.to_string())).with_source(self.input)
                })?
                .into_owned();

            attrs.push((qname, value));
        }
        Ok(attrs)
    }
}

// ============================================================================
// Deserializer
// ============================================================================

/// XML deserializer that processes events from a collected event stream.
struct XmlDeserializer<'input> {
    input: &'input str,
    events: Vec<SpannedEvent>,
    pos: usize,
    options: DeserializeOptions,
}

impl<'input> XmlDeserializer<'input> {
    /// Create a new deserializer by parsing the input and collecting all events.
    fn new(input: &'input str, options: DeserializeOptions) -> Result<Self> {
        let collector = EventCollector::new(input);
        let events = collector.collect_all()?;

        Ok(Self {
            input,
            events,
            pos: 0,
            options,
        })
    }

    /// Create an error with source code attached for diagnostics.
    fn err(&self, kind: impl Into<XmlErrorKind>) -> XmlError {
        XmlError::new(kind).with_source(self.input.to_string())
    }

    /// Create an error with source code and span attached for diagnostics.
    fn err_at(&self, kind: impl Into<XmlErrorKind>, span: impl Into<SourceSpan>) -> XmlError {
        XmlError::new(kind)
            .with_source(self.input.to_string())
            .with_span(span)
    }

    /// Consume and return the current event (cloned to avoid borrow issues).
    fn next(&mut self) -> Option<SpannedEvent> {
        if self.pos < self.events.len() {
            let event = self.events[self.pos].clone();
            self.pos += 1;
            Some(event)
        } else {
            None
        }
    }

    /// Save current position for potential rewind.
    #[allow(dead_code)]
    fn save_position(&self) -> usize {
        self.pos
    }

    /// Restore to a previously saved position.
    #[allow(dead_code)]
    fn restore_position(&mut self, pos: usize) {
        self.pos = pos;
    }

    /// Deserialize the document starting from the root element.
    fn deserialize_document<'facet>(
        &mut self,
        partial: Partial<'facet>,
    ) -> Result<Partial<'facet>> {
        // Expect a start or empty element
        let Some(event) = self.next() else {
            return Err(self.err(XmlErrorKind::UnexpectedEof));
        };

        let span = event.span();

        match event.event {
            OwnedEvent::Start { name, attributes } => {
                self.deserialize_element(partial, &name, &attributes, span, false)
            }
            OwnedEvent::Empty { name, attributes } => {
                self.deserialize_element(partial, &name, &attributes, span, true)
            }
            other => Err(self.err(XmlErrorKind::UnexpectedEvent(format!(
                "expected start element, got {other:?}"
            )))),
        }
    }

    /// Deserialize an element into a partial value.
    fn deserialize_element<'facet>(
        &mut self,
        partial: Partial<'facet>,
        element_name: &QName,
        attributes: &[(QName, String)],
        span: SourceSpan,
        is_empty: bool,
    ) -> Result<Partial<'facet>> {
        let mut partial = partial;
        let shape = partial.shape();

        log::trace!(
            "deserialize_element: {:?} into shape {:?}",
            element_name,
            shape.ty
        );

        // Check Def first for scalars (String, etc.)
        if matches!(&shape.def, Def::Scalar) {
            // For scalar types, we expect text content
            if is_empty {
                // Empty element for a string means empty string
                if shape.is_type::<String>() {
                    partial = partial.set(String::new())?;
                    return Ok(partial);
                }
                return Err(self.err_at(
                    XmlErrorKind::InvalidValueForShape(
                        "expected text content for scalar type".into(),
                    ),
                    span,
                ));
            }

            // Get text content
            let text = self.read_text_until_end(element_name)?;
            partial = self.set_scalar_value(partial, &text)?;
            return Ok(partial);
        }

        // Priority 1: Check for builder_shape (immutable collections like Bytes -> BytesMut)
        if shape.builder_shape.is_some() {
            partial = partial.begin_inner()?;
            partial =
                self.deserialize_element(partial, element_name, attributes, span, is_empty)?;
            partial = partial.end()?;
            return Ok(partial);
        }

        // Handle Vec<u8> as base64
        if let Def::List(list_def) = &shape.def
            && list_def.t().is_type::<u8>()
        {
            if is_empty {
                // Empty element = empty bytes
                partial = partial.begin_list()?;
                // Empty list, nothing to add
                return Ok(partial);
            }
            let text = self.read_text_until_end(element_name)?;
            let bytes = BASE64_STANDARD
                .decode(text.trim())
                .map_err(|e| self.err_at(XmlErrorKind::Base64Decode(e.to_string()), span))?;
            partial = partial.begin_list()?;
            for byte in bytes {
                partial = partial.begin_list_item()?;
                partial = partial.set(byte)?;
                partial = partial.end()?; // end list item
            }
            return Ok(partial);
        }

        // Handle [u8; N] as base64
        if let Def::Array(arr_def) = &shape.def
            && arr_def.t().is_type::<u8>()
        {
            if is_empty {
                return Err(self.err_at(
                    XmlErrorKind::InvalidValueForShape("empty element for byte array".into()),
                    span,
                ));
            }
            let text = self.read_text_until_end(element_name)?;
            let bytes = BASE64_STANDARD
                .decode(text.trim())
                .map_err(|e| self.err_at(XmlErrorKind::Base64Decode(e.to_string()), span))?;
            if bytes.len() != arr_def.n {
                return Err(self.err_at(
                    XmlErrorKind::InvalidValueForShape(format!(
                        "base64 decoded {} bytes, expected {}",
                        bytes.len(),
                        arr_def.n
                    )),
                    span,
                ));
            }
            for (idx, byte) in bytes.into_iter().enumerate() {
                partial = partial.begin_nth_field(idx)?;
                partial = partial.set(byte)?;
                partial = partial.end()?;
            }
            return Ok(partial);
        }

        // Handle fixed arrays (non-byte)
        if let Def::Array(arr_def) = &shape.def {
            if is_empty {
                return Err(self.err_at(
                    XmlErrorKind::InvalidValueForShape("empty element for array".into()),
                    span,
                ));
            }
            let array_len = arr_def.n;
            return self.deserialize_array_content(partial, array_len, element_name);
        }

        // Handle sets
        if matches!(&shape.def, Def::Set(_)) {
            if is_empty {
                partial = partial.begin_set()?;
                // Empty set - nothing to do
                return Ok(partial);
            }
            return self.deserialize_set_content(partial, element_name);
        }

        // Handle maps
        if matches!(&shape.def, Def::Map(_)) {
            if is_empty {
                partial = partial.begin_map()?;
                // Empty map - nothing to do
                return Ok(partial);
            }
            return self.deserialize_map_content(partial, element_name);
        }

        // Check for .inner (transparent wrappers like NonZero)
        // Collections (List/Map/Set/Array) have .inner for variance but shouldn't use this path
        if shape.inner.is_some()
            && !matches!(
                &shape.def,
                Def::List(_) | Def::Map(_) | Def::Set(_) | Def::Array(_)
            )
        {
            partial = partial.begin_inner()?;
            partial =
                self.deserialize_element(partial, element_name, attributes, span, is_empty)?;
            partial = partial.end()?;
            return Ok(partial);
        }

        // Handle different shapes
        match &shape.ty {
            Type::User(UserType::Struct(struct_def)) => {
                // Get fields
                let fields = struct_def.fields;
                // Deny unknown if either the option or the attribute is set
                let deny_unknown =
                    self.options.deny_unknown_fields || shape.has_deny_unknown_fields_attr();

                match struct_def.kind {
                    StructKind::Unit => {
                        // Unit struct - nothing to deserialize, just skip content
                        if !is_empty {
                            self.skip_element(element_name)?;
                        }
                        return Ok(partial);
                    }
                    StructKind::Tuple | StructKind::TupleStruct => {
                        // Tuple struct - deserialize fields by position
                        if is_empty {
                            // Set defaults for all fields
                            partial = self.set_defaults_for_unset_fields(partial, fields)?;
                            return Ok(partial);
                        }

                        // Deserialize tuple fields from child elements
                        partial = self.deserialize_tuple_content(partial, fields, element_name)?;

                        // Set defaults for any unset fields
                        partial = self.set_defaults_for_unset_fields(partial, fields)?;
                        return Ok(partial);
                    }
                    StructKind::Struct => {
                        // Check if this struct has flattened fields - if so, use the solver
                        if Self::has_flatten_fields(struct_def) {
                            return self.deserialize_struct_with_flatten(
                                partial,
                                struct_def,
                                element_name,
                                attributes,
                                span,
                                is_empty,
                            );
                        }
                        // Normal named struct - fall through to standard handling
                    }
                }

                let missing =
                    fields_missing_xml_annotations(fields, XmlAnnotationPhase::Deserialize);
                if !missing.is_empty() {
                    let field_info = missing
                        .into_iter()
                        .map(|field| (field.name, field.shape().type_identifier))
                        .collect();
                    return Err(self.err(XmlErrorKind::MissingXmlAnnotations {
                        type_name: shape.type_identifier,
                        phase: MissingAnnotationPhase::Deserialize,
                        fields: field_info,
                    }));
                }

                // First, deserialize attributes
                partial =
                    self.deserialize_attributes(partial, fields, attributes, deny_unknown, span)?;

                // If empty element, we're done with content
                if is_empty {
                    // Set defaults for missing fields
                    partial = self.set_defaults_for_unset_fields(partial, fields)?;
                    return Ok(partial);
                }

                // Deserialize child elements and text content
                partial =
                    self.deserialize_element_content(partial, fields, element_name, deny_unknown)?;

                // Set defaults for any unset fields
                partial = self.set_defaults_for_unset_fields(partial, fields)?;

                Ok(partial)
            }
            Type::User(UserType::Enum(enum_def)) => {
                // Determine enum tagging strategy
                let is_untagged = shape.is_untagged();
                let tag_attr = shape.get_tag_attr();
                let content_attr = shape.get_content_attr();

                if is_untagged {
                    // Untagged: try each variant until one works
                    return self.deserialize_untagged_enum(
                        partial,
                        enum_def,
                        element_name,
                        attributes,
                        span,
                        is_empty,
                    );
                } else if let Some(tag) = tag_attr {
                    // Get variant name from attribute
                    let variant_name = attributes
                        .iter()
                        .find(|(k, _)| k == tag)
                        .map(|(_, v)| v.clone())
                        .ok_or_else(|| {
                            self.err_at(XmlErrorKind::MissingAttribute(tag.to_string()), span)
                        })?;

                    // Find the variant by name
                    let variant = enum_def
                        .variants
                        .iter()
                        .find(|v| v.name == variant_name)
                        .ok_or_else(|| {
                            self.err_at(
                                XmlErrorKind::NoMatchingElement(variant_name.to_string()),
                                span,
                            )
                        })?;

                    // Select the variant
                    partial = partial.select_variant_named(&variant_name)?;
                    let variant_fields = variant.data.fields;

                    if let Some(content) = content_attr {
                        // Adjacently tagged: <Element tag="Variant"><content>...</content></Element>
                        if is_empty {
                            // No content element for empty element
                            partial =
                                self.set_defaults_for_unset_fields(partial, variant_fields)?;
                        } else {
                            // Find the content element
                            partial = self.deserialize_adjacently_tagged_content(
                                partial,
                                variant,
                                content,
                                element_name,
                            )?;
                        }
                    } else {
                        // Internally tagged: <Element tag="Variant">...fields...</Element>
                        // Filter out the tag attribute
                        let other_attrs: Vec<_> = attributes
                            .iter()
                            .filter(|(k, _)| k != tag)
                            .cloned()
                            .collect();

                        match variant.data.kind {
                            StructKind::Unit => {
                                // Unit variant - nothing to deserialize
                                if !is_empty {
                                    self.skip_element(element_name)?;
                                }
                            }
                            StructKind::Tuple | StructKind::TupleStruct => {
                                // Tuple variant - deserialize fields by position
                                if !is_empty {
                                    partial = self.deserialize_tuple_content(
                                        partial,
                                        variant_fields,
                                        element_name,
                                    )?;
                                }
                                partial =
                                    self.set_defaults_for_unset_fields(partial, variant_fields)?;
                            }
                            StructKind::Struct => {
                                // Struct variant - deserialize as struct
                                partial = self.deserialize_attributes(
                                    partial,
                                    variant_fields,
                                    &other_attrs,
                                    false,
                                    span,
                                )?;
                                if !is_empty {
                                    partial = self.deserialize_element_content(
                                        partial,
                                        variant_fields,
                                        element_name,
                                        false,
                                    )?;
                                }
                                partial =
                                    self.set_defaults_for_unset_fields(partial, variant_fields)?;
                            }
                        }
                    }

                    return Ok(partial);
                }

                // Externally tagged (default) - two modes:
                // 1. Element name IS a variant name: <VariantName attr="...">...</VariantName>
                // 2. Element is a wrapper: <Wrapper><VariantName>...</VariantName></Wrapper>

                // Check if element name matches a variant's display name
                if let Some(variant) = enum_def
                    .variants
                    .iter()
                    .find(|v| get_variant_display_name(v) == element_name)
                {
                    // Mode 1: The element itself is the variant
                    // Use the original variant name for selection
                    partial = partial.select_variant_named(variant.name)?;
                    let variant_fields = variant.data.fields;

                    match variant.data.kind {
                        StructKind::Unit => {
                            // Unit variant - nothing to deserialize
                            if !is_empty {
                                self.skip_element(element_name)?;
                            }
                        }
                        StructKind::Tuple | StructKind::TupleStruct => {
                            // Tuple variant - check for newtype pattern
                            if variant_fields.len() == 1 {
                                // Newtype variant - deserialize inner value from current element
                                partial = partial.begin_nth_field(0)?;
                                partial = self.deserialize_element(
                                    partial,
                                    element_name,
                                    attributes,
                                    span,
                                    is_empty,
                                )?;
                                partial = partial.end()?;
                            } else if !is_empty {
                                // Multi-field tuple - deserialize from child elements
                                partial = self.deserialize_tuple_content(
                                    partial,
                                    variant_fields,
                                    element_name,
                                )?;
                                partial =
                                    self.set_defaults_for_unset_fields(partial, variant_fields)?;
                            } else {
                                partial =
                                    self.set_defaults_for_unset_fields(partial, variant_fields)?;
                            }
                        }
                        StructKind::Struct => {
                            // Struct variant - deserialize attributes and content
                            partial = self.deserialize_attributes(
                                partial,
                                variant_fields,
                                attributes,
                                false,
                                span,
                            )?;
                            if !is_empty {
                                partial = self.deserialize_element_content(
                                    partial,
                                    variant_fields,
                                    element_name,
                                    false,
                                )?;
                            }
                            partial =
                                self.set_defaults_for_unset_fields(partial, variant_fields)?;
                        }
                    }

                    return Ok(partial);
                }

                // Mode 2: Element is a wrapper containing the variant element
                if is_empty {
                    return Err(self.err_at(
                        XmlErrorKind::InvalidValueForShape(
                            "empty element for externally tagged enum".into(),
                        ),
                        span,
                    ));
                }

                // Read the variant element
                let variant_event = loop {
                    let Some(event) = self.next() else {
                        return Err(self.err(XmlErrorKind::UnexpectedEof));
                    };

                    match &event.event {
                        OwnedEvent::Text { content } if content.trim().is_empty() => {
                            // Skip whitespace
                            continue;
                        }
                        OwnedEvent::Start { .. } | OwnedEvent::Empty { .. } => {
                            break event;
                        }
                        _ => {
                            return Err(self.err_at(
                                XmlErrorKind::UnexpectedEvent(format!(
                                    "expected variant element, got {:?}",
                                    event.event
                                )),
                                event.span(),
                            ));
                        }
                    }
                };

                let variant_span = variant_event.span();
                let (variant_name, variant_attrs, variant_is_empty) = match &variant_event.event {
                    OwnedEvent::Start { name, attributes } => {
                        (name.clone(), attributes.clone(), false)
                    }
                    OwnedEvent::Empty { name, attributes } => {
                        (name.clone(), attributes.clone(), true)
                    }
                    _ => unreachable!(),
                };

                // Find the variant by display name (considering rename)
                let variant = enum_def
                    .variants
                    .iter()
                    .find(|v| get_variant_display_name(v) == variant_name)
                    .ok_or_else(|| {
                        self.err_at(
                            XmlErrorKind::NoMatchingElement(variant_name.to_string()),
                            variant_span,
                        )
                    })?;

                // Select the variant using its original name
                partial = partial.select_variant_named(variant.name)?;

                let variant_fields = variant.data.fields;

                match variant.data.kind {
                    StructKind::Unit => {
                        // Unit variant - nothing to deserialize
                        if !variant_is_empty {
                            self.skip_element(&variant_name)?;
                        }
                    }
                    StructKind::Tuple | StructKind::TupleStruct => {
                        // Tuple variant - deserialize fields by position
                        if !variant_is_empty {
                            partial = self.deserialize_tuple_content(
                                partial,
                                variant_fields,
                                &variant_name,
                            )?;
                        }
                        partial = self.set_defaults_for_unset_fields(partial, variant_fields)?;
                    }
                    StructKind::Struct => {
                        // Struct variant - deserialize as struct
                        partial = self.deserialize_attributes(
                            partial,
                            variant_fields,
                            &variant_attrs,
                            false,
                            variant_span,
                        )?;
                        if !variant_is_empty {
                            partial = self.deserialize_element_content(
                                partial,
                                variant_fields,
                                &variant_name,
                                false,
                            )?;
                        }
                        partial = self.set_defaults_for_unset_fields(partial, variant_fields)?;
                    }
                }

                // Skip to the end of the wrapper element
                loop {
                    let Some(event) = self.next() else {
                        return Err(self.err(XmlErrorKind::UnexpectedEof));
                    };

                    match &event.event {
                        OwnedEvent::End { name } if name == element_name => {
                            break;
                        }
                        OwnedEvent::Text { content } if content.trim().is_empty() => {
                            // Skip whitespace
                            continue;
                        }
                        _ => {
                            return Err(self.err_at(
                                XmlErrorKind::UnexpectedEvent(format!(
                                    "expected end of enum wrapper, got {:?}",
                                    event.event
                                )),
                                event.span(),
                            ));
                        }
                    }
                }

                Ok(partial)
            }
            _ => Err(self.err_at(
                XmlErrorKind::UnsupportedShape(format!("cannot deserialize into {:?}", shape.ty)),
                span,
            )),
        }
    }

    /// Deserialize XML attributes into struct fields.
    fn deserialize_attributes<'facet>(
        &mut self,
        partial: Partial<'facet>,
        fields: &[Field],
        attributes: &[(QName, String)],
        deny_unknown: bool,
        element_span: SourceSpan,
    ) -> Result<Partial<'facet>> {
        let mut partial = partial;

        for (attr_name, attr_value) in attributes {
            // Find the field that matches this attribute.
            // Uses namespace-aware matching:
            // - If field has xml::ns, it must match exactly
            // - Otherwise, match any namespace (including "no namespace")
            //
            // NOTE: Unlike elements, attributes do NOT inherit the default namespace (ns_all).
            // In XML, unprefixed attributes are always in "no namespace", even when a default
            // xmlns is declared. Only prefixed attributes (e.g., foo:bar) have a namespace.
            // See: https://www.w3.org/TR/xml-names/#defaulting
            let field_match = fields.iter().enumerate().find(|(_, f)| {
                f.is_xml_attribute() && attr_name.matches(local_name_of(f.name), f.xml_ns())
            });

            if let Some((idx, field)) = field_match {
                log::trace!(
                    "deserialize attribute {} into field {}",
                    attr_name,
                    field.name
                );

                partial = partial.begin_nth_field(idx)?;

                // Check if field has custom deserialization - must check BEFORE navigating
                // into Option/Spanned wrappers, because begin_custom_deserialization needs
                // the field context (parent_field()) to access proxy_shape().
                let has_custom_deser = field.proxy_convert_in_fn().is_some();
                if has_custom_deser {
                    // When using proxy, the proxy type handles the full conversion including
                    // any Option/Spanned wrappers, so we deserialize directly into the proxy.
                    partial = partial.begin_custom_deserialization()?;
                    partial = self.set_scalar_value(partial, attr_value)?;
                    partial = partial.end()?; // end custom deserialization
                } else {
                    // No proxy - handle Option<T> and Spanned<T> wrappers manually
                    let is_option = matches!(&partial.shape().def, Def::Option(_));
                    if is_option {
                        partial = partial.begin_some()?;
                    }

                    // Handle Spanned<T>
                    if is_spanned_shape(partial.shape()) {
                        partial = partial.begin_field("value")?;
                    }

                    // Deserialize the value
                    partial = self.set_scalar_value(partial, attr_value)?;

                    // End Spanned<T> if needed
                    if is_spanned_shape(field.shape()) {
                        partial = partial.end()?; // end value field
                    }

                    // End Option<T> if needed
                    if is_option {
                        partial = partial.end()?; // end Some
                    }
                }

                partial = partial.end()?; // end field
            } else if deny_unknown {
                // Unknown attribute when deny_unknown_fields is set
                let expected: Vec<&'static str> = fields
                    .iter()
                    .filter(|f| f.is_xml_attribute())
                    .map(|f| f.name)
                    .collect();
                return Err(self.err_at(
                    XmlErrorKind::UnknownAttribute {
                        attribute: attr_name.to_string(),
                        expected,
                    },
                    element_span,
                ));
            }
            // Otherwise ignore unknown attributes
        }

        Ok(partial)
    }

    /// Deserialize child elements and text content.
    fn deserialize_element_content<'facet>(
        &mut self,
        partial: Partial<'facet>,
        fields: &[Field],
        parent_element_name: &QName,
        deny_unknown: bool,
    ) -> Result<Partial<'facet>> {
        let mut partial = partial;
        let mut text_content = String::new();

        // Track which element fields are lists (for xml::elements)
        let mut elements_field_started: Option<usize> = None;

        loop {
            let Some(event) = self.next() else {
                return Err(self.err(XmlErrorKind::UnexpectedEof));
            };

            let span = event.span();

            match event.event {
                OwnedEvent::End { ref name } if name == parent_element_name => {
                    // End any open elements list field
                    // Note: begin_list() doesn't push a frame, so we only end the field
                    if elements_field_started.is_some() {
                        partial = partial.end()?; // end the elements field
                    }

                    // Handle accumulated text content
                    if !text_content.is_empty() {
                        partial = self.set_text_field(partial, fields, &text_content)?;
                    }

                    break;
                }
                OwnedEvent::Start { name, attributes } => {
                    partial = self.deserialize_child_element(
                        partial,
                        fields,
                        &name,
                        &attributes,
                        span,
                        false,
                        &mut elements_field_started,
                        deny_unknown,
                    )?;
                }
                OwnedEvent::Empty { name, attributes } => {
                    partial = self.deserialize_child_element(
                        partial,
                        fields,
                        &name,
                        &attributes,
                        span,
                        true,
                        &mut elements_field_started,
                        deny_unknown,
                    )?;
                }
                OwnedEvent::Text { content } | OwnedEvent::CData { content } => {
                    text_content.push_str(&content);
                }
                OwnedEvent::End { name } => {
                    // End tag for a different element - this shouldn't happen
                    return Err(self.err_at(
                        XmlErrorKind::UnexpectedEvent(format!(
                            "unexpected end tag for '{name}' while parsing '{parent_element_name}'"
                        )),
                        span,
                    ));
                }
                OwnedEvent::Eof => {
                    return Err(self.err(XmlErrorKind::UnexpectedEof));
                }
            }
        }

        Ok(partial)
    }

    /// Deserialize tuple struct content - fields are numbered elements like `<_0>`, `<_1>`, etc.
    fn deserialize_tuple_content<'facet>(
        &mut self,
        partial: Partial<'facet>,
        fields: &[Field],
        parent_element_name: &QName,
    ) -> Result<Partial<'facet>> {
        let mut partial = partial;
        let mut field_idx = 0;

        loop {
            let Some(event) = self.next() else {
                return Err(self.err(XmlErrorKind::UnexpectedEof));
            };

            let span = event.span();

            match event.event {
                OwnedEvent::End { ref name } if name == parent_element_name => {
                    break;
                }
                OwnedEvent::Start { name, attributes } => {
                    if field_idx >= fields.len() {
                        return Err(self.err_at(
                            XmlErrorKind::UnexpectedEvent(format!(
                                "too many elements for tuple struct (expected {})",
                                fields.len()
                            )),
                            span,
                        ));
                    }

                    partial = partial.begin_nth_field(field_idx)?;

                    // Handle Option<T>
                    let is_option = matches!(&partial.shape().def, Def::Option(_));
                    if is_option {
                        partial = partial.begin_some()?;
                    }

                    partial = self.deserialize_element(partial, &name, &attributes, span, false)?;
                    if is_option {
                        partial = partial.end()?; // end Some
                    }
                    partial = partial.end()?; // end field
                    field_idx += 1;
                }
                OwnedEvent::Empty { name, attributes } => {
                    if field_idx >= fields.len() {
                        return Err(self.err_at(
                            XmlErrorKind::UnexpectedEvent(format!(
                                "too many elements for tuple struct (expected {})",
                                fields.len()
                            )),
                            span,
                        ));
                    }

                    partial = partial.begin_nth_field(field_idx)?;

                    // Handle Option<T>
                    let is_option = matches!(&partial.shape().def, Def::Option(_));
                    if is_option {
                        partial = partial.begin_some()?;
                    }

                    partial = self.deserialize_element(partial, &name, &attributes, span, true)?;
                    if is_option {
                        partial = partial.end()?; // end Some
                    }
                    partial = partial.end()?; // end field
                    field_idx += 1;
                }
                OwnedEvent::Text { .. } | OwnedEvent::CData { .. } => {
                    // Ignore text content in tuple structs
                }
                OwnedEvent::End { name } => {
                    return Err(self.err_at(
                        XmlErrorKind::UnexpectedEvent(format!(
                            "unexpected end tag for '{name}' while parsing '{parent_element_name}'"
                        )),
                        span,
                    ));
                }
                OwnedEvent::Eof => {
                    return Err(self.err(XmlErrorKind::UnexpectedEof));
                }
            }
        }

        Ok(partial)
    }

    /// Deserialize fixed array content - expects sequential child elements
    fn deserialize_array_content<'facet>(
        &mut self,
        partial: Partial<'facet>,
        array_len: usize,
        parent_element_name: &QName,
    ) -> Result<Partial<'facet>> {
        let mut partial = partial;
        let mut idx = 0;

        loop {
            let Some(event) = self.next() else {
                return Err(self.err(XmlErrorKind::UnexpectedEof));
            };

            let span = event.span();

            match event.event {
                OwnedEvent::End { ref name } if name == parent_element_name => {
                    if idx < array_len {
                        return Err(self.err_at(
                            XmlErrorKind::InvalidValueForShape(format!(
                                "not enough elements for array (got {idx}, expected {array_len})"
                            )),
                            span,
                        ));
                    }
                    break;
                }
                OwnedEvent::Start { name, attributes } => {
                    if idx >= array_len {
                        return Err(self.err_at(
                            XmlErrorKind::InvalidValueForShape(format!(
                                "too many elements for array (expected {array_len})"
                            )),
                            span,
                        ));
                    }
                    partial = partial.begin_nth_field(idx)?;
                    partial = self.deserialize_element(partial, &name, &attributes, span, false)?;
                    partial = partial.end()?;
                    idx += 1;
                }
                OwnedEvent::Empty { name, attributes } => {
                    if idx >= array_len {
                        return Err(self.err_at(
                            XmlErrorKind::InvalidValueForShape(format!(
                                "too many elements for array (expected {array_len})"
                            )),
                            span,
                        ));
                    }
                    partial = partial.begin_nth_field(idx)?;
                    partial = self.deserialize_element(partial, &name, &attributes, span, true)?;
                    partial = partial.end()?;
                    idx += 1;
                }
                OwnedEvent::Text { .. } | OwnedEvent::CData { .. } => {
                    // Ignore whitespace between elements
                }
                OwnedEvent::End { name } => {
                    return Err(self.err_at(
                        XmlErrorKind::UnexpectedEvent(format!(
                            "unexpected end tag for '{name}' while parsing '{parent_element_name}'"
                        )),
                        span,
                    ));
                }
                OwnedEvent::Eof => {
                    return Err(self.err(XmlErrorKind::UnexpectedEof));
                }
            }
        }

        Ok(partial)
    }

    /// Deserialize set content - each child element is a set item
    fn deserialize_set_content<'facet>(
        &mut self,
        partial: Partial<'facet>,
        parent_element_name: &QName,
    ) -> Result<Partial<'facet>> {
        let mut partial = partial;
        partial = partial.begin_set()?;

        loop {
            let Some(event) = self.next() else {
                return Err(self.err(XmlErrorKind::UnexpectedEof));
            };

            let span = event.span();

            match event.event {
                OwnedEvent::End { ref name } if name == parent_element_name => {
                    break;
                }
                OwnedEvent::Start { name, attributes } => {
                    partial = partial.begin_set_item()?;
                    partial = self.deserialize_element(partial, &name, &attributes, span, false)?;
                    partial = partial.end()?; // end set item
                }
                OwnedEvent::Empty { name, attributes } => {
                    partial = partial.begin_set_item()?;
                    partial = self.deserialize_element(partial, &name, &attributes, span, true)?;
                    partial = partial.end()?; // end set item
                }
                OwnedEvent::Text { .. } | OwnedEvent::CData { .. } => {
                    // Ignore whitespace between elements
                }
                OwnedEvent::End { name } => {
                    return Err(self.err_at(
                        XmlErrorKind::UnexpectedEvent(format!(
                            "unexpected end tag for '{name}' while parsing '{parent_element_name}'"
                        )),
                        span,
                    ));
                }
                OwnedEvent::Eof => {
                    return Err(self.err(XmlErrorKind::UnexpectedEof));
                }
            }
        }

        Ok(partial)
    }

    /// Deserialize map content - expects <entry key="...">value</entry> or similar structure
    fn deserialize_map_content<'facet>(
        &mut self,
        partial: Partial<'facet>,
        parent_element_name: &QName,
    ) -> Result<Partial<'facet>> {
        let mut partial = partial;
        partial = partial.begin_map()?;

        loop {
            let Some(event) = self.next() else {
                return Err(self.err(XmlErrorKind::UnexpectedEof));
            };

            let span = event.span();

            match event.event {
                OwnedEvent::End { ref name } if name == parent_element_name => {
                    break;
                }
                OwnedEvent::Start { name, attributes } => {
                    // Map entry: element name is the key, content is the value
                    partial = partial.begin_key()?;
                    partial = partial.set(name.local_name.clone())?;
                    partial = partial.end()?; // end key

                    partial = partial.begin_value()?;
                    // If there's a key attribute, use that as context; otherwise read content
                    partial = self.deserialize_map_entry_value(partial, &name, &attributes)?;
                    partial = partial.end()?; // end value
                }
                OwnedEvent::Empty { name, .. } => {
                    // Empty element as map entry - key is element name, value is default/empty
                    partial = partial.begin_key()?;
                    partial = partial.set(name.local_name.clone())?;
                    partial = partial.end()?; // end key

                    partial = partial.begin_value()?;
                    // Set default value for the map value type
                    let value_shape = partial.shape();
                    if value_shape.is_type::<String>() {
                        partial = partial.set(String::new())?;
                    } else if value_shape.is_type::<bool>() {
                        partial = partial.set(true)?; // presence implies true
                    } else {
                        return Err(self.err_at(
                            XmlErrorKind::InvalidValueForShape(
                                "empty element for non-string/bool map value".into(),
                            ),
                            span,
                        ));
                    }
                    partial = partial.end()?; // end value
                }
                OwnedEvent::Text { .. } | OwnedEvent::CData { .. } => {
                    // Ignore whitespace between elements
                }
                OwnedEvent::End { name } => {
                    return Err(self.err_at(
                        XmlErrorKind::UnexpectedEvent(format!(
                            "unexpected end tag for '{name}' while parsing '{parent_element_name}'"
                        )),
                        span,
                    ));
                }
                OwnedEvent::Eof => {
                    return Err(self.err(XmlErrorKind::UnexpectedEof));
                }
            }
        }

        Ok(partial)
    }

    /// Deserialize the value portion of a map entry
    fn deserialize_map_entry_value<'facet>(
        &mut self,
        partial: Partial<'facet>,
        element_name: &QName,
        _attributes: &[(QName, String)],
    ) -> Result<Partial<'facet>> {
        let mut partial = partial;
        let shape = partial.shape();

        // For scalar values, read text content
        if matches!(&shape.def, Def::Scalar) {
            let text = self.read_text_until_end(element_name)?;
            partial = self.set_scalar_value(partial, &text)?;
            return Ok(partial);
        }

        // For complex values, read the element content
        // This is a simplified version - complex map values would need more work
        let text = self.read_text_until_end(element_name)?;
        if shape.is_type::<String>() {
            partial = partial.set(text)?;
        } else {
            partial = self.set_scalar_value(partial, &text)?;
        }

        Ok(partial)
    }

    /// Deserialize a child element into the appropriate field.
    #[allow(clippy::too_many_arguments)]
    fn deserialize_child_element<'facet>(
        &mut self,
        partial: Partial<'facet>,
        fields: &[Field],
        element_name: &QName,
        attributes: &[(QName, String)],
        span: SourceSpan,
        is_empty: bool,
        elements_field_started: &mut Option<usize>,
        deny_unknown: bool,
    ) -> Result<Partial<'facet>> {
        let mut partial = partial;

        // Get container-level default namespace (xml::ns_all)
        let ns_all = partial.shape().xml_ns_all();

        // First try to find a direct element field match.
        // Uses namespace-aware matching:
        // - If field has xml::ns, it must match exactly
        // - Otherwise, if container has xml::ns_all, use that
        // - Otherwise, match any namespace
        if let Some((idx, field)) = fields.iter().enumerate().find(|(_, f)| {
            f.is_xml_element() && element_name.matches(local_name_of(f.name), f.xml_ns().or(ns_all))
        }) {
            log::trace!("matched element {} to field {}", element_name, field.name);

            // End any open elements list field
            // Note: begin_list() doesn't push a frame, so we only end the field
            if elements_field_started.is_some() {
                partial = partial.end()?; // end previous field
                *elements_field_started = None;
            }

            partial = partial.begin_nth_field(idx)?;

            // Check if field has custom deserialization - must check BEFORE navigating
            // into Option wrappers, because begin_custom_deserialization needs
            // the field context (parent_field()) to access proxy_shape().
            let has_custom_deser = field.proxy_convert_in_fn().is_some();
            if has_custom_deser {
                // When using proxy, the proxy type handles the full conversion including
                // any Option wrappers, so we deserialize directly into the proxy.
                partial = partial.begin_custom_deserialization()?;
                partial =
                    self.deserialize_element(partial, element_name, attributes, span, is_empty)?;
                partial = partial.end()?; // end custom deserialization
            } else {
                // No proxy - handle Option<T> wrapper manually
                let is_option = matches!(&partial.shape().def, Def::Option(_));
                if is_option {
                    partial = partial.begin_some()?;
                }

                // Deserialize the element content
                partial =
                    self.deserialize_element(partial, element_name, attributes, span, is_empty)?;

                // End Option<T> if needed
                if is_option {
                    partial = partial.end()?; // end Some
                }
            }

            partial = partial.end()?; // end field
            return Ok(partial);
        }

        // Try to find an elements (list) field that accepts this element
        // We check: 1) if the item type accepts this element name, or
        //           2) if the field name matches the element name (fallback)
        // Uses namespace-aware matching for field name comparison.
        if let Some((idx, _field)) = fields.iter().enumerate().find(|(_, f)| {
            if !f.is_xml_elements() {
                return false;
            }
            // First, check if field name matches element name (common case for Vec<T>)
            // Uses namespace-aware matching with ns_all fallback.
            if element_name.matches(
                local_name_of(get_field_display_name(f)),
                f.xml_ns().or(ns_all),
            ) {
                return true;
            }
            // Otherwise, check if the list item type accepts this element
            let field_shape = f.shape();
            if let Some(item_shape) = get_list_item_shape(field_shape) {
                shape_accepts_element(item_shape, &element_name.local_name)
            } else {
                // Not a list type - shouldn't happen for xml::elements
                false
            }
        }) {
            // If we haven't started this list yet, begin it
            if elements_field_started.is_none() || *elements_field_started != Some(idx) {
                // End previous list field if any
                // Note: begin_list() doesn't push a frame, so we only end the field
                if elements_field_started.is_some() {
                    partial = partial.end()?; // end previous field
                }

                partial = partial.begin_nth_field(idx)?;
                partial = partial.begin_list()?;
                *elements_field_started = Some(idx);
            }

            // Add item to list
            partial = partial.begin_list_item()?;
            partial =
                self.deserialize_element(partial, element_name, attributes, span, is_empty)?;
            partial = partial.end()?; // end list item

            return Ok(partial);
        }

        // No matching field found
        if deny_unknown {
            // Unknown element when deny_unknown_fields is set
            let expected: Vec<&'static str> = fields
                .iter()
                .filter(|f| f.is_xml_element() || f.is_xml_elements())
                .map(|f| f.name)
                .collect();
            return Err(self.err_at(
                XmlErrorKind::UnknownField {
                    field: element_name.to_string(),
                    expected,
                },
                span,
            ));
        }

        // Skip this element
        log::trace!("skipping unknown element: {element_name}");
        if !is_empty {
            self.skip_element(element_name)?;
        }
        Ok(partial)
    }

    /// Set the text content field.
    fn set_text_field<'facet>(
        &mut self,
        partial: Partial<'facet>,
        fields: &[Field],
        text: &str,
    ) -> Result<Partial<'facet>> {
        let mut partial = partial;

        // Trim leading/trailing whitespace from text content
        // (since we don't use quick-xml's trim_text, we do it here)
        let trimmed_text = text.trim();

        // Find the text field
        if let Some((idx, _field)) = fields.iter().enumerate().find(|(_, f)| f.is_xml_text()) {
            partial = partial.begin_nth_field(idx)?;

            // Handle Option<T>
            let is_option = matches!(&partial.shape().def, Def::Option(_));
            if is_option {
                partial = partial.begin_some()?;
            }

            partial = partial.set(trimmed_text.to_string())?;

            // End Option<T> if needed
            if is_option {
                partial = partial.end()?; // end Some
            }

            partial = partial.end()?; // end field
        }
        // If no text field, ignore the text content

        Ok(partial)
    }

    /// Read text content until the end tag.
    ///
    /// Returns the accumulated text with leading/trailing whitespace trimmed.
    fn read_text_until_end(&mut self, element_name: &QName) -> Result<String> {
        let mut text = String::new();

        loop {
            let Some(event) = self.next() else {
                return Err(self.err(XmlErrorKind::UnexpectedEof));
            };

            match event.event {
                OwnedEvent::End { ref name } if name == element_name => {
                    break;
                }
                OwnedEvent::Text { content } | OwnedEvent::CData { content } => {
                    text.push_str(&content);
                }
                other => {
                    return Err(self.err(XmlErrorKind::UnexpectedEvent(format!(
                        "expected text or end tag, got {other:?}"
                    ))));
                }
            }
        }

        // Trim leading/trailing whitespace (since we don't use quick-xml's trim_text)
        Ok(text.trim().to_string())
    }

    /// Skip an element and all its content.
    fn skip_element(&mut self, element_name: &QName) -> Result<()> {
        let mut depth = 1;

        while depth > 0 {
            let Some(event) = self.next() else {
                return Err(self.err(XmlErrorKind::UnexpectedEof));
            };

            match &event.event {
                OwnedEvent::Start { .. } => depth += 1,
                OwnedEvent::End { name } if name == element_name && depth == 1 => {
                    depth -= 1;
                }
                OwnedEvent::End { .. } => depth -= 1,
                OwnedEvent::Empty { .. } => {}
                OwnedEvent::Text { .. } | OwnedEvent::CData { .. } => {}
                OwnedEvent::Eof => return Err(self.err(XmlErrorKind::UnexpectedEof)),
            }
        }

        Ok(())
    }

    /// Set defaults for any unset fields.
    fn set_defaults_for_unset_fields<'facet>(
        &self,
        partial: Partial<'facet>,
        fields: &[Field],
    ) -> Result<Partial<'facet>> {
        use facet_core::Characteristic;
        let mut partial = partial;

        for (idx, field) in fields.iter().enumerate() {
            if partial.is_field_set(idx)? {
                continue;
            }

            let field_has_default = field.has_default();
            let field_type_has_default = field.shape().is(Characteristic::Default);
            let should_skip = field.should_skip_deserializing();
            let field_is_option = matches!(field.shape().def, Def::Option(_));

            if field_has_default || field_type_has_default || should_skip {
                log::trace!("setting default for unset field: {}", field.name);
                partial = partial.set_nth_field_to_default(idx)?;
            } else if field_is_option {
                log::trace!("initializing missing Option field `{}` to None", field.name);
                partial = partial.begin_field(field.name)?;
                partial = partial.set_default()?;
                partial = partial.end()?;
            }
        }

        Ok(partial)
    }

    /// Set a scalar value on the partial based on its type.
    fn set_scalar_value<'facet>(
        &self,
        partial: Partial<'facet>,
        value: &str,
    ) -> Result<Partial<'facet>> {
        let mut partial = partial;
        let shape = partial.shape();

        // Priority 1: Check for builder_shape (immutable collections like Bytes -> BytesMut)
        if shape.builder_shape.is_some() {
            partial = partial.begin_inner()?;
            partial = self.set_scalar_value(partial, value)?;
            partial = partial.end()?;
            return Ok(partial);
        }

        // Priority 2: Check for .inner (transparent wrappers like NonZero)
        // Collections (List/Map/Set/Array) have .inner for variance but shouldn't use this path
        if shape.inner.is_some()
            && !matches!(
                &shape.def,
                Def::List(_) | Def::Map(_) | Def::Set(_) | Def::Array(_)
            )
        {
            partial = partial.begin_inner()?;
            partial = self.set_scalar_value(partial, value)?;
            partial = partial.end()?;
            return Ok(partial);
        }

        // Handle usize and isize explicitly before other numeric types
        if shape.is_type::<usize>() {
            let n: usize = value.parse().map_err(|_| {
                self.err(XmlErrorKind::InvalidValueForShape(format!(
                    "cannot parse `{value}` as usize"
                )))
            })?;
            partial = partial.set(n)?;
            return Ok(partial);
        }

        if shape.is_type::<isize>() {
            let n: isize = value.parse().map_err(|_| {
                self.err(XmlErrorKind::InvalidValueForShape(format!(
                    "cannot parse `{value}` as isize"
                )))
            })?;
            partial = partial.set(n)?;
            return Ok(partial);
        }

        // Try numeric types
        if let Type::Primitive(PrimitiveType::Numeric(numeric_type)) = shape.ty {
            let size = match shape.layout {
                ShapeLayout::Sized(layout) => layout.size(),
                ShapeLayout::Unsized => {
                    return Err(self.err(XmlErrorKind::InvalidValueForShape(
                        "cannot assign to unsized type".into(),
                    )));
                }
            };

            return self.set_numeric_value(partial, value, numeric_type, size);
        }

        // Boolean
        if shape.is_type::<bool>() {
            let b = match value.to_lowercase().as_str() {
                "true" | "1" | "yes" => true,
                "false" | "0" | "no" => false,
                _ => {
                    return Err(self.err(XmlErrorKind::InvalidValueForShape(format!(
                        "cannot parse `{value}` as boolean"
                    ))));
                }
            };
            partial = partial.set(b)?;
            return Ok(partial);
        }

        // Char
        if shape.is_type::<char>() {
            let mut chars = value.chars();
            let c = chars.next().ok_or_else(|| {
                self.err(XmlErrorKind::InvalidValueForShape(
                    "empty string cannot be converted to char".into(),
                ))
            })?;
            if chars.next().is_some() {
                return Err(self.err(XmlErrorKind::InvalidValueForShape(
                    "string has more than one character".into(),
                )));
            }
            partial = partial.set(c)?;
            return Ok(partial);
        }

        // String
        if shape.is_type::<String>() {
            partial = partial.set(value.to_string())?;
            return Ok(partial);
        }

        // Try parse_from_str for other types (IpAddr, DateTime, etc.)
        if partial.shape().vtable.has_parse() {
            partial = partial
                .parse_from_str(value)
                .map_err(|e| self.err(XmlErrorKind::Reflect(e)))?;
            return Ok(partial);
        }

        // Last resort: try setting as string
        partial = partial
            .set(value.to_string())
            .map_err(|e| self.err(XmlErrorKind::Reflect(e)))?;

        Ok(partial)
    }

    /// Set a numeric value with proper type conversion.
    fn set_numeric_value<'facet>(
        &self,
        partial: Partial<'facet>,
        value: &str,
        numeric_type: NumericType,
        size: usize,
    ) -> Result<Partial<'facet>> {
        let mut partial = partial;
        match numeric_type {
            NumericType::Integer { signed: false } => {
                let n: u64 = value.parse().map_err(|_| {
                    self.err(XmlErrorKind::InvalidValueForShape(format!(
                        "cannot parse `{value}` as unsigned integer"
                    )))
                })?;

                match size {
                    1 => {
                        let v = u8::try_from(n).map_err(|_| {
                            self.err(XmlErrorKind::InvalidValueForShape(format!(
                                "`{value}` out of range for u8"
                            )))
                        })?;
                        partial = partial.set(v)?;
                    }
                    2 => {
                        let v = u16::try_from(n).map_err(|_| {
                            self.err(XmlErrorKind::InvalidValueForShape(format!(
                                "`{value}` out of range for u16"
                            )))
                        })?;
                        partial = partial.set(v)?;
                    }
                    4 => {
                        let v = u32::try_from(n).map_err(|_| {
                            self.err(XmlErrorKind::InvalidValueForShape(format!(
                                "`{value}` out of range for u32"
                            )))
                        })?;
                        partial = partial.set(v)?;
                    }
                    8 => {
                        partial = partial.set(n)?;
                    }
                    16 => {
                        let n: u128 = value.parse().map_err(|_| {
                            self.err(XmlErrorKind::InvalidValueForShape(format!(
                                "cannot parse `{value}` as u128"
                            )))
                        })?;
                        partial = partial.set(n)?;
                    }
                    _ => {
                        return Err(self.err(XmlErrorKind::InvalidValueForShape(format!(
                            "unsupported unsigned integer size: {size}"
                        ))));
                    }
                }
            }
            NumericType::Integer { signed: true } => {
                let n: i64 = value.parse().map_err(|_| {
                    self.err(XmlErrorKind::InvalidValueForShape(format!(
                        "cannot parse `{value}` as signed integer"
                    )))
                })?;

                match size {
                    1 => {
                        let v = i8::try_from(n).map_err(|_| {
                            self.err(XmlErrorKind::InvalidValueForShape(format!(
                                "`{value}` out of range for i8"
                            )))
                        })?;
                        partial = partial.set(v)?;
                    }
                    2 => {
                        let v = i16::try_from(n).map_err(|_| {
                            self.err(XmlErrorKind::InvalidValueForShape(format!(
                                "`{value}` out of range for i16"
                            )))
                        })?;
                        partial = partial.set(v)?;
                    }
                    4 => {
                        let v = i32::try_from(n).map_err(|_| {
                            self.err(XmlErrorKind::InvalidValueForShape(format!(
                                "`{value}` out of range for i32"
                            )))
                        })?;
                        partial = partial.set(v)?;
                    }
                    8 => {
                        partial = partial.set(n)?;
                    }
                    16 => {
                        let n: i128 = value.parse().map_err(|_| {
                            self.err(XmlErrorKind::InvalidValueForShape(format!(
                                "cannot parse `{value}` as i128"
                            )))
                        })?;
                        partial = partial.set(n)?;
                    }
                    _ => {
                        return Err(self.err(XmlErrorKind::InvalidValueForShape(format!(
                            "unsupported signed integer size: {size}"
                        ))));
                    }
                }
            }
            NumericType::Float => match size {
                4 => {
                    let v: f32 = value.parse().map_err(|_| {
                        self.err(XmlErrorKind::InvalidValueForShape(format!(
                            "cannot parse `{value}` as f32"
                        )))
                    })?;
                    partial = partial.set(v)?;
                }
                8 => {
                    let v: f64 = value.parse().map_err(|_| {
                        self.err(XmlErrorKind::InvalidValueForShape(format!(
                            "cannot parse `{value}` as f64"
                        )))
                    })?;
                    partial = partial.set(v)?;
                }
                _ => {
                    return Err(self.err(XmlErrorKind::InvalidValueForShape(format!(
                        "unsupported float size: {size}"
                    ))));
                }
            },
        }

        Ok(partial)
    }

    /// Deserialize adjacently tagged enum content.
    /// Format: <Element tag="Variant"><content>...</content></Element>
    fn deserialize_adjacently_tagged_content<'facet>(
        &mut self,
        partial: Partial<'facet>,
        variant: &Variant,
        content_tag: &str,
        parent_element_name: &QName,
    ) -> Result<Partial<'facet>> {
        let mut partial = partial;
        let variant_fields = variant.data.fields;

        loop {
            let Some(event) = self.next() else {
                return Err(self.err(XmlErrorKind::UnexpectedEof));
            };

            let span = event.span();

            match event.event {
                OwnedEvent::End { ref name } if name == parent_element_name => {
                    // End of wrapper - set defaults for unset fields
                    partial = self.set_defaults_for_unset_fields(partial, variant_fields)?;
                    break;
                }
                OwnedEvent::Start {
                    ref name,
                    ref attributes,
                } if name == content_tag => {
                    // Found content element - deserialize based on variant kind
                    match variant.data.kind {
                        StructKind::Unit => {
                            // Unit variant - skip content
                            self.skip_element(name)?;
                        }
                        StructKind::Tuple | StructKind::TupleStruct => {
                            partial =
                                self.deserialize_tuple_content(partial, variant_fields, name)?;
                        }
                        StructKind::Struct => {
                            partial = self.deserialize_attributes(
                                partial,
                                variant_fields,
                                attributes,
                                false,
                                span,
                            )?;
                            partial = self.deserialize_element_content(
                                partial,
                                variant_fields,
                                name,
                                false,
                            )?;
                        }
                    }
                    partial = self.set_defaults_for_unset_fields(partial, variant_fields)?;
                }
                OwnedEvent::Empty {
                    ref name,
                    ref attributes,
                } if name == content_tag => {
                    // Empty content element
                    match variant.data.kind {
                        StructKind::Unit => {}
                        StructKind::Struct => {
                            partial = self.deserialize_attributes(
                                partial,
                                variant_fields,
                                attributes,
                                false,
                                span,
                            )?;
                        }
                        _ => {}
                    }
                    partial = self.set_defaults_for_unset_fields(partial, variant_fields)?;
                }
                OwnedEvent::Text { ref content } if content.trim().is_empty() => {
                    // Skip whitespace
                    continue;
                }
                _ => {
                    return Err(self.err_at(
                        XmlErrorKind::UnexpectedEvent(format!(
                            "expected content element <{}>, got {:?}",
                            content_tag, event.event
                        )),
                        span,
                    ));
                }
            }
        }

        Ok(partial)
    }

    /// Deserialize untagged enum - try each variant until one succeeds.
    fn deserialize_untagged_enum<'facet>(
        &mut self,
        partial: Partial<'facet>,
        enum_type: &EnumType,
        element_name: &QName,
        attributes: &[(QName, String)],
        span: SourceSpan,
        is_empty: bool,
    ) -> Result<Partial<'facet>> {
        // For untagged enums, we need to try each variant
        // This is tricky because we can't easily "rewind" the partial
        // For now, we'll use a simple heuristic based on available fields

        // Collect child element names to help determine variant
        let saved_pos = self.pos;

        // For untagged enums, try variants in order
        for variant in enum_type.variants.iter() {
            // Try to match this variant
            self.pos = saved_pos; // Reset position for each attempt

            // Allocate a fresh partial for this attempt
            let attempt_partial = Partial::alloc_shape(partial.shape())?;
            let attempt_partial = attempt_partial.select_variant_named(variant.name)?;

            // Try to deserialize into this variant
            let result = self.try_deserialize_variant(
                attempt_partial,
                variant,
                element_name,
                attributes,
                span,
                is_empty,
            );

            if result.is_ok() {
                // Successfully matched - return this partial
                // But we need to transfer the data to the original partial
                // This is complex with the current API, so we return the new one
                return result;
            }
        }

        // No variant matched
        Err(self.err_at(
            XmlErrorKind::InvalidValueForShape("no variant matched for untagged enum".to_string()),
            span,
        ))
    }

    /// Try to deserialize a specific variant.
    fn try_deserialize_variant<'facet>(
        &mut self,
        partial: Partial<'facet>,
        variant: &Variant,
        element_name: &QName,
        attributes: &[(QName, String)],
        span: SourceSpan,
        is_empty: bool,
    ) -> Result<Partial<'facet>> {
        let mut partial = partial;
        let variant_fields = variant.data.fields;

        match variant.data.kind {
            StructKind::Unit => {
                // Unit variant - nothing to deserialize
                if !is_empty {
                    self.skip_element(element_name)?;
                }
            }
            StructKind::Tuple | StructKind::TupleStruct => {
                if !is_empty {
                    partial =
                        self.deserialize_tuple_content(partial, variant_fields, element_name)?;
                }
                partial = self.set_defaults_for_unset_fields(partial, variant_fields)?;
            }
            StructKind::Struct => {
                partial =
                    self.deserialize_attributes(partial, variant_fields, attributes, false, span)?;
                if !is_empty {
                    partial = self.deserialize_element_content(
                        partial,
                        variant_fields,
                        element_name,
                        false,
                    )?;
                }
                partial = self.set_defaults_for_unset_fields(partial, variant_fields)?;
            }
        }

        Ok(partial)
    }

    /// Check if a struct has any flattened fields.
    fn has_flatten_fields(struct_def: &StructType) -> bool {
        struct_def.fields.iter().any(|f| f.is_flattened())
    }

    /// Deserialize a struct with flattened fields using facet-solver.
    ///
    /// This uses a two-pass approach:
    /// 1. Peek mode: Scan all element names and attributes, feed to solver
    /// 2. Deserialize: Use the resolved Configuration to deserialize with proper path handling
    fn deserialize_struct_with_flatten<'facet>(
        &mut self,
        partial: Partial<'facet>,
        struct_def: &StructType,
        element_name: &QName,
        attributes: &[(QName, String)],
        span: SourceSpan,
        is_empty: bool,
    ) -> Result<Partial<'facet>> {
        let mut partial = partial;

        log::trace!(
            "deserialize_struct_with_flatten: {}",
            partial.shape().type_identifier
        );

        // Build the schema for this type
        let schema = Schema::build_auto(partial.shape())
            .map_err(|e| self.err_at(XmlErrorKind::SchemaError(e), span))?;

        // Create the solver
        let mut solver = Solver::new(&schema);

        // Feed attribute names to solver
        for (attr_name, _) in attributes {
            let _decision = solver.see_key(attr_name.local_name.clone());
        }

        // Track child element positions for pass 2
        let mut element_positions: Vec<(String, usize)> = Vec::new();
        let saved_pos = self.pos;

        // ========== PASS 1: Peek mode - scan all child elements ==========
        if !is_empty {
            loop {
                let Some(event) = self.next() else {
                    return Err(self.err(XmlErrorKind::UnexpectedEof));
                };

                match &event.event {
                    OwnedEvent::End { name } if name == element_name => {
                        break;
                    }
                    OwnedEvent::Start { name, .. } | OwnedEvent::Empty { name, .. } => {
                        // Record position before this element
                        let elem_pos = self.pos - 1; // We already consumed this event

                        let key = name.local_name.clone();
                        let _decision = solver.see_key(key.clone());
                        element_positions.push((key, elem_pos));

                        // Skip the element content if it's a Start event
                        if matches!(&event.event, OwnedEvent::Start { .. }) {
                            self.skip_element(name)?;
                        }
                    }
                    OwnedEvent::Text { content } if content.trim().is_empty() => {
                        // Skip whitespace
                        continue;
                    }
                    OwnedEvent::Text { .. } => {
                        // Text content - might be for a text field
                        // For now we skip it in the peek pass
                        continue;
                    }
                    _ => {
                        return Err(self.err_at(
                            XmlErrorKind::UnexpectedEvent(format!(
                                "expected element or end tag, got {:?}",
                                event.event
                            )),
                            event.span(),
                        ));
                    }
                }
            }
        }

        // ========== Get the resolved Configuration ==========
        let config = solver
            .finish()
            .map_err(|e| self.err_at(XmlErrorKind::Solver(e), span))?;

        // ========== PASS 2: Deserialize with proper path handling ==========

        // First, handle attributes using the configuration
        for (attr_name, attr_value) in attributes {
            if let Some(field_info) = config.resolution().field(&attr_name.local_name) {
                let segments = field_info.path.segments();

                // Navigate to the field through the path, tracking Option fields
                let mut option_count = 0;
                for segment in segments {
                    match segment {
                        PathSegment::Field(name) => {
                            partial = partial.begin_field(name)?;
                            // Handle Option fields
                            if matches!(partial.shape().def, Def::Option(_)) {
                                partial = partial.begin_some()?;
                                option_count += 1;
                            }
                        }
                        PathSegment::Variant(_, variant_name) => {
                            partial = partial.select_variant_named(variant_name)?;
                        }
                    }
                }

                // Handle Spanned<T>
                if is_spanned_shape(partial.shape()) {
                    partial = partial.begin_field("value")?;
                }

                // Deserialize the attribute value
                partial = self.set_scalar_value(partial, attr_value)?;

                // Unwind: end Spanned if needed
                if is_spanned_shape(partial.shape()) {
                    partial = partial.end()?;
                }

                // Unwind the path (including Option Some wrappers)
                for _ in 0..option_count {
                    partial = partial.end()?; // end Some
                }
                for segment in segments.iter().rev() {
                    if matches!(segment, PathSegment::Field(_)) {
                        partial = partial.end()?;
                    }
                }
            }
        }

        // Handle elements using the configuration
        // Reset position to replay elements
        self.pos = saved_pos;

        if !is_empty {
            loop {
                let Some(event) = self.next() else {
                    return Err(self.err(XmlErrorKind::UnexpectedEof));
                };

                let event_span = event.span();

                match event.event {
                    OwnedEvent::End { ref name } if name == element_name => {
                        break;
                    }
                    OwnedEvent::Start {
                        ref name,
                        ref attributes,
                    }
                    | OwnedEvent::Empty {
                        ref name,
                        ref attributes,
                    } => {
                        let is_elem_empty = matches!(event.event, OwnedEvent::Empty { .. });

                        if let Some(field_info) = config.resolution().field(&name.local_name) {
                            let segments = field_info.path.segments();

                            // Navigate to the field through the path, tracking Option fields
                            let mut option_count = 0;
                            for segment in segments {
                                match segment {
                                    PathSegment::Field(field_name) => {
                                        partial = partial.begin_field(field_name)?;
                                        // Handle Option fields
                                        if matches!(partial.shape().def, Def::Option(_)) {
                                            partial = partial.begin_some()?;
                                            option_count += 1;
                                        }
                                    }
                                    PathSegment::Variant(_, variant_name) => {
                                        partial = partial.select_variant_named(variant_name)?;
                                    }
                                }
                            }

                            // Deserialize the element
                            partial = self.deserialize_element(
                                partial,
                                name,
                                attributes,
                                event_span,
                                is_elem_empty,
                            )?;

                            // Unwind the path (including Option Some wrappers)
                            for _ in 0..option_count {
                                partial = partial.end()?; // end Some
                            }
                            for segment in segments.iter().rev() {
                                if matches!(segment, PathSegment::Field(_)) {
                                    partial = partial.end()?;
                                }
                            }
                        } else {
                            // Unknown element - skip it
                            if !is_elem_empty {
                                self.skip_element(name)?;
                            }
                        }
                    }
                    OwnedEvent::Text { ref content } if content.trim().is_empty() => {
                        continue;
                    }
                    OwnedEvent::Text { .. } => {
                        // Text content - handle if there's a text field
                        // For now skip
                        continue;
                    }
                    _ => {
                        return Err(self.err_at(
                            XmlErrorKind::UnexpectedEvent(format!(
                                "expected element or end tag, got {:?}",
                                event.event
                            )),
                            event_span,
                        ));
                    }
                }
            }
        }

        // Set defaults for any unset fields
        partial = self.set_defaults_for_unset_fields(partial, struct_def.fields)?;

        Ok(partial)
    }
}
