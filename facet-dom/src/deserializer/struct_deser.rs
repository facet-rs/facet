//! Struct deserialization logic extracted from the main deserializer.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::marker::PhantomData;

use facet_core::{Def, StructKind, StructType};
use facet_reflect::Partial;

use crate::error::DomDeserializeError;
use crate::tracing_macros::trace;
use crate::{AttributeRecord, DomEvent, DomParser, DomParserExt};

use super::PartialDeserializeExt;
use super::field_map::{FlattenedChildInfo, StructFieldMap};

/// State for a flat sequence field being deserialized.
pub(crate) enum SeqState {
    List { is_smart_ptr: bool },
    Array { next_idx: usize },
    Set,
}

/// Deserializer for struct types.
pub(crate) struct StructDeserializer<'de, 'p, const BORROW: bool, P: DomParser<'de>> {
    parser: &'p mut P,
    field_map: StructFieldMap,
    struct_def: &'static StructType,

    /// The partial being built - Option so we can take() it for chained operations
    wip: Option<Partial<'de, BORROW>>,

    /// Whether deferred mode is enabled (for flattened fields)
    using_deferred: bool,

    /// Accumulated text content for xml::text field
    text_content: String,

    /// Track which sequence fields have been started
    started_seqs: HashMap<usize, SeqState>,

    /// Currently active flat sequence
    active_seq_idx: Option<usize>,

    /// Whether we've started the xml::elements collection
    elements_list_started: bool,

    /// Which flattened maps have been initialized
    started_flattened_maps: HashSet<usize>,

    /// Position for tuple struct positional matching
    tuple_position: usize,

    /// Tag from NodeStart (for tracing)
    tag: String,

    _marker: PhantomData<&'de ()>,
}

impl<'de, 'p, const BORROW: bool, P: DomParser<'de>> StructDeserializer<'de, 'p, BORROW, P> {
    pub fn new(
        parser: &'p mut P,
        wip: Partial<'de, BORROW>,
        struct_def: &'static StructType,
        ns_all: Option<&'static str>,
    ) -> Self {
        let field_map = StructFieldMap::new(struct_def, ns_all);
        Self {
            parser,
            field_map,
            struct_def,
            wip: Some(wip),
            using_deferred: false,
            text_content: String::new(),
            started_seqs: HashMap::new(),
            active_seq_idx: None,
            elements_list_started: false,
            started_flattened_maps: HashSet::new(),
            tuple_position: 0,
            tag: String::new(),
            _marker: PhantomData,
        }
    }

    /// Apply a fallible operation to wip.
    fn with_wip<F>(&mut self, f: F) -> Result<(), DomDeserializeError<P::Error>>
    where
        F: FnOnce(
            Partial<'de, BORROW>,
        ) -> Result<Partial<'de, BORROW>, DomDeserializeError<P::Error>>,
    {
        let wip = self.wip.take().expect("wip already taken");
        self.wip = Some(f(wip)?);
        Ok(())
    }

    /// Take wip out (for final return).
    fn take_wip(&mut self) -> Partial<'de, BORROW> {
        self.wip.take().expect("wip already taken")
    }

    pub fn deserialize(
        mut self,
        dom_deser: &mut super::DomDeserializer<'de, BORROW, P>,
    ) -> Result<Partial<'de, BORROW>, DomDeserializeError<P::Error>> {
        if self.field_map.has_flatten {
            trace!("enabling deferred mode for struct with flatten");
            self.with_wip(|wip| Ok(wip.begin_deferred()?))?;
            self.using_deferred = true;
        }

        self.tag = self.parser.expect_node_start()?.to_string();
        trace!(tag = %self.tag, "got NodeStart");

        if let Some(wip) = self.process_attributes(dom_deser)? {
            return Ok(wip);
        }

        self.parser.expect_children_start()?;
        self.process_children(dom_deser)?;
        self.cleanup(dom_deser)?;
        self.parser.expect_children_end()?;
        self.parser.expect_node_end()?;

        let mut wip = self.take_wip();
        if self.using_deferred {
            trace!("finishing deferred mode for struct with flatten");
            wip = wip.finish_deferred()?;
        }

        trace!(tag = %self.tag, "struct deserialization complete");
        Ok(wip)
    }

    /// Process attributes. Returns Some(wip) for early return on void elements.
    fn process_attributes(
        &mut self,
        dom_deser: &mut super::DomDeserializer<'de, BORROW, P>,
    ) -> Result<Option<Partial<'de, BORROW>>, DomDeserializeError<P::Error>> {
        trace!("processing attributes");
        loop {
            match self
                .parser
                .peek_event_or_eof("Attribute or ChildrenStart")?
            {
                DomEvent::Attribute { .. } => {
                    let AttributeRecord {
                        name,
                        value,
                        namespace,
                    } = self.parser.expect_attribute()?;
                    trace!(name = %name, value = %value, namespace = ?namespace, "got Attribute");
                    if let Some(info) = self
                        .field_map
                        .find_attribute(&name, namespace.as_ref().map(|c| c.as_ref()))
                    {
                        let idx = info.idx;
                        trace!(idx, field_name = %info.field.name, "matched attribute field");
                        self.with_wip(|wip| {
                            Ok(dom_deser
                                .set_string_value(wip.begin_nth_field(idx)?, value)?
                                .end()?)
                        })?;
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
                    return Ok(Some(self.take_wip()));
                }
                other => {
                    return Err(DomDeserializeError::TypeMismatch {
                        expected: "Attribute or ChildrenStart",
                        got: format!("{other:?}"),
                    });
                }
            }
        }
        Ok(None)
    }

    fn process_children(
        &mut self,
        dom_deser: &mut super::DomDeserializer<'de, BORROW, P>,
    ) -> Result<(), DomDeserializeError<P::Error>> {
        trace!("processing children");
        loop {
            match self.parser.peek_event_or_eof("child or ChildrenEnd")? {
                DomEvent::ChildrenEnd => {
                    trace!("children done");
                    break;
                }
                DomEvent::Text(_) => self.handle_text(dom_deser)?,
                DomEvent::NodeStart { tag, namespace } => {
                    let tag = tag.clone();
                    let namespace = namespace.clone();
                    self.handle_child_element(&tag, namespace.as_deref(), dom_deser)?;
                }
                DomEvent::Comment(_) => {
                    trace!("skipping comment");
                    self.parser.expect_comment()?;
                }
                other => {
                    return Err(DomDeserializeError::TypeMismatch {
                        expected: "child content",
                        got: format!("{other:?}"),
                    });
                }
            }
        }
        Ok(())
    }

    fn handle_text(
        &mut self,
        dom_deser: &mut super::DomDeserializer<'de, BORROW, P>,
    ) -> Result<(), DomDeserializeError<P::Error>> {
        let text = self.parser.expect_text()?;
        trace!(text_len = text.len(), "got Text");

        if self.elements_list_started {
            trace!("adding text as list item (mixed content)");
            self.with_wip(|wip| {
                let wip = wip.begin_list_item()?;
                Ok(dom_deser.deserialize_text_into_enum(wip, text)?.end()?)
            })?;
        } else if self.field_map.text_field.is_some() {
            trace!("accumulating text for text field");
            self.text_content.push_str(&text);
        } else if self.struct_def.kind == StructKind::TupleStruct
            && self.struct_def.fields.len() == 1
        {
            trace!("setting text content for newtype field 0");
            self.with_wip(|wip| {
                Ok(dom_deser
                    .set_string_value(wip.begin_nth_field(0)?, text)?
                    .end()?)
            })?;
        } else {
            trace!("ignoring text (no text field)");
        }
        Ok(())
    }

    fn handle_child_element(
        &mut self,
        tag: &str,
        namespace: Option<&str>,
        dom_deser: &mut super::DomDeserializer<'de, BORROW, P>,
    ) -> Result<(), DomDeserializeError<P::Error>> {
        trace!(tag = %tag, namespace = ?namespace, "got child NodeStart");

        if let Some(info) = self.field_map.find_element(tag, namespace) {
            if info.is_list || info.is_array || info.is_set {
                self.handle_flat_sequence(
                    info.idx,
                    info.is_list,
                    info.is_array,
                    info.is_set,
                    info.field,
                    dom_deser,
                )
            } else {
                self.handle_scalar_element(info.idx, info.field.name, dom_deser)
            }
        } else if self.field_map.is_tuple() && tag == "item" {
            self.handle_tuple_item(dom_deser)
        } else if let Some(flattened) = self.field_map.find_flattened_child(tag, namespace).cloned()
        {
            self.handle_flattened_child(&flattened, dom_deser)
        } else if let Some(field_idx) = self.field_map.flattened_enum.as_ref().map(|e| e.field_idx)
        {
            self.handle_flattened_enum(field_idx, dom_deser)
        } else if self.field_map.elements_field.is_some() {
            self.handle_elements_collection(dom_deser)
        } else if !self.field_map.flattened_maps.is_empty() {
            self.handle_flattened_map(tag, namespace)
        } else {
            self.handle_unknown_element(tag)
        }
    }

    fn leave_active_sequence(&mut self) -> Result<(), DomDeserializeError<P::Error>> {
        if let Some(prev_idx) = self.active_seq_idx.take() {
            trace!(prev_idx, "leaving active flat sequence");
            let is_smart_ptr = matches!(
                self.started_seqs.get(&prev_idx),
                Some(SeqState::List { is_smart_ptr: true })
            );
            self.with_wip(|mut wip| {
                wip = wip.end()?;
                if is_smart_ptr {
                    wip = wip.end()?;
                }
                Ok(wip)
            })?;
        }
        if self.elements_list_started {
            trace!("leaving elements list");
            self.with_wip(|wip| Ok(wip.end()?))?;
            self.elements_list_started = false;
        }
        Ok(())
    }

    fn handle_flat_sequence(
        &mut self,
        idx: usize,
        is_list: bool,
        is_array: bool,
        is_set: bool,
        field: &'static facet_core::Field,
        dom_deser: &mut super::DomDeserializer<'de, BORROW, P>,
    ) -> Result<(), DomDeserializeError<P::Error>> {
        if self.elements_list_started {
            trace!("leaving elements list for flat sequence field");
            self.with_wip(|wip| Ok(wip.end()?))?;
            self.elements_list_started = false;
        }

        // Switch sequences if needed
        if let Some(prev_idx) = self.active_seq_idx {
            if prev_idx != idx {
                trace!(prev_idx, new_idx = idx, "switching active flat sequence");
                let is_smart_ptr = matches!(
                    self.started_seqs.get(&prev_idx),
                    Some(SeqState::List { is_smart_ptr: true })
                );
                self.with_wip(|mut wip| {
                    wip = wip.end()?;
                    if is_smart_ptr {
                        wip = wip.end()?;
                    }
                    Ok(wip)
                })?;
                self.active_seq_idx = None;
            }
        }

        use std::collections::hash_map::Entry;
        let need_start = matches!(self.started_seqs.entry(idx), Entry::Vacant(_));

        if need_start {
            trace!(idx, field_name = %field.name, "starting flat sequence field");
            if is_list {
                let is_smart_ptr = matches!(field.shape().def, Def::Pointer(_));
                self.with_wip(|mut wip| {
                    wip = wip.begin_nth_field(idx)?;
                    if is_smart_ptr {
                        wip = wip.begin_smart_ptr()?;
                    }
                    Ok(wip.init_list()?)
                })?;
                self.started_seqs
                    .insert(idx, SeqState::List { is_smart_ptr });
            } else if is_set {
                self.with_wip(|wip| Ok(wip.begin_nth_field(idx)?.init_set()?))?;
                self.started_seqs.insert(idx, SeqState::Set);
            } else {
                self.with_wip(|wip| Ok(wip.begin_nth_field(idx)?.init_array()?))?;
                self.started_seqs
                    .insert(idx, SeqState::Array { next_idx: 0 });
            }
            self.active_seq_idx = Some(idx);
        } else if self.active_seq_idx != Some(idx) {
            trace!(idx, field_name = %field.name, "re-entering flat sequence field");
            let state = self.started_seqs.get(&idx).unwrap();
            match state {
                SeqState::List { is_smart_ptr } => {
                    let is_smart_ptr = *is_smart_ptr;
                    self.with_wip(|mut wip| {
                        wip = wip.begin_nth_field(idx)?;
                        if is_smart_ptr {
                            wip = wip.begin_smart_ptr()?;
                        }
                        Ok(wip.init_list()?)
                    })?;
                }
                SeqState::Set => {
                    self.with_wip(|wip| Ok(wip.begin_nth_field(idx)?.init_set()?))?;
                }
                SeqState::Array { .. } => {
                    self.with_wip(|wip| Ok(wip.begin_nth_field(idx)?.init_array()?))?;
                }
            }
            self.active_seq_idx = Some(idx);
        }

        // Add item
        if is_list {
            trace!(idx, field_name = %field.name, "adding item to flat list");
            self.with_wip(|wip| Ok(wip.begin_list_item()?.deserialize_with(dom_deser)?.end()?))?;
        } else if is_set {
            trace!(idx, field_name = %field.name, "adding item to flat set");
            self.with_wip(|wip| Ok(wip.begin_set_item()?.deserialize_with(dom_deser)?.end()?))?;
        } else {
            // Get the current index first
            let item_idx = match self.started_seqs.get(&idx) {
                Some(SeqState::Array { next_idx }) => *next_idx,
                _ => return Ok(()),
            };
            trace!(idx, field_name = %field.name, item_idx, "adding item to flat array");
            self.with_wip(|wip| {
                Ok(wip
                    .begin_nth_field(item_idx)?
                    .deserialize_with(dom_deser)?
                    .end()?)
            })?;
            // Increment after
            if let Some(SeqState::Array { next_idx }) = self.started_seqs.get_mut(&idx) {
                *next_idx += 1;
            }
        }
        Ok(())
    }

    fn handle_scalar_element(
        &mut self,
        idx: usize,
        field_name: &str,
        dom_deser: &mut super::DomDeserializer<'de, BORROW, P>,
    ) -> Result<(), DomDeserializeError<P::Error>> {
        self.leave_active_sequence()?;
        trace!(idx, field_name, "matched scalar element field");
        self.with_wip(|wip| {
            Ok(wip
                .begin_nth_field(idx)?
                .deserialize_with(dom_deser)?
                .end()?)
        })
    }

    fn handle_tuple_item(
        &mut self,
        dom_deser: &mut super::DomDeserializer<'de, BORROW, P>,
    ) -> Result<(), DomDeserializeError<P::Error>> {
        if let Some(info) = self.field_map.get_tuple_field(self.tuple_position) {
            let idx = info.idx;
            trace!(
                idx,
                position = self.tuple_position,
                "matched tuple field by position"
            );
            self.with_wip(|wip| {
                Ok(wip
                    .begin_nth_field(idx)?
                    .deserialize_with(dom_deser)?
                    .end()?)
            })?;
            self.tuple_position += 1;
        } else {
            trace!(
                position = self.tuple_position,
                "tuple position out of bounds, skipping"
            );
            self.parser
                .skip_node()
                .map_err(DomDeserializeError::Parser)?;
        }
        Ok(())
    }

    fn handle_flattened_child(
        &mut self,
        flattened: &FlattenedChildInfo,
        dom_deser: &mut super::DomDeserializer<'de, BORROW, P>,
    ) -> Result<(), DomDeserializeError<P::Error>> {
        trace!(
            parent_idx = flattened.parent_idx,
            child_idx = flattened.child_idx,
            parent_is_option = flattened.parent_is_option,
            "matched flattened child field"
        );
        self.leave_active_sequence()?;

        let parent_idx = flattened.parent_idx;
        let child_idx = flattened.child_idx;
        let parent_is_option = flattened.parent_is_option;

        self.with_wip(|mut wip| {
            wip = wip.begin_nth_field(parent_idx)?;
            if parent_is_option {
                wip = wip.begin_some()?;
            }
            wip = wip
                .begin_nth_field(child_idx)?
                .deserialize_with(dom_deser)?
                .end()?;
            if parent_is_option {
                wip = wip.end()?;
            }
            Ok(wip.end()?)
        })
    }

    fn handle_flattened_enum(
        &mut self,
        field_idx: usize,
        dom_deser: &mut super::DomDeserializer<'de, BORROW, P>,
    ) -> Result<(), DomDeserializeError<P::Error>> {
        trace!(field_idx, "matched flattened enum field");
        self.leave_active_sequence()?;
        self.with_wip(|wip| {
            Ok(wip
                .begin_nth_field(field_idx)?
                .deserialize_with(dom_deser)?
                .end()?)
        })
    }

    fn handle_elements_collection(
        &mut self,
        dom_deser: &mut super::DomDeserializer<'de, BORROW, P>,
    ) -> Result<(), DomDeserializeError<P::Error>> {
        if !self.elements_list_started {
            let info = self.field_map.elements_field.as_ref().unwrap();
            let idx = info.idx;
            trace!(idx, field_name = %info.field.name, "starting elements list (lazy)");
            self.with_wip(|wip| Ok(wip.begin_nth_field(idx)?.init_list()?))?;
            self.elements_list_started = true;
        }
        trace!("adding element to elements collection");
        self.with_wip(|wip| Ok(wip.begin_list_item()?.deserialize_with(dom_deser)?.end()?))
    }

    fn handle_flattened_map(
        &mut self,
        tag: &str,
        namespace: Option<&str>,
    ) -> Result<(), DomDeserializeError<P::Error>> {
        let map_info = self
            .field_map
            .flattened_maps
            .iter()
            .find(|info| info.namespace.is_none() || info.namespace == namespace);

        if let Some(info) = map_info {
            let idx = info.idx;
            trace!(idx, field_name = %info.field.name, tag, "adding to flattened map");
            self.leave_active_sequence()?;

            self.parser.expect_node_start()?;
            let element_text = self.read_element_text()?;

            self.started_flattened_maps.insert(idx);
            let tag = tag.to_string();
            self.with_wip(|wip| {
                Ok(wip
                    .begin_nth_field(idx)?
                    .init_map()?
                    .begin_key()?
                    .set::<String>(tag)?
                    .end()?
                    .begin_value()?
                    .set::<String>(element_text)?
                    .end()?
                    .end()?)
            })
        } else {
            self.handle_unknown_element(tag)
        }
    }

    fn read_element_text(&mut self) -> Result<String, DomDeserializeError<P::Error>> {
        loop {
            match self
                .parser
                .peek_event_or_eof("Attribute or ChildrenStart")?
            {
                DomEvent::Attribute { .. } => {
                    self.parser.expect_attribute()?;
                }
                DomEvent::ChildrenStart => break,
                DomEvent::NodeEnd => {
                    self.parser.expect_node_end()?;
                    return Ok(String::new());
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

        let mut text = String::new();
        loop {
            match self.parser.peek_event_or_eof("text or ChildrenEnd")? {
                DomEvent::ChildrenEnd => break,
                DomEvent::Text(_) => text.push_str(&self.parser.expect_text()?),
                _ => self
                    .parser
                    .skip_node()
                    .map_err(DomDeserializeError::Parser)?,
            }
        }
        self.parser.expect_children_end()?;
        self.parser.expect_node_end()?;
        Ok(text)
    }

    fn handle_unknown_element(&mut self, tag: &str) -> Result<(), DomDeserializeError<P::Error>> {
        let wip = self.wip.as_ref().expect("wip missing");
        if wip.shape().has_deny_unknown_fields_attr() {
            return Err(DomDeserializeError::UnknownElement {
                tag: tag.to_string(),
            });
        }
        trace!(tag, "skipping unknown element");
        self.parser.skip_node().map_err(DomDeserializeError::Parser)
    }

    fn cleanup(
        &mut self,
        dom_deser: &mut super::DomDeserializer<'de, BORROW, P>,
    ) -> Result<(), DomDeserializeError<P::Error>> {
        if let Some(idx) = self.active_seq_idx {
            let state = self.started_seqs.get(&idx).unwrap();
            match state {
                SeqState::List { is_smart_ptr } => {
                    let is_smart_ptr = *is_smart_ptr;
                    self.with_wip(|mut wip| {
                        trace!(path = %wip.path(), is_smart_ptr, "ending active flat list");
                        wip = wip.end()?;
                        if is_smart_ptr {
                            wip = wip.end()?;
                        }
                        Ok(wip)
                    })?;
                }
                SeqState::Array { .. } => {
                    self.with_wip(|wip| {
                        trace!(path = %wip.path(), "ending active flat array");
                        Ok(wip.end()?)
                    })?;
                }
                SeqState::Set => {
                    self.with_wip(|wip| {
                        trace!(path = %wip.path(), "ending active flat set");
                        Ok(wip.end()?)
                    })?;
                }
            }
        }

        if self.elements_list_started {
            self.with_wip(|wip| {
                trace!(path = %wip.path(), "ending elements list");
                Ok(wip.end()?)
            })?;
        } else if let Some(info) = &self.field_map.elements_field {
            let idx = info.idx;
            trace!(idx, field_name = %info.field.name, "initializing empty elements list");
            self.with_wip(|wip| Ok(wip.begin_nth_field(idx)?.init_list()?.end()?))?;
        }

        if let Some(info) = &self.field_map.text_field {
            if !self.text_content.is_empty() {
                let idx = info.idx;
                trace!(idx, field_name = %info.field.name, text_len = self.text_content.len(), "setting text field");
                let text = std::mem::take(&mut self.text_content);
                self.with_wip(|wip| {
                    Ok(dom_deser
                        .set_string_value(wip.begin_nth_field(idx)?, Cow::Owned(text))?
                        .end()?)
                })?;
            }
        }
        Ok(())
    }
}
