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
    /// Deserialize into an existing Partial.
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
                Def::Option(_) => self.deserialize_option(wip),
                _ => Err(DomDeserializeError::Unsupported(format!(
                    "unsupported type: {:?}",
                    shape.ty
                ))),
            },
        }
    }

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
    /// Expects a NodeStart to have been peeked but not consumed.
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

        // Track which sequence fields have been started (for flat list/array deserialization)
        enum SeqState {
            List { is_smart_ptr: bool },
            Array { next_idx: usize },
        }
        let mut started_seqs: std::collections::HashMap<usize, SeqState> =
            std::collections::HashMap::new();

        let mut elements_list_started = false;
        if let Some(info) = &field_map.elements_field {
            trace!(idx = info.idx, field_name = %info.field.name, "beginning elements list");
            wip = wip.begin_nth_field(info.idx)?;
            wip = wip.begin_list()?;
            elements_list_started = true;
        }

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
                    } else {
                        trace!("ignoring text (no text field)");
                    }
                }
                DomEvent::NodeStart { tag, namespace } => {
                    let tag = tag.clone();
                    let namespace = namespace.clone();
                    trace!(tag = %tag, namespace = ?namespace, "got child NodeStart");

                    if !elements_list_started {
                        if let Some(info) =
                            field_map.find_element(&tag, namespace.as_ref().map(|c| c.as_ref()))
                        {
                            if info.is_list || info.is_array {
                                // Flat sequence: repeated elements directly as children
                                use std::collections::hash_map::Entry;
                                if let Entry::Vacant(entry) = started_seqs.entry(info.idx) {
                                    trace!(idx = info.idx, field_name = %info.field.name, is_list = info.is_list, is_array = info.is_array, "starting flat sequence field");
                                    wip = wip.begin_nth_field(info.idx)?;

                                    if info.is_list {
                                        // Handle pointer-wrapped lists (Arc<[T]>, Box<[T]>, etc.)
                                        let is_smart_ptr =
                                            matches!(info.field.shape().def, Def::Pointer(_));
                                        if is_smart_ptr {
                                            wip = wip.begin_smart_ptr()?;
                                        }
                                        wip = wip.begin_list()?;
                                        entry.insert(SeqState::List { is_smart_ptr });
                                    } else {
                                        // Array
                                        wip = wip.begin_array()?;
                                        entry.insert(SeqState::Array { next_idx: 0 });
                                    }
                                }

                                // Add item to sequence
                                if info.is_list {
                                    trace!(idx = info.idx, field_name = %info.field.name, "adding item to flat list");
                                    wip = wip.begin_list_item()?;
                                    wip = self.deserialize_into(wip)?;
                                    wip = wip.end()?;
                                } else {
                                    // Array: use begin_nth_field with the current index
                                    let state = started_seqs.get_mut(&info.idx).unwrap();
                                    if let SeqState::Array { next_idx } = state {
                                        trace!(idx = info.idx, field_name = %info.field.name, item_idx = *next_idx, "adding item to flat array");
                                        wip = wip.begin_nth_field(*next_idx)?;
                                        wip = self.deserialize_into(wip)?;
                                        wip = wip.end()?;
                                        *next_idx += 1;
                                    }
                                }
                            } else {
                                trace!(idx = info.idx, field_name = %info.field.name, "matched scalar element field");
                                wip = wip.begin_nth_field(info.idx)?;
                                wip = self.deserialize_into(wip)?;
                                wip = wip.end()?;
                            }
                        } else {
                            trace!(tag = %tag, "skipping unknown element");
                            self.parser
                                .skip_node()
                                .map_err(DomDeserializeError::Parser)?;
                        }
                    } else {
                        trace!("adding element to elements collection");
                        wip = wip.begin_list_item()?;
                        wip = self.deserialize_into(wip)?;
                        wip = wip.end()?;
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

        // Close all started flat sequences (lists and arrays)
        for state in started_seqs.values() {
            match state {
                SeqState::List { is_smart_ptr } => {
                    trace!(path = %wip.path(), is_smart_ptr, "ending flat list field");
                    if *is_smart_ptr {
                        wip = wip.end()?; // end smart pointer (converts builder to Arc/Box/etc)
                    }
                    wip = wip.end()?; // end field
                }
                SeqState::Array { .. } => {
                    trace!(path = %wip.path(), "ending flat array field");
                    wip = wip.end()?; // end field
                }
            }
        }

        if elements_list_started {
            trace!(path = %wip.path(), "ending elements list");
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
                        wip = wip.begin_nth_field(0)?;
                        wip = self.deserialize_into(wip)?;
                        wip = wip.end()?;
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

    fn deserialize_list(
        &mut self,
        mut wip: Partial<'de, BORROW>,
    ) -> Result<Partial<'de, BORROW>, DomDeserializeError<P::Error>> {
        wip = wip.begin_list()?;

        loop {
            let event = self.parser.peek_event_or_eof("child or ChildrenEnd")?;
            if matches!(event, DomEvent::ChildrenEnd) {
                break;
            }

            wip = wip.begin_list_item()?;
            wip = self.deserialize_into(wip)?;
            wip = wip.end()?;
        }

        Ok(wip)
    }

    fn deserialize_option(
        &mut self,
        mut wip: Partial<'de, BORROW>,
    ) -> Result<Partial<'de, BORROW>, DomDeserializeError<P::Error>> {
        let event = self.parser.peek_event_or_eof("value")?;
        if matches!(event, DomEvent::ChildrenEnd | DomEvent::NodeEnd) {
            wip = wip.set_default()?;
        } else {
            wip = wip.begin_some()?;
            wip = self.deserialize_into(wip)?;
            wip = wip.end()?;
        }
        Ok(wip)
    }

    fn deserialize_pointer(
        &mut self,
        wip: Partial<'de, BORROW>,
    ) -> Result<Partial<'de, BORROW>, DomDeserializeError<P::Error>> {
        use facet_dessert::{PointerAction, begin_pointer};

        let (mut wip, action) = begin_pointer(wip)?;

        match action {
            PointerAction::HandleAsScalar => self.deserialize_scalar(wip),
            PointerAction::SliceBuilder => {
                wip = self.deserialize_list(wip)?;
                Ok(wip.end()?)
            }
            PointerAction::SizedPointee => {
                wip = self.deserialize_into(wip)?;
                Ok(wip.end()?)
            }
        }
    }

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
