//! Tree-based deserializer for DOM documents.

use std::borrow::Cow;

use facet_core::{Def, Type, UserType};
use facet_reflect::Partial;

use crate::error::DomDeserializeError;
use crate::tracing_macros::{trace, trace_span};
use crate::{DomEvent, DomParser, DomParserExt};

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
                Def::Scalar | Def::Pointer(_) => self.deserialize_scalar(wip),
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
                    let (name, value) = self.parser.expect_attribute()?;
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

        let mut elements_list_started = false;
        if let Some(info) = &field_map.elements_field {
            trace!(idx = info.idx, field_name = %info.field.name, "beginning elements list");
            wip = wip
                .begin_nth_field(info.idx)
                .map_err(DomDeserializeError::Reflect)?;
            wip = wip.begin_list().map_err(DomDeserializeError::Reflect)?;
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
                        if let Some(info) = field_map.find_element(&tag) {
                            if info.is_list {
                                trace!(idx = info.idx, field_name = %info.field.name, "matched wrapped list field");

                                self.parser.expect_node_start()?;

                                trace!(path = %wip.path(), "begin_nth_field for wrapped list");
                                wip = wip
                                    .begin_nth_field(info.idx)
                                    .map_err(DomDeserializeError::Reflect)?;
                                trace!(path = %wip.path(), "begin_list");
                                wip = wip.begin_list().map_err(DomDeserializeError::Reflect)?;

                                let wrapper_event =
                                    self.parser.peek_event_or_eof("ChildrenStart or NodeEnd")?;
                                if matches!(wrapper_event, DomEvent::NodeEnd) {
                                    trace!("empty wrapper element");
                                    self.parser.expect_node_end()?;
                                } else {
                                    self.parser.expect_children_start()?;

                                    let item_name =
                                        info.item_element_name.as_deref().unwrap_or("item");
                                    trace!(item_name, "processing wrapped list items");

                                    loop {
                                        let item_event =
                                            self.parser.peek_event_or_eof("item or ChildrenEnd")?;
                                        match item_event {
                                            DomEvent::ChildrenEnd => {
                                                trace!("wrapped list items done");
                                                break;
                                            }
                                            DomEvent::NodeStart { tag: item_tag, .. } => {
                                                let item_tag = item_tag.clone();
                                                if item_tag == item_name {
                                                    trace!(item_tag = %item_tag, "matched list item");
                                                    wip = wip
                                                        .begin_list_item()
                                                        .map_err(DomDeserializeError::Reflect)?;
                                                    wip = self.deserialize_into(wip)?;
                                                    wip = wip
                                                        .end()
                                                        .map_err(DomDeserializeError::Reflect)?;
                                                } else {
                                                    trace!(item_tag = %item_tag, expected = item_name, "skipping non-matching element in list wrapper");
                                                    self.parser
                                                        .skip_node()
                                                        .map_err(DomDeserializeError::Parser)?;
                                                }
                                            }
                                            DomEvent::Text(_) => {
                                                let _text = self.parser.expect_text()?;
                                            }
                                            DomEvent::Comment(_) => {
                                                let _comment = self.parser.expect_comment()?;
                                            }
                                            other => {
                                                return Err(DomDeserializeError::TypeMismatch {
                                                    expected: "list item or ChildrenEnd",
                                                    got: format!("{other:?}"),
                                                });
                                            }
                                        }
                                    }

                                    self.parser.expect_children_end()?;
                                    self.parser.expect_node_end()?;
                                }

                                trace!(path = %wip.path(), "ending wrapped list field");
                                wip = wip.end().map_err(DomDeserializeError::Reflect)?;
                                trace!(path = %wip.path(), "after ending wrapped list field");
                            } else {
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

        if elements_list_started {
            trace!(path = %wip.path(), "ending elements list");
            wip = wip.end().map_err(DomDeserializeError::Reflect)?;
        }

        if let Some(info) = &field_map.text_field {
            if !text_content.is_empty() || !elements_list_started {
                trace!(idx = info.idx, field_name = %info.field.name, text_len = text_content.len(), "setting text field");
                wip = wip
                    .begin_nth_field(info.idx)
                    .map_err(DomDeserializeError::Reflect)?;
                wip = self.set_string_value(wip, Cow::Owned(text_content))?;
                wip = wip.end().map_err(DomDeserializeError::Reflect)?;
            }
        }

        self.parser.expect_children_end()?;
        self.parser.expect_node_end()?;

        trace!(tag = %tag, "struct deserialization complete");
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

                wip = wip
                    .select_nth_variant(variant_idx)
                    .map_err(DomDeserializeError::Reflect)?;
                wip = self.deserialize_into(wip)?;
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

        wip = wip
            .select_nth_variant(text_variant_idx)
            .map_err(DomDeserializeError::Reflect)?;
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
                            let (_name, _value) = self.parser.expect_attribute()?;
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
        wip = wip.begin_list().map_err(DomDeserializeError::Reflect)?;

        loop {
            let event = self.parser.peek_event_or_eof("child or ChildrenEnd")?;
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

    fn deserialize_option(
        &mut self,
        mut wip: Partial<'de, BORROW>,
    ) -> Result<Partial<'de, BORROW>, DomDeserializeError<P::Error>> {
        let event = self.parser.peek_event_or_eof("value")?;
        if matches!(event, DomEvent::ChildrenEnd | DomEvent::NodeEnd) {
            wip = wip.set_default().map_err(DomDeserializeError::Reflect)?;
        } else {
            wip = wip.begin_some().map_err(DomDeserializeError::Reflect)?;
            wip = self.deserialize_into(wip)?;
            wip = wip.end().map_err(DomDeserializeError::Reflect)?;
        }
        Ok(wip)
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
