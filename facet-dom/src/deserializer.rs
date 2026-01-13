//! Tree-based deserializer for DOM documents.

#![deny(let_underscore_drop)]
#![deny(clippy::let_underscore_must_use)]

use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt;

use facet_core::{Def, Facet, Field, Shape, StructType, Type, UserType};
use facet_reflect::{HeapValue, Partial};

use crate::{DomEvent, DomParser};

// Tracing macros - compile to nothing when tracing feature is disabled
macro_rules! trace {
    ($($arg:tt)*) => {
        #[cfg(feature = "tracing")]
        tracing::trace!($($arg)*);
    };
}

macro_rules! trace_span {
    ($($arg:tt)*) => {
        #[cfg(feature = "tracing")]
        let _span = tracing::trace_span!($($arg)*).entered();
    };
}

/// Info about a field in a struct for deserialization purposes.
#[derive(Clone, Copy)]
struct FieldInfo {
    idx: usize,
    field: &'static Field,
    /// True if this field is a list type (Vec, etc.)
    is_list: bool,
    /// For list fields, the shape of the list item
    item_shape: Option<&'static Shape>,
}

/// Precomputed field lookup map for a struct.
///
/// This separates "what fields does this struct have" from the parsing loop,
/// making the code cleaner and avoiding repeated linear scans.
struct StructFieldMap {
    /// Fields marked with `xml::attribute`, keyed by name/rename
    attribute_fields: HashMap<String, FieldInfo>,
    /// Fields that are child elements, keyed by name/rename
    element_fields: HashMap<String, FieldInfo>,
    /// The field marked with `xml::elements` (collects all unmatched children)
    elements_field: Option<FieldInfo>,
    /// The field marked with `xml::text` (collects text content)
    text_field: Option<FieldInfo>,
}

impl StructFieldMap {
    /// Build the field map from a struct definition.
    fn new(struct_def: &'static StructType) -> Self {
        trace_span!("StructFieldMap::new");

        let mut attribute_fields = HashMap::new();
        let mut element_fields = HashMap::new();
        let mut elements_field = None;
        let mut text_field = None;

        for (idx, field) in struct_def.fields.iter().enumerate() {
            // Use exact name - case-sensitive matching for XML
            let name = field.rename.unwrap_or(field.name).to_string();

            // Check if this field is a list type
            let shape = field.shape();
            let (is_list, item_shape) = match &shape.def {
                Def::List(list_def) => (true, Some(list_def.t)),
                _ => (false, None),
            };

            let info = FieldInfo {
                idx,
                field,
                is_list,
                item_shape,
            };

            if field.is_attribute() {
                trace!(idx, field_name = %field.name, key = %name, "found attribute field");
                attribute_fields.insert(name, info);
            } else if field.is_elements() {
                trace!(idx, field_name = %field.name, "found elements collection field");
                elements_field = Some(info);
            } else if field.is_text() {
                trace!(idx, field_name = %field.name, "found text field");
                text_field = Some(info);
            } else {
                // Default: unmarked fields and explicit xml::element fields are child elements
                trace!(idx, field_name = %field.name, key = %name, is_list, "found element field (default or explicit)");
                element_fields.insert(name, info);
            }
        }

        trace!(
            attribute_count = attribute_fields.len(),
            element_count = element_fields.len(),
            has_elements = elements_field.is_some(),
            has_text = text_field.is_some(),
            "field map built"
        );

        Self {
            attribute_fields,
            element_fields,
            elements_field,
            text_field,
        }
    }

    /// Find an attribute field by name (exact match).
    fn find_attribute(&self, name: &str) -> Option<FieldInfo> {
        let result = self.attribute_fields.get(name).copied();
        trace!(name, found = result.is_some(), "find_attribute");
        result
    }

    /// Find an element field by tag name (exact match).
    fn find_element(&self, tag: &str) -> Option<FieldInfo> {
        let result = self.element_fields.get(tag).copied();
        trace!(tag, found = result.is_some(), "find_element");
        result
    }
}

/// Error type for DOM deserialization.
#[derive(Debug)]
pub enum DomDeserializeError<E> {
    /// Parser error.
    Parser(E),
    /// Reflection error.
    Reflect(facet_reflect::ReflectError),
    /// Unexpected end of input.
    UnexpectedEof {
        /// What was expected.
        expected: &'static str,
    },
    /// Type mismatch.
    TypeMismatch {
        /// What was expected.
        expected: &'static str,
        /// What was found.
        got: String,
    },
    /// Unknown element.
    UnknownElement {
        /// The element tag name.
        tag: String,
    },
    /// Missing required attribute.
    MissingAttribute {
        /// The attribute name.
        name: &'static str,
    },
    /// Unsupported type.
    Unsupported(String),
}

impl<E: std::error::Error> fmt::Display for DomDeserializeError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parser(e) => write!(f, "parser error: {e}"),
            Self::Reflect(e) => write!(f, "reflection error: {e}"),
            Self::UnexpectedEof { expected } => write!(f, "unexpected EOF, expected {expected}"),
            Self::TypeMismatch { expected, got } => {
                write!(f, "type mismatch: expected {expected}, got {got}")
            }
            Self::UnknownElement { tag } => write!(f, "unknown element: <{tag}>"),
            Self::MissingAttribute { name } => write!(f, "missing required attribute: {name}"),
            Self::Unsupported(msg) => write!(f, "unsupported: {msg}"),
        }
    }
}

impl<E: std::error::Error + 'static> std::error::Error for DomDeserializeError<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Parser(e) => Some(e),
            Self::Reflect(e) => Some(e),
            _ => None,
        }
    }
}

/// DOM deserializer.
///
/// This deserializer understands tree-structured documents and maps them to
/// Rust types using facet's reflection system.
///
/// The `BORROW` parameter controls whether strings can be borrowed from the input:
/// - `BORROW = true`: Allows zero-copy deserialization of `&str` and `Cow<str>`
/// - `BORROW = false`: All strings are owned, input doesn't need to outlive result
pub struct DomDeserializer<'de, const BORROW: bool, P> {
    parser: P,
    _marker: std::marker::PhantomData<&'de ()>,
}

impl<'de, P> DomDeserializer<'de, true, P>
where
    P: DomParser<'de>,
{
    /// Create a new DOM deserializer that can borrow strings from input.
    pub fn new(parser: P) -> Self {
        Self {
            parser,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<'de, P> DomDeserializer<'de, false, P>
where
    P: DomParser<'de>,
{
    /// Create a new DOM deserializer that produces owned strings.
    pub fn new_owned(parser: P) -> Self {
        Self {
            parser,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<'de, P> DomDeserializer<'de, true, P>
where
    P: DomParser<'de>,
{
    /// Deserialize a value of type `T`, allowing borrowed strings from input.
    pub fn deserialize<T>(&mut self) -> Result<T, DomDeserializeError<P::Error>>
    where
        T: Facet<'de>,
    {
        let wip: Partial<'de, true> =
            Partial::alloc::<T>().map_err(DomDeserializeError::Reflect)?;
        let partial = self.deserialize_into(wip)?;
        let heap_value: HeapValue<'de, true> =
            partial.build().map_err(DomDeserializeError::Reflect)?;
        heap_value
            .materialize::<T>()
            .map_err(DomDeserializeError::Reflect)
    }
}

impl<'de, P> DomDeserializer<'de, false, P>
where
    P: DomParser<'de>,
{
    /// Deserialize a value of type `T` into an owned type.
    pub fn deserialize<T>(&mut self) -> Result<T, DomDeserializeError<P::Error>>
    where
        T: Facet<'static>,
    {
        // SAFETY: When BORROW=false, no references into the input are stored.
        // The Partial only contains owned data (String, Vec, etc.), so the
        // lifetime parameter is purely phantom. We transmute from 'static to 'de
        // to satisfy the type system, but the actual data has no lifetime dependency.
        #[allow(unsafe_code)]
        let wip: Partial<'de, false> = unsafe {
            core::mem::transmute::<Partial<'static, false>, Partial<'de, false>>(
                Partial::alloc_owned::<T>().map_err(DomDeserializeError::Reflect)?,
            )
        };
        let partial = self.deserialize_into(wip)?;
        // SAFETY: Same reasoning - with BORROW=false, HeapValue contains only
        // owned data. The 'de lifetime is phantom and we can safely transmute
        // back to 'static since T: Facet<'static>.
        #[allow(unsafe_code)]
        let heap_value: HeapValue<'static, false> = unsafe {
            core::mem::transmute::<HeapValue<'de, false>, HeapValue<'static, false>>(
                partial.build().map_err(DomDeserializeError::Reflect)?,
            )
        };
        heap_value
            .materialize::<T>()
            .map_err(DomDeserializeError::Reflect)
    }
}

impl<'de, const BORROW: bool, P> DomDeserializer<'de, BORROW, P>
where
    P: DomParser<'de>,
{
    /// Deserialize into an existing Partial.
    pub fn deserialize_into(
        &mut self,
        wip: Partial<'de, BORROW>,
    ) -> Result<Partial<'de, BORROW>, DomDeserializeError<P::Error>> {
        let shape = wip.shape();
        trace!(type_id = %shape.type_identifier, def = ?shape.def, "deserialize_into");

        // Dispatch based on type
        match &shape.ty {
            Type::User(UserType::Struct(_)) => self.deserialize_struct(wip),
            Type::User(UserType::Enum(_)) => self.deserialize_enum(wip),
            _ => match &shape.def {
                Def::Scalar => self.deserialize_scalar(wip),
                Def::List(_) => self.deserialize_list(wip),
                Def::Option(_) => self.deserialize_option(wip),
                _ => Err(DomDeserializeError::Unsupported(format!(
                    "unsupported type: {:?}",
                    shape.ty
                ))),
            },
        }
    }

    /// Get the next event, returning an error if EOF.
    fn next_event(
        &mut self,
        expected: &'static str,
    ) -> Result<DomEvent<'de>, DomDeserializeError<P::Error>> {
        let event = self
            .parser
            .next_event()
            .map_err(DomDeserializeError::Parser)?
            .ok_or(DomDeserializeError::UnexpectedEof { expected })?;
        trace!(expected, event = ?event, "next_event");
        Ok(event)
    }

    /// Peek at the next event.
    fn peek_event(
        &mut self,
        expected: &'static str,
    ) -> Result<&DomEvent<'de>, DomDeserializeError<P::Error>> {
        let event = self
            .parser
            .peek_event()
            .map_err(DomDeserializeError::Parser)?
            .ok_or(DomDeserializeError::UnexpectedEof { expected })?;
        trace!(expected, event = ?event, "peek_event");
        Ok(event)
    }

    /// Expect and consume a NodeStart event, returning the tag name.
    fn expect_node_start(&mut self) -> Result<Cow<'de, str>, DomDeserializeError<P::Error>> {
        match self.next_event("NodeStart")? {
            DomEvent::NodeStart { tag, .. } => Ok(tag),
            other => Err(DomDeserializeError::TypeMismatch {
                expected: "NodeStart",
                got: format!("{other:?}"),
            }),
        }
    }

    /// Expect and consume a ChildrenStart event.
    fn expect_children_start(&mut self) -> Result<(), DomDeserializeError<P::Error>> {
        match self.next_event("ChildrenStart")? {
            DomEvent::ChildrenStart => Ok(()),
            other => Err(DomDeserializeError::TypeMismatch {
                expected: "ChildrenStart",
                got: format!("{other:?}"),
            }),
        }
    }

    /// Expect and consume a ChildrenEnd event.
    fn expect_children_end(&mut self) -> Result<(), DomDeserializeError<P::Error>> {
        match self.next_event("ChildrenEnd")? {
            DomEvent::ChildrenEnd => Ok(()),
            other => Err(DomDeserializeError::TypeMismatch {
                expected: "ChildrenEnd",
                got: format!("{other:?}"),
            }),
        }
    }

    /// Expect and consume a NodeEnd event.
    fn expect_node_end(&mut self) -> Result<(), DomDeserializeError<P::Error>> {
        match self.next_event("NodeEnd")? {
            DomEvent::NodeEnd => Ok(()),
            other => Err(DomDeserializeError::TypeMismatch {
                expected: "NodeEnd",
                got: format!("{other:?}"),
            }),
        }
    }

    /// Expect and consume a Text event, returning the text content.
    fn expect_text(&mut self) -> Result<Cow<'de, str>, DomDeserializeError<P::Error>> {
        match self.next_event("Text")? {
            DomEvent::Text(text) => Ok(text),
            other => Err(DomDeserializeError::TypeMismatch {
                expected: "Text",
                got: format!("{other:?}"),
            }),
        }
    }

    /// Expect and consume an Attribute event, returning (name, value).
    fn expect_attribute(
        &mut self,
    ) -> Result<(Cow<'de, str>, Cow<'de, str>), DomDeserializeError<P::Error>> {
        match self.next_event("Attribute")? {
            DomEvent::Attribute { name, value, .. } => Ok((name, value)),
            other => Err(DomDeserializeError::TypeMismatch {
                expected: "Attribute",
                got: format!("{other:?}"),
            }),
        }
    }

    /// Expect and consume a Comment event, returning the comment text.
    fn expect_comment(&mut self) -> Result<Cow<'de, str>, DomDeserializeError<P::Error>> {
        match self.next_event("Comment")? {
            DomEvent::Comment(text) => Ok(text),
            other => Err(DomDeserializeError::TypeMismatch {
                expected: "Comment",
                got: format!("{other:?}"),
            }),
        }
    }

    /// Deserialize a struct from an element.
    fn deserialize_struct(
        &mut self,
        mut wip: Partial<'de, BORROW>,
    ) -> Result<Partial<'de, BORROW>, DomDeserializeError<P::Error>> {
        let struct_def = match &wip.shape().ty {
            Type::User(UserType::Struct(def)) => def,
            _ => {
                return Err(DomDeserializeError::Unsupported(
                    "expected struct type".into(),
                ));
            }
        };

        trace_span!("deserialize_struct");

        // Build field map once upfront
        let field_map = StructFieldMap::new(struct_def);

        // Expect NodeStart
        let tag = self.expect_node_start()?;
        trace!(tag = %tag, "got NodeStart");

        // Process attributes
        trace!("processing attributes");
        loop {
            let event = self.peek_event("Attribute or ChildrenStart")?;
            match event {
                DomEvent::Attribute { .. } => {
                    let (name, value) = self.expect_attribute()?;
                    trace!(name = %name, value = %value, "got Attribute");
                    if let Some(info) = field_map.find_attribute(&name) {
                        trace!(idx = info.idx, field_name = %info.field.name, "matched attribute field");
                        wip = wip
                            .begin_nth_field(info.idx)
                            .map_err(DomDeserializeError::Reflect)?;
                        wip = self.set_string_value(wip, value)?;
                        wip = wip.end().map_err(DomDeserializeError::Reflect)?;
                    } else {
                        trace!(name = %name, "ignoring unknown attribute");
                    }
                }
                DomEvent::ChildrenStart => {
                    trace!("attributes done, starting children");
                    break;
                }
                DomEvent::NodeEnd => {
                    trace!("void element (no children)");
                    self.expect_node_end()?;
                    return Ok(wip);
                }
                other => {
                    return Err(DomDeserializeError::TypeMismatch {
                        expected: "Attribute or ChildrenStart",
                        got: format!("{other:?}"),
                    });
                }
            }
        }

        // Consume ChildrenStart
        self.expect_children_start()?;

        // Track text content for text field
        let mut text_content = String::new();

        // Track which list fields have been started (field index -> true if started)
        let mut started_lists: HashMap<usize, bool> = HashMap::new();

        // If there's an elements field, begin the list before processing children
        let mut elements_list_started = false;
        if let Some(info) = field_map.elements_field {
            trace!(idx = info.idx, field_name = %info.field.name, "beginning elements list");
            wip = wip
                .begin_nth_field(info.idx)
                .map_err(DomDeserializeError::Reflect)?;
            wip = wip.begin_list().map_err(DomDeserializeError::Reflect)?;
            elements_list_started = true;
        }

        // Process children
        trace!("processing children");
        loop {
            let event = self.peek_event("child or ChildrenEnd")?;
            match event {
                DomEvent::ChildrenEnd => {
                    trace!("children done");
                    break;
                }
                DomEvent::Text(_) => {
                    let text = self.expect_text()?;
                    trace!(text_len = text.len(), "got Text");
                    if elements_list_started {
                        trace!("adding text as list item (mixed content)");
                        wip = wip
                            .begin_list_item()
                            .map_err(DomDeserializeError::Reflect)?;
                        wip = self.deserialize_text_into_enum(wip, text)?;
                        wip = wip.end().map_err(DomDeserializeError::Reflect)?;
                    } else if field_map.text_field.is_some() {
                        trace!("accumulating text for text field");
                        text_content.push_str(&text);
                    } else {
                        trace!("ignoring text (no text field)");
                    }
                }
                DomEvent::NodeStart { tag, .. } => {
                    let tag = tag.clone();
                    trace!(tag = %tag, "got child NodeStart");

                    if !elements_list_started {
                        // Check if this matches an individual element field
                        if let Some(info) = field_map.find_element(&tag) {
                            if info.is_list {
                                // List field: add item to the list
                                trace!(idx = info.idx, field_name = %info.field.name, "matched list element field");

                                // Start the list if not already started
                                if !started_lists.get(&info.idx).copied().unwrap_or(false) {
                                    trace!(path = %wip.path(), "begin_nth_field for list");
                                    wip = wip
                                        .begin_nth_field(info.idx)
                                        .map_err(DomDeserializeError::Reflect)?;
                                    trace!(path = %wip.path(), "begin_list");
                                    wip = wip.begin_list().map_err(DomDeserializeError::Reflect)?;
                                    started_lists.insert(info.idx, true);
                                }

                                // Add item to the list - deserialize the element content as the item type
                                trace!(path = %wip.path(), "begin_list_item");
                                wip = wip
                                    .begin_list_item()
                                    .map_err(DomDeserializeError::Reflect)?;
                                trace!(path = %wip.path(), "after begin_list_item, before deserialize_into");
                                wip = self.deserialize_into(wip)?;
                                trace!(path = %wip.path(), "after deserialize_into, before end");
                                wip = wip.end().map_err(DomDeserializeError::Reflect)?;
                                trace!(path = %wip.path(), "after end (list item)");
                            } else {
                                // Non-list field: deserialize directly
                                trace!(idx = info.idx, field_name = %info.field.name, "matched scalar element field");
                                wip = wip
                                    .begin_nth_field(info.idx)
                                    .map_err(DomDeserializeError::Reflect)?;
                                wip = self.deserialize_into(wip)?;
                                wip = wip.end().map_err(DomDeserializeError::Reflect)?;
                            }
                        } else {
                            trace!(tag = %tag, "skipping unknown element");
                            self.parser
                                .skip_node()
                                .map_err(DomDeserializeError::Parser)?;
                        }
                    } else {
                        trace!("adding element to elements collection");
                        wip = wip
                            .begin_list_item()
                            .map_err(DomDeserializeError::Reflect)?;
                        wip = self.deserialize_into(wip)?;
                        wip = wip.end().map_err(DomDeserializeError::Reflect)?;
                    }
                }
                DomEvent::Comment(_) => {
                    trace!("skipping comment");
                    let _comment = self.expect_comment()?;
                }
                other => {
                    return Err(DomDeserializeError::TypeMismatch {
                        expected: "child content",
                        got: format!("{other:?}"),
                    });
                }
            }
        }

        // End all started list fields
        // Note: begin_nth_field pushes a frame, begin_list() just initializes the list on that frame
        // So we only need one end() to pop the field frame
        for (idx, started) in &started_lists {
            if *started {
                trace!(idx, path = %wip.path(), "ending list field");
                wip = wip.end().map_err(DomDeserializeError::Reflect)?;
                trace!(idx, path = %wip.path(), "after ending list field");
            }
        }

        // End the elements list if it was started
        // Same as above: begin_nth_field + begin_list = one frame, so one end()
        if elements_list_started {
            trace!(path = %wip.path(), "ending elements list");
            wip = wip.end().map_err(DomDeserializeError::Reflect)?;
        }

        // Set the text field if we accumulated text
        if let Some(info) = field_map.text_field {
            if !text_content.is_empty() || !elements_list_started {
                trace!(idx = info.idx, field_name = %info.field.name, text_len = text_content.len(), "setting text field");
                wip = wip
                    .begin_nth_field(info.idx)
                    .map_err(DomDeserializeError::Reflect)?;
                wip = self.set_string_value(wip, Cow::Owned(text_content))?;
                wip = wip.end().map_err(DomDeserializeError::Reflect)?;
            }
        }

        // Consume ChildrenEnd
        self.expect_children_end()?;

        // Consume NodeEnd
        self.expect_node_end()?;

        trace!(tag = %tag, "struct deserialization complete");
        Ok(wip)
    }

    /// Deserialize an enum (for mixed content).
    fn deserialize_enum(
        &mut self,
        mut wip: Partial<'de, BORROW>,
    ) -> Result<Partial<'de, BORROW>, DomDeserializeError<P::Error>> {
        let event = self.peek_event("NodeStart or Text")?;

        match event {
            DomEvent::NodeStart { tag, .. } => {
                let tag = tag.clone();
                // Find variant matching the tag
                let enum_def = match &wip.shape().ty {
                    Type::User(UserType::Enum(def)) => def,
                    _ => {
                        return Err(DomDeserializeError::Unsupported(
                            "expected enum type".into(),
                        ));
                    }
                };

                // Find matching variant
                let variant_idx = enum_def
                    .variants
                    .iter()
                    .position(|v| {
                        let variant_name = v
                            .get_builtin_attr("rename")
                            .and_then(|a| a.get_as::<&str>().copied())
                            .unwrap_or(v.name);
                        variant_name.eq_ignore_ascii_case(&tag)
                    })
                    .or_else(|| {
                        // Check for custom_element fallback
                        enum_def.variants.iter().position(|v| v.is_custom_element())
                    })
                    .ok_or_else(|| DomDeserializeError::UnknownElement {
                        tag: tag.to_string(),
                    })?;

                wip = wip
                    .select_nth_variant(variant_idx)
                    .map_err(DomDeserializeError::Reflect)?;
                wip = self.deserialize_into(wip)?;
            }
            DomEvent::Text(_) => {
                // Text variant
                let text = self.expect_text()?;
                wip = self.deserialize_text_into_enum(wip, text)?;
            }
            other => {
                return Err(DomDeserializeError::TypeMismatch {
                    expected: "NodeStart or Text",
                    got: format!("{other:?}"),
                });
            }
        }

        Ok(wip)
    }

    /// Deserialize text content into an enum's Text variant.
    fn deserialize_text_into_enum(
        &mut self,
        mut wip: Partial<'de, BORROW>,
        text: Cow<'de, str>,
    ) -> Result<Partial<'de, BORROW>, DomDeserializeError<P::Error>> {
        let enum_def = match &wip.shape().ty {
            Type::User(UserType::Enum(def)) => def,
            _ => {
                // Not an enum - try setting as string directly
                return self.set_string_value(wip, text);
            }
        };

        // Find the Text variant
        let text_variant_idx = enum_def
            .variants
            .iter()
            .position(|v| v.is_text())
            .ok_or_else(|| {
                DomDeserializeError::Unsupported("enum has no Text variant for text content".into())
            })?;

        wip = wip
            .select_nth_variant(text_variant_idx)
            .map_err(DomDeserializeError::Reflect)?;
        wip = self.set_string_value(wip, text)?;

        Ok(wip)
    }

    /// Deserialize a scalar value.
    fn deserialize_scalar(
        &mut self,
        wip: Partial<'de, BORROW>,
    ) -> Result<Partial<'de, BORROW>, DomDeserializeError<P::Error>> {
        trace!("deserialize_scalar called");
        // For scalars in DOM context, we might get:
        // 1. Raw text (e.g., inside a parent element's text content)
        // 2. Text wrapped in an element (e.g., <name>facet</name>)
        let event = self.peek_event("Text or NodeStart")?;
        trace!(event = ?event, "peeked event in deserialize_scalar");
        match event {
            DomEvent::Text(_) => {
                trace!("deserialize_scalar: matched Text arm");
                let text = self.expect_text()?;
                self.set_string_value(wip, text)
            }
            DomEvent::NodeStart { .. } => {
                trace!("deserialize_scalar: matched NodeStart arm");
                let tag = self.expect_node_start()?;
                trace!(tag = %tag, "deserialize_scalar: consumed NodeStart");

                // Skip attributes if any, then consume ChildrenStart
                loop {
                    let event = self.peek_event("Attribute or ChildrenStart or NodeEnd")?;
                    trace!(event = ?event, "deserialize_scalar: in attr loop");
                    match event {
                        DomEvent::Attribute { .. } => {
                            let (name, _value) = self.expect_attribute()?;
                            trace!(name = %name, "deserialize_scalar: consumed Attribute");
                        }
                        DomEvent::ChildrenStart => {
                            self.expect_children_start()?;
                            trace!("deserialize_scalar: consumed ChildrenStart");
                            break;
                        }
                        DomEvent::NodeEnd => {
                            // Void element with no content - empty string
                            self.expect_node_end()?;
                            trace!("deserialize_scalar: void element, returning empty string");
                            return self.set_string_value(wip, Cow::Borrowed(""));
                        }
                        other => {
                            trace!(other = ?other, "deserialize_scalar: unexpected event in attr loop");
                            return Err(DomDeserializeError::TypeMismatch {
                                expected: "Attribute or ChildrenStart or NodeEnd",
                                got: format!("{other:?}"),
                            });
                        }
                    }
                }

                // Now read text content (might be empty or have nested elements we ignore)
                trace!("deserialize_scalar: starting text content loop");
                let mut text_content = String::new();
                loop {
                    let event = self.peek_event("Text or ChildrenEnd")?;
                    trace!(event = ?event, "deserialize_scalar: in text content loop");
                    match event {
                        DomEvent::Text(_) => {
                            let text = self.expect_text()?;
                            trace!(text = %text, "deserialize_scalar: got text");
                            text_content.push_str(&text);
                        }
                        DomEvent::ChildrenEnd => {
                            trace!("deserialize_scalar: got ChildrenEnd, breaking text loop");
                            break;
                        }
                        DomEvent::NodeStart { .. } => {
                            // Skip nested elements
                            trace!("deserialize_scalar: skipping nested NodeStart");
                            self.parser
                                .skip_node()
                                .map_err(DomDeserializeError::Parser)?;
                        }
                        DomEvent::Comment(_) => {
                            let _comment = self.expect_comment()?;
                        }
                        other => {
                            return Err(DomDeserializeError::TypeMismatch {
                                expected: "Text or ChildrenEnd",
                                got: format!("{other:?}"),
                            });
                        }
                    }
                }

                // Consume ChildrenEnd and NodeEnd
                trace!("deserialize_scalar: consuming ChildrenEnd");
                self.expect_children_end()?;
                trace!("deserialize_scalar: consuming NodeEnd");
                self.expect_node_end()?;
                trace!(text_content = %text_content, "deserialize_scalar: setting string value");

                self.set_string_value(wip, Cow::Owned(text_content))
            }
            other => Err(DomDeserializeError::TypeMismatch {
                expected: "Text or NodeStart",
                got: format!("{other:?}"),
            }),
        }
    }

    /// Deserialize a list.
    fn deserialize_list(
        &mut self,
        mut wip: Partial<'de, BORROW>,
    ) -> Result<Partial<'de, BORROW>, DomDeserializeError<P::Error>> {
        wip = wip.begin_list().map_err(DomDeserializeError::Reflect)?;

        // Process children until ChildrenEnd
        loop {
            let event = self.peek_event("child or ChildrenEnd")?;
            if matches!(event, DomEvent::ChildrenEnd) {
                break;
            }

            wip = wip
                .begin_list_item()
                .map_err(DomDeserializeError::Reflect)?;
            wip = self.deserialize_into(wip)?;
            wip = wip.end().map_err(DomDeserializeError::Reflect)?;
        }

        Ok(wip)
    }

    /// Deserialize an Option.
    fn deserialize_option(
        &mut self,
        mut wip: Partial<'de, BORROW>,
    ) -> Result<Partial<'de, BORROW>, DomDeserializeError<P::Error>> {
        // In DOM context, presence of content means Some
        let event = self.peek_event("value")?;
        if matches!(event, DomEvent::ChildrenEnd | DomEvent::NodeEnd) {
            // No content - None
            wip = wip.set_default().map_err(DomDeserializeError::Reflect)?;
        } else {
            // Has content - Some
            wip = wip.begin_some().map_err(DomDeserializeError::Reflect)?;
            wip = self.deserialize_into(wip)?;
            wip = wip.end().map_err(DomDeserializeError::Reflect)?;
        }
        Ok(wip)
    }

    /// Set a string value, handling type conversion.
    ///
    /// This is "stringly typed" - the value came from XML text content and needs
    /// to be parsed into the appropriate type (u32, bool, etc.) if possible.
    fn set_string_value(
        &mut self,
        mut wip: Partial<'de, BORROW>,
        value: Cow<'de, str>,
    ) -> Result<Partial<'de, BORROW>, DomDeserializeError<P::Error>> {
        let shape = wip.shape();

        // Handle Option<T>
        if matches!(&shape.def, Def::Option(_)) {
            wip = wip.begin_some().map_err(DomDeserializeError::Reflect)?;
            wip = self.set_string_value(wip, value)?;
            wip = wip.end().map_err(DomDeserializeError::Reflect)?;
            return Ok(wip);
        }

        // For types that support parsing (numbers, bools, etc.), use parse_from_str
        if shape.vtable.has_parse() {
            wip = wip
                .parse_from_str(value.as_ref())
                .map_err(DomDeserializeError::Reflect)?;
            return Ok(wip);
        }

        // Use facet-dessert for string types (String, &str, Cow<str>)
        facet_dessert::set_string_value(wip, value, self.parser.current_span()).map_err(|e| match e
        {
            facet_dessert::DessertError::Reflect { error, .. } => {
                DomDeserializeError::Reflect(error)
            }
            facet_dessert::DessertError::CannotBorrow { message } => {
                DomDeserializeError::Unsupported(message.into_owned())
            }
        })
    }
}
