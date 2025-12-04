//! XML serialization implementation.

use std::collections::HashMap;
use std::io::Write;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use facet_core::{Def, Facet, Field, Shape, StructKind};
use facet_reflect::{HasFields, Peek, is_spanned_shape};

use crate::deserialize::{XmlFieldExt, XmlShapeExt};
use crate::error::{XmlError, XmlErrorKind};

/// Well-known XML namespace URIs and their conventional prefixes.
const WELL_KNOWN_NAMESPACES: &[(&str, &str)] = &[
    ("http://www.w3.org/2001/XMLSchema-instance", "xsi"),
    ("http://www.w3.org/2001/XMLSchema", "xs"),
    ("http://www.w3.org/XML/1998/namespace", "xml"),
    ("http://www.w3.org/1999/xlink", "xlink"),
    ("http://www.w3.org/2000/svg", "svg"),
    ("http://www.w3.org/1999/xhtml", "xhtml"),
    ("http://schemas.xmlsoap.org/soap/envelope/", "soap"),
    ("http://www.w3.org/2003/05/soap-envelope", "soap12"),
    ("http://schemas.android.com/apk/res/android", "android"),
];

pub(crate) type Result<T> = std::result::Result<T, XmlError>;

/// Serialize a value of type `T` to an XML string.
///
/// The type `T` must be a struct where fields are marked with XML attributes like
/// `#[facet(xml::element)]`, `#[facet(xml::attribute)]`, or `#[facet(xml::text)]`.
///
/// # Example
/// ```
/// # use facet::Facet;
/// # use facet_xml as xml;
/// # use facet_xml::to_string;
/// #[derive(Facet)]
/// struct Person {
///     #[facet(xml::attribute)]
///     id: u32,
///     #[facet(xml::element)]
///     name: String,
/// }
///
/// # fn main() -> Result<(), facet_xml::XmlError> {
/// let person = Person { id: 42, name: "Alice".into() };
/// let xml = to_string(&person)?;
/// assert_eq!(xml, r#"<Person id="42"><name>Alice</name></Person>"#);
/// # Ok(())
/// # }
/// ```
pub fn to_string<T: Facet<'static>>(value: &T) -> Result<String> {
    let mut output = Vec::new();
    to_writer(&mut output, value)?;
    Ok(String::from_utf8(output).expect("XML output should be valid UTF-8"))
}

/// Serialize a value of type `T` to a writer as XML.
///
/// This is the streaming version of [`to_string`] - it writes directly to any
/// type implementing [`std::io::Write`].
///
/// # Example
///
/// Writing to a `Vec<u8>` buffer:
/// ```
/// # use facet::Facet;
/// # use facet_xml as xml;
/// # use facet_xml::to_writer;
/// #[derive(Facet)]
/// struct Person {
///     #[facet(xml::attribute)]
///     id: u32,
///     #[facet(xml::element)]
///     name: String,
/// }
///
/// # fn main() -> Result<(), facet_xml::XmlError> {
/// let person = Person { id: 42, name: "Alice".into() };
/// let mut buffer = Vec::new();
/// to_writer(&mut buffer, &person)?;
/// let xml = String::from_utf8(buffer).unwrap();
/// assert_eq!(xml, r#"<Person id="42"><name>Alice</name></Person>"#);
/// # Ok(())
/// # }
/// ```
pub fn to_writer<W: Write, T: Facet<'static>>(writer: &mut W, value: &T) -> Result<()> {
    let peek = Peek::new(value);
    let mut serializer = XmlSerializer::new(writer);

    // Get the type name for the root element
    let type_name = peek.shape().type_identifier;
    serializer.serialize_element(type_name, peek)
}

struct XmlSerializer<W> {
    writer: W,
    /// Namespace URI -> prefix mapping for already-declared namespaces.
    declared_namespaces: HashMap<String, String>,
    /// Counter for auto-generating namespace prefixes (ns0, ns1, ...).
    next_ns_index: usize,
}

impl<W: Write> XmlSerializer<W> {
    fn new(writer: W) -> Self {
        Self {
            writer,
            declared_namespaces: HashMap::new(),
            next_ns_index: 0,
        }
    }

    /// Get or create a prefix for the given namespace URI.
    /// Returns the prefix (without colon).
    ///
    /// Note: We always need to emit xmlns declarations on each element that uses a prefix,
    /// because XML namespace declarations are scoped to the element and its descendants.
    /// A declaration on a sibling or earlier element doesn't apply.
    fn get_or_create_prefix(&mut self, namespace_uri: &str) -> String {
        // Check if we've already assigned a prefix to this URI
        if let Some(prefix) = self.declared_namespaces.get(namespace_uri) {
            return prefix.clone();
        }

        // Try well-known namespaces
        let prefix = WELL_KNOWN_NAMESPACES
            .iter()
            .find(|(uri, _)| *uri == namespace_uri)
            .map(|(_, prefix)| (*prefix).to_string())
            .unwrap_or_else(|| {
                // Auto-generate a prefix
                let prefix = format!("ns{}", self.next_ns_index);
                self.next_ns_index += 1;
                prefix
            });

        // Ensure the prefix isn't already in use for a different namespace
        let final_prefix = if self.declared_namespaces.values().any(|p| p == &prefix) {
            // Conflict! Generate a new one
            let prefix = format!("ns{}", self.next_ns_index);
            self.next_ns_index += 1;
            prefix
        } else {
            prefix
        };

        self.declared_namespaces
            .insert(namespace_uri.to_string(), final_prefix.clone());
        final_prefix
    }

    /// Get the effective namespace for a field, considering field-level xml::ns
    /// and container-level xml::ns_all.
    fn get_field_namespace(field: &Field, ns_all: Option<&'static str>) -> Option<&'static str> {
        field.xml_ns().or(ns_all)
    }

    fn serialize_element<'mem, 'facet>(
        &mut self,
        element_name: &str,
        peek: Peek<'mem, 'facet>,
    ) -> Result<()> {
        // Handle Option<T> - skip if None
        if let Ok(opt_peek) = peek.into_option() {
            if opt_peek.is_none() {
                return Ok(());
            }
            if let Some(inner) = opt_peek.value() {
                return self.serialize_element(element_name, inner);
            }
            return Ok(());
        }

        // Handle Spanned<T> - unwrap to the inner value
        if is_spanned_shape(peek.shape()) {
            if let Ok(struct_peek) = peek.into_struct() {
                if let Ok(value_field) = struct_peek.field_by_name("value") {
                    return self.serialize_element(element_name, value_field);
                }
            }
        }

        // Check if this is a struct
        let shape = peek.shape();
        if let Ok(struct_peek) = peek.into_struct() {
            return self.serialize_struct_as_element(element_name, struct_peek, shape);
        }

        // Check if this is an enum
        if let Ok(enum_peek) = peek.into_enum() {
            return self.serialize_enum_as_element(element_name, enum_peek);
        }

        // Check if this is a byte slice/array - serialize as base64
        if self.try_serialize_bytes_as_element(element_name, peek)? {
            return Ok(());
        }

        // Check if this is a list-like type (Vec, array, set)
        if let Ok(list_peek) = peek.into_list_like() {
            return self.serialize_list_as_element(element_name, list_peek);
        }

        // Check if this is a map
        if let Ok(map_peek) = peek.into_map() {
            return self.serialize_map_as_element(element_name, map_peek);
        }

        // For scalars/primitives, serialize as element with text content
        write!(self.writer, "<{}>", escape_element_name(element_name))
            .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
        self.serialize_value(peek)?;
        write!(self.writer, "</{}>", escape_element_name(element_name))
            .map_err(|e| XmlErrorKind::Io(e.to_string()))?;

        Ok(())
    }

    fn serialize_enum_as_element<'mem, 'facet>(
        &mut self,
        element_name: &str,
        enum_peek: facet_reflect::PeekEnum<'mem, 'facet>,
    ) -> Result<()> {
        let shape = enum_peek.shape();
        let variant_name = enum_peek
            .variant_name_active()
            .map_err(|_| XmlErrorKind::SerializeUnknownElementType)?;

        let fields: Vec<_> = enum_peek.fields_for_serialize().collect();

        // Determine enum tagging strategy
        let is_untagged = shape.is_untagged();
        let tag_attr = shape.get_tag_attr();
        let content_attr = shape.get_content_attr();

        if is_untagged {
            // Untagged: serialize content directly with element name
            self.serialize_enum_content(element_name, variant_name, &fields)?;
        } else if let Some(tag) = tag_attr {
            if let Some(content) = content_attr {
                // Adjacently tagged: <Element tag="Variant"><content>...</content></Element>
                write!(
                    self.writer,
                    "<{} {}=\"{}\">",
                    escape_element_name(element_name),
                    escape_element_name(tag),
                    escape_attribute_value(variant_name)
                )
                .map_err(|e| XmlErrorKind::Io(e.to_string()))?;

                if !fields.is_empty() {
                    // Wrap content in the content element
                    write!(self.writer, "<{}>", escape_element_name(content))
                        .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
                    self.serialize_variant_fields(&fields)?;
                    write!(self.writer, "</{}>", escape_element_name(content))
                        .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
                }

                write!(self.writer, "</{}>", escape_element_name(element_name))
                    .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
            } else {
                // Internally tagged: <Element tag="Variant">...fields...</Element>
                write!(
                    self.writer,
                    "<{} {}=\"{}\">",
                    escape_element_name(element_name),
                    escape_element_name(tag),
                    escape_attribute_value(variant_name)
                )
                .map_err(|e| XmlErrorKind::Io(e.to_string()))?;

                // Serialize fields directly (not wrapped in variant element)
                self.serialize_variant_fields(&fields)?;

                write!(self.writer, "</{}>", escape_element_name(element_name))
                    .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
            }
        } else {
            // Externally tagged (default): <Element><Variant>...</Variant></Element>
            write!(self.writer, "<{}>", escape_element_name(element_name))
                .map_err(|e| XmlErrorKind::Io(e.to_string()))?;

            if fields.is_empty() {
                // Unit variant - just the variant name as an empty element
                write!(self.writer, "<{}/>", escape_element_name(variant_name))
                    .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
            } else if fields.len() == 1 && fields[0].0.name.parse::<usize>().is_ok() {
                // Newtype variant - serialize the inner value with variant name
                self.serialize_element(variant_name, fields[0].1)?;
            } else {
                // Struct-like variant
                write!(self.writer, "<{}>", escape_element_name(variant_name))
                    .map_err(|e| XmlErrorKind::Io(e.to_string()))?;

                for (field_item, field_peek) in fields {
                    self.serialize_element(field_item.name, field_peek)?;
                }

                write!(self.writer, "</{}>", escape_element_name(variant_name))
                    .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
            }

            write!(self.writer, "</{}>", escape_element_name(element_name))
                .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
        }

        Ok(())
    }

    /// Serialize enum content for untagged enums
    fn serialize_enum_content<'mem, 'facet>(
        &mut self,
        element_name: &str,
        variant_name: &str,
        fields: &[(facet_reflect::FieldItem, Peek<'mem, 'facet>)],
    ) -> Result<()> {
        if fields.is_empty() {
            // Unit variant - empty element
            write!(self.writer, "<{}/>", escape_element_name(element_name))
                .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
        } else if fields.len() == 1 && fields[0].0.name.parse::<usize>().is_ok() {
            // Newtype variant - serialize inner directly
            self.serialize_element(element_name, fields[0].1)?;
        } else {
            // Struct-like variant - serialize as struct
            write!(self.writer, "<{}>", escape_element_name(element_name))
                .map_err(|e| XmlErrorKind::Io(e.to_string()))?;

            for (field_item, field_peek) in fields {
                // Use variant_name as hint for structs in untagged context
                let _ = variant_name; // Available if needed for disambiguation
                self.serialize_element(field_item.name, *field_peek)?;
            }

            write!(self.writer, "</{}>", escape_element_name(element_name))
                .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
        }
        Ok(())
    }

    /// Serialize variant fields without wrapper
    fn serialize_variant_fields<'mem, 'facet>(
        &mut self,
        fields: &[(facet_reflect::FieldItem, Peek<'mem, 'facet>)],
    ) -> Result<()> {
        if fields.len() == 1 && fields[0].0.name.parse::<usize>().is_ok() {
            // Single tuple field - serialize value directly
            self.serialize_value(fields[0].1)?;
        } else {
            for (field_item, field_peek) in fields {
                self.serialize_element(field_item.name, *field_peek)?;
            }
        }
        Ok(())
    }

    fn serialize_list_as_element<'mem, 'facet>(
        &mut self,
        element_name: &str,
        list_peek: facet_reflect::PeekListLike<'mem, 'facet>,
    ) -> Result<()> {
        write!(self.writer, "<{}>", escape_element_name(element_name))
            .map_err(|e| XmlErrorKind::Io(e.to_string()))?;

        for item in list_peek.iter() {
            self.serialize_list_item_element(item)?;
        }

        write!(self.writer, "</{}>", escape_element_name(element_name))
            .map_err(|e| XmlErrorKind::Io(e.to_string()))?;

        Ok(())
    }

    fn serialize_map_as_element<'mem, 'facet>(
        &mut self,
        element_name: &str,
        map_peek: facet_reflect::PeekMap<'mem, 'facet>,
    ) -> Result<()> {
        write!(self.writer, "<{}>", escape_element_name(element_name))
            .map_err(|e| XmlErrorKind::Io(e.to_string()))?;

        for (key, value) in map_peek.iter() {
            // Use the key as the element name
            if let Some(key_str) = key.as_str() {
                self.serialize_element(key_str, value)?;
            } else if let Some(key_val) = value_to_string(key) {
                self.serialize_element(&key_val, value)?;
            } else {
                // Fallback: use "entry" as element name with key as text
                write!(self.writer, "<entry>").map_err(|e| XmlErrorKind::Io(e.to_string()))?;
                self.serialize_value(key)?;
                self.serialize_value(value)?;
                write!(self.writer, "</entry>").map_err(|e| XmlErrorKind::Io(e.to_string()))?;
            }
        }

        write!(self.writer, "</{}>", escape_element_name(element_name))
            .map_err(|e| XmlErrorKind::Io(e.to_string()))?;

        Ok(())
    }

    /// Try to serialize bytes (`Vec<u8>`, `&[u8]`, `[u8; N]`) as base64-encoded element.
    /// Returns Ok(true) if bytes were handled, Ok(false) if not bytes.
    fn try_serialize_bytes_as_element<'mem, 'facet>(
        &mut self,
        element_name: &str,
        peek: Peek<'mem, 'facet>,
    ) -> Result<bool> {
        let shape = peek.shape();

        // Check for Vec<u8>
        if let Def::List(ld) = &shape.def {
            if ld.t().is_type::<u8>() {
                if let Some(bytes) = peek.as_bytes() {
                    let encoded = BASE64_STANDARD.encode(bytes);
                    write!(self.writer, "<{}>", escape_element_name(element_name))
                        .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
                    write!(self.writer, "{}", escape_text(&encoded))
                        .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
                    write!(self.writer, "</{}>", escape_element_name(element_name))
                        .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
                    return Ok(true);
                }
            }
        }

        // Check for [u8; N]
        if let Def::Array(ad) = &shape.def {
            if ad.t().is_type::<u8>() {
                // Collect bytes from the array
                if let Ok(list_peek) = peek.into_list_like() {
                    let bytes: Vec<u8> = list_peek
                        .iter()
                        .filter_map(|p| p.get::<u8>().ok().copied())
                        .collect();
                    let encoded = BASE64_STANDARD.encode(&bytes);
                    write!(self.writer, "<{}>", escape_element_name(element_name))
                        .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
                    write!(self.writer, "{}", escape_text(&encoded))
                        .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
                    write!(self.writer, "</{}>", escape_element_name(element_name))
                        .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
                    return Ok(true);
                }
            }
        }

        // Check for &[u8]
        if let Def::Slice(sd) = &shape.def {
            if sd.t().is_type::<u8>() {
                if let Some(bytes) = peek.as_bytes() {
                    let encoded = BASE64_STANDARD.encode(bytes);
                    write!(self.writer, "<{}>", escape_element_name(element_name))
                        .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
                    write!(self.writer, "{}", escape_text(&encoded))
                        .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
                    write!(self.writer, "</{}>", escape_element_name(element_name))
                        .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }

    fn serialize_struct_as_element<'mem, 'facet>(
        &mut self,
        element_name: &str,
        struct_peek: facet_reflect::PeekStruct<'mem, 'facet>,
        shape: &'static Shape,
    ) -> Result<()> {
        match struct_peek.ty().kind {
            StructKind::Unit => {
                // Unit struct - just output empty element
                write!(self.writer, "<{}/>", escape_element_name(element_name))
                    .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
                return Ok(());
            }
            StructKind::Tuple | StructKind::TupleStruct => {
                // Tuple struct - serialize fields in order as child elements
                let fields: Vec<_> = struct_peek.fields_for_serialize().collect();
                if fields.is_empty() {
                    write!(self.writer, "<{}/>", escape_element_name(element_name))
                        .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
                    return Ok(());
                }

                write!(self.writer, "<{}>", escape_element_name(element_name))
                    .map_err(|e| XmlErrorKind::Io(e.to_string()))?;

                for (i, (_, field_peek)) in fields.into_iter().enumerate() {
                    // Use indexed element names for tuple fields
                    let field_name = format!("_{i}");
                    self.serialize_element(&field_name, field_peek)?;
                }

                write!(self.writer, "</{}>", escape_element_name(element_name))
                    .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
                return Ok(());
            }
            StructKind::Struct => {
                // Named struct - fall through to normal handling
            }
        }

        // Get container-level namespace default
        let ns_all = shape.xml_ns_all();

        // Collect attributes (with field info for namespace), elements, and text content
        struct AttrInfo<'a> {
            name: &'a str,
            value: String,
            namespace: Option<&'static str>,
        }
        let mut attributes: Vec<AttrInfo> = Vec::new();
        let mut elements: Vec<(facet_reflect::FieldItem, Peek<'mem, 'facet>)> = Vec::new();
        let mut elements_list: Vec<Peek<'mem, 'facet>> = Vec::new();
        let mut text_content: Option<Peek<'mem, 'facet>> = None;

        for (field_item, field_peek) in struct_peek.fields_for_serialize() {
            let field = &field_item.field;

            // Handle custom serialization for attributes - get value immediately
            if field.is_xml_attribute() {
                let value = if field.proxy_convert_out_fn().is_some() {
                    // Get the intermediate representation for serialization
                    if let Ok(owned) = field_peek.custom_serialization(*field) {
                        value_to_string(owned.as_peek())
                    } else {
                        value_to_string(field_peek)
                    }
                } else {
                    value_to_string(field_peek)
                };
                if let Some(value) = value {
                    let namespace = Self::get_field_namespace(field, ns_all);
                    attributes.push(AttrInfo {
                        name: field_item.name,
                        value,
                        namespace,
                    });
                }
            } else if field.is_xml_element() {
                elements.push((field_item, field_peek));
            } else if field.is_xml_elements() {
                elements_list.push(field_peek);
            } else if field.is_xml_text() {
                text_content = Some(field_peek);
            }
        }

        // Determine if we need content
        let has_content =
            !elements.is_empty() || !elements_list.is_empty() || text_content.is_some();

        // Collect xmlns declarations needed for attributes on this element
        // We always emit xmlns declarations on the element that uses them, because
        // XML namespace scope is limited to an element and its descendants.
        let mut xmlns_decls: Vec<(String, String)> = Vec::new(); // (prefix, uri)
        let mut attr_prefixes: Vec<Option<String>> = Vec::new(); // prefix for each attribute

        for attr in &attributes {
            if let Some(ns_uri) = attr.namespace {
                let prefix = self.get_or_create_prefix(ns_uri);
                // Always emit xmlns declaration on this element
                if !xmlns_decls.iter().any(|(_, u)| u == ns_uri) {
                    xmlns_decls.push((prefix.clone(), ns_uri.to_string()));
                }
                attr_prefixes.push(Some(prefix));
            } else {
                attr_prefixes.push(None);
            }
        }

        // Write opening tag
        write!(self.writer, "<{}", escape_element_name(element_name))
            .map_err(|e| XmlErrorKind::Io(e.to_string()))?;

        // Write xmlns declarations for attributes
        for (prefix, uri) in &xmlns_decls {
            write!(
                self.writer,
                " xmlns:{}=\"{}\"",
                escape_element_name(prefix),
                escape_attribute_value(uri)
            )
            .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
        }

        // Write attributes (with prefix if namespaced)
        for (attr, prefix) in attributes.iter().zip(attr_prefixes.iter()) {
            let attr_name = if let Some(p) = prefix {
                format!("{p}:{}", attr.name)
            } else {
                attr.name.to_string()
            };
            write!(
                self.writer,
                " {}=\"{}\"",
                escape_element_name(&attr_name),
                escape_attribute_value(&attr.value)
            )
            .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
        }

        if !has_content {
            // Self-closing tag
            write!(self.writer, "/>").map_err(|e| XmlErrorKind::Io(e.to_string()))?;
            return Ok(());
        }

        write!(self.writer, ">").map_err(|e| XmlErrorKind::Io(e.to_string()))?;

        // Write text content if present
        if let Some(text_peek) = text_content {
            self.serialize_text_value(text_peek)?;
        }

        // Write child elements (with namespace support)
        for (field_item, field_peek) in elements {
            let field_ns = Self::get_field_namespace(&field_item.field, ns_all);

            // Handle custom serialization for elements
            if field_item.field.proxy_convert_out_fn().is_some() {
                if let Ok(owned) = field_peek.custom_serialization(field_item.field) {
                    self.serialize_namespaced_element(field_item.name, owned.as_peek(), field_ns)?;
                } else {
                    self.serialize_namespaced_element(field_item.name, field_peek, field_ns)?;
                }
            } else {
                self.serialize_namespaced_element(field_item.name, field_peek, field_ns)?;
            }
        }

        // Write elements lists
        for field_peek in elements_list {
            self.serialize_elements_list(field_peek)?;
        }

        // Write closing tag
        write!(self.writer, "</{}>", escape_element_name(element_name))
            .map_err(|e| XmlErrorKind::Io(e.to_string()))?;

        Ok(())
    }

    /// Serialize an element with optional namespace.
    /// If namespace is provided, uses a prefix and emits xmlns declaration if needed.
    fn serialize_namespaced_element<'mem, 'facet>(
        &mut self,
        element_name: &str,
        peek: Peek<'mem, 'facet>,
        namespace: Option<&str>,
    ) -> Result<()> {
        // Handle Option<T> - skip if None
        if let Ok(opt_peek) = peek.into_option() {
            if opt_peek.is_none() {
                return Ok(());
            }
            if let Some(inner) = opt_peek.value() {
                return self.serialize_namespaced_element(element_name, inner, namespace);
            }
            return Ok(());
        }

        // Handle Spanned<T> - unwrap to the inner value
        if is_spanned_shape(peek.shape()) {
            if let Ok(struct_peek) = peek.into_struct() {
                if let Ok(value_field) = struct_peek.field_by_name("value") {
                    return self.serialize_namespaced_element(element_name, value_field, namespace);
                }
            }
        }

        // Determine prefixed name and xmlns declaration
        // We always emit xmlns declarations on each element that uses a prefix,
        // because XML namespace scope is limited to the element and its descendants.
        let (prefixed_name, xmlns_decl) = if let Some(ns_uri) = namespace {
            let prefix = self.get_or_create_prefix(ns_uri);
            let prefixed = format!("{prefix}:{element_name}");
            // Always emit xmlns declaration on this element
            (prefixed, Some((prefix, ns_uri.to_string())))
        } else {
            (element_name.to_string(), None)
        };

        // Check if this is a struct - handle specially for proper namespace propagation
        let shape = peek.shape();
        if let Ok(struct_peek) = peek.into_struct() {
            return self.serialize_struct_as_namespaced_element(
                &prefixed_name,
                struct_peek,
                xmlns_decl,
                shape,
            );
        }

        // Check if this is an enum
        if let Ok(enum_peek) = peek.into_enum() {
            return self.serialize_enum_as_namespaced_element(
                &prefixed_name,
                enum_peek,
                xmlns_decl,
            );
        }

        // For scalars/primitives, serialize as element with text content
        write!(self.writer, "<{}", escape_element_name(&prefixed_name))
            .map_err(|e| XmlErrorKind::Io(e.to_string()))?;

        // Add xmlns declaration if needed
        if let Some((prefix, uri)) = xmlns_decl {
            write!(
                self.writer,
                " xmlns:{}=\"{}\"",
                escape_element_name(&prefix),
                escape_attribute_value(&uri)
            )
            .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
        }

        write!(self.writer, ">").map_err(|e| XmlErrorKind::Io(e.to_string()))?;
        self.serialize_value(peek)?;
        write!(self.writer, "</{}>", escape_element_name(&prefixed_name))
            .map_err(|e| XmlErrorKind::Io(e.to_string()))?;

        Ok(())
    }

    /// Serialize a struct as an element with optional xmlns declaration on the opening tag.
    fn serialize_struct_as_namespaced_element<'mem, 'facet>(
        &mut self,
        prefixed_element_name: &str,
        struct_peek: facet_reflect::PeekStruct<'mem, 'facet>,
        xmlns_decl: Option<(String, String)>,
        shape: &'static Shape,
    ) -> Result<()> {
        match struct_peek.ty().kind {
            StructKind::Unit => {
                // Unit struct - just output empty element with xmlns if needed
                write!(
                    self.writer,
                    "<{}",
                    escape_element_name(prefixed_element_name)
                )
                .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
                if let Some((prefix, uri)) = xmlns_decl {
                    write!(
                        self.writer,
                        " xmlns:{}=\"{}\"",
                        escape_element_name(&prefix),
                        escape_attribute_value(&uri)
                    )
                    .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
                }
                write!(self.writer, "/>").map_err(|e| XmlErrorKind::Io(e.to_string()))?;
                return Ok(());
            }
            StructKind::Tuple | StructKind::TupleStruct => {
                // Tuple struct - serialize fields in order as child elements
                let fields: Vec<_> = struct_peek.fields_for_serialize().collect();
                if fields.is_empty() {
                    write!(
                        self.writer,
                        "<{}",
                        escape_element_name(prefixed_element_name)
                    )
                    .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
                    if let Some((prefix, uri)) = xmlns_decl {
                        write!(
                            self.writer,
                            " xmlns:{}=\"{}\"",
                            escape_element_name(&prefix),
                            escape_attribute_value(&uri)
                        )
                        .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
                    }
                    write!(self.writer, "/>").map_err(|e| XmlErrorKind::Io(e.to_string()))?;
                    return Ok(());
                }

                write!(
                    self.writer,
                    "<{}",
                    escape_element_name(prefixed_element_name)
                )
                .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
                if let Some((prefix, uri)) = xmlns_decl {
                    write!(
                        self.writer,
                        " xmlns:{}=\"{}\"",
                        escape_element_name(&prefix),
                        escape_attribute_value(&uri)
                    )
                    .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
                }
                write!(self.writer, ">").map_err(|e| XmlErrorKind::Io(e.to_string()))?;

                for (i, (_, field_peek)) in fields.into_iter().enumerate() {
                    let field_name = format!("_{i}");
                    self.serialize_element(&field_name, field_peek)?;
                }

                write!(
                    self.writer,
                    "</{}>",
                    escape_element_name(prefixed_element_name)
                )
                .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
                return Ok(());
            }
            StructKind::Struct => {
                // Named struct - fall through to normal handling
            }
        }

        // Get container-level namespace default for the nested struct
        let ns_all = shape.xml_ns_all();

        // Collect attributes, elements, and text content
        struct AttrInfo<'a> {
            name: &'a str,
            value: String,
            namespace: Option<&'static str>,
        }
        let mut attributes: Vec<AttrInfo> = Vec::new();
        let mut elements: Vec<(facet_reflect::FieldItem, Peek<'mem, 'facet>)> = Vec::new();
        let mut elements_list: Vec<Peek<'mem, 'facet>> = Vec::new();
        let mut text_content: Option<Peek<'mem, 'facet>> = None;

        for (field_item, field_peek) in struct_peek.fields_for_serialize() {
            let field = &field_item.field;

            if field.is_xml_attribute() {
                let value = if field.proxy_convert_out_fn().is_some() {
                    if let Ok(owned) = field_peek.custom_serialization(*field) {
                        value_to_string(owned.as_peek())
                    } else {
                        value_to_string(field_peek)
                    }
                } else {
                    value_to_string(field_peek)
                };
                if let Some(value) = value {
                    let namespace = Self::get_field_namespace(field, ns_all);
                    attributes.push(AttrInfo {
                        name: field_item.name,
                        value,
                        namespace,
                    });
                }
            } else if field.is_xml_element() {
                elements.push((field_item, field_peek));
            } else if field.is_xml_elements() {
                elements_list.push(field_peek);
            } else if field.is_xml_text() {
                text_content = Some(field_peek);
            }
        }

        let has_content =
            !elements.is_empty() || !elements_list.is_empty() || text_content.is_some();

        // Collect xmlns declarations needed for attributes
        // We always emit xmlns declarations on each element that uses them.
        let mut xmlns_decls: Vec<(String, String)> = Vec::new();
        let mut attr_prefixes: Vec<Option<String>> = Vec::new();

        // Start with the element's own xmlns declaration if any
        if let Some((prefix, uri)) = xmlns_decl {
            xmlns_decls.push((prefix, uri));
        }

        for attr in &attributes {
            if let Some(ns_uri) = attr.namespace {
                let prefix = self.get_or_create_prefix(ns_uri);
                // Always emit xmlns declaration on this element (if not already)
                if !xmlns_decls.iter().any(|(_, u)| u == ns_uri) {
                    xmlns_decls.push((prefix.clone(), ns_uri.to_string()));
                }
                attr_prefixes.push(Some(prefix));
            } else {
                attr_prefixes.push(None);
            }
        }

        // Write opening tag
        write!(
            self.writer,
            "<{}",
            escape_element_name(prefixed_element_name)
        )
        .map_err(|e| XmlErrorKind::Io(e.to_string()))?;

        // Write all xmlns declarations
        for (prefix, uri) in &xmlns_decls {
            write!(
                self.writer,
                " xmlns:{}=\"{}\"",
                escape_element_name(prefix),
                escape_attribute_value(uri)
            )
            .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
        }

        // Write attributes
        for (attr, prefix) in attributes.iter().zip(attr_prefixes.iter()) {
            let attr_name = if let Some(p) = prefix {
                format!("{p}:{}", attr.name)
            } else {
                attr.name.to_string()
            };
            write!(
                self.writer,
                " {}=\"{}\"",
                escape_element_name(&attr_name),
                escape_attribute_value(&attr.value)
            )
            .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
        }

        if !has_content {
            write!(self.writer, "/>").map_err(|e| XmlErrorKind::Io(e.to_string()))?;
            return Ok(());
        }

        write!(self.writer, ">").map_err(|e| XmlErrorKind::Io(e.to_string()))?;

        if let Some(text_peek) = text_content {
            self.serialize_text_value(text_peek)?;
        }

        for (field_item, field_peek) in elements {
            let field_ns = Self::get_field_namespace(&field_item.field, ns_all);

            if field_item.field.proxy_convert_out_fn().is_some() {
                if let Ok(owned) = field_peek.custom_serialization(field_item.field) {
                    self.serialize_namespaced_element(field_item.name, owned.as_peek(), field_ns)?;
                } else {
                    self.serialize_namespaced_element(field_item.name, field_peek, field_ns)?;
                }
            } else {
                self.serialize_namespaced_element(field_item.name, field_peek, field_ns)?;
            }
        }

        for field_peek in elements_list {
            self.serialize_elements_list(field_peek)?;
        }

        write!(
            self.writer,
            "</{}>",
            escape_element_name(prefixed_element_name)
        )
        .map_err(|e| XmlErrorKind::Io(e.to_string()))?;

        Ok(())
    }

    /// Serialize an enum as an element with optional xmlns declaration.
    fn serialize_enum_as_namespaced_element<'mem, 'facet>(
        &mut self,
        prefixed_element_name: &str,
        enum_peek: facet_reflect::PeekEnum<'mem, 'facet>,
        xmlns_decl: Option<(String, String)>,
    ) -> Result<()> {
        let shape = enum_peek.shape();
        let variant_name = enum_peek
            .variant_name_active()
            .map_err(|_| XmlErrorKind::SerializeUnknownElementType)?;

        let fields: Vec<_> = enum_peek.fields_for_serialize().collect();

        let is_untagged = shape.is_untagged();
        let tag_attr = shape.get_tag_attr();
        let content_attr = shape.get_content_attr();

        // Helper to write opening tag with optional xmlns
        let write_open_tag =
            |writer: &mut W, name: &str, xmlns: &Option<(String, String)>| -> Result<()> {
                write!(writer, "<{}", escape_element_name(name))
                    .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
                if let Some((prefix, uri)) = xmlns {
                    write!(
                        writer,
                        " xmlns:{}=\"{}\"",
                        escape_element_name(prefix),
                        escape_attribute_value(uri)
                    )
                    .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
                }
                Ok(())
            };

        if is_untagged {
            // Untagged: serialize content directly
            if fields.is_empty() {
                write_open_tag(&mut self.writer, prefixed_element_name, &xmlns_decl)?;
                write!(self.writer, "/>").map_err(|e| XmlErrorKind::Io(e.to_string()))?;
            } else if fields.len() == 1 && fields[0].0.name.parse::<usize>().is_ok() {
                // Newtype variant
                write_open_tag(&mut self.writer, prefixed_element_name, &xmlns_decl)?;
                write!(self.writer, ">").map_err(|e| XmlErrorKind::Io(e.to_string()))?;
                self.serialize_value(fields[0].1)?;
                write!(
                    self.writer,
                    "</{}>",
                    escape_element_name(prefixed_element_name)
                )
                .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
            } else {
                write_open_tag(&mut self.writer, prefixed_element_name, &xmlns_decl)?;
                write!(self.writer, ">").map_err(|e| XmlErrorKind::Io(e.to_string()))?;
                for (field_item, field_peek) in fields {
                    self.serialize_element(field_item.name, field_peek)?;
                }
                write!(
                    self.writer,
                    "</{}>",
                    escape_element_name(prefixed_element_name)
                )
                .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
            }
        } else if let Some(tag) = tag_attr {
            if let Some(content) = content_attr {
                // Adjacently tagged
                write_open_tag(&mut self.writer, prefixed_element_name, &xmlns_decl)?;
                write!(
                    self.writer,
                    " {}=\"{}\">",
                    escape_element_name(tag),
                    escape_attribute_value(variant_name)
                )
                .map_err(|e| XmlErrorKind::Io(e.to_string()))?;

                if !fields.is_empty() {
                    write!(self.writer, "<{}>", escape_element_name(content))
                        .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
                    self.serialize_variant_fields(&fields)?;
                    write!(self.writer, "</{}>", escape_element_name(content))
                        .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
                }

                write!(
                    self.writer,
                    "</{}>",
                    escape_element_name(prefixed_element_name)
                )
                .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
            } else {
                // Internally tagged
                write_open_tag(&mut self.writer, prefixed_element_name, &xmlns_decl)?;
                write!(
                    self.writer,
                    " {}=\"{}\">",
                    escape_element_name(tag),
                    escape_attribute_value(variant_name)
                )
                .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
                self.serialize_variant_fields(&fields)?;
                write!(
                    self.writer,
                    "</{}>",
                    escape_element_name(prefixed_element_name)
                )
                .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
            }
        } else {
            // Externally tagged (default)
            write_open_tag(&mut self.writer, prefixed_element_name, &xmlns_decl)?;
            write!(self.writer, ">").map_err(|e| XmlErrorKind::Io(e.to_string()))?;

            if fields.is_empty() {
                write!(self.writer, "<{}/>", escape_element_name(variant_name))
                    .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
            } else if fields.len() == 1 && fields[0].0.name.parse::<usize>().is_ok() {
                self.serialize_element(variant_name, fields[0].1)?;
            } else {
                write!(self.writer, "<{}>", escape_element_name(variant_name))
                    .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
                for (field_item, field_peek) in fields {
                    self.serialize_element(field_item.name, field_peek)?;
                }
                write!(self.writer, "</{}>", escape_element_name(variant_name))
                    .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
            }

            write!(
                self.writer,
                "</{}>",
                escape_element_name(prefixed_element_name)
            )
            .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
        }

        Ok(())
    }

    fn serialize_elements_list<'mem, 'facet>(&mut self, peek: Peek<'mem, 'facet>) -> Result<()> {
        let list_peek = peek
            .into_list()
            .map_err(|_| XmlErrorKind::SerializeNotList)?;

        for item_peek in list_peek.iter() {
            self.serialize_list_item_element(item_peek)?;
        }

        Ok(())
    }

    fn serialize_list_item_element<'mem, 'facet>(
        &mut self,
        peek: Peek<'mem, 'facet>,
    ) -> Result<()> {
        // For enums, use variant name as element name
        if let Ok(enum_peek) = peek.into_enum() {
            let variant_name = enum_peek
                .variant_name_active()
                .map_err(|_| XmlErrorKind::SerializeUnknownElementType)?;

            // Get the variant's fields (respecting skip_serializing)
            let fields: Vec<_> = enum_peek.fields_for_serialize().collect();

            if fields.is_empty() {
                // Unit variant - empty element
                write!(self.writer, "<{}/>", escape_element_name(variant_name))
                    .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
            } else if fields.len() == 1 && fields[0].0.name.parse::<usize>().is_ok() {
                // Tuple variant with single field - serialize the inner value
                self.serialize_element(variant_name, fields[0].1)?;
            } else {
                // Struct-like variant
                write!(self.writer, "<{}>", escape_element_name(variant_name))
                    .map_err(|e| XmlErrorKind::Io(e.to_string()))?;

                for (field_item, field_peek) in fields {
                    self.serialize_element(field_item.name, field_peek)?;
                }

                write!(self.writer, "</{}>", escape_element_name(variant_name))
                    .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
            }
            return Ok(());
        }

        // For structs, use type name as element name
        let type_name = peek.shape().type_identifier;
        self.serialize_element(type_name, peek)
    }

    fn serialize_text_value<'mem, 'facet>(&mut self, peek: Peek<'mem, 'facet>) -> Result<()> {
        // Handle Option<T>
        if let Ok(opt_peek) = peek.into_option() {
            if opt_peek.is_none() {
                return Ok(());
            }
            if let Some(inner) = opt_peek.value() {
                return self.serialize_text_value(inner);
            }
            return Ok(());
        }

        // Handle Spanned<T>
        if is_spanned_shape(peek.shape()) {
            if let Ok(struct_peek) = peek.into_struct() {
                if let Ok(value_peek) = struct_peek.field_by_name("value") {
                    return self.serialize_text_value(value_peek);
                }
            }
        }

        self.serialize_value(peek)
    }

    fn serialize_value<'mem, 'facet>(&mut self, peek: Peek<'mem, 'facet>) -> Result<()> {
        // Handle Option<T>
        if let Ok(opt_peek) = peek.into_option() {
            if opt_peek.is_none() {
                return Ok(());
            }
            if let Some(inner) = opt_peek.value() {
                return self.serialize_value(inner);
            }
            return Ok(());
        }

        // Handle Spanned<T>
        if is_spanned_shape(peek.shape()) {
            if let Ok(struct_peek) = peek.into_struct() {
                if let Ok(value_peek) = struct_peek.field_by_name("value") {
                    return self.serialize_value(value_peek);
                }
            }
        }

        // Unwrap transparent wrappers
        let peek = peek.innermost_peek();

        // Try string first
        if let Some(s) = peek.as_str() {
            write!(self.writer, "{}", escape_text(s))
                .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
            return Ok(());
        }

        // Try various types
        if let Ok(v) = peek.get::<bool>() {
            write!(self.writer, "{v}").map_err(|e| XmlErrorKind::Io(e.to_string()))?;
            return Ok(());
        }

        if let Ok(v) = peek.get::<i8>() {
            write!(self.writer, "{v}").map_err(|e| XmlErrorKind::Io(e.to_string()))?;
            return Ok(());
        }
        if let Ok(v) = peek.get::<i16>() {
            write!(self.writer, "{v}").map_err(|e| XmlErrorKind::Io(e.to_string()))?;
            return Ok(());
        }
        if let Ok(v) = peek.get::<i32>() {
            write!(self.writer, "{v}").map_err(|e| XmlErrorKind::Io(e.to_string()))?;
            return Ok(());
        }
        if let Ok(v) = peek.get::<i64>() {
            write!(self.writer, "{v}").map_err(|e| XmlErrorKind::Io(e.to_string()))?;
            return Ok(());
        }

        if let Ok(v) = peek.get::<u8>() {
            write!(self.writer, "{v}").map_err(|e| XmlErrorKind::Io(e.to_string()))?;
            return Ok(());
        }
        if let Ok(v) = peek.get::<u16>() {
            write!(self.writer, "{v}").map_err(|e| XmlErrorKind::Io(e.to_string()))?;
            return Ok(());
        }
        if let Ok(v) = peek.get::<u32>() {
            write!(self.writer, "{v}").map_err(|e| XmlErrorKind::Io(e.to_string()))?;
            return Ok(());
        }
        if let Ok(v) = peek.get::<u64>() {
            write!(self.writer, "{v}").map_err(|e| XmlErrorKind::Io(e.to_string()))?;
            return Ok(());
        }

        if let Ok(v) = peek.get::<f32>() {
            write!(self.writer, "{v}").map_err(|e| XmlErrorKind::Io(e.to_string()))?;
            return Ok(());
        }
        if let Ok(v) = peek.get::<f64>() {
            write!(self.writer, "{v}").map_err(|e| XmlErrorKind::Io(e.to_string()))?;
            return Ok(());
        }

        if let Ok(v) = peek.get::<char>() {
            write!(self.writer, "{}", escape_text(&v.to_string()))
                .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
            return Ok(());
        }

        Err(XmlErrorKind::SerializeUnknownValueType.into())
    }
}

/// Convert a Peek value to a string representation, handling Options, Spanned, and transparent wrappers.
fn value_to_string<'mem, 'facet>(peek: Peek<'mem, 'facet>) -> Option<String> {
    // Handle Option<T>
    if let Ok(opt_peek) = peek.into_option() {
        if opt_peek.is_none() {
            return None;
        }
        if let Some(inner) = opt_peek.value() {
            return value_to_string(inner);
        }
        return None;
    }

    // Handle Spanned<T>
    if is_spanned_shape(peek.shape()) {
        if let Ok(struct_peek) = peek.into_struct() {
            if let Ok(value_peek) = struct_peek.field_by_name("value") {
                return value_to_string(value_peek);
            }
        }
    }

    // Unwrap transparent wrappers
    let peek = peek.innermost_peek();

    // Try string first
    if let Some(s) = peek.as_str() {
        return Some(s.to_string());
    }

    // Try various types
    if let Ok(v) = peek.get::<bool>() {
        return Some(v.to_string());
    }

    if let Ok(v) = peek.get::<i8>() {
        return Some(v.to_string());
    }
    if let Ok(v) = peek.get::<i16>() {
        return Some(v.to_string());
    }
    if let Ok(v) = peek.get::<i32>() {
        return Some(v.to_string());
    }
    if let Ok(v) = peek.get::<i64>() {
        return Some(v.to_string());
    }

    if let Ok(v) = peek.get::<u8>() {
        return Some(v.to_string());
    }
    if let Ok(v) = peek.get::<u16>() {
        return Some(v.to_string());
    }
    if let Ok(v) = peek.get::<u32>() {
        return Some(v.to_string());
    }
    if let Ok(v) = peek.get::<u64>() {
        return Some(v.to_string());
    }

    if let Ok(v) = peek.get::<f32>() {
        return Some(v.to_string());
    }
    if let Ok(v) = peek.get::<f64>() {
        return Some(v.to_string());
    }

    if let Ok(v) = peek.get::<char>() {
        return Some(v.to_string());
    }

    None
}

/// Escape special characters in XML text content.
fn escape_text(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            '&' => result.push_str("&amp;"),
            _ => result.push(c),
        }
    }
    result
}

/// Escape special characters in XML attribute values.
fn escape_attribute_value(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            '&' => result.push_str("&amp;"),
            '"' => result.push_str("&quot;"),
            '\'' => result.push_str("&apos;"),
            _ => result.push(c),
        }
    }
    result
}

/// Escape element name (for now, assume valid XML names).
fn escape_element_name(name: &str) -> &str {
    name
}
