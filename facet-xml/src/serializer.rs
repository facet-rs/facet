extern crate alloc;

use alloc::{borrow::Cow, format, string::String, vec::Vec};
use std::collections::HashMap;
use std::io::Write;

use facet_core::{Def, Facet, ScalarType};
use facet_dom::{DomSerializeError, DomSerializer};
use facet_reflect::Peek;

use crate::escaping::EscapingWriter;

pub use facet_dom::FloatFormatter;

/// Write a scalar value directly to a writer.
/// Returns `Ok(true)` if the value was a scalar and was written,
/// `Ok(false)` if not a scalar, `Err` if write failed.
fn write_scalar_value(
    out: &mut dyn Write,
    value: Peek<'_, '_>,
    float_formatter: Option<FloatFormatter>,
) -> std::io::Result<bool> {
    // Unwrap transparent wrappers (e.g., PointsProxy -> String)
    let value = value.innermost_peek();

    // Handle Option<T> by unwrapping if Some
    if let Def::Option(_) = &value.shape().def
        && let Ok(opt) = value.into_option()
    {
        return match opt.value() {
            Some(inner) => write_scalar_value(out, inner, float_formatter),
            None => Ok(false),
        };
    }

    let Some(scalar_type) = value.scalar_type() else {
        // Try Display for Def::Scalar types (SmolStr, etc.)
        if matches!(value.shape().def, Def::Scalar) && value.shape().vtable.has_display() {
            write!(out, "{}", value)?;
            return Ok(true);
        }
        return Ok(false);
    };

    match scalar_type {
        ScalarType::Unit => {
            out.write_all(b"null")?;
        }
        ScalarType::Bool => {
            let b = value.get::<bool>().unwrap();
            out.write_all(if *b { b"true" } else { b"false" })?;
        }
        ScalarType::Char => {
            let c = value.get::<char>().unwrap();
            let mut buf = [0u8; 4];
            let s = c.encode_utf8(&mut buf);
            out.write_all(s.as_bytes())?;
        }
        ScalarType::Str | ScalarType::String | ScalarType::CowStr => {
            let s = value.as_str().unwrap();
            out.write_all(s.as_bytes())?;
        }
        ScalarType::F32 => {
            let v = value.get::<f32>().unwrap();
            if let Some(fmt) = float_formatter {
                fmt(*v as f64, out)?;
            } else {
                write!(out, "{}", v)?;
            }
        }
        ScalarType::F64 => {
            let v = value.get::<f64>().unwrap();
            if let Some(fmt) = float_formatter {
                fmt(*v, out)?;
            } else {
                write!(out, "{}", v)?;
            }
        }
        ScalarType::U8 => write!(out, "{}", value.get::<u8>().unwrap())?,
        ScalarType::U16 => write!(out, "{}", value.get::<u16>().unwrap())?,
        ScalarType::U32 => write!(out, "{}", value.get::<u32>().unwrap())?,
        ScalarType::U64 => write!(out, "{}", value.get::<u64>().unwrap())?,
        ScalarType::U128 => write!(out, "{}", value.get::<u128>().unwrap())?,
        ScalarType::USize => write!(out, "{}", value.get::<usize>().unwrap())?,
        ScalarType::I8 => write!(out, "{}", value.get::<i8>().unwrap())?,
        ScalarType::I16 => write!(out, "{}", value.get::<i16>().unwrap())?,
        ScalarType::I32 => write!(out, "{}", value.get::<i32>().unwrap())?,
        ScalarType::I64 => write!(out, "{}", value.get::<i64>().unwrap())?,
        ScalarType::I128 => write!(out, "{}", value.get::<i128>().unwrap())?,
        ScalarType::ISize => write!(out, "{}", value.get::<isize>().unwrap())?,
        #[cfg(feature = "net")]
        ScalarType::IpAddr => write!(out, "{}", value.get::<core::net::IpAddr>().unwrap())?,
        #[cfg(feature = "net")]
        ScalarType::Ipv4Addr => write!(out, "{}", value.get::<core::net::Ipv4Addr>().unwrap())?,
        #[cfg(feature = "net")]
        ScalarType::Ipv6Addr => write!(out, "{}", value.get::<core::net::Ipv6Addr>().unwrap())?,
        #[cfg(feature = "net")]
        ScalarType::SocketAddr => write!(out, "{}", value.get::<core::net::SocketAddr>().unwrap())?,
        _ => return Ok(false),
    }
    Ok(true)
}

/// Options for XML serialization.
#[derive(Clone)]
pub struct SerializeOptions {
    /// Whether to pretty-print with indentation (default: false)
    pub pretty: bool,
    /// Indentation string for pretty-printing (default: "  ")
    pub indent: Cow<'static, str>,
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
    pub preserve_entities: bool,
}

impl Default for SerializeOptions {
    fn default() -> Self {
        Self {
            pretty: false,
            indent: Cow::Borrowed("  "),
            float_formatter: None,
            preserve_entities: false,
        }
    }
}

impl core::fmt::Debug for SerializeOptions {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
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
    pub const fn pretty(mut self) -> Self {
        self.pretty = true;
        self
    }

    /// Set a custom indentation string (implies pretty-printing).
    pub fn indent(mut self, indent: impl Into<Cow<'static, str>>) -> Self {
        self.indent = indent.into();
        self.pretty = true;
        self
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
    /// assert_eq!(xml, r#"<Point x="1.5" y="2"></Point>"#);
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
    pub const fn preserve_entities(mut self, preserve: bool) -> Self {
        self.preserve_entities = preserve;
        self
    }
}

/// Well-known XML namespace URIs and their conventional prefixes.
#[allow(dead_code)] // Used in namespace serialization
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

#[derive(Debug)]
pub struct XmlSerializeError {
    msg: Cow<'static, str>,
}

impl core::fmt::Display for XmlSerializeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.msg)
    }
}

impl std::error::Error for XmlSerializeError {}

/// XML serializer with configurable output options.
///
/// The output is designed to round-trip through `facet-xml`'s parser:
/// - structs are elements whose children are field elements
/// - sequences are elements whose children are repeated `<item>` elements
/// - element names are treated as map keys; the root element name is ignored
pub struct XmlSerializer {
    out: Vec<u8>,
    /// Stack of element names for closing tags
    element_stack: Vec<String>,
    /// Namespace URI -> prefix mapping for already-declared namespaces.
    declared_namespaces: HashMap<String, String>,
    /// Counter for auto-generating namespace prefixes (ns0, ns1, ...).
    next_ns_index: usize,
    /// The currently active default namespace (from xmlns="..." on an ancestor).
    /// When set, elements in this namespace use unprefixed names.
    current_default_ns: Option<String>,
    /// Container-level default namespace (from xml::ns_all) for current struct
    current_ns_all: Option<String>,
    /// True if the current field is an attribute (vs element)
    pending_is_attribute: bool,
    /// True if the current field is text content (xml::text)
    pending_is_text: bool,
    /// True if the current field is an xml::elements list (no wrapper element)
    pending_is_elements: bool,
    /// Pending namespace for the next field
    pending_namespace: Option<String>,
    /// Serialization options (pretty-printing, float formatting, etc.)
    options: SerializeOptions,
    /// Current indentation depth for pretty-printing
    depth: usize,
    /// True if we're collecting attributes (between element_start and children_start)
    collecting_attributes: bool,
    /// True if the next element should establish a default namespace (from ns_all)
    pending_establish_default_ns: bool,
}

impl XmlSerializer {
    /// Create a new XML serializer with default options.
    pub fn new() -> Self {
        Self::with_options(SerializeOptions::default())
    }

    /// Create a new XML serializer with the given options.
    pub fn with_options(options: SerializeOptions) -> Self {
        Self {
            out: Vec::new(),
            element_stack: Vec::new(),
            declared_namespaces: HashMap::new(),
            next_ns_index: 0,
            current_default_ns: None,
            current_ns_all: None,
            pending_is_attribute: false,
            pending_is_text: false,
            pending_is_elements: false,
            pending_namespace: None,
            options,
            depth: 0,
            collecting_attributes: false,
            pending_establish_default_ns: false,
        }
    }

    pub fn finish(self) -> Vec<u8> {
        self.out
    }

    /// Write the opening part of an element tag: `<tag` (without the closing `>`)
    /// This allows attributes to be written directly afterwards.
    fn write_element_tag_start(&mut self, name: &str, namespace: Option<&str>) {
        self.write_indent();
        self.out.push(b'<');

        // Track the close tag (may include prefix)
        let close_tag: String;

        // Handle namespace for element
        if let Some(ns_uri) = namespace {
            if self.current_default_ns.as_deref() == Some(ns_uri) {
                // Element is in the current default namespace - use unprefixed form
                self.out.extend_from_slice(name.as_bytes());
                close_tag = name.to_string();
            } else if self.pending_establish_default_ns {
                // This is a struct root with ns_all - establish as default namespace
                self.out.extend_from_slice(name.as_bytes());
                self.out.extend_from_slice(b" xmlns=\"");
                self.out.extend_from_slice(ns_uri.as_bytes());
                self.out.push(b'"');
                self.current_default_ns = Some(ns_uri.to_string());
                self.pending_establish_default_ns = false;
                close_tag = name.to_string();
            } else {
                // Field-level namespace - use prefix
                let prefix = self.get_or_create_prefix(ns_uri);
                self.out.extend_from_slice(prefix.as_bytes());
                self.out.push(b':');
                self.out.extend_from_slice(name.as_bytes());
                // Write xmlns declaration for this prefix
                self.out.extend_from_slice(b" xmlns:");
                self.out.extend_from_slice(prefix.as_bytes());
                self.out.extend_from_slice(b"=\"");
                self.out.extend_from_slice(ns_uri.as_bytes());
                self.out.push(b'"');
                close_tag = format!("{}:{}", prefix, name);
            }
        } else {
            self.out.extend_from_slice(name.as_bytes());
            close_tag = name.to_string();
        }

        // Push the close tag for element_end
        self.element_stack.push(close_tag);
    }

    /// Write an attribute directly to the output: ` name="escaped_value"`
    /// Returns Ok(true) if written, Ok(false) if value wasn't a scalar (attribute skipped).
    fn write_attribute(
        &mut self,
        name: &str,
        value: Peek<'_, '_>,
        namespace: Option<&str>,
    ) -> std::io::Result<bool> {
        // First, write the value to a temporary buffer to check if it's a scalar
        let mut value_buf = Vec::new();
        let written = write_scalar_value(
            &mut EscapingWriter::attribute(&mut value_buf),
            value,
            self.options.float_formatter,
        )?;

        if !written {
            // Not a scalar (e.g., None) - skip the attribute entirely
            return Ok(false);
        }

        // Now write the attribute
        self.out.push(b' ');
        if let Some(ns_uri) = namespace {
            let prefix = self.get_or_create_prefix(ns_uri);
            // Write xmlns declaration
            self.out.extend_from_slice(b"xmlns:");
            self.out.extend_from_slice(prefix.as_bytes());
            self.out.extend_from_slice(b"=\"");
            self.out.extend_from_slice(ns_uri.as_bytes());
            self.out.extend_from_slice(b"\" ");
            // Write prefixed attribute
            self.out.extend_from_slice(prefix.as_bytes());
            self.out.push(b':');
        }
        self.out.extend_from_slice(name.as_bytes());
        self.out.extend_from_slice(b"=\"");
        self.out.extend_from_slice(&value_buf);
        self.out.push(b'"');
        Ok(true)
    }

    /// Finish the element opening tag by writing `>` and incrementing depth.
    fn write_element_tag_end(&mut self) {
        self.out.push(b'>');
        self.write_newline();
        self.depth += 1;
    }

    fn write_close_tag(&mut self, name: &str) {
        self.depth = self.depth.saturating_sub(1);
        self.write_indent();
        self.out.extend_from_slice(b"</");
        self.out.extend_from_slice(name.as_bytes());
        self.out.push(b'>');
        self.write_newline();
    }

    fn write_text_escaped(&mut self, text: &str) {
        use std::io::Write;
        if self.options.preserve_entities {
            let escaped = escape_preserving_entities(text, false);
            self.out.extend_from_slice(escaped.as_bytes());
        } else {
            // Use EscapingWriter for consistency with attribute escaping
            let _ = EscapingWriter::text(&mut self.out).write_all(text.as_bytes());
        }
    }

    /// Write indentation for the current depth (if pretty-printing is enabled).
    fn write_indent(&mut self) {
        if self.options.pretty {
            for _ in 0..self.depth {
                self.out.extend_from_slice(self.options.indent.as_bytes());
            }
        }
    }

    /// Write a newline (if pretty-printing is enabled).
    fn write_newline(&mut self) {
        if self.options.pretty {
            self.out.push(b'\n');
        }
    }

    /// Get or create a prefix for the given namespace URI.
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

    fn clear_field_state_impl(&mut self) {
        self.pending_is_attribute = false;
        self.pending_is_text = false;
        self.pending_is_elements = false;
        self.pending_namespace = None;
    }
}

impl Default for XmlSerializer {
    fn default() -> Self {
        Self::new()
    }
}

impl DomSerializer for XmlSerializer {
    type Error = XmlSerializeError;

    fn element_start(&mut self, tag: &str, namespace: Option<&str>) -> Result<(), Self::Error> {
        // Priority: explicit namespace > pending_namespace > current_ns_all (for struct roots)
        let ns = namespace
            .map(|s| s.to_string())
            .or_else(|| self.pending_namespace.take())
            .or_else(|| self.current_ns_all.clone());

        // Write the opening tag immediately: `<tag` (attributes will follow)
        self.write_element_tag_start(tag, ns.as_deref());
        self.collecting_attributes = true;

        Ok(())
    }

    fn attribute(
        &mut self,
        name: &str,
        value: Peek<'_, '_>,
        namespace: Option<&str>,
    ) -> Result<(), Self::Error> {
        // Attributes must come before children_start
        if !self.collecting_attributes {
            return Err(XmlSerializeError {
                msg: Cow::Borrowed("attribute() called after children_start()"),
            });
        }

        // Use the pending namespace from field_metadata if no explicit namespace given
        let ns: Option<String> = match namespace {
            Some(ns) => Some(ns.to_string()),
            None => self.pending_namespace.clone(),
        };

        // Write directly to output
        self.write_attribute(name, value, ns.as_deref())
            .map_err(|e| XmlSerializeError {
                msg: Cow::Owned(format!("write error: {}", e)),
            })?;
        Ok(())
    }

    fn children_start(&mut self) -> Result<(), Self::Error> {
        // Close the element opening tag
        self.write_element_tag_end();
        self.collecting_attributes = false;
        Ok(())
    }

    fn children_end(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn element_end(&mut self, _tag: &str) -> Result<(), Self::Error> {
        if let Some(close_tag) = self.element_stack.pop() {
            self.write_close_tag(&close_tag);
        }
        Ok(())
    }

    fn text(&mut self, content: &str) -> Result<(), Self::Error> {
        self.write_text_escaped(content);
        Ok(())
    }

    fn struct_metadata(&mut self, shape: &facet_core::Shape) -> Result<(), Self::Error> {
        // Extract xml::ns_all attribute from the struct
        self.current_ns_all = shape
            .attributes
            .iter()
            .find(|attr| attr.ns == Some("xml") && attr.key == "ns_all")
            .and_then(|attr| attr.get_as::<&str>().copied())
            .map(String::from);

        // If ns_all is set, the next element_start should establish it as default namespace
        self.pending_establish_default_ns = self.current_ns_all.is_some();

        Ok(())
    }

    fn field_metadata(&mut self, field: &facet_reflect::FieldItem) -> Result<(), Self::Error> {
        let Some(field_def) = field.field else {
            // For flattened map entries, treat them as attributes
            self.pending_is_attribute = true;
            self.pending_is_text = false;
            self.pending_is_elements = false;
            return Ok(());
        };

        // Check if this field is an attribute
        self.pending_is_attribute = field_def.get_attr(Some("xml"), "attribute").is_some();
        // Check if this field is text content
        self.pending_is_text = field_def.get_attr(Some("xml"), "text").is_some();
        // Check if this field is an xml::elements list
        self.pending_is_elements = field_def.get_attr(Some("xml"), "elements").is_some();

        // Extract xml::ns attribute from the field
        if let Some(ns_attr) = field_def.get_attr(Some("xml"), "ns")
            && let Some(ns_uri) = ns_attr.get_as::<&str>().copied()
        {
            self.pending_namespace = Some(ns_uri.to_string());
        } else if !self.pending_is_attribute && !self.pending_is_text {
            // Apply ns_all to elements only (or None if no ns_all)
            self.pending_namespace = self.current_ns_all.clone();
        } else {
            // Attributes and text don't get namespace from ns_all
            self.pending_namespace = None;
        }

        Ok(())
    }

    fn variant_metadata(
        &mut self,
        _variant: &'static facet_core::Variant,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    fn is_attribute_field(&self) -> bool {
        self.pending_is_attribute
    }

    fn is_text_field(&self) -> bool {
        self.pending_is_text
    }

    fn is_elements_field(&self) -> bool {
        self.pending_is_elements
    }

    fn clear_field_state(&mut self) {
        self.clear_field_state_impl();
    }

    fn format_float(&self, value: f64) -> String {
        if let Some(formatter) = self.options.float_formatter {
            let mut buf = Vec::new();
            // If the formatter fails, fall back to default Display
            if formatter(value, &mut buf).is_ok()
                && let Ok(s) = String::from_utf8(buf)
            {
                return s;
            }
        }
        value.to_string()
    }

    fn serialize_none(&mut self) -> Result<(), Self::Error> {
        // For XML, None values should not emit any content
        Ok(())
    }
}

/// Serialize a value to XML bytes with default options.
pub fn to_vec<'facet, T>(value: &'_ T) -> Result<Vec<u8>, DomSerializeError<XmlSerializeError>>
where
    T: Facet<'facet> + ?Sized,
{
    to_vec_with_options(value, &SerializeOptions::default())
}

/// Serialize a value to XML bytes with custom options.
pub fn to_vec_with_options<'facet, T>(
    value: &'_ T,
    options: &SerializeOptions,
) -> Result<Vec<u8>, DomSerializeError<XmlSerializeError>>
where
    T: Facet<'facet> + ?Sized,
{
    let mut serializer = XmlSerializer::with_options(options.clone());
    facet_dom::serialize(&mut serializer, Peek::new(value))?;
    Ok(serializer.finish())
}

/// Serialize a value to an XML string with default options.
pub fn to_string<'facet, T>(value: &'_ T) -> Result<String, DomSerializeError<XmlSerializeError>>
where
    T: Facet<'facet> + ?Sized,
{
    let bytes = to_vec(value)?;
    // SAFETY: XmlSerializer produces valid UTF-8
    Ok(String::from_utf8(bytes).expect("XmlSerializer produces valid UTF-8"))
}

/// Serialize a value to a pretty-printed XML string with default indentation.
pub fn to_string_pretty<'facet, T>(
    value: &'_ T,
) -> Result<String, DomSerializeError<XmlSerializeError>>
where
    T: Facet<'facet> + ?Sized,
{
    to_string_with_options(value, &SerializeOptions::default().pretty())
}

/// Serialize a value to an XML string with custom options.
pub fn to_string_with_options<'facet, T>(
    value: &'_ T,
    options: &SerializeOptions,
) -> Result<String, DomSerializeError<XmlSerializeError>>
where
    T: Facet<'facet> + ?Sized,
{
    let bytes = to_vec_with_options(value, options)?;
    // SAFETY: XmlSerializer produces valid UTF-8
    Ok(String::from_utf8(bytes).expect("XmlSerializer produces valid UTF-8"))
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
