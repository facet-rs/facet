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
///
/// Methods take `wip` as input and return it as output, threading it through.
pub(crate) struct StructDeserializer<'de, 'p, const BORROW: bool, P: DomParser<'de>> {
    dom_deser: &'p mut super::DomDeserializer<'de, BORROW, P>,
    field_map: StructFieldMap,
    struct_def: &'static StructType,

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
        dom_deser: &'p mut super::DomDeserializer<'de, BORROW, P>,
        struct_def: &'static StructType,
        ns_all: Option<&'static str>,
    ) -> Self {
        let field_map = StructFieldMap::new(struct_def, ns_all);
        Self {
            dom_deser,
            field_map,
            struct_def,
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

    /// Convenience accessor for the parser.
    fn parser(&mut self) -> &mut P {
        &mut self.dom_deser.parser
    }

    pub fn deserialize(
        mut self,
        mut wip: Partial<'de, BORROW>,
    ) -> Result<Partial<'de, BORROW>, DomDeserializeError<P::Error>> {
        if self.field_map.has_flatten {
            trace!("enabling deferred mode for struct with flatten");
            wip = wip.begin_deferred()?;
            self.using_deferred = true;
        }

        self.tag = self.parser().expect_node_start()?.to_string();
        trace!(tag = %self.tag, "got NodeStart");

        wip = self.process_attributes(wip)?;

        self.parser().expect_children_start()?;
        wip = self.process_children(wip)?;
        wip = self.cleanup(wip)?;
        self.parser().expect_children_end()?;
        self.parser().expect_node_end()?;

        if self.using_deferred {
            trace!("finishing deferred mode for struct with flatten");
            wip = wip.finish_deferred()?;
        }

        trace!(tag = %self.tag, "struct deserialization complete");
        Ok(wip)
    }

    fn process_attributes(
        &mut self,
        mut wip: Partial<'de, BORROW>,
    ) -> Result<Partial<'de, BORROW>, DomDeserializeError<P::Error>> {
        trace!("processing attributes");
        loop {
            match self
                .parser()
                .peek_event_or_eof("Attribute or ChildrenStart")?
            {
                DomEvent::Attribute { .. } => {
                    let AttributeRecord {
                        name,
                        value,
                        namespace,
                    } = self.parser().expect_attribute()?;
                    trace!(name = %name, value = %value, namespace = ?namespace, "got Attribute");
                    if let Some(info) = self
                        .field_map
                        .find_attribute(&name, namespace.as_ref().map(|c| c.as_ref()))
                    {
                        let idx = info.idx;
                        trace!(idx, field_name = %info.field.name, "matched attribute field");
                        wip = self
                            .dom_deser
                            .set_string_value(wip.begin_nth_field(idx)?, value)?
                            .end()?;
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
                    self.parser().expect_node_end()?;
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
        Ok(wip)
    }

    fn process_children(
        &mut self,
        mut wip: Partial<'de, BORROW>,
    ) -> Result<Partial<'de, BORROW>, DomDeserializeError<P::Error>> {
        trace!("processing children");
        loop {
            match self.parser().peek_event_or_eof("child or ChildrenEnd")? {
                DomEvent::ChildrenEnd => {
                    trace!("children done");
                    break;
                }
                DomEvent::Text(_) => {
                    wip = self.handle_text(wip)?;
                }
                DomEvent::NodeStart { tag, namespace } => {
                    let tag = tag.clone();
                    let namespace = namespace.clone();
                    wip = self.handle_child_element(wip, &tag, namespace.as_deref())?;
                }
                DomEvent::Comment(_) => {
                    trace!("skipping comment");
                    self.parser().expect_comment()?;
                }
                other => {
                    return Err(DomDeserializeError::TypeMismatch {
                        expected: "child content",
                        got: format!("{other:?}"),
                    });
                }
            }
        }
        Ok(wip)
    }

    fn handle_text(
        &mut self,
        mut wip: Partial<'de, BORROW>,
    ) -> Result<Partial<'de, BORROW>, DomDeserializeError<P::Error>> {
        let text = self.parser().expect_text()?;
        trace!(text_len = text.len(), "got Text");

        if self.elements_list_started {
            trace!("adding text as list item (mixed content)");
            wip = wip.begin_list_item()?;
            wip = self
                .dom_deser
                .deserialize_text_into_enum(wip, text)?
                .end()?;
        } else if self.field_map.text_field.is_some() {
            trace!("accumulating text for text field");
            self.text_content.push_str(&text);
        } else if self.struct_def.kind == StructKind::TupleStruct
            && self.struct_def.fields.len() == 1
        {
            trace!("setting text content for newtype field 0");
            wip = self
                .dom_deser
                .set_string_value(wip.begin_nth_field(0)?, text)?
                .end()?;
        } else {
            trace!("ignoring text (no text field)");
        }
        Ok(wip)
    }

    fn handle_child_element(
        &mut self,
        wip: Partial<'de, BORROW>,
        tag: &str,
        namespace: Option<&str>,
    ) -> Result<Partial<'de, BORROW>, DomDeserializeError<P::Error>> {
        trace!(tag = %tag, namespace = ?namespace, "got child NodeStart");

        if let Some(info) = self.field_map.find_element(tag, namespace) {
            if info.is_list || info.is_array || info.is_set {
                self.handle_flat_sequence(wip, info.idx, info.is_list, info.is_set, info.field)
            } else {
                self.handle_scalar_element(wip, info.idx)
            }
        } else if self.field_map.is_tuple() && tag == "item" {
            self.handle_tuple_item(wip)
        } else if let Some(flattened) = self.field_map.find_flattened_child(tag, namespace).cloned()
        {
            self.handle_flattened_child(wip, &flattened)
        } else if let Some(field_idx) = self.field_map.flattened_enum.as_ref().map(|e| e.field_idx)
        {
            self.handle_flattened_enum(wip, field_idx)
        } else if self.field_map.elements_field.is_some() {
            self.handle_elements_collection(wip)
        } else if !self.field_map.flattened_maps.is_empty() {
            self.handle_flattened_map(wip, tag, namespace)
        } else {
            self.handle_unknown_element(wip, tag)
        }
    }

    fn leave_active_sequence(
        &mut self,
        mut wip: Partial<'de, BORROW>,
    ) -> Result<Partial<'de, BORROW>, DomDeserializeError<P::Error>> {
        if let Some(prev_idx) = self.active_seq_idx.take() {
            trace!(prev_idx, "leaving active flat sequence");
            let is_smart_ptr = matches!(
                self.started_seqs.get(&prev_idx),
                Some(SeqState::List { is_smart_ptr: true })
            );
            wip = wip.end()?;
            if is_smart_ptr {
                wip = wip.end()?;
            }
        }
        if self.elements_list_started {
            trace!("leaving elements list");
            wip = wip.end()?;
            self.elements_list_started = false;
        }
        Ok(wip)
    }

    fn handle_flat_sequence(
        &mut self,
        mut wip: Partial<'de, BORROW>,
        idx: usize,
        is_list: bool,
        is_set: bool,
        field: &'static facet_core::Field,
    ) -> Result<Partial<'de, BORROW>, DomDeserializeError<P::Error>> {
        if self.elements_list_started {
            trace!("leaving elements list for flat sequence field");
            wip = wip.end()?;
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
                wip = wip.end()?;
                if is_smart_ptr {
                    wip = wip.end()?;
                }
                self.active_seq_idx = None;
            }
        }

        use std::collections::hash_map::Entry;
        let need_start = matches!(self.started_seqs.entry(idx), Entry::Vacant(_));

        if need_start {
            trace!(idx, field_name = %field.name, "starting flat sequence field");
            if is_list {
                let is_smart_ptr = matches!(field.shape().def, Def::Pointer(_));
                wip = wip.begin_nth_field(idx)?;
                if is_smart_ptr {
                    wip = wip.begin_smart_ptr()?;
                }
                wip = wip.init_list()?;
                self.started_seqs
                    .insert(idx, SeqState::List { is_smart_ptr });
            } else if is_set {
                wip = wip.begin_nth_field(idx)?.init_set()?;
                self.started_seqs.insert(idx, SeqState::Set);
            } else {
                wip = wip.begin_nth_field(idx)?.init_array()?;
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
                    wip = wip.begin_nth_field(idx)?;
                    if is_smart_ptr {
                        wip = wip.begin_smart_ptr()?;
                    }
                    wip = wip.init_list()?;
                }
                SeqState::Set => {
                    wip = wip.begin_nth_field(idx)?.init_set()?;
                }
                SeqState::Array { .. } => {
                    wip = wip.begin_nth_field(idx)?.init_array()?;
                }
            }
            self.active_seq_idx = Some(idx);
        }

        // Add item
        if is_list {
            trace!(idx, field_name = %field.name, "adding item to flat list");
            wip = wip
                .begin_list_item()?
                .deserialize_with(self.dom_deser)?
                .end()?;
        } else if is_set {
            trace!(idx, field_name = %field.name, "adding item to flat set");
            wip = wip
                .begin_set_item()?
                .deserialize_with(self.dom_deser)?
                .end()?;
        } else {
            // Get the current index first
            let item_idx = match self.started_seqs.get(&idx) {
                Some(SeqState::Array { next_idx }) => *next_idx,
                _ => return Ok(wip),
            };
            trace!(idx, field_name = %field.name, item_idx, "adding item to flat array");
            wip = wip
                .begin_nth_field(item_idx)?
                .deserialize_with(self.dom_deser)?
                .end()?;
            // Increment after
            if let Some(SeqState::Array { next_idx }) = self.started_seqs.get_mut(&idx) {
                *next_idx += 1;
            }
        }
        Ok(wip)
    }

    fn handle_scalar_element(
        &mut self,
        mut wip: Partial<'de, BORROW>,
        idx: usize,
    ) -> Result<Partial<'de, BORROW>, DomDeserializeError<P::Error>> {
        wip = self.leave_active_sequence(wip)?;
        trace!(idx, "matched scalar element field");
        wip = wip
            .begin_nth_field(idx)?
            .deserialize_with(self.dom_deser)?
            .end()?;
        Ok(wip)
    }

    fn handle_tuple_item(
        &mut self,
        mut wip: Partial<'de, BORROW>,
    ) -> Result<Partial<'de, BORROW>, DomDeserializeError<P::Error>> {
        if let Some(info) = self.field_map.get_tuple_field(self.tuple_position) {
            let idx = info.idx;
            trace!(
                idx,
                position = self.tuple_position,
                "matched tuple field by position"
            );
            wip = wip
                .begin_nth_field(idx)?
                .deserialize_with(self.dom_deser)?
                .end()?;
            self.tuple_position += 1;
        } else {
            trace!(
                position = self.tuple_position,
                "tuple position out of bounds, skipping"
            );
            self.parser()
                .skip_node()
                .map_err(DomDeserializeError::Parser)?;
        }
        Ok(wip)
    }

    fn handle_flattened_child(
        &mut self,
        mut wip: Partial<'de, BORROW>,
        flattened: &FlattenedChildInfo,
    ) -> Result<Partial<'de, BORROW>, DomDeserializeError<P::Error>> {
        trace!(
            parent_idx = flattened.parent_idx,
            child_idx = flattened.child_idx,
            parent_is_option = flattened.parent_is_option,
            "matched flattened child field"
        );
        wip = self.leave_active_sequence(wip)?;

        wip = wip.begin_nth_field(flattened.parent_idx)?;
        if flattened.parent_is_option {
            wip = wip.begin_some()?;
        }
        wip = wip
            .begin_nth_field(flattened.child_idx)?
            .deserialize_with(self.dom_deser)?
            .end()?;
        if flattened.parent_is_option {
            wip = wip.end()?;
        }
        wip = wip.end()?;
        Ok(wip)
    }

    fn handle_flattened_enum(
        &mut self,
        mut wip: Partial<'de, BORROW>,
        field_idx: usize,
    ) -> Result<Partial<'de, BORROW>, DomDeserializeError<P::Error>> {
        trace!(field_idx, "matched flattened enum field");
        wip = self.leave_active_sequence(wip)?;
        wip = wip
            .begin_nth_field(field_idx)?
            .deserialize_with(self.dom_deser)?
            .end()?;
        Ok(wip)
    }

    fn handle_elements_collection(
        &mut self,
        mut wip: Partial<'de, BORROW>,
    ) -> Result<Partial<'de, BORROW>, DomDeserializeError<P::Error>> {
        if !self.elements_list_started {
            let info = self.field_map.elements_field.as_ref().unwrap();
            let idx = info.idx;
            trace!(idx, field_name = %info.field.name, "starting elements list (lazy)");
            wip = wip.begin_nth_field(idx)?.init_list()?;
            self.elements_list_started = true;
        }
        trace!("adding element to elements collection");
        wip = wip
            .begin_list_item()?
            .deserialize_with(self.dom_deser)?
            .end()?;
        Ok(wip)
    }

    fn handle_flattened_map(
        &mut self,
        mut wip: Partial<'de, BORROW>,
        tag: &str,
        namespace: Option<&str>,
    ) -> Result<Partial<'de, BORROW>, DomDeserializeError<P::Error>> {
        let map_info = self
            .field_map
            .flattened_maps
            .iter()
            .find(|info| info.namespace.is_none() || info.namespace == namespace);

        if let Some(info) = map_info {
            let idx = info.idx;
            trace!(idx, field_name = %info.field.name, tag, "adding to flattened map");
            wip = self.leave_active_sequence(wip)?;

            self.parser().expect_node_start()?;
            let element_text = self.read_element_text()?;

            self.started_flattened_maps.insert(idx);
            wip = wip
                .begin_nth_field(idx)?
                .init_map()?
                .begin_key()?
                .set::<String>(tag.to_string())?
                .end()?
                .begin_value()?
                .set::<String>(element_text)?
                .end()?
                .end()?;
            Ok(wip)
        } else {
            self.handle_unknown_element(wip, tag)
        }
    }

    fn read_element_text(&mut self) -> Result<String, DomDeserializeError<P::Error>> {
        loop {
            match self
                .parser()
                .peek_event_or_eof("Attribute or ChildrenStart")?
            {
                DomEvent::Attribute { .. } => {
                    self.parser().expect_attribute()?;
                }
                DomEvent::ChildrenStart => break,
                DomEvent::NodeEnd => {
                    self.parser().expect_node_end()?;
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
        self.parser().expect_children_start()?;

        let mut text = String::new();
        loop {
            match self.parser().peek_event_or_eof("text or ChildrenEnd")? {
                DomEvent::ChildrenEnd => break,
                DomEvent::Text(_) => text.push_str(&self.parser().expect_text()?),
                _ => self
                    .parser()
                    .skip_node()
                    .map_err(DomDeserializeError::Parser)?,
            }
        }
        self.parser().expect_children_end()?;
        self.parser().expect_node_end()?;
        Ok(text)
    }

    fn handle_unknown_element(
        &mut self,
        wip: Partial<'de, BORROW>,
        tag: &str,
    ) -> Result<Partial<'de, BORROW>, DomDeserializeError<P::Error>> {
        if wip.shape().has_deny_unknown_fields_attr() {
            return Err(DomDeserializeError::UnknownElement {
                tag: tag.to_string(),
            });
        }
        trace!(tag, "skipping unknown element");
        self.parser()
            .skip_node()
            .map_err(DomDeserializeError::Parser)?;
        Ok(wip)
    }

    fn cleanup(
        &mut self,
        mut wip: Partial<'de, BORROW>,
    ) -> Result<Partial<'de, BORROW>, DomDeserializeError<P::Error>> {
        if let Some(idx) = self.active_seq_idx {
            let state = self.started_seqs.get(&idx).unwrap();
            match state {
                SeqState::List { is_smart_ptr } => {
                    let is_smart_ptr = *is_smart_ptr;
                    trace!(path = %wip.path(), is_smart_ptr, "ending active flat list");
                    wip = wip.end()?;
                    if is_smart_ptr {
                        wip = wip.end()?;
                    }
                }
                SeqState::Array { .. } => {
                    trace!(path = %wip.path(), "ending active flat array");
                    wip = wip.end()?;
                }
                SeqState::Set => {
                    trace!(path = %wip.path(), "ending active flat set");
                    wip = wip.end()?;
                }
            }
        }

        if self.elements_list_started {
            trace!(path = %wip.path(), "ending elements list");
            wip = wip.end()?;
        } else if let Some(info) = &self.field_map.elements_field {
            let idx = info.idx;
            trace!(idx, field_name = %info.field.name, "initializing empty elements list");
            wip = wip.begin_nth_field(idx)?.init_list()?.end()?;
        }

        if let Some(info) = &self.field_map.text_field {
            if !self.text_content.is_empty() {
                let idx = info.idx;
                trace!(idx, field_name = %info.field.name, text_len = self.text_content.len(), "setting text field");
                let text = std::mem::take(&mut self.text_content);
                wip = self
                    .dom_deser
                    .set_string_value(wip.begin_nth_field(idx)?, Cow::Owned(text))?
                    .end()?;
            }
        }
        Ok(wip)
    }
}
