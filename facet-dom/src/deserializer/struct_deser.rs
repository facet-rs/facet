//! Struct deserialization logic extracted from the main deserializer.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

use facet_core::{Def, Shape, StructKind, StructType, Type, UserType};
use facet_reflect::Partial;

use crate::error::DomDeserializeError;
use crate::trace;
use crate::{AttributeRecord, DomEvent, DomParser, DomParserExt};

use super::PartialDeserializeExt;
use super::field_map::{FlattenedChildInfo, StructFieldMap};

/// State for a flat sequence field being deserialized.
pub(crate) enum SeqState {
    List { is_smart_ptr: bool },
    Array { next_idx: usize },
    Set,
    Tuple { next_idx: usize },
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

    /// Whether we've started the xml::text list (for Vec<String> text fields)
    text_list_started: bool,

    /// Whether we've started the xml::attribute catch-all list (for Vec<String> attribute fields)
    attributes_list_started: bool,

    /// Which flattened element maps have been initialized
    started_flattened_maps: HashSet<usize>,

    /// Which flattened attribute maps have been initialized
    started_flattened_attr_maps: HashSet<usize>,

    /// Which nested flattened attribute maps have been initialized (parent_idx, child_idx)
    started_nested_flattened_attr_maps: HashSet<(usize, usize)>,

    /// Whether we've ever started the flattened enum list (for Vec<Enum> with flatten)
    flattened_enum_list_started: bool,

    /// Whether the flattened enum list is currently active (we're inside it)
    flattened_enum_list_active: bool,

    /// Whether unknown fields should cause an error
    deny_unknown_fields: bool,

    /// Position for tuple struct positional matching
    tuple_position: usize,

    /// Tag from NodeStart (for tracing and xml::tag field)
    tag: Cow<'de, str>,

    /// Expected element name for root element validation
    expected_name: Cow<'static, str>,
}

impl<'de, 'p, const BORROW: bool, P: DomParser<'de>> StructDeserializer<'de, 'p, BORROW, P> {
    pub fn new(
        dom_deser: &'p mut super::DomDeserializer<'de, BORROW, P>,
        struct_def: &'static StructType,
        ns_all: Option<&'static str>,
        expected_name: Cow<'static, str>,
        deny_unknown_fields: bool,
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
            text_list_started: false,
            attributes_list_started: false,
            started_flattened_maps: HashSet::new(),
            started_flattened_attr_maps: HashSet::new(),
            started_nested_flattened_attr_maps: HashSet::new(),
            flattened_enum_list_started: false,
            flattened_enum_list_active: false,
            deny_unknown_fields,
            tuple_position: 0,
            tag: Cow::Borrowed(""),
            expected_name,
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
        if self.field_map.has_flatten && !wip.is_deferred() {
            trace!("enabling deferred mode for struct with flatten");
            wip = wip.begin_deferred()?;
            self.using_deferred = true;
        }

        self.tag = self.parser().expect_node_start()?;

        // Validate root element name matches expected, unless struct has a tag field
        // (which means it accepts any element name)
        if self.field_map.tag_field.is_none() && self.tag != self.expected_name {
            return Err(DomDeserializeError::UnknownElement {
                tag: self.tag.to_string(),
            });
        }

        // Set the tag field if present (xml::tag or html::tag)
        if let Some(info) = &self.field_map.tag_field {
            let idx = info.idx;
            trace!("→ .{}", info.field.name);
            let tag = self.tag.clone();
            wip = self
                .dom_deser
                .set_string_value(wip.begin_nth_field(idx)?, tag)?
                .end()?;
        }

        wip = self.process_attributes(wip)?;

        self.parser().expect_children_start()?;
        wip = self.process_children(wip)?;
        wip = self.cleanup(wip)?;
        self.parser().expect_children_end()?;
        self.parser().expect_node_end()?;

        if self.using_deferred {
            wip = wip.finish_deferred()?;
        }

        Ok(wip)
    }

    fn process_attributes(
        &mut self,
        mut wip: Partial<'de, BORROW>,
    ) -> Result<Partial<'de, BORROW>, DomDeserializeError<P::Error>> {
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
                    if let Some(info) = self
                        .field_map
                        .find_attribute(&name, namespace.as_ref().map(|c| c.as_ref()))
                    {
                        trace!("→ .{}", info.field.name);
                        // Use set_string_value_with_proxy to handle field-level proxies
                        wip = self
                            .dom_deser
                            .set_string_value_with_proxy(wip.begin_nth_field(info.idx)?, value)?
                            .end()?;
                    } else if let Some(flattened) = self
                        .field_map
                        .find_flattened_attribute(&name, namespace.as_ref().map(|c| c.as_ref()))
                        .cloned()
                    {
                        // Handle attribute from a flattened struct (e.g., GlobalAttrs)
                        trace!("→ (flatten).{}", flattened.child_info.field.name);

                        // Navigate into the flattened parent, then to the child field
                        let parent_idx = flattened.parent_idx;
                        let child_idx = flattened.child_idx;
                        let parent_wip = wip.begin_nth_field(parent_idx)?;
                        let parent_wip = if flattened.parent_is_option {
                            parent_wip.begin_some()?
                        } else {
                            parent_wip
                        };
                        wip = self
                            .dom_deser
                            .set_string_value_with_proxy(
                                parent_wip.begin_nth_field(child_idx)?,
                                value,
                            )?
                            .end()?;
                        if flattened.parent_is_option {
                            wip = wip.end()?;
                        }
                        wip = wip.end()?;
                    } else if let Some(info) = &self.field_map.attributes_field {
                        // Catch-all Vec<String> for all attribute values
                        if !self.attributes_list_started {
                            trace!("→ .{}[]", info.field.name);
                            wip = wip.begin_nth_field(info.idx)?.init_list()?;
                            self.attributes_list_started = true;
                        }
                        wip = wip.begin_list_item()?;
                        wip = self.dom_deser.set_string_value(wip, value)?.end()?;
                    } else {
                        // Try to add to flattened attribute map (direct or nested)
                        let mut handled = false;

                        // First try direct flattened attr maps
                        if !self.field_map.flattened_attr_maps.is_empty() {
                            let map_info = self.field_map.flattened_attr_maps.iter().find(|info| {
                                info.namespace.is_none()
                                    || info.namespace == namespace.as_ref().map(|c| c.as_ref())
                            });

                            if let Some(info) = map_info {
                                trace!("→ .{}[{}]", info.field.name, name);
                                self.started_flattened_attr_maps.insert(info.idx);
                                wip = wip
                                    .begin_nth_field(info.idx)?
                                    .init_map()?
                                    .begin_key()?
                                    .set::<String>(name.to_string())?
                                    .end()?
                                    .begin_value()?
                                    .set::<String>(value.to_string())?
                                    .end()?
                                    .end()?;
                                handled = true;
                            }
                        }

                        // Then try nested flattened attr maps (e.g., flattened struct with flattened HashMap inside)
                        if !handled && !self.field_map.nested_flattened_attr_maps.is_empty() {
                            let nested_info = self
                                .field_map
                                .nested_flattened_attr_maps
                                .iter()
                                .find(|info| {
                                    info.child_info.namespace.is_none()
                                        || info.child_info.namespace
                                            == namespace.as_ref().map(|c| c.as_ref())
                                });

                            if let Some(info) = nested_info {
                                trace!("→ (flatten).{}[{}]", info.child_info.field.name, name);
                                let key = (info.parent_idx, info.child_idx);
                                let is_first =
                                    !self.started_nested_flattened_attr_maps.contains(&key);
                                self.started_nested_flattened_attr_maps.insert(key);

                                // Navigate to parent field, then child field
                                wip = wip.begin_nth_field(info.parent_idx)?;
                                if info.parent_is_option {
                                    wip = wip.begin_some()?;
                                }
                                wip = wip.begin_nth_field(info.child_idx)?;
                                if is_first {
                                    wip = wip.init_map()?;
                                }
                                wip = wip
                                    .begin_key()?
                                    .set::<String>(name.to_string())?
                                    .end()?
                                    .begin_value()?
                                    .set::<String>(value.to_string())?
                                    .end()?
                                    .end()?;
                                // End parent (and option if needed)
                                if info.parent_is_option {
                                    wip = wip.end()?;
                                }
                                wip = wip.end()?;
                                handled = true;
                            }
                        }

                        if !handled && self.deny_unknown_fields {
                            return Err(DomDeserializeError::UnknownAttribute {
                                name: name.to_string(),
                            });
                        }
                    }
                }
                DomEvent::ChildrenStart => {
                    break;
                }
                DomEvent::NodeEnd => {
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
        loop {
            match self.parser().peek_event_or_eof("child or ChildrenEnd")? {
                DomEvent::ChildrenEnd => {
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

        if self.elements_list_started {
            wip = wip.begin_list_item()?;
            wip = self
                .dom_deser
                .deserialize_text_into_enum(wip, text)?
                .end()?;
        } else if let Some(info) = &self.field_map.text_field {
            if info.is_list || info.is_set {
                // Vec<String> or HashSet<String> with xml::text - each text node is a list item
                if !self.text_list_started {
                    trace!("→ .{}[]", info.field.name);
                    wip = wip.begin_nth_field(info.idx)?.init_list()?;
                    self.text_list_started = true;
                }
                wip = wip.begin_list_item()?;
                wip = self.dom_deser.set_string_value(wip, text)?.end()?;
            } else {
                // Single String with xml::text - accumulate text
                self.text_content.push_str(&text);
            }
        } else if self.field_map.elements_field.is_some() {
            // Mixed content: text before any elements - start the list and add text
            let info = self.field_map.elements_field.as_ref().unwrap();
            trace!("→ .{}[]", info.field.name);
            wip = wip.begin_nth_field(info.idx)?.init_list()?;
            self.elements_list_started = true;
            wip = wip.begin_list_item()?;
            wip = self
                .dom_deser
                .deserialize_text_into_enum(wip, text)?
                .end()?;
        } else if self.struct_def.kind == StructKind::TupleStruct
            && self.struct_def.fields.len() == 1
        {
            trace!("→ .0");
            wip = self
                .dom_deser
                .set_string_value(wip.begin_nth_field(0)?, text)?
                .end()?;
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
            if info.is_list || info.is_array || info.is_set || info.is_tuple {
                self.handle_flat_sequence(
                    wip,
                    info.idx,
                    info.is_list,
                    info.is_set,
                    info.is_tuple,
                    info.field,
                )
            } else {
                self.handle_scalar_element(wip, info.idx)
            }
        } else if self.field_map.is_tuple() && tag == "item" {
            // Legacy support for <item> elements in tuple structs (deprecated)
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
        if self.flattened_enum_list_active {
            trace!("leaving flattened enum list (staying started)");
            wip = wip.end()?;
            self.flattened_enum_list_active = false;
        }
        Ok(wip)
    }

    fn handle_flat_sequence(
        &mut self,
        mut wip: Partial<'de, BORROW>,
        idx: usize,
        is_list: bool,
        is_set: bool,
        is_tuple: bool,
        field: &'static facet_core::Field,
    ) -> Result<Partial<'de, BORROW>, DomDeserializeError<P::Error>> {
        if self.elements_list_started {
            trace!("leaving elements list for flat sequence field");
            wip = wip.end()?;
            self.elements_list_started = false;
        }

        // Switch sequences if needed
        if let Some(prev_idx) = self.active_seq_idx
            && prev_idx != idx
        {
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
            } else if is_tuple {
                // Tuples: just navigate into the field, items are accessed by position
                wip = wip.begin_nth_field(idx)?;
                self.started_seqs
                    .insert(idx, SeqState::Tuple { next_idx: 0 });
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
                SeqState::Tuple { .. } => {
                    wip = wip.begin_nth_field(idx)?;
                }
            }
            self.active_seq_idx = Some(idx);
        }

        // Add item
        if is_list {
            trace!(idx, field_name = %field.name, "adding item to flat list");
            wip = wip.begin_list_item()?;
            wip = self.deserialize_sequence_item(wip, field)?;
            wip = wip.end()?;
        } else if is_set {
            trace!(idx, field_name = %field.name, "adding item to flat set");
            wip = wip.begin_set_item()?;
            wip = self.deserialize_sequence_item(wip, field)?;
            wip = wip.end()?;
        } else if is_tuple {
            // Tuples: access by position using begin_nth_field
            let item_idx = match self.started_seqs.get(&idx) {
                Some(SeqState::Tuple { next_idx }) => *next_idx,
                _ => return Ok(wip),
            };
            trace!(idx, field_name = %field.name, item_idx, "adding item to flat tuple");
            wip = wip
                .begin_nth_field(item_idx)?
                .deserialize_with(self.dom_deser)?
                .end()?;
            // Increment after
            if let Some(SeqState::Tuple { next_idx }) = self.started_seqs.get_mut(&idx) {
                *next_idx += 1;
            }
        } else {
            // Arrays: access by position using begin_nth_field
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

    /// Deserialize a sequence item (list/set element), handling struct items specially.
    ///
    /// For struct items, we use the field's effective name as the element name
    /// (from rename attribute or singularized field name) rather than the type's name.
    fn deserialize_sequence_item(
        &mut self,
        mut wip: Partial<'de, BORROW>,
        field: &'static facet_core::Field,
    ) -> Result<Partial<'de, BORROW>, DomDeserializeError<P::Error>> {
        let item_shape = wip.shape();

        // Check if the item type is a struct
        if let Type::User(UserType::Struct(inner_struct_def)) = &item_shape.ty {
            // Compute expected element name from field: rename > singularized(lowerCamelCase(field.name))
            let expected_name: Cow<'static, str> = if field.rename.is_some() {
                Cow::Borrowed(field.effective_name())
            } else {
                // For list fields without rename, use singularized lowerCamelCase
                let camel = crate::naming::to_element_name(field.name);
                Cow::Owned(facet_singularize::singularize(&camel))
            };

            // Get ns_all from the item struct's shape
            let ns_all = item_shape
                .attributes
                .iter()
                .find(|attr| attr.ns == Some("xml") && attr.key == "ns_all")
                .and_then(|attr| attr.get_as::<&str>().copied());

            let deny_unknown_fields = item_shape.has_deny_unknown_fields_attr();

            wip = StructDeserializer::new(
                self.dom_deser,
                inner_struct_def,
                ns_all,
                expected_name,
                deny_unknown_fields,
            )
            .deserialize(wip)?;
        } else {
            wip = wip.deserialize_with(self.dom_deser)?;
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

        let field = &self.struct_def.fields[idx];
        let field_shape = field.shape();

        // Find the innermost struct type, looking through Option/Pointer wrappers
        let inner_struct_info = self.find_inner_struct(field_shape);

        // Check if the field type (or its inner type) is a struct - if so, we need to deserialize it
        // using the field's element name, not the type's name
        if let Some((inner_struct_def, inner_shape)) = inner_struct_info {
            // Compute expected element name from field: rename > lowerCamelCase(field.name)
            // Note: effective_name() returns rename if set, else field.name
            // We still need to convert to lowerCamelCase if not renamed
            let expected_name: Cow<'static, str> = if field.rename.is_some() {
                Cow::Borrowed(field.effective_name())
            } else {
                crate::naming::to_element_name(field.name)
            };

            // Get ns_all from the inner struct's shape
            let ns_all = inner_shape
                .attributes
                .iter()
                .find(|attr| attr.ns == Some("xml") && attr.key == "ns_all")
                .and_then(|attr| attr.get_as::<&str>().copied());

            let deny_unknown_fields = inner_shape.has_deny_unknown_fields_attr();

            wip = wip.begin_nth_field(idx)?;

            // Handle Option wrapper if present
            if matches!(&field_shape.def, Def::Option(_)) {
                wip = wip.begin_some()?;
            }

            wip = StructDeserializer::new(
                self.dom_deser,
                inner_struct_def,
                ns_all,
                expected_name,
                deny_unknown_fields,
            )
            .deserialize(wip)?;

            // Close Option wrapper if present
            if matches!(&field_shape.def, Def::Option(_)) {
                wip = wip.end()?;
            }

            wip = wip.end()?;
        } else {
            wip = wip
                .begin_nth_field(idx)?
                .deserialize_with(self.dom_deser)?
                .end()?;
        }
        Ok(wip)
    }

    /// Find the innermost struct type, looking through Option wrappers.
    /// Returns the struct definition and its shape if found.
    fn find_inner_struct(
        &self,
        shape: &'static Shape,
    ) -> Option<(&'static StructType, &'static Shape)> {
        // Direct struct type
        if let Type::User(UserType::Struct(struct_def)) = &shape.ty {
            return Some((struct_def, shape));
        }

        // Option<T> - check inner type
        if let Def::Option(option_def) = &shape.def {
            let inner_shape = option_def.t;
            if let Type::User(UserType::Struct(struct_def)) = &inner_shape.ty {
                return Some((struct_def, inner_shape));
            }
        }

        None
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
        let is_list = self
            .field_map
            .flattened_enum
            .as_ref()
            .map(|e| e.field_info.is_list)
            .unwrap_or(false);

        if is_list {
            // Vec<Enum> case: initialize list on first item, then push each item
            trace!(field_idx, "matched flattened enum list field");

            if !self.flattened_enum_list_started {
                // First time: start the list
                trace!(field_idx, "starting flattened enum list");
                wip = wip.begin_nth_field(field_idx)?.init_list()?;
                self.flattened_enum_list_started = true;
                self.flattened_enum_list_active = true;
            } else if !self.flattened_enum_list_active {
                // Re-entering the list after leaving for a regular element
                trace!(field_idx, "re-entering flattened enum list");
                wip = wip.begin_nth_field(field_idx)?.init_list()?;
                self.flattened_enum_list_active = true;
            }

            wip = wip
                .begin_list_item()?
                .deserialize_with(self.dom_deser)?
                .end()?;
        } else {
            // Single enum case: deserialize directly into the field
            trace!(field_idx, "matched flattened enum field");
            wip = self.leave_active_sequence(wip)?;
            wip = wip
                .begin_nth_field(field_idx)?
                .deserialize_with(self.dom_deser)?
                .end()?;
        }
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
                SeqState::Tuple { .. } => {
                    trace!(path = %wip.path(), "ending active flat tuple");
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

        // Handle attributes catch-all field finalization
        if let Some(info) = &self.field_map.attributes_field {
            if self.attributes_list_started {
                // End the attributes list (Vec<String> with xml::attribute catch-all)
                trace!(path = %wip.path(), "ending attributes list");
                wip = wip.end()?;
            } else {
                // Empty attributes list - initialize empty
                let idx = info.idx;
                trace!(idx, field_name = %info.field.name, "initializing empty attributes list");
                wip = wip.begin_nth_field(idx)?.init_list()?.end()?;
            }
        }

        // Handle text field finalization
        if let Some(info) = &self.field_map.text_field {
            if self.text_list_started {
                // End the text list (Vec<String> with xml::text)
                trace!(path = %wip.path(), "ending text list");
                wip = wip.end()?;
            } else if info.is_list || info.is_set {
                // Empty text list - initialize empty
                let idx = info.idx;
                trace!(idx, field_name = %info.field.name, "initializing empty text list");
                wip = wip.begin_nth_field(idx)?.init_list()?.end()?;
            } else if !self.text_content.is_empty() {
                // Single String with accumulated text
                let idx = info.idx;
                trace!(idx, field_name = %info.field.name, text_len = self.text_content.len(), "setting text field");
                let text = std::mem::take(&mut self.text_content);
                wip = self
                    .dom_deser
                    .set_string_value(wip.begin_nth_field(idx)?, Cow::Owned(text))?
                    .end()?;
            }
        }

        // Handle flattened enum list finalization
        if let Some(enum_info) = &self.field_map.flattened_enum {
            if enum_info.field_info.is_list {
                if self.flattened_enum_list_active {
                    // Currently inside the list - close it
                    trace!(path = %wip.path(), "ending flattened enum list (active)");
                    wip = wip.end()?;
                } else if self.flattened_enum_list_started {
                    // List was started but we left it - it's already closed, nothing to do
                    trace!(path = %wip.path(), "flattened enum list already closed");
                } else {
                    // Empty list - initialize empty
                    let idx = enum_info.field_idx;
                    trace!(idx, "initializing empty flattened enum list");
                    wip = wip.begin_nth_field(idx)?.init_list()?.end()?;
                }
            }
        }

        Ok(wip)
    }
}
