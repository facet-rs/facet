extern crate alloc;

use alloc::{format, string::String, vec::Vec};
use std::collections::HashMap;

use facet_core::Facet;
use facet_format::{FormatSerializer, ScalarValue, SerializeError, serialize_root};
use facet_reflect::Peek;

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

/// Minimal XML serializer for the codex prototype.
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
    /// Deferred element tag - we wait to write the opening tag until we've collected all attributes.
    /// Format: (element_name, namespace, close_name)
    /// When Some, we haven't written `<tag ...>` yet; attributes are being collected in pending_attributes.
    deferred_open_tag: Option<(String, Option<String>, String)>,
}

impl XmlSerializer {
    pub fn new() -> Self {
        Self {
            out: Vec::new(),
            stack: vec![Ctx::Root { kind: None }],
            pending_field: None,
            pending_namespace: None,
            pending_is_attribute: false,
            pending_is_text: false,
            current_ns_all: None,
            pending_attributes: Vec::new(),
            item_tag: "item",
            declared_namespaces: HashMap::new(),
            next_ns_index: 0,
            current_default_ns: None,
            root_tag_written: false,
            deferred_open_tag: None,
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
                        self.write_close_tag(&name);
                    }
                }
            }
        }
        self.out.extend_from_slice(b"</root>");
        self.out
    }

    /// Flush any deferred opening tag (writing `<tag attrs>`) before we need to write content.
    /// This is called when we encounter a non-attribute field or element content.
    fn flush_deferred_open_tag(&mut self) {
        if let Some((element_name, element_ns, _close_name)) = self.deferred_open_tag.take() {
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
        }
    }

    fn write_open_tag(&mut self, name: &str) {
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

    fn write_close_tag(&mut self, name: &str) {
        self.out.extend_from_slice(b"</");
        self.out.extend_from_slice(name.as_bytes());
        self.out.push(b'>');
    }

    fn write_text_escaped(&mut self, text: &str) {
        for b in text.as_bytes() {
            match *b {
                b'&' => self.out.extend_from_slice(b"&amp;"),
                b'<' => self.out.extend_from_slice(b"&lt;"),
                b'>' => self.out.extend_from_slice(b"&gt;"),
                _ => self.out.push(*b),
            }
        }
    }

    fn ensure_root_tag_written(&mut self) {
        if !self.root_tag_written {
            self.out.extend_from_slice(b"<root");

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
                // For sequences, don't defer - write immediately
                let name = self.item_tag.to_string();
                self.write_open_tag(&name);
                Ok(Some(name))
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
        self.pending_field = Some(key.to_string());
        Ok(())
    }

    fn end_struct(&mut self) -> Result<(), Self::Error> {
        // Flush any deferred opening tag before closing
        self.flush_deferred_open_tag();

        match self.stack.pop() {
            Some(Ctx::Struct { close }) => {
                if let Some(name) = close {
                    self.write_close_tag(&name);
                }
                Ok(())
            }
            _ => Err(XmlSerializeError {
                msg: "end_struct called without matching begin_struct",
            }),
        }
    }

    fn begin_seq(&mut self) -> Result<(), Self::Error> {
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
                let close = self.open_value_element_if_needed()?;
                self.stack.push(Ctx::Seq { close });
                Ok(())
            }
            None => Err(XmlSerializeError {
                msg: "serializer state missing root context",
            }),
        }
    }

    fn end_seq(&mut self) -> Result<(), Self::Error> {
        match self.stack.pop() {
            Some(Ctx::Seq { close }) => {
                if let Some(name) = close {
                    self.write_close_tag(&name);
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
                ScalarValue::F64(v) => v.to_string(),
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
                ScalarValue::F64(v) => self.write_text_escaped(&v.to_string()),
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
            ScalarValue::F64(v) => self.write_text_escaped(&v.to_string()),
            ScalarValue::Str(s) => self.write_text_escaped(&s),
            ScalarValue::Bytes(_) => {
                return Err(XmlSerializeError {
                    msg: "bytes serialization unsupported for xml",
                });
            }
        }

        if let Some(name) = close {
            self.write_close_tag(&name);
        }

        Ok(())
    }

    fn field_metadata(&mut self, field: &facet_reflect::FieldItem) -> Result<(), Self::Error> {
        // Check if this field is an attribute
        self.pending_is_attribute = field.field.get_attr(Some("xml"), "attribute").is_some();
        // Check if this field is text content
        self.pending_is_text = field.field.get_attr(Some("xml"), "text").is_some();

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

pub fn to_vec<'facet, T>(value: &'_ T) -> Result<Vec<u8>, SerializeError<XmlSerializeError>>
where
    T: Facet<'facet> + ?Sized,
{
    let mut serializer = XmlSerializer::new();
    serialize_root(&mut serializer, Peek::new(value))?;
    Ok(serializer.finish())
}
