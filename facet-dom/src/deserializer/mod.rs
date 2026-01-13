//! Tree-based deserializer for DOM documents.

use std::borrow::Cow;

use facet_core::{Def, StructKind, Type, UserType};
use facet_reflect::Partial;

use crate::error::DomDeserializeError;
use crate::tracing_macros::{trace, trace_span};
use crate::{AttributeRecord, DomEvent, DomParser, DomParserExt};

mod entrypoints;
mod field_map;

use field_map::StructFieldMap;

/// Extension trait for chaining deserialization on `Partial`.
trait PartialDeserializeExt<'de, const BORROW: bool, P: DomParser<'de>> {
    /// Deserialize into this partial using the given deserializer.
    fn deserialize_with(
        self,
        deserializer: &mut DomDeserializer<'de, BORROW, P>,
    ) -> Result<Partial<'de, BORROW>, DomDeserializeError<P::Error>>;
}

impl<'de, const BORROW: bool, P: DomParser<'de>> PartialDeserializeExt<'de, BORROW, P>
    for Partial<'de, BORROW>
{
    fn deserialize_with(
        self,
        deserializer: &mut DomDeserializer<'de, BORROW, P>,
    ) -> Result<Partial<'de, BORROW>, DomDeserializeError<P::Error>> {
        deserializer.deserialize_into(self)
    }
}

/// DOM deserializer.
///
/// The `BORROW` parameter controls whether strings can be borrowed from the input:
/// - `BORROW = true`: Allows zero-copy deserialization of `&str` and `Cow<str>`
/// - `BORROW = false`: All strings are owned, input doesn't need to outlive result
pub struct DomDeserializer<'de, const BORROW: bool, P> {
    parser: P,
    _marker: std::marker::PhantomData<&'de ()>,
}

impl<'de, const BORROW: bool, P> DomDeserializer<'de, BORROW, P>
where
    P: DomParser<'de>,
{
    /// Deserialize a value into an existing Partial.
    ///
    /// # Parser State Contract
    ///
    /// **Entry:** The parser should be positioned such that the next event represents
    /// the value to deserialize. For structs/enums, this means a `NodeStart` is next
    /// (peeked but not consumed). For scalars within an element, the parser should be
    /// inside the element (after `ChildrenStart`).
    ///
    /// **Exit:** The parser will have consumed all events related to this value,
    /// including the closing `NodeEnd` for struct types.
    pub fn deserialize_into(
        &mut self,
        wip: Partial<'de, BORROW>,
    ) -> Result<Partial<'de, BORROW>, DomDeserializeError<P::Error>> {
        let shape = wip.shape();
        trace!(type_id = %shape.type_identifier, def = ?shape.def, "deserialize_into");

        match &shape.ty {
            Type::User(UserType::Struct(_)) => self.deserialize_struct(wip),
            Type::User(UserType::Enum(_)) => self.deserialize_enum(wip),
            _ => match &shape.def {
                Def::Scalar => self.deserialize_scalar(wip),
                Def::Pointer(_) => self.deserialize_pointer(wip),
                Def::List(_) => self.deserialize_list(wip),
                Def::Set(_) => self.deserialize_set(wip),
                Def::Option(_) => self.deserialize_option(wip),
                _ => Err(DomDeserializeError::Unsupported(format!(
                    "unsupported type: {:?}",
                    shape.ty
                ))),
            },
        }
    }

    /// Deserialize a struct type.
    ///
    /// # Parser State Contract
    ///
    /// **Entry:** Parser is positioned before the struct's `NodeStart` (peeked, not consumed).
    ///
    /// **Exit:** Parser has consumed through the struct's closing `NodeEnd`.
    fn deserialize_struct(
        &mut self,
        wip: Partial<'de, BORROW>,
    ) -> Result<Partial<'de, BORROW>, DomDeserializeError<P::Error>> {
        let struct_def = match &wip.shape().ty {
            Type::User(UserType::Struct(def)) => def,
            _ => {
                return Err(DomDeserializeError::Unsupported(
                    "expected struct type".into(),
                ));
            }
        };

        self.deserialize_struct_innards(wip, struct_def)
    }

    /// Deserialize the innards of a struct-like thing (struct, tuple, or enum variant data).
    ///
    /// # Parser State Contract
    ///
    /// **Entry:** Parser is AFTER the struct's `NodeStart` — the NodeStart has already been consumed.
    ///
    /// **Exit:** Parser has consumed through the struct's closing `NodeEnd`.
    ///
    /// # Processing Order
    ///
    /// 1. Process attributes (match to fields marked with `xml::attribute`)
    /// 2. Consume `ChildrenStart`
    /// 3. Process child elements and text (match to fields by element name or `xml::text`)
    /// 4. Consume `ChildrenEnd`
    /// 5. Consume `NodeEnd`
    fn deserialize_struct_innards(
        &mut self,
        mut wip: Partial<'de, BORROW>,
        struct_def: &'static facet_core::StructType,
    ) -> Result<Partial<'de, BORROW>, DomDeserializeError<P::Error>> {
        trace_span!("deserialize_struct_innards");

        let field_map = StructFieldMap::new(struct_def);

        let _tag = self.parser.expect_node_start()?;
        trace!(tag = %_tag, "got NodeStart");

        trace!("processing attributes");
        loop {
            let event = self
                .parser
                .peek_event_or_eof("Attribute or ChildrenStart")?;
            match event {
                DomEvent::Attribute { .. } => {
                    let AttributeRecord {
                        name,
                        value,
                        namespace,
                    } = self.parser.expect_attribute()?;
                    trace!(name = %name, value = %value, namespace = ?namespace, "got Attribute");
                    if let Some(info) =
                        field_map.find_attribute(&name, namespace.as_ref().map(|c| c.as_ref()))
                    {
                        trace!(idx = info.idx, field_name = %info.field.name, "matched attribute field");
                        wip = wip.begin_nth_field(info.idx)?;
                        wip = self.set_string_value(wip, value)?;
                        wip = wip.end()?;
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
                    self.parser.expect_node_end()?;
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

        self.parser.expect_children_start()?;

        let mut text_content = String::new();

        // Track which sequence fields have been started (for flat list/array/set deserialization)
        enum SeqState {
            List { is_smart_ptr: bool },
            Array { next_idx: usize },
            Set,
        }
        let mut started_seqs: std::collections::HashMap<usize, SeqState> =
            std::collections::HashMap::new();
        // Track which flat sequence is currently active (we're inside its list/array context)
        let mut active_seq_idx: Option<usize> = None;

        // Track whether we've started the xml::elements collection (lazy initialization)
        let mut elements_list_started = false;

        // For tuple structs: track position for positional matching
        let mut tuple_position: usize = 0;

        trace!("processing children");
        loop {
            let event = self.parser.peek_event_or_eof("child or ChildrenEnd")?;
            match event {
                DomEvent::ChildrenEnd => {
                    trace!("children done");
                    break;
                }
                DomEvent::Text(_) => {
                    let text = self.parser.expect_text()?;
                    trace!(text_len = text.len(), "got Text");
                    if elements_list_started {
                        trace!("adding text as list item (mixed content)");
                        wip = wip.begin_list_item()?;
                        wip = self.deserialize_text_into_enum(wip, text)?;
                        wip = wip.end()?;
                    } else if field_map.text_field.is_some() {
                        trace!("accumulating text for text field");
                        text_content.push_str(&text);
                    } else if struct_def.kind == StructKind::TupleStruct
                        && struct_def.fields.len() == 1
                    {
                        // Transparent newtype: text content goes to field 0
                        trace!("setting text content for newtype field 0");
                        wip = wip.begin_nth_field(0)?;
                        wip = self.set_string_value(wip, text)?;
                        wip = wip.end()?;
                    } else {
                        trace!("ignoring text (no text field)");
                    }
                }
                DomEvent::NodeStart { tag, namespace } => {
                    let tag = tag.clone();
                    let namespace = namespace.clone();
                    trace!(tag = %tag, namespace = ?namespace, "got child NodeStart");

                    // First, check if this element matches a specific xml::element field
                    // (this takes priority over xml::elements collection)
                    if let Some(info) =
                        field_map.find_element(&tag, namespace.as_ref().map(|c| c.as_ref()))
                    {
                        if info.is_list || info.is_array || info.is_set {
                            // Flat sequence: repeated elements directly as children
                            // If we're currently inside the elements list, end it first
                            if elements_list_started {
                                trace!("leaving elements list for flat sequence field");
                                wip = wip.end()?;
                                elements_list_started = false;
                            }
                            // If we're currently inside a different sequence, end it first
                            if let Some(prev_idx) = active_seq_idx
                                && prev_idx != info.idx
                            {
                                trace!(
                                    prev_idx = prev_idx,
                                    new_idx = info.idx,
                                    "switching active flat sequence"
                                );
                                // End the list/array (begin_nth_field doesn't push a frame,
                                // so ending the list/array returns us to the struct level)
                                wip = wip.end()?;
                                // End smart_ptr if applicable
                                if let Some(SeqState::List { is_smart_ptr: true }) =
                                    started_seqs.get(&prev_idx)
                                {
                                    wip = wip.end()?;
                                }
                                active_seq_idx = None;
                            }

                            use std::collections::hash_map::Entry;
                            let need_start =
                                matches!(started_seqs.entry(info.idx), Entry::Vacant(_));

                            if need_start {
                                trace!(idx = info.idx, field_name = %info.field.name, is_list = info.is_list, is_array = info.is_array, is_set = info.is_set, "starting flat sequence field");
                                wip = wip.begin_nth_field(info.idx)?;

                                if info.is_list {
                                    // Handle pointer-wrapped lists (Arc<[T]>, Box<[T]>, etc.)
                                    let is_smart_ptr =
                                        matches!(info.field.shape().def, Def::Pointer(_));
                                    if is_smart_ptr {
                                        wip = wip.begin_smart_ptr()?;
                                    }
                                    wip = wip.init_list()?;
                                    started_seqs.insert(info.idx, SeqState::List { is_smart_ptr });
                                } else if info.is_set {
                                    // Set (HashSet, BTreeSet, etc.)
                                    wip = wip.init_set()?;
                                    started_seqs.insert(info.idx, SeqState::Set);
                                } else {
                                    // Array
                                    wip = wip.init_array()?;
                                    started_seqs.insert(info.idx, SeqState::Array { next_idx: 0 });
                                }
                                active_seq_idx = Some(info.idx);
                            } else if active_seq_idx != Some(info.idx) {
                                // Sequence was started before but we left it; re-enter it
                                trace!(idx = info.idx, field_name = %info.field.name, "re-entering flat sequence field");
                                wip = wip.begin_nth_field(info.idx)?;
                                let state = started_seqs.get(&info.idx).unwrap();
                                if let SeqState::List { is_smart_ptr } = state {
                                    if *is_smart_ptr {
                                        wip = wip.begin_smart_ptr()?;
                                    }
                                    wip = wip.init_list()?;
                                } else if let SeqState::Set = state {
                                    wip = wip.init_set()?;
                                } else {
                                    wip = wip.init_array()?;
                                }
                                active_seq_idx = Some(info.idx);
                            }

                            // Add item to sequence
                            if info.is_list {
                                trace!(idx = info.idx, field_name = %info.field.name, "adding item to flat list");
                                wip = wip.begin_list_item()?.deserialize_with(self)?.end()?;
                            } else if info.is_set {
                                trace!(idx = info.idx, field_name = %info.field.name, "adding item to flat set");
                                wip = wip.begin_set_item()?.deserialize_with(self)?.end()?;
                            } else {
                                // Array: use begin_nth_field with the current index
                                let state = started_seqs.get_mut(&info.idx).unwrap();
                                if let SeqState::Array { next_idx } = state {
                                    trace!(idx = info.idx, field_name = %info.field.name, item_idx = *next_idx, "adding item to flat array");
                                    wip = wip
                                        .begin_nth_field(*next_idx)?
                                        .deserialize_with(self)?
                                        .end()?;
                                    *next_idx += 1;
                                }
                            }
                        } else {
                            // Scalar field - need to leave any active sequence first
                            if let Some(prev_idx) = active_seq_idx {
                                trace!(
                                    prev_idx = prev_idx,
                                    "leaving active flat sequence for scalar field"
                                );
                                // End the list/array
                                wip = wip.end()?;
                                // End smart_ptr if applicable
                                if let Some(SeqState::List { is_smart_ptr: true }) =
                                    started_seqs.get(&prev_idx)
                                {
                                    wip = wip.end()?;
                                }
                                active_seq_idx = None;
                            }
                            // Also need to leave the elements list if it's active
                            if elements_list_started {
                                trace!("leaving elements list for scalar field");
                                wip = wip.end()?;
                                elements_list_started = false;
                            }
                            trace!(idx = info.idx, field_name = %info.field.name, "matched scalar element field");
                            wip = wip
                                .begin_nth_field(info.idx)?
                                .deserialize_with(self)?
                                .end()?;
                        }
                    } else if field_map.is_tuple() && tag == "item" {
                        // Tuple struct: match <item> elements by position
                        if let Some(info) = field_map.get_tuple_field(tuple_position) {
                            trace!(
                                idx = info.idx,
                                position = tuple_position,
                                "matched tuple field by position"
                            );
                            wip = wip
                                .begin_nth_field(info.idx)?
                                .deserialize_with(self)?
                                .end()?;
                            tuple_position += 1;
                        } else {
                            trace!(
                                position = tuple_position,
                                "tuple position out of bounds, skipping"
                            );
                            self.parser
                                .skip_node()
                                .map_err(DomDeserializeError::Parser)?;
                        }
                    } else if field_map.elements_field.is_some() {
                        // No specific field matched, add to the elements collection
                        // Lazily start the elements list if not already started
                        if !elements_list_started {
                            let info = field_map.elements_field.as_ref().unwrap();
                            trace!(idx = info.idx, field_name = %info.field.name, "starting elements list (lazy)");
                            wip = wip.begin_nth_field(info.idx)?;
                            wip = wip.init_list()?;
                            elements_list_started = true;
                        }
                        trace!("adding element to elements collection");
                        wip = wip.begin_list_item()?.deserialize_with(self)?.end()?;
                    } else {
                        trace!(tag = %tag, "skipping unknown element");
                        self.parser
                            .skip_node()
                            .map_err(DomDeserializeError::Parser)?;
                    }
                }
                DomEvent::Comment(_) => {
                    trace!("skipping comment");
                    let _comment = self.parser.expect_comment()?;
                }
                other => {
                    return Err(DomDeserializeError::TypeMismatch {
                        expected: "child content",
                        got: format!("{other:?}"),
                    });
                }
            }
        }

        // Close the currently active flat sequence (if any)
        if let Some(idx) = active_seq_idx {
            let state = started_seqs.get(&idx).unwrap();
            match state {
                SeqState::List { is_smart_ptr } => {
                    trace!(path = %wip.path(), is_smart_ptr, "ending active flat list");
                    wip = wip.end()?; // end list
                    if *is_smart_ptr {
                        wip = wip.end()?; // end smart pointer
                    }
                    // Note: begin_nth_field doesn't push a frame, so no extra end() needed
                }
                SeqState::Array { .. } => {
                    trace!(path = %wip.path(), "ending active flat array");
                    wip = wip.end()?; // end array
                    // Note: begin_nth_field doesn't push a frame, so no extra end() needed
                }
                SeqState::Set => {
                    trace!(path = %wip.path(), "ending active flat set");
                    wip = wip.end()?; // end set
                    // Note: begin_nth_field doesn't push a frame, so no extra end() needed
                }
            }
        }

        if elements_list_started {
            trace!(path = %wip.path(), "ending elements list");
            wip = wip.end()?;
        } else if let Some(info) = &field_map.elements_field {
            // Elements list was never started (no unmatched elements) - initialize as empty
            trace!(idx = info.idx, field_name = %info.field.name, "initializing empty elements list");
            wip = wip.begin_nth_field(info.idx)?;
            wip = wip.init_list()?;
            wip = wip.end()?;
        }

        if let Some(info) = &field_map.text_field
            && (!text_content.is_empty() || !elements_list_started)
        {
            trace!(idx = info.idx, field_name = %info.field.name, text_len = text_content.len(), "setting text field");
            wip = wip.begin_nth_field(info.idx)?;
            wip = self.set_string_value(wip, Cow::Owned(text_content))?;
            wip = wip.end()?;
        }

        self.parser.expect_children_end()?;
        self.parser.expect_node_end()?;

        trace!(tag = %_tag, "struct deserialization complete");
        Ok(wip)
    }

    /// Deserialize an enum type.
    ///
    /// # Parser State Contract
    ///
    /// **Entry:** Parser is positioned at either:
    /// - A `NodeStart` event (element-based variant), or
    /// - A `Text` event (text-based variant, e.g., for enums with a `#[xml::text]` variant)
    ///
    /// **Exit:** All events for this enum have been consumed:
    /// - If entry was `NodeStart`: through the closing `NodeEnd`
    /// - If entry was `Text`: just that text event
    ///
    /// # Variant Selection
    ///
    /// For `NodeStart`: The element tag name is matched against variant names (considering
    /// `#[rename]` attributes). If no match, looks for a variant with `#[xml::custom_element]`.
    ///
    /// For `Text`: Looks for a variant with `#[xml::text]` attribute.
    fn deserialize_enum(
        &mut self,
        mut wip: Partial<'de, BORROW>,
    ) -> Result<Partial<'de, BORROW>, DomDeserializeError<P::Error>> {
        let event = self.parser.peek_event_or_eof("NodeStart or Text")?;

        match event {
            DomEvent::NodeStart { tag, .. } => {
                let tag = tag.clone();
                let enum_def = match &wip.shape().ty {
                    Type::User(UserType::Enum(def)) => def,
                    _ => {
                        return Err(DomDeserializeError::Unsupported(
                            "expected enum type".into(),
                        ));
                    }
                };

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
                    .or_else(|| enum_def.variants.iter().position(|v| v.is_custom_element()))
                    .ok_or_else(|| DomDeserializeError::UnknownElement {
                        tag: tag.to_string(),
                    })?;

                let variant = &enum_def.variants[variant_idx];
                wip = wip.select_nth_variant(variant_idx)?;
                trace!(variant_name = variant.name, variant_kind = ?variant.data.kind, "selected variant");

                // Handle variant based on its kind
                match variant.data.kind {
                    StructKind::Unit => {
                        // Unit variant: just consume the element
                        self.parser.expect_node_start()?;
                        // Skip to end of element
                        let event = self.parser.peek_event_or_eof("ChildrenStart or NodeEnd")?;
                        if matches!(event, DomEvent::ChildrenStart) {
                            self.parser.expect_children_start()?;
                            self.parser.expect_children_end()?;
                        }
                        self.parser.expect_node_end()?;
                    }
                    StructKind::TupleStruct => {
                        // Newtype variant: deserialize the inner type
                        // The variant data has one field (index 0)
                        wip = wip.begin_nth_field(0)?.deserialize_with(self)?.end()?;
                    }
                    StructKind::Struct | StructKind::Tuple => {
                        // Struct/tuple variant: deserialize using the variant's data as a StructType
                        wip = self.deserialize_struct_innards(wip, &variant.data)?;
                    }
                }
            }
            DomEvent::Text(_) => {
                let text = self.parser.expect_text()?;
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

    /// Deserialize text content into an enum by selecting the `#[xml::text]` variant.
    ///
    /// # Parser State Contract
    ///
    /// **Entry:** The text has already been consumed from the parser (passed as argument).
    ///
    /// **Exit:** No parser state change (text was already consumed).
    ///
    /// # Fallback
    ///
    /// If `wip` is not actually an enum, falls back to `set_string_value`.
    fn deserialize_text_into_enum(
        &mut self,
        mut wip: Partial<'de, BORROW>,
        text: Cow<'de, str>,
    ) -> Result<Partial<'de, BORROW>, DomDeserializeError<P::Error>> {
        let enum_def = match &wip.shape().ty {
            Type::User(UserType::Enum(def)) => def,
            _ => {
                return self.set_string_value(wip, text);
            }
        };

        let text_variant_idx = enum_def
            .variants
            .iter()
            .position(|v| v.is_text())
            .ok_or_else(|| {
                DomDeserializeError::Unsupported("enum has no Text variant for text content".into())
            })?;

        wip = wip.select_nth_variant(text_variant_idx)?;
        wip = self.set_string_value(wip, text)?;

        Ok(wip)
    }

    /// Deserialize a scalar value (string, number, bool, etc.).
    ///
    /// # Parser State Contract
    ///
    /// **Entry:** Parser is positioned at either:
    /// - A `Text` event (inline text content), or
    /// - A `NodeStart` event (element wrapping the text content)
    ///
    /// **Exit:** All events for this scalar have been consumed:
    /// - If entry was `Text`: just that text event
    /// - If entry was `NodeStart`: through the closing `NodeEnd`
    ///
    /// # XML Data Model
    ///
    /// In XML, scalars can appear as:
    /// - Attribute values (handled elsewhere)
    /// - Text content: `<parent>text here</parent>`
    /// - Element with text: `<field>value</field>` (element is consumed)
    fn deserialize_scalar(
        &mut self,
        wip: Partial<'de, BORROW>,
    ) -> Result<Partial<'de, BORROW>, DomDeserializeError<P::Error>> {
        trace!("deserialize_scalar called");
        let event = self.parser.peek_event_or_eof("Text or NodeStart")?;
        trace!(event = ?event, "peeked event in deserialize_scalar");
        match event {
            DomEvent::Text(_) => {
                trace!("deserialize_scalar: matched Text arm");
                let text = self.parser.expect_text()?;
                self.set_string_value(wip, text)
            }
            DomEvent::NodeStart { .. } => {
                trace!("deserialize_scalar: matched NodeStart arm");
                let _tag = self.parser.expect_node_start()?;
                trace!(tag = %_tag, "deserialize_scalar: consumed NodeStart");

                loop {
                    let event = self
                        .parser
                        .peek_event_or_eof("Attribute or ChildrenStart or NodeEnd")?;
                    trace!(event = ?event, "deserialize_scalar: in attr loop");
                    match event {
                        DomEvent::Attribute { .. } => {
                            let AttributeRecord {
                                name: _name,
                                value: _value,
                                namespace: _namespace,
                            } = self.parser.expect_attribute()?;
                            trace!(name = %_name, "deserialize_scalar: consumed Attribute");
                        }
                        DomEvent::ChildrenStart => {
                            self.parser.expect_children_start()?;
                            trace!("deserialize_scalar: consumed ChildrenStart");
                            break;
                        }
                        DomEvent::NodeEnd => {
                            self.parser.expect_node_end()?;
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

                trace!("deserialize_scalar: starting text content loop");
                let mut text_content = String::new();
                loop {
                    let event = self.parser.peek_event_or_eof("Text or ChildrenEnd")?;
                    trace!(event = ?event, "deserialize_scalar: in text content loop");
                    match event {
                        DomEvent::Text(_) => {
                            let text = self.parser.expect_text()?;
                            trace!(text = %text, "deserialize_scalar: got text");
                            text_content.push_str(&text);
                        }
                        DomEvent::ChildrenEnd => {
                            trace!("deserialize_scalar: got ChildrenEnd, breaking text loop");
                            break;
                        }
                        DomEvent::NodeStart { .. } => {
                            trace!("deserialize_scalar: skipping nested NodeStart");
                            self.parser
                                .skip_node()
                                .map_err(DomDeserializeError::Parser)?;
                        }
                        DomEvent::Comment(_) => {
                            let _comment = self.parser.expect_comment()?;
                        }
                        other => {
                            return Err(DomDeserializeError::TypeMismatch {
                                expected: "Text or ChildrenEnd",
                                got: format!("{other:?}"),
                            });
                        }
                    }
                }

                trace!("deserialize_scalar: consuming ChildrenEnd");
                self.parser.expect_children_end()?;
                trace!("deserialize_scalar: consuming NodeEnd");
                self.parser.expect_node_end()?;
                trace!(text_content = %text_content, "deserialize_scalar: setting string value");

                self.set_string_value(wip, Cow::Owned(text_content))
            }
            other => Err(DomDeserializeError::TypeMismatch {
                expected: "Text or NodeStart",
                got: format!("{other:?}"),
            }),
        }
    }

    /// Deserialize a list (Vec, slice, etc.) from repeated child elements.
    ///
    /// # Parser State Contract
    ///
    /// **Entry:** Parser is positioned inside an element, after `ChildrenStart`.
    /// Child elements will be deserialized as list items.
    ///
    /// **Exit:** Parser is positioned at `ChildrenEnd` (peeked, not consumed).
    /// The caller is responsible for consuming `ChildrenEnd` and `NodeEnd`.
    ///
    /// # Note
    ///
    /// This is used for "wrapped" list semantics where a parent element contains
    /// the list items. For "flat" list semantics (items directly as siblings),
    /// see the flat sequence handling in `deserialize_struct_innards`.
    fn deserialize_list(
        &mut self,
        mut wip: Partial<'de, BORROW>,
    ) -> Result<Partial<'de, BORROW>, DomDeserializeError<P::Error>> {
        wip = wip.init_list()?;

        loop {
            let event = self.parser.peek_event_or_eof("child or ChildrenEnd")?;
            if matches!(event, DomEvent::ChildrenEnd) {
                break;
            }

            wip = wip.begin_list_item()?.deserialize_with(self)?.end()?;
        }

        Ok(wip)
    }

    /// Deserialize a set type (HashSet, BTreeSet, etc.).
    ///
    /// Works the same as lists: each child element becomes a set item.
    fn deserialize_set(
        &mut self,
        mut wip: Partial<'de, BORROW>,
    ) -> Result<Partial<'de, BORROW>, DomDeserializeError<P::Error>> {
        wip = wip.init_set()?;

        loop {
            let event = self.parser.peek_event_or_eof("child or ChildrenEnd")?;
            if matches!(event, DomEvent::ChildrenEnd) {
                break;
            }

            wip = wip.begin_set_item()?.deserialize_with(self)?.end()?;
        }

        Ok(wip)
    }

    /// Deserialize an Option type.
    ///
    /// # Parser State Contract
    ///
    /// **Entry:** Parser is positioned where the optional value would be.
    ///
    /// **Exit:** If value was present, all events for the value have been consumed.
    /// If value was absent, no events consumed.
    ///
    /// # None Detection
    ///
    /// The option is `None` if the next event is `ChildrenEnd` or `NodeEnd`
    /// (indicating no content). Otherwise, the inner value is deserialized.
    fn deserialize_option(
        &mut self,
        mut wip: Partial<'de, BORROW>,
    ) -> Result<Partial<'de, BORROW>, DomDeserializeError<P::Error>> {
        let event = self.parser.peek_event_or_eof("value")?;
        if matches!(event, DomEvent::ChildrenEnd | DomEvent::NodeEnd) {
            wip = wip.set_default()?;
        } else {
            wip = wip.begin_some()?.deserialize_with(self)?.end()?;
        }
        Ok(wip)
    }

    /// Deserialize a pointer type (Box, Arc, Rc, etc.).
    ///
    /// # Parser State Contract
    ///
    /// **Entry:** Parser is positioned at the value that the pointer will wrap.
    ///
    /// **Exit:** All events for the inner value have been consumed.
    ///
    /// # Pointer Actions
    ///
    /// Uses `facet_dessert::begin_pointer` to determine how to handle the pointer:
    /// - `HandleAsScalar`: Treat as scalar (e.g., `Box<str>`)
    /// - `SliceBuilder`: Build a slice (e.g., `Arc<[T]>`)
    /// - `SizedPointee`: Regular pointer to sized type
    fn deserialize_pointer(
        &mut self,
        wip: Partial<'de, BORROW>,
    ) -> Result<Partial<'de, BORROW>, DomDeserializeError<P::Error>> {
        use facet_dessert::{PointerAction, begin_pointer};

        let (wip, action) = begin_pointer(wip)?;

        match action {
            PointerAction::HandleAsScalar => self.deserialize_scalar(wip),
            PointerAction::SliceBuilder => Ok(self.deserialize_list(wip)?.end()?),
            PointerAction::SizedPointee => Ok(wip.deserialize_with(self)?.end()?),
        }
    }

    /// Set a string value on the current partial, parsing it to the appropriate type.
    ///
    /// # Parser State Contract
    ///
    /// **Entry/Exit:** No parser state change. The string value is passed as an argument.
    ///
    /// # Type Handling
    ///
    /// Delegates to `facet_dessert::set_string_value` which handles parsing the string
    /// into the appropriate scalar type (String, &str, integers, floats, bools, etc.).
    fn set_string_value(
        &mut self,
        wip: Partial<'de, BORROW>,
        value: Cow<'de, str>,
    ) -> Result<Partial<'de, BORROW>, DomDeserializeError<P::Error>> {
        Ok(facet_dessert::set_string_value(
            wip,
            value,
            self.parser.current_span(),
        )?)
    }
}
