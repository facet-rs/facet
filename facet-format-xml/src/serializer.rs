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
    item_tag: &'static str,
    /// Namespace URI -> prefix mapping for already-declared namespaces.
    #[allow(dead_code)] // Used in Phase 4 namespace serialization (partial implementation)
    declared_namespaces: HashMap<String, String>,
    /// Counter for auto-generating namespace prefixes (ns0, ns1, ...).
    #[allow(dead_code)] // Used in Phase 4 namespace serialization (partial implementation)
    next_ns_index: usize,
    /// The currently active default namespace (from xmlns="..." on an ancestor).
    #[allow(dead_code)] // Used in Phase 4 namespace serialization (partial implementation)
    current_default_ns: Option<String>,
}

impl XmlSerializer {
    pub fn new() -> Self {
        let mut out = Vec::new();
        out.extend_from_slice(b"<root>");
        Self {
            out,
            stack: vec![Ctx::Root { kind: None }],
            pending_field: None,
            item_tag: "item",
            declared_namespaces: HashMap::new(),
            next_ns_index: 0,
            current_default_ns: None,
        }
    }

    pub fn finish(mut self) -> Vec<u8> {
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

    fn write_open_tag(&mut self, name: &str) {
        self.out.push(b'<');
        self.out.extend_from_slice(name.as_bytes());
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

    fn open_value_element_if_needed(&mut self) -> Result<Option<String>, XmlSerializeError> {
        match self.stack.last() {
            Some(Ctx::Root { .. }) => Ok(None),
            Some(Ctx::Struct { .. }) => {
                let Some(name) = self.pending_field.take() else {
                    return Err(XmlSerializeError {
                        msg: "value emitted in struct without field key",
                    });
                };
                self.write_open_tag(&name);
                Ok(Some(name))
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
    #[allow(dead_code)] // Used in Phase 4 namespace serialization (partial implementation)
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
                let close = self.open_value_element_if_needed()?;
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
        let close = self.open_value_element_if_needed()?;

        match scalar {
            ScalarValue::Null => {
                // Encode as the literal "null" to round-trip through parse_scalar.
                self.write_text_escaped("null");
            }
            ScalarValue::Bool(v) => self.write_text_escaped(if v { "true" } else { "false" }),
            ScalarValue::I64(v) => self.write_text_escaped(&v.to_string()),
            ScalarValue::U64(v) => self.write_text_escaped(&v.to_string()),
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
}

pub fn to_vec<'facet, T>(value: &'_ T) -> Result<Vec<u8>, SerializeError<XmlSerializeError>>
where
    T: Facet<'facet> + ?Sized,
{
    let mut serializer = XmlSerializer::new();
    serialize_root(&mut serializer, Peek::new(value))?;
    Ok(serializer.finish())
}
