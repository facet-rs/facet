//! KDL serialization implementation using FormatSerializer trait.

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

use facet_core::Facet;
use facet_format::{FormatSerializer, ScalarValue, SerializeError, serialize_root};
use facet_reflect::Peek;

/// Error type for KDL serialization.
#[derive(Debug)]
pub struct KdlSerializeError {
    msg: &'static str,
}

impl core::fmt::Display for KdlSerializeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.msg)
    }
}

impl std::error::Error for KdlSerializeError {}

/// Context for tracking serialization state.
#[derive(Debug, Clone)]
enum Ctx {
    /// At the root level - next struct becomes root element
    Root {
        /// Name of the root element (from struct_metadata)
        name: Option<String>,
    },
    /// In a struct/node - fields become children
    Struct {
        /// Node name
        name: String,
        /// Whether we've written the opening brace
        opened_brace: bool,
        /// Pending properties (kdl::property fields)
        properties: Vec<(String, String)>,
        /// Pending arguments (kdl::argument fields)
        arguments: Vec<String>,
    },
    /// In a sequence - items become child nodes named "item"
    Seq {
        /// The wrapper node name (from pending field, e.g., "triple")
        wrapper_name: String,
        /// Whether we've written the opening `wrapper {`
        opened: bool,
    },
}

/// KDL serializer implementing FormatSerializer.
pub struct KdlSerializer {
    out: Vec<u8>,
    stack: Vec<Ctx>,
    pending_field: Option<String>,
    pending_is_property: bool,
    pending_is_argument: bool,
    pending_is_child: bool,
    indent_level: usize,
}

impl KdlSerializer {
    /// Create a new KDL serializer.
    pub fn new() -> Self {
        Self {
            out: Vec::new(),
            stack: vec![Ctx::Root { name: None }],
            pending_field: None,
            pending_is_property: false,
            pending_is_argument: false,
            pending_is_child: false,
            indent_level: 0,
        }
    }

    /// Consume the serializer and return the output bytes.
    pub fn finish(self) -> Vec<u8> {
        self.out
    }

    fn write_indent(&mut self) {
        for _ in 0..self.indent_level {
            self.out.extend_from_slice(b"    ");
        }
    }

    fn scalar_to_string(&self, scalar: &ScalarValue<'_>) -> String {
        match scalar {
            ScalarValue::Null => "#null".to_string(),
            ScalarValue::Bool(true) => "#true".to_string(),
            ScalarValue::Bool(false) => "#false".to_string(),
            ScalarValue::Char(c) => {
                let mut result = String::with_capacity(3);
                result.push('"');
                result.push(*c);
                result.push('"');
                result
            }
            ScalarValue::I64(n) => {
                #[cfg(feature = "fast")]
                return itoa::Buffer::new().format(*n).to_string();
                #[cfg(not(feature = "fast"))]
                n.to_string()
            }
            ScalarValue::U64(n) => {
                #[cfg(feature = "fast")]
                return itoa::Buffer::new().format(*n).to_string();
                #[cfg(not(feature = "fast"))]
                n.to_string()
            }
            ScalarValue::I128(n) => {
                #[cfg(feature = "fast")]
                return itoa::Buffer::new().format(*n).to_string();
                #[cfg(not(feature = "fast"))]
                n.to_string()
            }
            ScalarValue::U128(n) => {
                #[cfg(feature = "fast")]
                return itoa::Buffer::new().format(*n).to_string();
                #[cfg(not(feature = "fast"))]
                n.to_string()
            }
            ScalarValue::F64(n) => {
                if n.is_nan() {
                    "#nan".to_string()
                } else if n.is_infinite() {
                    if *n > 0.0 {
                        "#inf".to_string()
                    } else {
                        "#-inf".to_string()
                    }
                } else {
                    #[cfg(feature = "fast")]
                    return zmij::Buffer::new().format(*n).to_string();
                    #[cfg(not(feature = "fast"))]
                    n.to_string()
                }
            }
            ScalarValue::Str(s) | ScalarValue::StringlyTyped(s) => {
                // Return with quotes and proper escaping
                let mut result = String::with_capacity(s.len() + 2);
                result.push('"');
                for c in s.chars() {
                    match c {
                        '"' => result.push_str("\\\""),
                        '\\' => result.push_str("\\\\"),
                        '\n' => result.push_str("\\n"),
                        '\r' => result.push_str("\\r"),
                        '\t' => result.push_str("\\t"),
                        '\u{0008}' => result.push_str("\\b"), // backspace
                        '\u{000C}' => result.push_str("\\f"), // form feed
                        c if c.is_control() => {
                            // Other control characters as unicode escapes
                            result.push_str(&format!("\\u{{{:04X}}}", c as u32));
                        }
                        _ => result.push(c),
                    }
                }
                result.push('"');
                result
            }
            ScalarValue::Bytes(_) => {
                // KDL doesn't have native bytes support
                "\"<bytes>\"".to_string()
            }
        }
    }

    /// Ensure the current struct has an opened brace (for adding children).
    fn ensure_struct_opened(&mut self) {
        if let Some(Ctx::Struct {
            name,
            opened_brace,
            properties,
            arguments,
        }) = self.stack.last_mut()
            && !*opened_brace
        {
            // Write node name
            self.out.extend_from_slice(name.as_bytes());

            // Write arguments first
            for arg in arguments.drain(..) {
                self.out.push(b' ');
                self.out.extend_from_slice(arg.as_bytes());
            }

            // Write properties
            for (k, v) in properties.drain(..) {
                self.out.push(b' ');
                self.out.extend_from_slice(k.as_bytes());
                self.out.push(b'=');
                self.out.extend_from_slice(v.as_bytes());
            }

            // Open brace for children
            self.out.extend_from_slice(b" {");
            *opened_brace = true;
            self.indent_level += 1;
        }
    }

    /// Ensure the current sequence wrapper is opened.
    fn ensure_seq_opened(&mut self) {
        // Check if we need to open, and get the wrapper name
        let needs_open = matches!(self.stack.last(), Some(Ctx::Seq { opened: false, .. }));

        if needs_open
            && let Some(Ctx::Seq {
                wrapper_name,
                opened,
            }) = self.stack.last_mut()
        {
            let name = wrapper_name.clone();
            *opened = true;
            // Now do the writing without borrowing stack
            self.out.push(b'\n');
            self.write_indent();
            self.out.extend_from_slice(name.as_bytes());
            self.out.extend_from_slice(b" {");
            self.indent_level += 1;
        }
    }
}

impl Default for KdlSerializer {
    fn default() -> Self {
        Self::new()
    }
}

impl FormatSerializer for KdlSerializer {
    type Error = KdlSerializeError;

    fn struct_metadata(&mut self, shape: &facet_core::Shape) -> Result<(), Self::Error> {
        // Get the element name (respecting rename attribute, otherwise lowercase type name)
        let element_name = shape
            .get_builtin_attr_value::<&str>("rename")
            .map(|s| s.to_string())
            .unwrap_or_else(|| to_kebab_case(shape.type_identifier));

        // If this is the root, save the name
        if let Some(Ctx::Root { name }) = self.stack.last_mut() {
            *name = Some(element_name);
        }

        Ok(())
    }

    fn begin_struct(&mut self) -> Result<(), Self::Error> {
        // Determine what context we're in
        enum Action {
            Root(Option<String>),
            NestedStruct,
            SeqItem,
            NoStack,
        }

        let action = match self.stack.last_mut() {
            Some(Ctx::Root { name }) => Action::Root(name.take()),
            Some(Ctx::Struct { .. }) => Action::NestedStruct,
            Some(Ctx::Seq { .. }) => Action::SeqItem,
            None => Action::NoStack,
        };

        let node_name = match action {
            Action::Root(name) => name.unwrap_or_else(|| "root".to_string()),
            Action::NestedStruct => {
                // Need to ensure parent is opened first
                self.ensure_struct_opened();
                self.out.push(b'\n');
                self.write_indent();
                // Nested struct - use pending field name
                self.pending_field
                    .take()
                    .unwrap_or_else(|| "node".to_string())
            }
            Action::SeqItem => {
                // Struct inside a sequence - ensure seq wrapper is opened first
                self.ensure_seq_opened();
                self.out.push(b'\n');
                self.write_indent();
                // Use "item" as the default node name for struct items in sequences
                "item".to_string()
            }
            Action::NoStack => "root".to_string(),
        };

        self.stack.push(Ctx::Struct {
            name: node_name,
            opened_brace: false,
            properties: Vec::new(),
            arguments: Vec::new(),
        });

        Ok(())
    }

    fn field_key(&mut self, key: &str) -> Result<(), Self::Error> {
        self.pending_field = Some(key.to_string());
        // NOTE: Do NOT reset field type flags here - they are set by field_metadata()
        // which is called BEFORE field_key(). Resetting them would lose the metadata.
        Ok(())
    }

    fn end_struct(&mut self) -> Result<(), Self::Error> {
        match self.stack.pop() {
            Some(Ctx::Struct {
                name,
                opened_brace,
                arguments,
                properties,
            }) => {
                if opened_brace {
                    // Had children - close the brace
                    self.indent_level = self.indent_level.saturating_sub(1);
                    self.out.push(b'\n');
                    self.write_indent();
                    self.out.push(b'}');
                } else {
                    // No children - write node with args/props only
                    self.out.extend_from_slice(name.as_bytes());
                    for arg in arguments {
                        self.out.push(b' ');
                        self.out.extend_from_slice(arg.as_bytes());
                    }
                    for (k, v) in properties {
                        self.out.push(b' ');
                        self.out.extend_from_slice(k.as_bytes());
                        self.out.push(b'=');
                        self.out.extend_from_slice(v.as_bytes());
                    }
                }
                Ok(())
            }
            Some(Ctx::Root { name }) => {
                // Root struct that was never started - write empty node
                if let Some(n) = name {
                    self.out.extend_from_slice(n.as_bytes());
                }
                Ok(())
            }
            _ => Err(KdlSerializeError {
                msg: "end_struct without matching begin_struct",
            }),
        }
    }

    fn begin_seq(&mut self) -> Result<(), Self::Error> {
        // Check if we're inside a sequence - if so, this is a nested sequence that
        // should be wrapped in an "item" node
        let is_nested_seq = matches!(self.stack.last(), Some(Ctx::Seq { .. }));

        if is_nested_seq {
            // For nested sequences, open the parent seq first, then wrap in "item { }"
            self.ensure_seq_opened();
            self.out.push(b'\n');
            self.write_indent();
            self.out.extend_from_slice(b"item {");
            self.indent_level += 1;

            // Push a Seq context for the inner sequence items
            self.stack.push(Ctx::Seq {
                wrapper_name: "item".to_string(), // Already wrote this
                opened: true,                     // Already opened
            });
        } else if self.pending_is_child {
            // kdl::children - items should be emitted directly as children
            // without a wrapper node. Just ensure parent struct is opened.
            self.ensure_struct_opened();

            // Use a special Seq context that won't write a wrapper
            self.stack.push(Ctx::Seq {
                wrapper_name: String::new(), // No wrapper
                opened: true,                // Already "opened" (no wrapper to open)
            });

            // Clear the pending field - we don't need the field name
            self.pending_field = None;
        } else {
            // Get wrapper name from pending field
            let wrapper_name = self
                .pending_field
                .take()
                .unwrap_or_else(|| "items".to_string());

            // If we're in a struct, ensure parent brace is opened
            self.ensure_struct_opened();

            self.stack.push(Ctx::Seq {
                wrapper_name,
                opened: false,
            });
        }
        Ok(())
    }

    fn end_seq(&mut self) -> Result<(), Self::Error> {
        match self.stack.pop() {
            Some(Ctx::Seq {
                opened,
                wrapper_name,
            }) => {
                // Only close brace if we actually wrote a wrapper
                // (kdl::children has empty wrapper_name and doesn't write a wrapper)
                if opened && !wrapper_name.is_empty() {
                    // Close the wrapper brace
                    self.indent_level = self.indent_level.saturating_sub(1);
                    self.out.push(b'\n');
                    self.write_indent();
                    self.out.push(b'}');
                }
                Ok(())
            }
            _ => Err(KdlSerializeError {
                msg: "end_seq without matching begin_seq",
            }),
        }
    }

    fn scalar(&mut self, scalar: ScalarValue<'_>) -> Result<(), Self::Error> {
        let value_str = self.scalar_to_string(&scalar);

        match self.stack.last_mut() {
            Some(Ctx::Struct {
                opened_brace,
                properties,
                arguments,
                ..
            }) => {
                if self.pending_is_property {
                    // KDL property: buffer it
                    if let Some(key) = self.pending_field.take() {
                        properties.push((key, value_str));
                    }
                } else if self.pending_is_argument {
                    // KDL argument: buffer it
                    arguments.push(value_str);
                    self.pending_field = None;
                } else if self.pending_is_child || *opened_brace {
                    // Child node with scalar value
                    // Ensure struct is opened
                    if !*opened_brace {
                        self.ensure_struct_opened();
                    }
                    self.out.push(b'\n');
                    self.write_indent();
                    if let Some(key) = self.pending_field.take() {
                        self.out.extend_from_slice(key.as_bytes());
                        self.out.push(b' ');
                    }
                    self.out.extend_from_slice(value_str.as_bytes());
                } else {
                    // Default for fields without attributes: emit as child node
                    self.ensure_struct_opened();
                    self.out.push(b'\n');
                    self.write_indent();
                    if let Some(key) = self.pending_field.take() {
                        self.out.extend_from_slice(key.as_bytes());
                        self.out.push(b' ');
                    }
                    self.out.extend_from_slice(value_str.as_bytes());
                }
            }
            Some(Ctx::Seq { .. }) => {
                // Sequence item - ensure wrapper is opened, then write item node
                self.ensure_seq_opened();
                self.out.push(b'\n');
                self.write_indent();
                self.out.extend_from_slice(b"item ");
                self.out.extend_from_slice(value_str.as_bytes());
            }
            Some(Ctx::Root { .. }) | None => {
                // Top level scalar - write as value node
                self.out.extend_from_slice(b"value ");
                self.out.extend_from_slice(value_str.as_bytes());
            }
        }

        self.pending_field = None;
        Ok(())
    }

    fn field_metadata(&mut self, field: &facet_reflect::FieldItem) -> Result<(), Self::Error> {
        // For flattened map entries (field is None), treat as properties
        let Some(field_def) = field.field else {
            self.pending_is_property = true;
            self.pending_is_argument = false;
            self.pending_is_child = false;
            return Ok(());
        };

        // Check for kdl-specific attributes
        self.pending_is_property = field_def.get_attr(Some("kdl"), "property").is_some();
        self.pending_is_argument = field_def.get_attr(Some("kdl"), "argument").is_some()
            || field_def.get_attr(Some("kdl"), "arguments").is_some();
        self.pending_is_child = field_def.get_attr(Some("kdl"), "child").is_some()
            || field_def.get_attr(Some("kdl"), "children").is_some();
        Ok(())
    }
}

/// Convert a PascalCase type name to a lowercase name suitable for KDL nodes.
fn to_kebab_case(s: &str) -> String {
    // Simple approach: just lowercase the whole thing
    s.to_lowercase()
}

/// Serialize a value to KDL bytes.
pub fn to_vec<'facet, T>(value: &T) -> Result<Vec<u8>, SerializeError<KdlSerializeError>>
where
    T: Facet<'facet> + ?Sized,
{
    let mut serializer = KdlSerializer::new();
    serialize_root(&mut serializer, Peek::new(value))?;
    Ok(serializer.finish())
}

/// Serialize a value to a KDL string.
pub fn to_string<'facet, T>(value: &T) -> Result<String, SerializeError<KdlSerializeError>>
where
    T: Facet<'facet> + ?Sized,
{
    let bytes = to_vec(value)?;
    Ok(String::from_utf8(bytes).expect("KDL output should always be valid UTF-8"))
}
