extern crate alloc;

use alloc::{borrow::Cow, format, string::String, vec::Vec};
use std::{collections::HashMap, io::Write};

use facet_core::Facet;
use facet_dom::{DomSerializeError, DomSerializer};
use facet_reflect::Peek;

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
    /// True if we're collecting attributes for a deferred element
    collecting_attributes: bool,
    /// Buffered attributes for the current element (name, value, namespace_opt)
    pending_attributes: Vec<(String, String, Option<String>)>,
    /// Deferred element info: (tag_name, namespace)
    deferred_element: Option<(String, Option<String>)>,
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
            pending_attributes: Vec::new(),
            deferred_element: None,
        }
    }

    pub fn finish(self) -> Vec<u8> {
        self.out
    }

    /// Flush any deferred element opening tag.
    fn flush_deferred_element(&mut self) {
        if let Some((tag, ns)) = self.deferred_element.take() {
            self.write_open_tag_impl(&tag, ns.as_deref());
        }
    }

    fn write_open_tag_impl(&mut self, name: &str, namespace: Option<&str>) {
        self.write_indent();
        self.out.push(b'<');

        // Handle namespace for element
        if let Some(ns_uri) = namespace {
            if self.current_default_ns.as_deref() == Some(ns_uri) {
                // Element is in the default namespace - use unprefixed form
                self.out.extend_from_slice(name.as_bytes());
            } else {
                // Get or create a prefix for this namespace
                let prefix = self.get_or_create_prefix(ns_uri);
                self.out.extend_from_slice(prefix.as_bytes());
                self.out.push(b':');
                self.out.extend_from_slice(name.as_bytes());
                // Write xmlns declaration
                self.out.extend_from_slice(b" xmlns:");
                self.out.extend_from_slice(prefix.as_bytes());
                self.out.extend_from_slice(b"=\"");
                self.out.extend_from_slice(ns_uri.as_bytes());
                self.out.push(b'"');
            }
        } else {
            self.out.extend_from_slice(name.as_bytes());
        }

        // Write buffered attributes
        let attrs: Vec<_> = self.pending_attributes.drain(..).collect();
        for (attr_name, attr_value, attr_ns) in attrs {
            self.out.push(b' ');
            if let Some(ns_uri) = attr_ns {
                let prefix = self.get_or_create_prefix(&ns_uri);
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
            self.out.extend_from_slice(attr_name.as_bytes());
            self.out.extend_from_slice(b"=\"");
            // Escape attribute value
            for b in attr_value.as_bytes() {
                match *b {
                    b'&' => self.out.extend_from_slice(b"&amp;"),
                    b'<' => self.out.extend_from_slice(b"&lt;"),
                    b'>' => self.out.extend_from_slice(b"&gt;"),
                    b'"' => self.out.extend_from_slice(b"&quot;"),
                    _ => self.out.push(*b),
                }
            }
            self.out.push(b'"');
        }

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
        if self.options.preserve_entities {
            let escaped = escape_preserving_entities(text, false);
            self.out.extend_from_slice(escaped.as_bytes());
        } else {
            for b in text.as_bytes() {
                match *b {
                    b'&' => self.out.extend_from_slice(b"&amp;"),
                    b'<' => self.out.extend_from_slice(b"&lt;"),
                    b'>' => self.out.extend_from_slice(b"&gt;"),
                    _ => self.out.push(*b),
                }
            }
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
        // Flush any previous deferred element
        self.flush_deferred_element();

        // Defer this element until we've collected all attributes
        let ns = namespace
            .map(String::from)
            .or_else(|| self.pending_namespace.take());

        // Compute the close tag before storing the deferred element
        let close_tag = if let Some(ref ns_uri) = ns {
            if self.current_default_ns.as_deref() == Some(ns_uri.as_str()) {
                tag.to_string()
            } else {
                let prefix = self.get_or_create_prefix(ns_uri);
                format!("{}:{}", prefix, tag)
            }
        } else {
            tag.to_string()
        };

        self.deferred_element = Some((tag.to_string(), ns));
        self.collecting_attributes = true;
        self.element_stack.push(close_tag);

        Ok(())
    }

    fn attribute(
        &mut self,
        name: &str,
        value: &str,
        namespace: Option<&str>,
    ) -> Result<(), Self::Error> {
        let ns = namespace.map(String::from);
        self.pending_attributes
            .push((name.to_string(), value.to_string(), ns));
        Ok(())
    }

    fn children_start(&mut self) -> Result<(), Self::Error> {
        // Flush the deferred element now that attributes are done
        self.flush_deferred_element();
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
        self.flush_deferred_element();
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
            // Apply ns_all to elements only
            if let Some(ns_all) = &self.current_ns_all {
                self.pending_namespace = Some(ns_all.clone());
            }
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
