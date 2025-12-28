extern crate alloc;

use alloc::{borrow::Cow, format, string::String, vec::Vec};
use std::{collections::HashMap, io::Write};

use facet_core::Facet;
use facet_format::{FormatSerializer, ScalarValue, SerializeError, serialize_root};
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
    pub fn pretty(mut self) -> Self {
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
    /// # use facet_format_xml as xml;
    /// # use facet_format_xml::{to_string_with_options, SerializeOptions};
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
    pub fn preserve_entities(mut self, preserve: bool) -> Self {
        self.preserve_entities = preserve;
        self
    }
}

/// Well-known XML namespace URIs and their conventional prefixes.
#[allow(dead_code)] // Used in Phase 4 namespace serialization (partial implementation)
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
    msg: &'static str,
}

impl core::fmt::Display for XmlSerializeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.msg)
    }
}

impl std::error::Error for XmlSerializeError {}

#[derive(Debug)]
enum Ctx {
    Root { kind: Option<Kind> },
    Struct { close: Option<String> },
    Seq { close: Option<String> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Struct,
    Seq,
}

/// XML serializer with configurable output options.
///
/// The output is designed to round-trip through `facet-format-xml`'s parser:
/// - structs are elements whose children are field elements
/// - sequences are elements whose children are repeated `<item>` elements
/// - element names are treated as map keys; the root element name is ignored
pub struct XmlSerializer {
    out: Vec<u8>,
    stack: Vec<Ctx>,
    pending_field: Option<String>,
    /// Pending namespace for the next field to be serialized
    pending_namespace: Option<String>,
    /// True if the current field is an attribute (vs element)
    pending_is_attribute: bool,
    /// True if the current field is text content (xml::text)
    pending_is_text: bool,
    /// True if the current field is an xml::elements list (no wrapper element)
    pending_is_elements: bool,
    /// Container-level default namespace (from xml::ns_all) for current struct
    current_ns_all: Option<String>,
    /// Buffered attributes for the current element (name, value, namespace_opt)
    pending_attributes: Vec<(String, String, Option<String>)>,
    item_tag: &'static str,
    /// Namespace URI -> prefix mapping for already-declared namespaces.
    declared_namespaces: HashMap<String, String>,
    /// Counter for auto-generating namespace prefixes (ns0, ns1, ...).
    next_ns_index: usize,
    /// The currently active default namespace (from xmlns="..." on an ancestor).
    /// When set, elements in this namespace use unprefixed names.
    current_default_ns: Option<String>,
    /// True if we've written the opening `<root>` tag
    root_tag_written: bool,
    /// Name to use for the root element (from struct's rename attribute)
    root_element_name: Option<String>,
    /// Deferred element tag - we wait to write the opening tag until we've collected all attributes.
    /// Format: (element_name, namespace, close_name)
    /// When Some, we haven't written `<tag ...>` yet; attributes are being collected in pending_attributes.
    deferred_open_tag: Option<(String, Option<String>, String)>,
    /// Stack of xml::elements state - when true, we're inside an xml::elements list
    /// and should not emit a wrapper element for the list items.
    elements_stack: Vec<bool>,
    /// When set, we're about to serialize an externally-tagged enum inside xml::elements.
    /// The next begin_struct() should be skipped (it's the wrapper struct), and the
    /// following field_key(variant_name) should also be skipped because variant_metadata
    /// already set up pending_field with the variant name.
    skip_enum_wrapper: Option<String>,
    /// Serialization options (pretty-printing, float formatting, etc.)
    options: SerializeOptions,
    /// Current indentation depth for pretty-printing
    depth: usize,
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
            stack: vec![Ctx::Root { kind: None }],
            pending_field: None,
            pending_namespace: None,
            pending_is_attribute: false,
            pending_is_text: false,
            pending_is_elements: false,
            current_ns_all: None,
            pending_attributes: Vec::new(),
            item_tag: "item",
            declared_namespaces: HashMap::new(),
            next_ns_index: 0,
            current_default_ns: None,
            root_tag_written: false,
            root_element_name: None,
            deferred_open_tag: None,
            elements_stack: Vec::new(),
            skip_enum_wrapper: None,
            options,
            depth: 0,
        }
    }

    pub fn finish(mut self) -> Vec<u8> {
        // Ensure root tag is written (even if struct is empty)
        self.ensure_root_tag_written();

        // Close any remaining non-root elements.
        while let Some(ctx) = self.stack.pop() {
            match ctx {
                Ctx::Root { .. } => break,
                Ctx::Struct { close } | Ctx::Seq { close } => {
                    if let Some(name) = close {
                        self.write_close_tag(&name, true);
                    }
                }
            }
        }
        // Write root closing tag
        self.depth = self.depth.saturating_sub(1);
        self.write_indent();
        let root_name = self.root_element_name.as_deref().unwrap_or("root");
        self.out.extend_from_slice(b"</");
        self.out.extend_from_slice(root_name.as_bytes());
        self.out.push(b'>');
        self.out
    }

    /// Flush any deferred opening tag (writing `<tag attrs>`) before we need to write content.
    /// This is called when we encounter a non-attribute field or element content.
    fn flush_deferred_open_tag(&mut self) {
        if let Some((element_name, element_ns, _close_name)) = self.deferred_open_tag.take() {
            self.write_indent();
            self.out.push(b'<');

            // Handle namespace for element
            if let Some(ns_uri) = element_ns {
                if self.current_default_ns.as_deref() == Some(&ns_uri) {
                    // Element is in the default namespace - use unprefixed form
                    self.out.extend_from_slice(element_name.as_bytes());
                } else {
                    // Get or create a prefix for this namespace
                    let prefix = self.get_or_create_prefix(&ns_uri);
                    self.out.extend_from_slice(prefix.as_bytes());
                    self.out.push(b':');
                    self.out.extend_from_slice(element_name.as_bytes());
                    // Write xmlns declaration
                    self.out.extend_from_slice(b" xmlns:");
                    self.out.extend_from_slice(prefix.as_bytes());
                    self.out.extend_from_slice(b"=\"");
                    self.out.extend_from_slice(ns_uri.as_bytes());
                    self.out.push(b'"');
                }
            } else {
                self.out.extend_from_slice(element_name.as_bytes());
            }

            // Write buffered attributes
            let attrs: Vec<_> = self.pending_attributes.drain(..).collect();
            let mut attrs_with_prefixes = Vec::new();
            for (name, value, ns) in attrs {
                let prefix = ns.as_ref().map(|uri| self.get_or_create_prefix(uri));
                attrs_with_prefixes.push((name, value, ns, prefix));
            }

            for (attr_name, attr_value, attr_ns, prefix_opt) in attrs_with_prefixes {
                self.out.push(b' ');
                if let (Some(ns_uri), Some(prefix)) = (attr_ns, prefix_opt) {
                    // Namespaced attribute - write xmlns declaration first
                    self.out.extend_from_slice(b"xmlns:");
                    self.out.extend_from_slice(prefix.as_bytes());
                    self.out.extend_from_slice(b"=\"");
                    self.out.extend_from_slice(ns_uri.as_bytes());
                    self.out.extend_from_slice(b"\" ");
                    // Now write the prefixed attribute
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
    }

    fn write_open_tag(&mut self, name: &str) {
        self.write_indent();
        self.out.push(b'<');

        // Check if we have a pending namespace for this field
        if let Some(ns_uri) = self.pending_namespace.take() {
            // Check if this namespace matches the current default namespace
            // If so, we can use an unprefixed element name (it inherits the default)
            if self.current_default_ns.as_deref() == Some(&ns_uri) {
                // Element is in the default namespace - use unprefixed form
                self.out.extend_from_slice(name.as_bytes());
            } else {
                // Get or create a prefix for this namespace
                let prefix = self.get_or_create_prefix(&ns_uri);

                // Write prefixed element name
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
            // No namespace - just write the element name
            self.out.extend_from_slice(name.as_bytes());
        }

        // Write buffered attributes
        // Drain attributes first to avoid borrow checker issues
        let attrs: Vec<_> = self.pending_attributes.drain(..).collect();

        // Now resolve prefixes for namespaced attributes
        let mut attrs_with_prefixes = Vec::new();
        for (name, value, ns) in attrs {
            let prefix = ns.as_ref().map(|uri| self.get_or_create_prefix(uri));
            attrs_with_prefixes.push((name, value, ns, prefix));
        }

        for (attr_name, attr_value, attr_ns, prefix_opt) in attrs_with_prefixes {
            self.out.push(b' ');

            if let (Some(ns_uri), Some(prefix)) = (attr_ns, prefix_opt) {
                // Namespaced attribute - write xmlns declaration first
                self.out.extend_from_slice(b"xmlns:");
                self.out.extend_from_slice(prefix.as_bytes());
                self.out.extend_from_slice(b"=\"");
                self.out.extend_from_slice(ns_uri.as_bytes());
                self.out.extend_from_slice(b"\" ");

                // Now write the prefixed attribute
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
    }

    /// Write a closing tag.
    /// If `block` is true, decrement depth and add indentation (for container elements).
    /// If `block` is false, write inline (for scalar elements where content preceded this).
    fn write_close_tag(&mut self, name: &str, block: bool) {
        if block {
            self.depth = self.depth.saturating_sub(1);
            self.write_indent();
        }
        self.out.extend_from_slice(b"</");
        self.out.extend_from_slice(name.as_bytes());
        self.out.push(b'>');
        if block {
            self.write_newline();
        }
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

    /// Format a float value using the custom formatter if set, otherwise default.
    fn format_float(&self, v: f64) -> String {
        if let Some(fmt) = self.options.float_formatter {
            let mut buf = Vec::new();
            if fmt(v, &mut buf).is_ok()
                && let Ok(s) = String::from_utf8(buf)
            {
                return s;
            }
        }
        v.to_string()
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

    fn ensure_root_tag_written(&mut self) {
        if !self.root_tag_written {
            let root_name = self.root_element_name.as_deref().unwrap_or("root");
            self.out.push(b'<');
            self.out.extend_from_slice(root_name.as_bytes());

            // If ns_all is set, emit a default namespace declaration (xmlns="...")
            // and set current_default_ns so child elements can use unprefixed form
            if let Some(ns_all) = &self.current_ns_all {
                self.out.extend_from_slice(b" xmlns=\"");
                self.out.extend_from_slice(ns_all.as_bytes());
                self.out.push(b'"');
                self.current_default_ns = Some(ns_all.clone());
            }

            // Write buffered attributes if any (for root-level attributes)
            let attrs: Vec<_> = self.pending_attributes.drain(..).collect();
            let mut attrs_with_prefixes = Vec::new();
            for (name, value, ns) in attrs {
                let prefix = ns.as_ref().map(|uri| self.get_or_create_prefix(uri));
                attrs_with_prefixes.push((name, value, ns, prefix));
            }

            for (attr_name, attr_value, attr_ns, prefix_opt) in attrs_with_prefixes {
                self.out.push(b' ');

                if let (Some(ns_uri), Some(prefix)) = (attr_ns, prefix_opt) {
                    // Namespaced attribute - write xmlns declaration first
                    self.out.extend_from_slice(b"xmlns:");
                    self.out.extend_from_slice(prefix.as_bytes());
                    self.out.extend_from_slice(b"=\"");
                    self.out.extend_from_slice(ns_uri.as_bytes());
                    self.out.extend_from_slice(b"\" ");

                    // Now write the prefixed attribute
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
            self.root_tag_written = true;
        }
    }

    fn open_value_element_if_needed(&mut self) -> Result<Option<String>, XmlSerializeError> {
        // Flush any deferred tag before opening a new element
        self.flush_deferred_open_tag();
        self.ensure_root_tag_written();
        match self.stack.last() {
            Some(Ctx::Root { .. }) => Ok(None),
            Some(Ctx::Struct { .. }) => {
                let Some(name) = self.pending_field.take() else {
                    return Err(XmlSerializeError {
                        msg: "value emitted in struct without field key",
                    });
                };

                // Compute the full tag name (with prefix if namespaced) for closing
                // If namespace matches current default, use unprefixed
                let full_name = if let Some(ns_uri) = self.pending_namespace.clone() {
                    if self.current_default_ns.as_deref() == Some(&ns_uri) {
                        // Element is in the default namespace - use unprefixed form
                        name.clone()
                    } else {
                        let prefix = self.get_or_create_prefix(&ns_uri);
                        format!("{}:{}", prefix, name)
                    }
                } else {
                    name.clone()
                };

                self.write_open_tag(&name);
                Ok(Some(full_name))
            }
            Some(Ctx::Seq { .. }) => {
                let name = self.item_tag.to_string();
                self.write_open_tag(&name);
                Ok(Some(name))
            }
            None => Err(XmlSerializeError {
                msg: "serializer state missing root context",
            }),
        }
    }

    /// Like `open_value_element_if_needed`, but defers writing the opening tag
    /// until we've collected all attributes. Returns the close tag name.
    fn defer_value_element_if_needed(&mut self) -> Result<Option<String>, XmlSerializeError> {
        self.ensure_root_tag_written();
        match self.stack.last() {
            Some(Ctx::Root { .. }) => Ok(None),
            Some(Ctx::Struct { .. }) => {
                let Some(name) = self.pending_field.take() else {
                    return Err(XmlSerializeError {
                        msg: "value emitted in struct without field key",
                    });
                };

                // Compute the full tag name (with prefix if namespaced) for closing
                let (close_name, element_ns) = if let Some(ns_uri) = self.pending_namespace.clone()
                {
                    if self.current_default_ns.as_deref() == Some(&ns_uri) {
                        (name.clone(), Some(ns_uri))
                    } else {
                        let prefix = self.get_or_create_prefix(&ns_uri);
                        (format!("{}:{}", prefix, name), Some(ns_uri))
                    }
                } else {
                    (name.clone(), None)
                };

                // Store the deferred tag info instead of writing it
                self.deferred_open_tag = Some((name, element_ns, close_name.clone()));
                self.pending_namespace = None;
                Ok(Some(close_name))
            }
            Some(Ctx::Seq { .. }) => {
                // For sequences, check if we have a pending field name (from xml::elements)
                // or fall back to item_tag for regular sequences
                if let Some(name) = self.pending_field.take() {
                    // xml::elements case - use the item's type name and defer the tag
                    let (close_name, element_ns) =
                        if let Some(ns_uri) = self.pending_namespace.clone() {
                            if self.current_default_ns.as_deref() == Some(&ns_uri) {
                                (name.clone(), Some(ns_uri))
                            } else {
                                let prefix = self.get_or_create_prefix(&ns_uri);
                                (format!("{}:{}", prefix, name), Some(ns_uri))
                            }
                        } else {
                            (name.clone(), None)
                        };
                    self.deferred_open_tag = Some((name, element_ns, close_name.clone()));
                    self.pending_namespace = None;
                    Ok(Some(close_name))
                } else {
                    // Regular sequence - use item_tag and write immediately
                    let name = self.item_tag.to_string();
                    self.write_open_tag(&name);
                    Ok(Some(name))
                }
            }
            None => Err(XmlSerializeError {
                msg: "serializer state missing root context",
            }),
        }
    }

    fn enter_struct_root(&mut self) {
        if let Some(Ctx::Root { kind }) = self.stack.last_mut() {
            *kind = Some(Kind::Struct);
        }
        self.stack.push(Ctx::Struct { close: None });
    }

    fn enter_seq_root(&mut self) {
        if let Some(Ctx::Root { kind }) = self.stack.last_mut() {
            *kind = Some(Kind::Seq);
        }
        self.stack.push(Ctx::Seq { close: None });
    }

    /// Get or create a prefix for the given namespace URI.
    /// Returns the prefix (without colon).
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
}

impl Default for XmlSerializer {
    fn default() -> Self {
        Self::new()
    }
}

impl FormatSerializer for XmlSerializer {
    type Error = XmlSerializeError;

    fn begin_struct(&mut self) -> Result<(), Self::Error> {
        // Flush any deferred tag from parent before starting a new struct
        self.flush_deferred_open_tag();

        // If we're skipping the enum wrapper struct (for xml::elements enum serialization),
        // just push a struct context without creating any element
        if self.skip_enum_wrapper.is_some() {
            self.stack.push(Ctx::Struct { close: None });
            return Ok(());
        }

        match self.stack.last() {
            Some(Ctx::Root { kind: None }) => {
                self.enter_struct_root();
                Ok(())
            }
            Some(Ctx::Root {
                kind: Some(Kind::Struct),
            }) => Err(XmlSerializeError {
                msg: "multiple root values are not supported",
            }),
            Some(Ctx::Root {
                kind: Some(Kind::Seq),
            })
            | Some(Ctx::Seq { .. })
            | Some(Ctx::Struct { .. }) => {
                // For nested structs, defer the opening tag until we've collected all attributes
                let close = self.defer_value_element_if_needed()?;
                self.stack.push(Ctx::Struct { close });
                Ok(())
            }
            None => Err(XmlSerializeError {
                msg: "serializer state missing root context",
            }),
        }
    }

    fn field_key(&mut self, key: &str) -> Result<(), Self::Error> {
        // If we're skipping the enum wrapper, check if this is the variant name field_key
        // that we should skip (variant_metadata already set up pending_field)
        if let Some(ref variant_name) = self.skip_enum_wrapper
            && key == variant_name
        {
            // Clear the skip flag - the wrapper struct's field_key is now consumed
            // The next begin_struct will be the actual content struct
            self.skip_enum_wrapper = None;
            return Ok(());
        }
        self.pending_field = Some(key.to_string());
        Ok(())
    }

    fn end_struct(&mut self) -> Result<(), Self::Error> {
        // Flush any deferred opening tag before closing
        self.flush_deferred_open_tag();

        match self.stack.pop() {
            Some(Ctx::Struct { close }) => {
                if let Some(name) = close {
                    self.write_close_tag(&name, true);
                }
                Ok(())
            }
            _ => Err(XmlSerializeError {
                msg: "end_struct called without matching begin_struct",
            }),
        }
    }

    fn begin_seq(&mut self) -> Result<(), Self::Error> {
        // Track if this is an xml::elements sequence (no wrapper element)
        let is_elements = self.pending_is_elements;
        self.pending_is_elements = false;
        self.elements_stack.push(is_elements);

        match self.stack.last() {
            Some(Ctx::Root { kind: None }) => {
                self.enter_seq_root();
                Ok(())
            }
            Some(Ctx::Root {
                kind: Some(Kind::Seq),
            }) => Err(XmlSerializeError {
                msg: "multiple root values are not supported",
            }),
            Some(Ctx::Root {
                kind: Some(Kind::Struct),
            })
            | Some(Ctx::Seq { .. })
            | Some(Ctx::Struct { .. }) => {
                // For xml::elements, don't create a wrapper element - items go directly as children
                if is_elements {
                    self.pending_field = None;
                    self.pending_namespace = None;
                    self.stack.push(Ctx::Seq { close: None });
                } else {
                    let close = self.open_value_element_if_needed()?;
                    self.stack.push(Ctx::Seq { close });
                }
                Ok(())
            }
            None => Err(XmlSerializeError {
                msg: "serializer state missing root context",
            }),
        }
    }

    fn end_seq(&mut self) -> Result<(), Self::Error> {
        // Pop the xml::elements state
        self.elements_stack.pop();

        match self.stack.pop() {
            Some(Ctx::Seq { close }) => {
                if let Some(name) = close {
                    self.write_close_tag(&name, true);
                }
                Ok(())
            }
            _ => Err(XmlSerializeError {
                msg: "end_seq called without matching begin_seq",
            }),
        }
    }

    fn scalar(&mut self, scalar: ScalarValue<'_>) -> Result<(), Self::Error> {
        // If this is an attribute, buffer it instead of writing as a child element
        if self.pending_is_attribute {
            let name = self.pending_field.take().ok_or(XmlSerializeError {
                msg: "attribute value without field name",
            })?;
            let namespace = self.pending_namespace.take();

            // Convert scalar to string for attribute value
            let value = match scalar {
                ScalarValue::Null => "null".to_string(),
                ScalarValue::Bool(v) => if v { "true" } else { "false" }.to_string(),
                ScalarValue::I64(v) => v.to_string(),
                ScalarValue::U64(v) => v.to_string(),
                ScalarValue::I128(v) => v.to_string(),
                ScalarValue::U128(v) => v.to_string(),
                ScalarValue::F64(v) => self.format_float(v),
                ScalarValue::Str(s) => s.into_owned(),
                ScalarValue::Bytes(_) => {
                    return Err(XmlSerializeError {
                        msg: "bytes serialization unsupported for xml",
                    });
                }
            };

            self.pending_attributes.push((name, value, namespace));
            self.pending_is_attribute = false;
            return Ok(());
        }

        // If this is text content (xml::text), write it directly without element wrapper
        if self.pending_is_text {
            // Clear pending field - we're writing text content, not an element
            self.pending_field = None;
            self.pending_namespace = None;
            self.pending_is_text = false;

            // Flush any deferred opening tag first
            self.flush_deferred_open_tag();
            self.ensure_root_tag_written();

            // Write the text content directly
            match scalar {
                ScalarValue::Null => self.write_text_escaped("null"),
                ScalarValue::Bool(v) => self.write_text_escaped(if v { "true" } else { "false" }),
                ScalarValue::I64(v) => self.write_text_escaped(&v.to_string()),
                ScalarValue::U64(v) => self.write_text_escaped(&v.to_string()),
                ScalarValue::I128(v) => self.write_text_escaped(&v.to_string()),
                ScalarValue::U128(v) => self.write_text_escaped(&v.to_string()),
                ScalarValue::F64(v) => {
                    let formatted = self.format_float(v);
                    self.write_text_escaped(&formatted);
                }
                ScalarValue::Str(s) => self.write_text_escaped(&s),
                ScalarValue::Bytes(_) => {
                    return Err(XmlSerializeError {
                        msg: "bytes serialization unsupported for xml",
                    });
                }
            }
            return Ok(());
        }

        // Regular child element
        let close = self.open_value_element_if_needed()?;

        match scalar {
            ScalarValue::Null => {
                // Encode as the literal "null" to round-trip through parse_scalar.
                self.write_text_escaped("null");
            }
            ScalarValue::Bool(v) => self.write_text_escaped(if v { "true" } else { "false" }),
            ScalarValue::I64(v) => self.write_text_escaped(&v.to_string()),
            ScalarValue::U64(v) => self.write_text_escaped(&v.to_string()),
            ScalarValue::I128(v) => self.write_text_escaped(&v.to_string()),
            ScalarValue::U128(v) => self.write_text_escaped(&v.to_string()),
            ScalarValue::F64(v) => {
                let formatted = self.format_float(v);
                self.write_text_escaped(&formatted);
            }
            ScalarValue::Str(s) => self.write_text_escaped(&s),
            ScalarValue::Bytes(_) => {
                return Err(XmlSerializeError {
                    msg: "bytes serialization unsupported for xml",
                });
            }
        }

        if let Some(name) = close {
            // Scalar close is inline (no indent), then newline
            self.write_close_tag(&name, false);
            self.write_newline();
        }

        Ok(())
    }

    fn field_metadata(&mut self, field: &facet_reflect::FieldItem) -> Result<(), Self::Error> {
        // Check if this field is an attribute
        self.pending_is_attribute = field.field.get_attr(Some("xml"), "attribute").is_some();
        // Check if this field is text content
        self.pending_is_text = field.field.get_attr(Some("xml"), "text").is_some();
        // Check if this field is an xml::elements list (no wrapper element)
        self.pending_is_elements = field.field.get_attr(Some("xml"), "elements").is_some();

        // Extract xml::ns attribute from the field
        if let Some(ns_attr) = field.field.get_attr(Some("xml"), "ns")
            && let Some(ns_uri) = ns_attr.get_as::<&str>().copied()
        {
            self.pending_namespace = Some(ns_uri.to_string());
            return Ok(());
        }

        // If field doesn't have explicit xml::ns, check for container-level xml::ns_all
        // Only apply ns_all to elements, not attributes or text content (per XML spec)
        if !self.pending_is_attribute
            && !self.pending_is_text
            && let Some(ns_all) = &self.current_ns_all
        {
            self.pending_namespace = Some(ns_all.clone());
        }

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

        // Get the element name from the shape (respecting rename attribute)
        let element_name = shape
            .get_builtin_attr_value::<&str>("rename")
            .unwrap_or(shape.type_identifier);

        // If this is the root element (stack only has Root context), save the name
        if matches!(self.stack.last(), Some(Ctx::Root { kind: None })) {
            self.root_element_name = Some(element_name.to_string());
        }

        // If we're inside an xml::elements list, use the shape's element name
        if self.elements_stack.last() == Some(&true) && self.pending_field.is_none() {
            self.pending_field = Some(element_name.to_string());

            // Also apply xml::ns_all if set
            if let Some(ns_all) = &self.current_ns_all {
                self.pending_namespace = Some(ns_all.clone());
            }
        }

        Ok(())
    }

    fn variant_metadata(
        &mut self,
        variant: &'static facet_core::Variant,
    ) -> Result<(), Self::Error> {
        // If we're inside an xml::elements list, set the pending field to the variant name
        // and mark that we should skip the externally-tagged wrapper struct.
        //
        // For externally-tagged enums, the serialization flow is:
        //   1. variant_metadata(variant) - we're here
        //   2. begin_struct() - creates wrapper struct (we want to SKIP this)
        //   3. field_key(variant.name) - sets field name (we want to SKIP this)
        //   4. shared_serialize(inner) - serializes the actual content
        //
        // We set pending_field to the variant name, and skip_enum_wrapper to tell
        // begin_struct() to not create an element, and field_key() to not override
        // the pending_field we just set.
        if self.elements_stack.last() == Some(&true) {
            // Get the element name from the variant (respecting rename attribute)
            let element_name = variant
                .get_builtin_attr("rename")
                .and_then(|attr| attr.get_as::<&str>().copied())
                .unwrap_or(variant.name);
            self.pending_field = Some(element_name.to_string());
            // Set the skip flag with the variant name so field_key knows what to skip
            self.skip_enum_wrapper = Some(variant.name.to_string());
        }
        Ok(())
    }

    /// For XML, `None` values should not emit any content.
    /// We skip emitting an element entirely rather than writing `<field>null</field>`.
    fn serialize_none(&mut self) -> Result<(), Self::Error> {
        // Clear pending field state - we're skipping this value
        self.pending_field = None;
        self.pending_namespace = None;
        self.pending_is_attribute = false;
        self.pending_is_text = false;
        // Do nothing - don't emit anything for None
        Ok(())
    }

    fn preferred_field_order(&self) -> facet_format::FieldOrdering {
        facet_format::FieldOrdering::AttributesFirst
    }
}

/// Serialize a value to XML bytes with default options.
pub fn to_vec<'facet, T>(value: &'_ T) -> Result<Vec<u8>, SerializeError<XmlSerializeError>>
where
    T: Facet<'facet> + ?Sized,
{
    to_vec_with_options(value, &SerializeOptions::default())
}

/// Serialize a value to XML bytes with custom options.
pub fn to_vec_with_options<'facet, T>(
    value: &'_ T,
    options: &SerializeOptions,
) -> Result<Vec<u8>, SerializeError<XmlSerializeError>>
where
    T: Facet<'facet> + ?Sized,
{
    let mut serializer = XmlSerializer::with_options(options.clone());
    serialize_root(&mut serializer, Peek::new(value))?;
    Ok(serializer.finish())
}

/// Serialize a value to an XML string with default options.
pub fn to_string<'facet, T>(value: &'_ T) -> Result<String, SerializeError<XmlSerializeError>>
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
) -> Result<String, SerializeError<XmlSerializeError>>
where
    T: Facet<'facet> + ?Sized,
{
    to_string_with_options(value, &SerializeOptions::default().pretty())
}

/// Serialize a value to an XML string with custom options.
pub fn to_string_with_options<'facet, T>(
    value: &'_ T,
    options: &SerializeOptions,
) -> Result<String, SerializeError<XmlSerializeError>>
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
