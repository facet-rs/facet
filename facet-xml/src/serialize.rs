//! XML serialization implementation.

use std::io::Write;

use facet_core::Facet;
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

        // For scalars/primitives, serialize as element with text content
        write!(self.writer, "<{}>", escape_element_name(element_name))
            .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
        self.serialize_value(peek)?;
        write!(self.writer, "</{}>", escape_element_name(element_name))
            .map_err(|e| XmlErrorKind::Io(e.to_string()))?;

        Ok(())
    }

    fn serialize_struct_as_element<'mem, 'facet>(
        &mut self,
        element_name: &str,
        struct_peek: facet_reflect::PeekStruct<'mem, 'facet>,
    ) -> Result<()> {
        // Collect attributes, elements, and text content
        // We store field name (&'static str) instead of &Field to avoid lifetime issues
        let mut attributes: Vec<(&str, String)> = Vec::new();
        let mut elements: Vec<(&'static str, Peek<'mem, 'facet>)> = Vec::new();
        let mut elements_list: Vec<Peek<'mem, 'facet>> = Vec::new();
        let mut text_content: Option<Peek<'mem, 'facet>> = None;

        for (field, field_peek) in struct_peek.fields() {
            if field.is_xml_attribute() {
                // Serialize attribute value to string
                if let Some(value) = self.value_to_string(field_peek) {
                    attributes.push((field.name, value));
                }
            } else if field.is_xml_element() {
                elements.push((field.name, field_peek));
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
        for (field_name, field_peek) in elements {
            self.serialize_named_element(field_name, field_peek)?;
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

            // Get the variant's fields
            let fields: Vec<_> = enum_peek.fields().collect();

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

                for (field, field_peek) in fields {
                    self.serialize_element(field.name, field_peek)?;
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

    fn value_to_string<'mem, 'facet>(&self, peek: Peek<'mem, 'facet>) -> Option<String> {
        // Handle Option<T>
        if let Ok(opt_peek) = peek.into_option() {
            if opt_peek.is_none() {
                return None;
            }
            if let Some(inner) = opt_peek.value() {
                return self.value_to_string(inner);
            }
            return None;
        }

        // Handle Spanned<T>
        if is_spanned_shape(peek.shape()) {
            if let Ok(struct_peek) = peek.into_struct() {
                if let Ok(value_peek) = struct_peek.field_by_name("value") {
                    return self.value_to_string(value_peek);
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
