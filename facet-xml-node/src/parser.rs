//! DomParser implementation for walking Element trees.

use std::borrow::Cow;
use std::fmt;

use facet_dom::{DomDeserializer, DomEvent, DomParser};

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
