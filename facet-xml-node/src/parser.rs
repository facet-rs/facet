//! DomParser implementation for walking Element trees.

use std::borrow::Cow;
use std::fmt;

use facet_dom::{DomDeserializer, DomEvent, DomParser, DomSerializer, WriteScalar};

use crate::{Content, Element};

#[derive(Debug)]
pub struct ElementParseError;

impl fmt::Display for ElementParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "element parse error")
    }
}

impl std::error::Error for ElementParseError {}

/// Deserialize from an Element tree into a typed value.
pub fn from_element<T>(
    element: &Element,
) -> Result<T, facet_dom::DomDeserializeError<ElementParseError>>
where
    T: facet_core::Facet<'static>,
{
    let parser = ElementParser::new(element);
    let mut de = DomDeserializer::new_owned(parser);
    de.deserialize()
}

/// Parser that walks an Element tree and emits DomEvents.
pub struct ElementParser<'a> {
    /// Stack of frames - each frame is an element being processed
    stack: Vec<Frame<'a>>,
    /// Peeked event
    peeked: Option<DomEvent<'static>>,
    /// Current depth for skip_node
    depth: usize,
}

struct Frame<'a> {
    element: &'a Element,
    state: FrameState,
    attr_iter: std::collections::hash_map::Iter<'a, String, String>,
    child_idx: usize,
}

#[derive(Clone, Copy, PartialEq)]
enum FrameState {
    Start,
    Attrs,
    ChildrenStart,
    Children,
    ChildrenEnd,
    NodeEnd,
    Done,
}

impl<'a> ElementParser<'a> {
    pub fn new(root: &'a Element) -> Self {
        Self {
            stack: vec![Frame {
                element: root,
                state: FrameState::Start,
                attr_iter: root.attrs.iter(),
                child_idx: 0,
            }],
            peeked: None,
            depth: 0,
        }
    }

    fn read_next(&mut self) -> Result<Option<DomEvent<'static>>, ElementParseError> {
        loop {
            let frame = match self.stack.last_mut() {
                Some(f) => f,
                None => return Ok(None),
            };

            match frame.state {
                FrameState::Start => {
                    self.depth += 1;
                    frame.state = FrameState::Attrs;
                    return Ok(Some(DomEvent::NodeStart {
                        tag: Cow::Owned(frame.element.tag.clone()),
                        namespace: None,
                    }));
                }
                FrameState::Attrs => {
                    if let Some((name, value)) = frame.attr_iter.next() {
                        return Ok(Some(DomEvent::Attribute {
                            name: Cow::Owned(name.clone()),
                            value: Cow::Owned(value.clone()),
                            namespace: None,
                        }));
                    }
                    frame.state = FrameState::ChildrenStart;
                }
                FrameState::ChildrenStart => {
                    frame.state = FrameState::Children;
                    return Ok(Some(DomEvent::ChildrenStart));
                }
                FrameState::Children => {
                    if frame.child_idx < frame.element.children.len() {
                        let child = &frame.element.children[frame.child_idx];
                        frame.child_idx += 1;

                        match child {
                            Content::Text(t) => {
                                return Ok(Some(DomEvent::Text(Cow::Owned(t.clone()))));
                            }
                            Content::Element(e) => {
                                // Push new frame for child element
                                self.stack.push(Frame {
                                    element: e,
                                    state: FrameState::Start,
                                    attr_iter: e.attrs.iter(),
                                    child_idx: 0,
                                });
                                // Loop to process the new frame
                            }
                        }
                    } else {
                        frame.state = FrameState::ChildrenEnd;
                    }
                }
                FrameState::ChildrenEnd => {
                    frame.state = FrameState::NodeEnd;
                    return Ok(Some(DomEvent::ChildrenEnd));
                }
                FrameState::NodeEnd => {
                    frame.state = FrameState::Done;
                    self.depth -= 1;
                    return Ok(Some(DomEvent::NodeEnd));
                }
                FrameState::Done => {
                    self.stack.pop();
                }
            }
        }
    }
}

impl<'a> DomParser<'static> for ElementParser<'a> {
    type Error = ElementParseError;

    fn next_event(&mut self) -> Result<Option<DomEvent<'static>>, Self::Error> {
        if let Some(event) = self.peeked.take() {
            return Ok(Some(event));
        }
        self.read_next()
    }

    fn peek_event(&mut self) -> Result<Option<&DomEvent<'static>>, Self::Error> {
        if self.peeked.is_none() {
            self.peeked = self.read_next()?;
        }
        Ok(self.peeked.as_ref())
    }

    fn skip_node(&mut self) -> Result<(), Self::Error> {
        let start_depth = self.depth;
        loop {
            match self.next_event()? {
                Some(DomEvent::NodeEnd) if self.depth < start_depth => break,
                None => break,
                _ => {}
            }
        }
        Ok(())
    }

    fn format_namespace(&self) -> Option<&'static str> {
        Some("xml")
    }
}

#[derive(Debug)]
pub struct ElementSerializeError;

impl fmt::Display for ElementSerializeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "element serialize error")
    }
}

impl std::error::Error for ElementSerializeError {}

/// Serialize a typed value into an Element tree.
pub fn to_element<T>(
    value: &T,
) -> Result<Element, facet_dom::DomSerializeError<ElementSerializeError>>
where
    T: facet_core::Facet<'static>,
{
    let mut serializer = ElementSerializer::default();
    let peek = facet_reflect::Peek::new(value);
    facet_dom::serialize(&mut serializer, peek)?;
    serializer.finish()
}

/// Serializer that builds an Element tree from DomSerializer callbacks.
#[derive(Default)]
pub struct ElementSerializer {
    /// Stack of elements being built
    stack: Vec<Element>,
    /// The root element (after serialization completes)
    root: Option<Element>,
    /// Whether the current field should be serialized as an attribute
    is_attribute: bool,
    /// Whether the current field should be serialized as text content
    is_text: bool,
    /// Whether the current field is an xml::elements list
    is_elements: bool,
    /// Whether the current field is a tag field
    is_tag: bool,
    /// Whether the current field is a doctype field
    is_doctype: bool,
}

impl ElementSerializer {
    /// Finish serialization and return the root element.
    fn finish(mut self) -> Result<Element, facet_dom::DomSerializeError<ElementSerializeError>> {
        // If we have a root, return it
        if let Some(root) = self.root.take() {
            return Ok(root);
        }

        // Otherwise, pop the last element from the stack
        if self.stack.len() == 1 {
            Ok(self.stack.pop().unwrap())
        } else {
            Err(facet_dom::DomSerializeError::Backend(ElementSerializeError))
        }
    }
}

impl DomSerializer for ElementSerializer {
    type Error = ElementSerializeError;

    fn element_start(&mut self, tag: &str, _namespace: Option<&str>) -> Result<(), Self::Error> {
        self.stack.push(Element::new(tag));
        Ok(())
    }

    fn attribute(
        &mut self,
        name: &str,
        value: facet_reflect::Peek<'_, '_>,
        _namespace: Option<&str>,
    ) -> Result<(), Self::Error> {
        // Convert the value to a string using format_scalar (before borrowing elem)
        if let Some(value_str) = self.format_scalar(value) {
            let elem = self.stack.last_mut().ok_or(ElementSerializeError)?;
            elem.attrs.insert(name.to_string(), value_str);
        }
        Ok(())
    }

    fn children_start(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn children_end(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn element_end(&mut self, _tag: &str) -> Result<(), Self::Error> {
        let elem = self.stack.pop().ok_or(ElementSerializeError)?;

        if let Some(parent) = self.stack.last_mut() {
            parent.children.push(Content::Element(elem));
        } else {
            self.root = Some(elem);
        }
        Ok(())
    }

    fn text(&mut self, content: &str) -> Result<(), Self::Error> {
        if let Some(elem) = self.stack.last_mut() {
            elem.children.push(Content::Text(content.to_string()));
        } else {
            return Err(ElementSerializeError);
        }
        Ok(())
    }

    fn format_namespace(&self) -> Option<&'static str> {
        Some("xml")
    }

    fn field_metadata(&mut self, field: &facet_reflect::FieldItem) -> Result<(), Self::Error> {
        let Some(field_def) = field.field else {
            // For flattened map entries, treat them as attributes
            self.is_attribute = true;
            self.is_text = false;
            self.is_elements = false;
            self.is_tag = false;
            self.is_doctype = false;
            return Ok(());
        };

        // Check field attributes
        self.is_attribute = field_def.get_attr(Some("xml"), "attribute").is_some();
        self.is_text = field_def.get_attr(Some("xml"), "text").is_some();
        self.is_elements = field_def.get_attr(Some("xml"), "elements").is_some();
        self.is_tag = field_def.get_attr(Some("xml"), "tag").is_some();
        self.is_doctype = field_def.get_attr(Some("xml"), "doctype").is_some();
        Ok(())
    }

    fn is_attribute_field(&self) -> bool {
        self.is_attribute
    }

    fn is_text_field(&self) -> bool {
        self.is_text
    }

    fn is_elements_field(&self) -> bool {
        self.is_elements
    }

    fn is_tag_field(&self) -> bool {
        self.is_tag
    }

    fn is_doctype_field(&self) -> bool {
        self.is_doctype
    }

    fn clear_field_state(&mut self) {
        self.is_attribute = false;
        self.is_text = false;
        self.is_elements = false;
        self.is_tag = false;
        self.is_doctype = false;
    }
}
