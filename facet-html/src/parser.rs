//! Streaming DomParser implementation for HTML using html5gum.

extern crate alloc;

use alloc::borrow::Cow;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use facet_dom::{DomEvent, DomParser};
use html5gum::{State, Token, Tokenizer};

/// HTML parsing error.
#[derive(Debug, Clone)]
pub enum HtmlError {
    /// General parse error with message.
    ParseError(String),
    /// Unexpected end of input.
    UnexpectedEof,
    /// Invalid UTF-8 in input.
    InvalidUtf8,
}

impl fmt::Display for HtmlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HtmlError::ParseError(msg) => write!(f, "HTML parse error: {}", msg),
            HtmlError::UnexpectedEof => write!(f, "Unexpected end of HTML"),
            HtmlError::InvalidUtf8 => write!(f, "Invalid UTF-8 in HTML"),
        }
    }
}

impl std::error::Error for HtmlError {}

/// HTML void elements that cannot have children.
const VOID_ELEMENTS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];

fn is_void_element(name: &str) -> bool {
    VOID_ELEMENTS.contains(&name.to_ascii_lowercase().as_str())
}

/// A child node in the DOM tree - either text or an element.
#[derive(Debug, Clone)]
enum ChildNode {
    /// A text node.
    Text(String),
    /// An element node.
    Element(Element),
}

/// An HTML element in the DOM tree.
#[derive(Debug, Clone)]
struct Element {
    /// Tag name (lowercase).
    name: String,
    /// Attributes as (name, value) pairs.
    attributes: Vec<(String, String)>,
    /// Child nodes (text and elements interleaved, preserving order).
    children: Vec<ChildNode>,
}

impl Element {
    const fn new(name: String, attributes: Vec<(String, String)>) -> Self {
        Self {
            name,
            attributes,
            children: Vec::new(),
        }
    }

    /// Push text content, preserving all whitespace exactly as in the source HTML.
    fn push_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }

        // Append to existing text node or create a new one
        if let Some(ChildNode::Text(existing)) = self.children.last_mut() {
            existing.push_str(text);
        } else {
            self.children.push(ChildNode::Text(text.to_string()));
        }
    }

    fn push_child(&mut self, child: Element) {
        self.children.push(ChildNode::Element(child));
    }
}

/// Streaming HTML parser implementing `DomParser`.
///
/// This parser builds a tree from the HTML input, then emits DomEvents
/// by walking the tree structure.
pub struct HtmlParser<'de> {
    /// Pre-built events from parsing
    events: Vec<DomEvent<'de>>,
    /// Current index into events
    idx: usize,
    /// Peeked event
    peeked: Option<DomEvent<'de>>,
    /// Pending error from construction
    pending_error: Option<HtmlError>,
    /// Depth tracking for skip_node
    depth: usize,
}

impl<'de> HtmlParser<'de> {
    /// Create a new HTML parser from input bytes.
    pub fn new(input: &'de [u8]) -> Self {
        match build_dom_events(input) {
            Ok(events) => Self {
                events,
                idx: 0,
                peeked: None,
                pending_error: None,
                depth: 0,
            },
            Err(err) => Self {
                events: Vec::new(),
                idx: 0,
                peeked: None,
                pending_error: Some(err),
                depth: 0,
            },
        }
    }

    fn read_next(&mut self) -> Result<Option<DomEvent<'de>>, HtmlError> {
        if let Some(err) = &self.pending_error {
            return Err(err.clone());
        }
        if self.idx >= self.events.len() {
            return Ok(None);
        }
        let event = self.events[self.idx].clone();
        self.idx += 1;

        // Track depth for skip_node
        match &event {
            DomEvent::NodeStart { .. } => self.depth += 1,
            DomEvent::NodeEnd => self.depth = self.depth.saturating_sub(1),
            _ => {}
        }

        Ok(Some(event))
    }
}

impl<'de> DomParser<'de> for HtmlParser<'de> {
    type Error = HtmlError;

    fn next_event(&mut self) -> Result<Option<DomEvent<'de>>, Self::Error> {
        if let Some(event) = self.peeked.take() {
            // Depth was already tracked when this was read into peeked via read_next
            return Ok(Some(event));
        }
        self.read_next()
    }

    fn peek_event(&mut self) -> Result<Option<&DomEvent<'de>>, Self::Error> {
        if self.peeked.is_none() {
            // Use read_next to properly track depth when the event is first read
            self.peeked = self.read_next()?;
        }
        Ok(self.peeked.as_ref())
    }

    fn skip_node(&mut self) -> Result<(), Self::Error> {
        let start_depth = self.depth;

        loop {
            let event = self.next_event()?;
            match event {
                Some(DomEvent::NodeEnd) => {
                    if self.depth < start_depth {
                        break;
                    }
                }
                None => break,
                _ => {}
            }
        }

        Ok(())
    }

    fn current_span(&self) -> Option<facet_reflect::Span> {
        None
    }

    fn is_lenient(&self) -> bool {
        true
    }
}

/// Build DomEvents from HTML input.
fn build_dom_events<'de>(input: &'de [u8]) -> Result<Vec<DomEvent<'de>>, HtmlError> {
    let input_str = core::str::from_utf8(input).map_err(|_| HtmlError::InvalidUtf8)?;

    let mut tokenizer = Tokenizer::new(input_str);
    let mut stack: Vec<Element> = Vec::new();
    let mut roots: Vec<Element> = Vec::new();
    let mut doctype_name: Option<String> = None;

    while let Some(token_result) = tokenizer.next() {
        let token = token_result.map_err(|_| HtmlError::ParseError("tokenizer error".into()))?;

        match token {
            Token::StartTag(tag) => {
                let name = String::from_utf8_lossy(&tag.name).to_ascii_lowercase();
                let attributes: Vec<(String, String)> = tag
                    .attributes
                    .iter()
                    .map(|(k, v)| {
                        (
                            String::from_utf8_lossy(k).into_owned(),
                            String::from_utf8_lossy(v).into_owned(),
                        )
                    })
                    .collect();

                let elem = Element::new(name.clone(), attributes);

                if tag.self_closing || is_void_element(&name) {
                    // Self-closing or void element - attach immediately
                    attach_element(&mut stack, elem, &mut roots);
                } else {
                    // Switch tokenizer state for raw text elements per HTML5 spec
                    match name.as_str() {
                        "script" | "style" => tokenizer.set_state(State::ScriptData),
                        "textarea" | "title" => tokenizer.set_state(State::RcData),
                        _ => {}
                    }
                    // Push onto stack to collect children
                    stack.push(elem);
                }
            }
            Token::EndTag(tag) => {
                let end_name = String::from_utf8_lossy(&tag.name).to_ascii_lowercase();

                // Find if there's a matching start tag on the stack
                let matching_idx = stack.iter().rposition(|elem| elem.name == end_name);

                if let Some(idx) = matching_idx {
                    // Pop elements from the top down to (and including) the matching element
                    while stack.len() > idx {
                        let elem = stack.pop().unwrap();
                        attach_element(&mut stack, elem, &mut roots);
                    }
                }
                // If no matching start tag found, ignore the stray end tag
            }
            Token::String(text) => {
                let text_str = String::from_utf8_lossy(&text);
                if let Some(current) = stack.last_mut() {
                    current.push_text(&text_str);
                }
                // Text outside elements is ignored
            }
            Token::Doctype(doctype) => {
                let name = String::from_utf8_lossy(&doctype.name).to_ascii_lowercase();
                if !name.is_empty() {
                    doctype_name = Some(name);
                }
            }
            Token::Comment(_) | Token::Error(_) => {
                // Ignore comments and errors
            }
        }
    }

    // Close any remaining open elements
    while let Some(elem) = stack.pop() {
        attach_element(&mut stack, elem, &mut roots);
    }

    // If we have a doctype and the root is an html element, inject it as a pseudo-attribute
    if let Some(ref doctype) = doctype_name
        && roots.len() == 1
        && roots[0].name == "html"
    {
        roots[0]
            .attributes
            .insert(0, ("doctype".to_string(), doctype.clone()));
    }

    // Generate DomEvents from the tree
    let mut events = Vec::new();

    if roots.is_empty() {
        // Empty document - emit empty element
        events.push(DomEvent::NodeStart {
            tag: Cow::Borrowed("div"),
            namespace: None,
        });
        events.push(DomEvent::ChildrenStart);
        events.push(DomEvent::ChildrenEnd);
        events.push(DomEvent::NodeEnd);
    } else if roots.len() == 1 {
        // Single root element
        emit_element_events(&roots[0], &mut events);
    } else {
        // Multiple roots - wrap in a virtual document element
        events.push(DomEvent::NodeStart {
            tag: Cow::Borrowed("document"),
            namespace: None,
        });
        events.push(DomEvent::ChildrenStart);
        for root in &roots {
            emit_element_events(root, &mut events);
        }
        events.push(DomEvent::ChildrenEnd);
        events.push(DomEvent::NodeEnd);
    }

    Ok(events)
}

/// Attach an element to its parent or to the roots list.
fn attach_element(stack: &mut [Element], elem: Element, roots: &mut Vec<Element>) {
    if let Some(parent) = stack.last_mut() {
        parent.push_child(elem);
    } else {
        roots.push(elem);
    }
}

/// Emit DomEvents from an Element.
fn emit_element_events<'de>(elem: &Element, events: &mut Vec<DomEvent<'de>>) {
    // NodeStart with tag name
    // Note: The tag name is captured by the DomDeserializer from NodeStart
    // and set on fields marked with html::tag - we don't need to emit it as an attribute
    events.push(DomEvent::NodeStart {
        tag: Cow::Owned(elem.name.clone()),
        namespace: None,
    });

    // Emit attributes
    for (name, value) in &elem.attributes {
        events.push(DomEvent::Attribute {
            name: Cow::Owned(name.clone()),
            value: Cow::Owned(value.clone()),
            namespace: None,
        });
    }

    // ChildrenStart
    events.push(DomEvent::ChildrenStart);

    // Emit children in order
    for child in &elem.children {
        match child {
            ChildNode::Text(text) => {
                events.push(DomEvent::Text(Cow::Owned(text.clone())));
            }
            ChildNode::Element(child_elem) => {
                emit_element_events(child_elem, events);
            }
        }
    }

    // ChildrenEnd and NodeEnd
    events.push(DomEvent::ChildrenEnd);
    events.push(DomEvent::NodeEnd);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_element() {
        let html = b"<div>hello</div>";
        let events = build_dom_events(html).unwrap();

        assert_eq!(
            events,
            vec![
                DomEvent::NodeStart {
                    tag: Cow::Borrowed("div"),
                    namespace: None
                },
                DomEvent::ChildrenStart,
                DomEvent::Text(Cow::Borrowed("hello")),
                DomEvent::ChildrenEnd,
                DomEvent::NodeEnd,
            ]
        );
    }

    #[test]
    fn test_element_with_attribute() {
        let html = b"<div class=\"foo\">hello</div>";
        let events = build_dom_events(html).unwrap();

        assert_eq!(
            events,
            vec![
                DomEvent::NodeStart {
                    tag: Cow::Borrowed("div"),
                    namespace: None
                },
                DomEvent::Attribute {
                    name: Cow::Borrowed("class"),
                    value: Cow::Borrowed("foo"),
                    namespace: None
                },
                DomEvent::ChildrenStart,
                DomEvent::Text(Cow::Borrowed("hello")),
                DomEvent::ChildrenEnd,
                DomEvent::NodeEnd,
            ]
        );
    }

    #[test]
    fn test_nested_elements() {
        let html = b"<div><span>inner</span></div>";
        let events = build_dom_events(html).unwrap();

        assert_eq!(
            events,
            vec![
                DomEvent::NodeStart {
                    tag: Cow::Borrowed("div"),
                    namespace: None
                },
                DomEvent::ChildrenStart,
                DomEvent::NodeStart {
                    tag: Cow::Borrowed("span"),
                    namespace: None
                },
                DomEvent::ChildrenStart,
                DomEvent::Text(Cow::Borrowed("inner")),
                DomEvent::ChildrenEnd,
                DomEvent::NodeEnd,
                DomEvent::ChildrenEnd,
                DomEvent::NodeEnd,
            ]
        );
    }

    #[test]
    fn test_void_element() {
        let html = b"<div><br><span>after</span></div>";
        let events = build_dom_events(html).unwrap();
        // br is a void element, should be parsed correctly
        assert!(!events.is_empty());
        // Check that br appears as a complete node
        assert!(
            events
                .iter()
                .any(|e| matches!(e, DomEvent::NodeStart { tag, .. } if tag == "br"))
        );
    }

    #[test]
    fn test_mixed_content() {
        let html = b"<p>Hello <strong>world</strong> there</p>";
        let events = build_dom_events(html).unwrap();

        // Should have text, element, text in children
        assert_eq!(
            events,
            vec![
                DomEvent::NodeStart {
                    tag: Cow::Borrowed("p"),
                    namespace: None
                },
                DomEvent::ChildrenStart,
                DomEvent::Text(Cow::Borrowed("Hello ")),
                DomEvent::NodeStart {
                    tag: Cow::Borrowed("strong"),
                    namespace: None
                },
                DomEvent::ChildrenStart,
                DomEvent::Text(Cow::Borrowed("world")),
                DomEvent::ChildrenEnd,
                DomEvent::NodeEnd,
                DomEvent::Text(Cow::Borrowed(" there")),
                DomEvent::ChildrenEnd,
                DomEvent::NodeEnd,
            ]
        );
    }
}
