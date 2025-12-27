//! XML serialization implementation.

use std::collections::HashMap;
use std::io::Write;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use facet_core::{Def, Facet, Field, Shape, StructKind};
use facet_reflect::{HasFields, Peek, is_spanned_shape};

use crate::annotation::{XmlAnnotationPhase, fields_missing_xml_annotations};
use crate::deserialize::{XmlFieldExt, XmlShapeExt};
use crate::error::{MissingAnnotationPhase, XmlError, XmlErrorKind};

/// A function that formats a floating-point number to a writer.
///
/// This is used to customize how `f32` and `f64` values are serialized to XML.
/// The function receives the value (as `f64`, with `f32` values upcast) and
/// a writer to write the formatted output to.
pub type FloatFormatter = fn(f64, &mut dyn Write) -> std::io::Result<()>;

/// Options for XML serialization.
#[derive(Clone)]
pub struct SerializeOptions {
    /// Whether to pretty-print with indentation (default: false)
    pub pretty: bool,
    /// Indentation string for pretty-printing (default: "  ")
    pub indent: &'static str,
    /// Custom formatter for floating-point numbers (f32 and f64).
    /// If `None`, uses the default `Display` implementation.
    pub float_formatter: Option<FloatFormatter>,
    /// Whether to preserve entity references (like `&sup1;`, `&#92;`, `&#x5C;`) in string values.
    ///
    /// When `true`, entity references in strings are not escaped - the `&` in entity references
    /// is left as-is instead of being escaped to `&amp;`. This is useful when serializing
    /// content that already contains entity references (like HTML entities in SVG).
    ///
    /// Default: `false` (all `&` characters are escaped to `&amp;`).
    ///
    /// # Example
    ///
    /// ```
    /// # use facet::Facet;
    /// # use facet_xml as xml;
    /// # use facet_xml::{to_string_with_options, SerializeOptions};
    ///
    /// #[derive(Facet)]
    /// struct Text {
    ///     #[facet(xml::attribute)]
    ///     content: String,
    /// }
    ///
    /// let text = Text { content: ".end&sup1;".to_string() };
    ///
    /// // Without preserve_entities: &sup1; becomes &amp;sup1;
    /// let xml = xml::to_string(&text).unwrap();
    /// assert!(xml.contains("&amp;sup1;"));
    ///
    /// // With preserve_entities: &sup1; is preserved
    /// let options = SerializeOptions::new().preserve_entities(true);
    /// let xml = to_string_with_options(&text, &options).unwrap();
    /// assert!(xml.contains("&sup1;"));
    /// ```
    pub preserve_entities: bool,
}

impl Default for SerializeOptions {
    fn default() -> Self {
        Self {
            pretty: false,
            indent: "  ",
            float_formatter: None,
            preserve_entities: false,
        }
    }
}

impl std::fmt::Debug for SerializeOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SerializeOptions")
            .field("pretty", &self.pretty)
            .field("indent", &self.indent)
            .field("float_formatter", &self.float_formatter.map(|_| "..."))
            .field("preserve_entities", &self.preserve_entities)
            .finish()
    }
}

impl SerializeOptions {
    /// Create new default options (compact output).
    pub fn new() -> Self {
        Self::default()
    }

    /// Enable pretty-printing with default indentation.
    pub fn pretty(mut self) -> Self {
        self.pretty = true;
        self
    }

    /// Set a custom indentation string (implies pretty-printing).
    pub fn indent(mut self, indent: &'static str) -> Self {
        self.indent = indent;
        self.pretty = true;
        self
    }

    /// Get the indent string if pretty-printing is enabled, otherwise None.
    fn indent_str(&self) -> Option<&str> {
        if self.pretty { Some(self.indent) } else { None }
    }

    /// Set a custom formatter for floating-point numbers (f32 and f64).
    ///
    /// The formatter function receives the value as `f64` (f32 values are upcast)
    /// and writes the formatted output to the provided writer.
    ///
    /// # Example
    ///
    /// ```
    /// # use facet::Facet;
    /// # use facet_xml as xml;
    /// # use facet_xml::{to_string_with_options, SerializeOptions};
    /// # use std::io::Write;
    /// fn fmt_g(value: f64, w: &mut dyn Write) -> std::io::Result<()> {
    ///     // Format like C's %g: 6 significant digits, trim trailing zeros
    ///     let s = format!("{:.6}", value);
    ///     let s = s.trim_end_matches('0').trim_end_matches('.');
    ///     write!(w, "{}", s)
    /// }
    ///
    /// #[derive(Facet)]
    /// struct Point {
    ///     #[facet(xml::attribute)]
    ///     x: f64,
    ///     #[facet(xml::attribute)]
    ///     y: f64,
    /// }
    ///
    /// let point = Point { x: 1.5, y: 2.0 };
    /// let options = SerializeOptions::new().float_formatter(fmt_g);
    /// let xml = to_string_with_options(&point, &options).unwrap();
    /// assert_eq!(xml, r#"<Point x="1.5" y="2"/>"#);
    /// ```
    pub fn float_formatter(mut self, formatter: FloatFormatter) -> Self {
        self.float_formatter = Some(formatter);
        self
    }

    /// Enable preservation of entity references in string values.
    ///
    /// When enabled, entity references like `&sup1;`, `&#92;`, `&#x5C;` are not escaped.
    /// The `&` in recognized entity patterns is left as-is instead of being escaped to `&amp;`.
    ///
    /// This is useful when serializing content that already contains entity references,
    /// such as HTML entities in SVG content.
    ///
    /// # Example
    ///
    /// ```
    /// # use facet::Facet;
    /// # use facet_xml as xml;
    /// # use facet_xml::{to_string_with_options, SerializeOptions};
    ///
    /// #[derive(Facet)]
    /// struct Text {
    ///     #[facet(xml::attribute)]
    ///     content: String,
    /// }
    ///
    /// let text = Text { content: ".end&sup1;".to_string() };
    /// let options = SerializeOptions::new().preserve_entities(true);
    /// let xml = to_string_with_options(&text, &options).unwrap();
    /// assert!(xml.contains("&sup1;"));
    /// ```
    pub fn preserve_entities(mut self, preserve: bool) -> Self {
        self.preserve_entities = preserve;
        self
    }
}

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
    to_string_with_options(value, &SerializeOptions::default())
}

/// Serialize a value of type `T` to a pretty-printed XML string.
///
/// This is a convenience function that enables pretty-printing with default indentation.
///
/// # Example
/// ```
/// # use facet::Facet;
/// # use facet_xml as xml;
/// # use facet_xml::to_string_pretty;
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
/// let xml = to_string_pretty(&person)?;
/// // Output will have newlines and indentation
/// # Ok(())
/// # }
/// ```
pub fn to_string_pretty<T: Facet<'static>>(value: &T) -> Result<String> {
    to_string_with_options(value, &SerializeOptions::default().pretty())
}

/// Serialize a value of type `T` to an XML string with custom options.
///
/// # Example
///
/// ```
/// # use facet::Facet;
/// # use facet_xml as xml;
/// # use facet_xml::{to_string_with_options, SerializeOptions};
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
///
/// // Compact output
/// let xml = to_string_with_options(&person, &SerializeOptions::default())?;
/// assert_eq!(xml, r#"<Person id="42"><name>Alice</name></Person>"#);
///
/// // Pretty output with tabs
/// let xml = to_string_with_options(&person, &SerializeOptions::default().indent("\t"))?;
/// # Ok(())
/// # }
/// ```
pub fn to_string_with_options<T: Facet<'static>>(
    value: &T,
    options: &SerializeOptions,
) -> Result<String> {
    let mut output = Vec::new();
    to_writer_with_options(&mut output, value, options)?;
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
    to_writer_with_options(writer, value, &SerializeOptions::default())
}

/// Serialize a value of type `T` to a writer as pretty-printed XML.
///
/// This is a convenience function that enables pretty-printing with default indentation.
///
/// # Example
///
/// ```
/// # use facet::Facet;
/// # use facet_xml as xml;
/// # use facet_xml::to_writer_pretty;
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
/// to_writer_pretty(&mut buffer, &person)?;
/// // Output will have newlines and indentation
/// # Ok(())
/// # }
/// ```
pub fn to_writer_pretty<W: Write, T: Facet<'static>>(writer: &mut W, value: &T) -> Result<()> {
    to_writer_with_options(writer, value, &SerializeOptions::default().pretty())
}

/// Serialize a value of type `T` to a writer as XML with custom options.
///
/// # Example
///
/// ```
/// # use facet::Facet;
/// # use facet_xml as xml;
/// # use facet_xml::{to_writer_with_options, SerializeOptions};
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
///
/// // Compact output (default)
/// let mut buffer = Vec::new();
/// to_writer_with_options(&mut buffer, &person, &SerializeOptions::default())?;
/// assert_eq!(buffer, br#"<Person id="42"><name>Alice</name></Person>"#);
///
/// // Pretty output with default indent
/// let mut buffer = Vec::new();
/// to_writer_with_options(&mut buffer, &person, &SerializeOptions::default().pretty())?;
///
/// // Pretty output with custom indent (tabs)
/// let mut buffer = Vec::new();
/// to_writer_with_options(&mut buffer, &person, &SerializeOptions::default().indent("\t"))?;
/// # Ok(())
/// # }
/// ```
pub fn to_writer_with_options<W: Write, T: Facet<'static>>(
    writer: &mut W,
    value: &T,
    options: &SerializeOptions,
) -> Result<()> {
    let peek = Peek::new(value);
    let mut serializer = XmlSerializer::new(
        writer,
        options.indent_str(),
        options.float_formatter,
        options.preserve_entities,
    );

    // Get the type name for the root element, respecting `rename` attribute
    let type_name = crate::deserialize::get_shape_display_name(peek.shape());
    serializer.serialize_element(type_name, peek)
}

struct XmlSerializer<'a, W> {
    writer: W,
    /// Namespace URI -> prefix mapping for already-declared namespaces.
    declared_namespaces: HashMap<String, String>,
    /// Counter for auto-generating namespace prefixes (ns0, ns1, ...).
    next_ns_index: usize,
    /// Indentation string for pretty-printing (None for compact output).
    indent: Option<&'a str>,
    /// Current indentation depth.
    depth: usize,
    /// The currently active default namespace (from xmlns="..." on an ancestor).
    /// Child elements in this namespace don't need to re-declare it.
    current_default_ns: Option<String>,
    /// Custom formatter for floating-point numbers.
    float_formatter: Option<FloatFormatter>,
    /// Whether to preserve entity references in string values.
    preserve_entities: bool,
}

impl<'a, W: Write> XmlSerializer<'a, W> {
    fn new(
        writer: W,
        indent: Option<&'a str>,
        float_formatter: Option<FloatFormatter>,
        preserve_entities: bool,
    ) -> Self {
        Self {
            writer,
            declared_namespaces: HashMap::new(),
            next_ns_index: 0,
            indent,
            depth: 0,
            current_default_ns: None,
            float_formatter,
            preserve_entities,
        }
    }

    /// Write indentation for the current depth.
    fn write_indent(&mut self) -> Result<()> {
        if let Some(indent_str) = self.indent {
            for _ in 0..self.depth {
                self.writer
                    .write_all(indent_str.as_bytes())
                    .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
            }
        }
        Ok(())
    }

    /// Write a newline if pretty-printing is enabled.
    fn write_newline(&mut self) -> Result<()> {
        if self.indent.is_some() {
            self.writer
                .write_all(b"\n")
                .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
        }
        Ok(())
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
    ///
    /// For attributes: `ns_all` is NOT applied because unprefixed attributes in XML
    /// are always in "no namespace", regardless of any default xmlns declaration.
    /// Only explicit `xml::ns` on the attribute field is used.
    ///
    /// For elements: Both `xml::ns` and `ns_all` are considered.
    fn get_field_namespace(
        field: &Field,
        ns_all: Option<&'static str>,
        is_attribute: bool,
    ) -> Option<&'static str> {
        if is_attribute {
            // Attributes only use explicit xml::ns, not ns_all
            // Per XML spec: unprefixed attributes are in "no namespace"
            field.xml_ns()
        } else {
            // Elements use xml::ns or fall back to ns_all
            field.xml_ns().or(ns_all)
        }
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
        if is_spanned_shape(peek.shape())
            && let Ok(struct_peek) = peek.into_struct()
            && let Ok(value_field) = struct_peek.field_by_name("value")
        {
            return self.serialize_element(element_name, value_field);
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
                    escape_attribute_value(variant_name, self.preserve_entities)
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
                    escape_attribute_value(variant_name, self.preserve_entities)
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

        let has_items = list_peek.iter().next().is_some();
        if has_items {
            self.depth += 1;
        }

        for item in list_peek.iter() {
            self.write_newline()?;
            self.write_indent()?;
            self.serialize_list_item_element(item)?;
        }

        if has_items {
            self.depth -= 1;
            self.write_newline()?;
            self.write_indent()?;
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

        let has_items = map_peek.iter().next().is_some();
        if has_items {
            self.depth += 1;
        }

        for (key, value) in map_peek.iter() {
            self.write_newline()?;
            self.write_indent()?;
            // Use the key as the element name
            if let Some(key_str) = key.as_str() {
                self.serialize_element(key_str, value)?;
            } else if let Some(key_val) = value_to_string(key, self.float_formatter) {
                self.serialize_element(&key_val, value)?;
            } else {
                // Fallback: use "entry" as element name with key as text
                write!(self.writer, "<entry>").map_err(|e| XmlErrorKind::Io(e.to_string()))?;
                self.serialize_value(key)?;
                self.serialize_value(value)?;
                write!(self.writer, "</entry>").map_err(|e| XmlErrorKind::Io(e.to_string()))?;
            }
        }

        if has_items {
            self.depth -= 1;
            self.write_newline()?;
            self.write_indent()?;
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
        if let Def::List(ld) = &shape.def
            && ld.t().is_type::<u8>()
            && let Some(bytes) = peek.as_bytes()
        {
            let encoded = BASE64_STANDARD.encode(bytes);
            write!(self.writer, "<{}>", escape_element_name(element_name))
                .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
            write!(
                self.writer,
                "{}",
                escape_text(&encoded, self.preserve_entities)
            )
            .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
            write!(self.writer, "</{}>", escape_element_name(element_name))
                .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
            return Ok(true);
        }

        // Check for [u8; N]
        if let Def::Array(ad) = &shape.def
            && ad.t().is_type::<u8>()
        {
            // Collect bytes from the array
            if let Ok(list_peek) = peek.into_list_like() {
                let bytes: Vec<u8> = list_peek
                    .iter()
                    .filter_map(|p| p.get::<u8>().ok().copied())
                    .collect();
                let encoded = BASE64_STANDARD.encode(&bytes);
                write!(self.writer, "<{}>", escape_element_name(element_name))
                    .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
                write!(
                    self.writer,
                    "{}",
                    escape_text(&encoded, self.preserve_entities)
                )
                .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
                write!(self.writer, "</{}>", escape_element_name(element_name))
                    .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
                return Ok(true);
            }
        }

        // Check for &[u8]
        if let Def::Slice(sd) = &shape.def
            && sd.t().is_type::<u8>()
            && let Some(bytes) = peek.as_bytes()
        {
            let encoded = BASE64_STANDARD.encode(bytes);
            write!(self.writer, "<{}>", escape_element_name(element_name))
                .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
            write!(
                self.writer,
                "{}",
                escape_text(&encoded, self.preserve_entities)
            )
            .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
            write!(self.writer, "</{}>", escape_element_name(element_name))
                .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
            return Ok(true);
        }

        Ok(false)
    }

    fn serialize_struct_as_element<'mem, 'facet>(
        &mut self,
        element_name: &str,
        struct_peek: facet_reflect::PeekStruct<'mem, 'facet>,
        shape: &'static Shape,
    ) -> Result<()> {
        let struct_ty = struct_peek.ty();

        match struct_ty.kind {
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

        let fields = struct_ty.fields;
        let missing = fields_missing_xml_annotations(fields, XmlAnnotationPhase::Serialize);
        if !missing.is_empty() {
            let field_info = missing
                .into_iter()
                .map(|field| (field.name, field.shape().type_identifier))
                .collect();
            return Err(XmlError::new(XmlErrorKind::MissingXmlAnnotations {
                type_name: shape.type_identifier,
                phase: MissingAnnotationPhase::Serialize,
                fields: field_info,
            }));
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
                        value_to_string(owned.as_peek(), self.float_formatter)
                    } else {
                        value_to_string(field_peek, self.float_formatter)
                    }
                } else {
                    value_to_string(field_peek, self.float_formatter)
                };
                if let Some(value) = value {
                    // Pass is_attribute=true so ns_all is NOT applied
                    let namespace = Self::get_field_namespace(field, ns_all, true);
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

        // If ns_all is set and differs from the current default namespace,
        // emit a default namespace declaration (xmlns="...").
        // Child elements with the same namespace will be unprefixed and inherit it.
        let emitting_new_default_ns = if let Some(ns_uri) = ns_all {
            let dominated = self
                .current_default_ns
                .as_ref()
                .is_some_and(|current| current == ns_uri);
            if !dominated {
                write!(
                    self.writer,
                    " xmlns=\"{}\"",
                    escape_attribute_value(ns_uri, self.preserve_entities)
                )
                .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
                true
            } else {
                false
            }
        } else {
            false
        };

        // Write xmlns declarations for attributes (only for explicitly namespaced attributes)
        for (prefix, uri) in &xmlns_decls {
            write!(
                self.writer,
                " xmlns:{}=\"{}\"",
                escape_element_name(prefix),
                escape_attribute_value(uri, self.preserve_entities)
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
                escape_attribute_value(&attr.value, self.preserve_entities)
            )
            .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
        }

        if !has_content {
            // Self-closing tag
            write!(self.writer, "/>").map_err(|e| XmlErrorKind::Io(e.to_string()))?;
            return Ok(());
        }

        write!(self.writer, ">").map_err(|e| XmlErrorKind::Io(e.to_string()))?;

        // Save and update the default namespace for children
        let old_default_ns = if emitting_new_default_ns {
            let old = self.current_default_ns.take();
            self.current_default_ns = ns_all.map(|s| s.to_string());
            old
        } else {
            None
        };

        // Write text content if present (no indentation for text content)
        if let Some(text_peek) = text_content {
            self.serialize_text_value(text_peek)?;
        }

        // Check if we have child elements (for indentation purposes)
        let has_child_elements = !elements.is_empty() || !elements_list.is_empty();

        // Write child elements (with namespace support)
        // Pass ns_all as the default namespace so child elements with matching
        // namespace use unprefixed form (they inherit the default xmlns).
        if has_child_elements {
            self.depth += 1;
        }

        for (field_item, field_peek) in elements {
            // Pass is_attribute=false for elements
            let field_ns = Self::get_field_namespace(&field_item.field, ns_all, false);

            self.write_newline()?;
            self.write_indent()?;

            // Handle custom serialization for elements
            if field_item.field.proxy_convert_out_fn().is_some() {
                if let Ok(owned) = field_peek.custom_serialization(field_item.field) {
                    self.serialize_namespaced_element(
                        field_item.name,
                        owned.as_peek(),
                        field_ns,
                        ns_all,
                    )?;
                } else {
                    self.serialize_namespaced_element(
                        field_item.name,
                        field_peek,
                        field_ns,
                        ns_all,
                    )?;
                }
            } else {
                self.serialize_namespaced_element(field_item.name, field_peek, field_ns, ns_all)?;
            }
        }

        // Write elements lists
        for field_peek in elements_list {
            self.serialize_elements_list(field_peek)?;
        }

        if has_child_elements {
            self.depth -= 1;
            self.write_newline()?;
            self.write_indent()?;
        }

        // Restore the old default namespace
        if emitting_new_default_ns {
            self.current_default_ns = old_default_ns;
        }

        // Write closing tag
        write!(self.writer, "</{}>", escape_element_name(element_name))
            .map_err(|e| XmlErrorKind::Io(e.to_string()))?;

        Ok(())
    }

    /// Serialize an element with optional namespace.
    ///
    /// - `namespace`: The namespace this element should be in
    /// - `default_ns`: The currently active default namespace (from parent's xmlns="...")
    ///
    /// If the element's namespace matches the default namespace, we use an unprefixed
    /// element name (the element inherits the default namespace).
    /// Otherwise, we use a prefix and emit an xmlns:prefix declaration.
    fn serialize_namespaced_element<'mem, 'facet>(
        &mut self,
        element_name: &str,
        peek: Peek<'mem, 'facet>,
        namespace: Option<&str>,
        default_ns: Option<&str>,
    ) -> Result<()> {
        // Handle Option<T> - skip if None
        if let Ok(opt_peek) = peek.into_option() {
            if opt_peek.is_none() {
                return Ok(());
            }
            if let Some(inner) = opt_peek.value() {
                return self.serialize_namespaced_element(
                    element_name,
                    inner,
                    namespace,
                    default_ns,
                );
            }
            return Ok(());
        }

        // Handle Spanned<T> - unwrap to the inner value
        if is_spanned_shape(peek.shape())
            && let Ok(struct_peek) = peek.into_struct()
            && let Ok(value_field) = struct_peek.field_by_name("value")
        {
            return self.serialize_namespaced_element(
                element_name,
                value_field,
                namespace,
                default_ns,
            );
        }

        // Determine element name and xmlns declaration
        // If namespace matches the current default, use unprefixed form (inherit default).
        // Otherwise, use a prefix and emit xmlns:prefix declaration.
        let (final_name, xmlns_decl) = if let Some(ns_uri) = namespace {
            if default_ns == Some(ns_uri) {
                // Element is in the default namespace - use unprefixed form
                (element_name.to_string(), None)
            } else {
                // Element is in a different namespace - use prefix
                let prefix = self.get_or_create_prefix(ns_uri);
                let prefixed = format!("{prefix}:{element_name}");
                (prefixed, Some((prefix, ns_uri.to_string())))
            }
        } else {
            (element_name.to_string(), None)
        };

        // Check if this is a struct - handle specially for proper namespace propagation
        let shape = peek.shape();
        if let Ok(struct_peek) = peek.into_struct() {
            return self.serialize_struct_as_namespaced_element(
                &final_name,
                struct_peek,
                xmlns_decl,
                shape,
                default_ns,
            );
        }

        // Check if this is an enum
        if let Ok(enum_peek) = peek.into_enum() {
            return self.serialize_enum_as_namespaced_element(&final_name, enum_peek, xmlns_decl);
        }

        // For scalars/primitives, serialize as element with text content
        write!(self.writer, "<{}", escape_element_name(&final_name))
            .map_err(|e| XmlErrorKind::Io(e.to_string()))?;

        // Add xmlns declaration if needed
        if let Some((prefix, uri)) = xmlns_decl {
            write!(
                self.writer,
                " xmlns:{}=\"{}\"",
                escape_element_name(&prefix),
                escape_attribute_value(&uri, self.preserve_entities)
            )
            .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
        }

        write!(self.writer, ">").map_err(|e| XmlErrorKind::Io(e.to_string()))?;
        self.serialize_value(peek)?;
        write!(self.writer, "</{}>", escape_element_name(&final_name))
            .map_err(|e| XmlErrorKind::Io(e.to_string()))?;

        Ok(())
    }

    /// Serialize a struct as an element with optional xmlns declaration on the opening tag.
    ///
    /// - `parent_default_ns`: The default namespace inherited from the parent element
    fn serialize_struct_as_namespaced_element<'mem, 'facet>(
        &mut self,
        element_name: &str,
        struct_peek: facet_reflect::PeekStruct<'mem, 'facet>,
        xmlns_decl: Option<(String, String)>,
        shape: &'static Shape,
        parent_default_ns: Option<&str>,
    ) -> Result<()> {
        match struct_peek.ty().kind {
            StructKind::Unit => {
                // Unit struct - just output empty element with xmlns if needed
                write!(self.writer, "<{}", escape_element_name(element_name))
                    .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
                if let Some((prefix, uri)) = xmlns_decl {
                    write!(
                        self.writer,
                        " xmlns:{}=\"{}\"",
                        escape_element_name(&prefix),
                        escape_attribute_value(&uri, self.preserve_entities)
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
                    write!(self.writer, "<{}", escape_element_name(element_name))
                        .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
                    if let Some((prefix, uri)) = xmlns_decl {
                        write!(
                            self.writer,
                            " xmlns:{}=\"{}\"",
                            escape_element_name(&prefix),
                            escape_attribute_value(&uri, self.preserve_entities)
                        )
                        .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
                    }
                    write!(self.writer, "/>").map_err(|e| XmlErrorKind::Io(e.to_string()))?;
                    return Ok(());
                }

                write!(self.writer, "<{}", escape_element_name(element_name))
                    .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
                if let Some((prefix, uri)) = xmlns_decl {
                    write!(
                        self.writer,
                        " xmlns:{}=\"{}\"",
                        escape_element_name(&prefix),
                        escape_attribute_value(&uri, self.preserve_entities)
                    )
                    .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
                }
                write!(self.writer, ">").map_err(|e| XmlErrorKind::Io(e.to_string()))?;

                for (i, (_, field_peek)) in fields.into_iter().enumerate() {
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

        // Get container-level namespace default for the nested struct
        let ns_all = shape.xml_ns_all();

        // The actual default namespace in the XML is inherited from parent.
        // We don't emit xmlns="..." on nested elements because that would
        // change the element's own namespace. Child elements in a different
        // namespace will use prefixed form.
        let effective_default_ns: Option<&str> = parent_default_ns;

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
                        value_to_string(owned.as_peek(), self.float_formatter)
                    } else {
                        value_to_string(field_peek, self.float_formatter)
                    }
                } else {
                    value_to_string(field_peek, self.float_formatter)
                };
                if let Some(value) = value {
                    // Pass is_attribute=true so ns_all is NOT applied
                    let namespace = Self::get_field_namespace(field, ns_all, true);
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

        // Start with the element's own xmlns declaration if any (from prefixed form)
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
        write!(self.writer, "<{}", escape_element_name(element_name))
            .map_err(|e| XmlErrorKind::Io(e.to_string()))?;

        // NOTE: We intentionally do NOT emit xmlns="..." on nested struct elements here.
        // The element itself is in the parent's namespace (determined by the context
        // where it's used). Only the struct's CHILD elements are affected by ns_all.
        // Child elements in a different namespace will use prefixed form.

        // Write prefixed xmlns declarations (for explicitly namespaced attributes)
        for (prefix, uri) in &xmlns_decls {
            write!(
                self.writer,
                " xmlns:{}=\"{}\"",
                escape_element_name(prefix),
                escape_attribute_value(uri, self.preserve_entities)
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
                escape_attribute_value(&attr.value, self.preserve_entities)
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

        // Check if we have child elements (for indentation purposes)
        let has_child_elements = !elements.is_empty() || !elements_list.is_empty();

        if has_child_elements {
            self.depth += 1;
        }

        for (field_item, field_peek) in elements {
            // Pass is_attribute=false for elements
            let field_ns = Self::get_field_namespace(&field_item.field, ns_all, false);

            self.write_newline()?;
            self.write_indent()?;

            if field_item.field.proxy_convert_out_fn().is_some() {
                if let Ok(owned) = field_peek.custom_serialization(field_item.field) {
                    self.serialize_namespaced_element(
                        field_item.name,
                        owned.as_peek(),
                        field_ns,
                        effective_default_ns,
                    )?;
                } else {
                    self.serialize_namespaced_element(
                        field_item.name,
                        field_peek,
                        field_ns,
                        effective_default_ns,
                    )?;
                }
            } else {
                self.serialize_namespaced_element(
                    field_item.name,
                    field_peek,
                    field_ns,
                    effective_default_ns,
                )?;
            }
        }

        for field_peek in elements_list {
            self.serialize_elements_list(field_peek)?;
        }

        if has_child_elements {
            self.depth -= 1;
            self.write_newline()?;
            self.write_indent()?;
        }

        write!(self.writer, "</{}>", escape_element_name(element_name))
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
                        escape_attribute_value(uri, self.preserve_entities)
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
                    escape_attribute_value(variant_name, self.preserve_entities)
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
                    escape_attribute_value(variant_name, self.preserve_entities)
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
            self.write_newline()?;
            self.write_indent()?;
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
        if is_spanned_shape(peek.shape())
            && let Ok(struct_peek) = peek.into_struct()
            && let Ok(value_peek) = struct_peek.field_by_name("value")
        {
            return self.serialize_text_value(value_peek);
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
        if is_spanned_shape(peek.shape())
            && let Ok(struct_peek) = peek.into_struct()
            && let Ok(value_peek) = struct_peek.field_by_name("value")
        {
            return self.serialize_value(value_peek);
        }

        // Unwrap transparent wrappers
        let peek = peek.innermost_peek();

        // Try string first
        if let Some(s) = peek.as_str() {
            write!(self.writer, "{}", escape_text(s, self.preserve_entities))
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
            if let Some(fmt) = self.float_formatter {
                fmt(f64::from(*v), &mut self.writer)
                    .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
            } else {
                write!(self.writer, "{v}").map_err(|e| XmlErrorKind::Io(e.to_string()))?;
            }
            return Ok(());
        }
        if let Ok(v) = peek.get::<f64>() {
            if let Some(fmt) = self.float_formatter {
                fmt(*v, &mut self.writer).map_err(|e| XmlErrorKind::Io(e.to_string()))?;
            } else {
                write!(self.writer, "{v}").map_err(|e| XmlErrorKind::Io(e.to_string()))?;
            }
            return Ok(());
        }

        if let Ok(v) = peek.get::<char>() {
            write!(
                self.writer,
                "{}",
                escape_text(&v.to_string(), self.preserve_entities)
            )
            .map_err(|e| XmlErrorKind::Io(e.to_string()))?;
            return Ok(());
        }

        Err(XmlErrorKind::SerializeUnknownValueType.into())
    }
}

/// Convert a Peek value to a string representation, handling Options, Spanned, and transparent wrappers.
fn value_to_string<'mem, 'facet>(
    peek: Peek<'mem, 'facet>,
    float_formatter: Option<FloatFormatter>,
) -> Option<String> {
    // Handle Option<T>
    if let Ok(opt_peek) = peek.into_option() {
        if opt_peek.is_none() {
            return None;
        }
        if let Some(inner) = opt_peek.value() {
            return value_to_string(inner, float_formatter);
        }
        return None;
    }

    // Handle Spanned<T>
    if is_spanned_shape(peek.shape())
        && let Ok(struct_peek) = peek.into_struct()
        && let Ok(value_peek) = struct_peek.field_by_name("value")
    {
        return value_to_string(value_peek, float_formatter);
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
        if let Some(fmt) = float_formatter {
            let mut buf = Vec::new();
            if fmt(f64::from(*v), &mut buf).is_ok() {
                return String::from_utf8(buf).ok();
            }
        }
        return Some(v.to_string());
    }
    if let Ok(v) = peek.get::<f64>() {
        if let Some(fmt) = float_formatter {
            let mut buf = Vec::new();
            if fmt(*v, &mut buf).is_ok() {
                return String::from_utf8(buf).ok();
            }
        }
        return Some(v.to_string());
    }

    if let Ok(v) = peek.get::<char>() {
        return Some(v.to_string());
    }

    None
}

/// Escape special characters in XML text content.
fn escape_text(s: &str, preserve_entities: bool) -> String {
    if preserve_entities {
        escape_preserving_entities(s, false)
    } else {
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
}

/// Escape special characters in XML attribute values.
fn escape_attribute_value(s: &str, preserve_entities: bool) -> String {
    if preserve_entities {
        escape_preserving_entities(s, true)
    } else {
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
}

/// Escape special characters while preserving entity references.
///
/// Recognizes entity reference patterns:
/// - Named entities: `&name;` (alphanumeric name)
/// - Decimal numeric entities: `&#digits;`
/// - Hexadecimal numeric entities: `&#xhex;` or `&#Xhex;`
fn escape_preserving_entities(s: &str, is_attribute: bool) -> String {
    let mut result = String::with_capacity(s.len());
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];
        match c {
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            '"' if is_attribute => result.push_str("&quot;"),
            '\'' if is_attribute => result.push_str("&apos;"),
            '&' => {
                // Check if this is the start of an entity reference
                if let Some(entity_len) = try_parse_entity_reference(&chars[i..]) {
                    // It's a valid entity reference - copy it as-is
                    for j in 0..entity_len {
                        result.push(chars[i + j]);
                    }
                    i += entity_len;
                    continue;
                } else {
                    // Not a valid entity reference - escape the ampersand
                    result.push_str("&amp;");
                }
            }
            _ => result.push(c),
        }
        i += 1;
    }

    result
}

/// Try to parse an entity reference starting at the given position.
/// Returns the length of the entity reference if valid, or None if not.
///
/// Valid patterns:
/// - `&name;` where name is one or more alphanumeric characters
/// - `&#digits;` where digits are decimal digits
/// - `&#xhex;` or `&#Xhex;` where hex is hexadecimal digits
fn try_parse_entity_reference(chars: &[char]) -> Option<usize> {
    if chars.is_empty() || chars[0] != '&' {
        return None;
    }

    // Need at least `&x;` (3 chars minimum)
    if chars.len() < 3 {
        return None;
    }

    let mut len = 1; // Start after '&'

    if chars[1] == '#' {
        // Numeric entity reference
        len = 2;

        if len < chars.len() && (chars[len] == 'x' || chars[len] == 'X') {
            // Hexadecimal: &#xHEX;
            len += 1;
            let start = len;
            while len < chars.len() && chars[len].is_ascii_hexdigit() {
                len += 1;
            }
            // Need at least one hex digit
            if len == start {
                return None;
            }
        } else {
            // Decimal: &#DIGITS;
            let start = len;
            while len < chars.len() && chars[len].is_ascii_digit() {
                len += 1;
            }
            // Need at least one digit
            if len == start {
                return None;
            }
        }
    } else {
        // Named entity reference: &NAME;
        // Name must start with a letter or underscore, then letters, digits, underscores, hyphens, periods
        if !chars[len].is_ascii_alphabetic() && chars[len] != '_' {
            return None;
        }
        len += 1;
        while len < chars.len()
            && (chars[len].is_ascii_alphanumeric()
                || chars[len] == '_'
                || chars[len] == '-'
                || chars[len] == '.')
        {
            len += 1;
        }
    }

    // Must end with ';'
    if len >= chars.len() || chars[len] != ';' {
        return None;
    }

    Some(len + 1) // Include the semicolon
}

/// Escape element name (for now, assume valid XML names).
fn escape_element_name(name: &str) -> &str {
    name
}
