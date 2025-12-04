//! XML serialization implementation.

use std::io::Write;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use facet_core::{Def, Facet, StructKind};
use facet_reflect::{HasFields, Peek, is_spanned_shape};

use crate::deserialize::XmlFieldExt;
use crate::error::{XmlError, XmlErrorKind};

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
}

impl<W: Write> XmlSerializer<W> {
    fn new(writer: W) -> Self {
        Self { writer }
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
        if let Ok(struct_peek) = peek.into_struct() {
            return self.serialize_struct_as_element(element_name, struct_peek);
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

    /// Try to serialize bytes (Vec<u8>, &[u8], [u8; N]) as base64-encoded element.
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

        // Collect attributes, elements, and text content
        // We store field name (&'static str) instead of &Field to avoid lifetime issues
        let mut attributes: Vec<(&str, String)> = Vec::new();
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
                    attributes.push((field_item.name, value));
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

        // Write opening tag with attributes
        write!(self.writer, "<{}", escape_element_name(element_name))
            .map_err(|e| XmlErrorKind::Io(e.to_string()))?;

        for (attr_name, attr_value) in &attributes {
            write!(
                self.writer,
                " {}=\"{}\"",
                escape_element_name(attr_name),
                escape_attribute_value(attr_value)
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

        // Write child elements
        for (field_item, field_peek) in elements {
            // Handle custom serialization for elements
            if field_item.field.proxy_convert_out_fn().is_some() {
                if let Ok(owned) = field_peek.custom_serialization(field_item.field) {
                    self.serialize_named_element(field_item.name, owned.as_peek())?;
                } else {
                    self.serialize_named_element(field_item.name, field_peek)?;
                }
            } else {
                self.serialize_named_element(field_item.name, field_peek)?;
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

    fn serialize_named_element<'mem, 'facet>(
        &mut self,
        name: &str,
        peek: Peek<'mem, 'facet>,
    ) -> Result<()> {
        // Handle Option<T> - skip if None
        if let Ok(opt_peek) = peek.into_option() {
            if opt_peek.is_none() {
                return Ok(());
            }
            if let Some(inner) = opt_peek.value() {
                return self.serialize_named_element(name, inner);
            }
            return Ok(());
        }

        self.serialize_element(name, peek)
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
