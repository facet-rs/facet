extern crate alloc;

use alloc::borrow::Cow;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use facet_format::{
    ContainerKind, FieldEvidence, FieldKey, FieldLocationHint, FormatParser, ParseEvent,
    ProbeStream, ScalarValue,
};
use html5gum::{State, Token, Tokenizer};

/// HTML parser implementing the `FormatParser` trait.
///
/// This parser builds a tree of HTML elements from the tokenizer output,
/// then emits ParseEvents from the tree structure.
pub struct HtmlParser<'de> {
    events: Vec<ParseEvent<'de>>,
    idx: usize,
    pending_error: Option<HtmlError>,
}

impl<'de> HtmlParser<'de> {
    /// Create a new HTML parser from input bytes.
    pub fn new(input: &'de [u8]) -> Self {
        match build_events(input) {
            Ok(events) => Self {
                events,
                idx: 0,
                pending_error: None,
            },
            Err(err) => Self {
                events: Vec::new(),
                idx: 0,
                pending_error: Some(err),
            },
        }
    }
}

/// Error type for HTML parsing.
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

impl<'de> FormatParser<'de> for HtmlParser<'de> {
    type Error = HtmlError;
    type Probe<'a>
        = HtmlProbe<'de>
    where
        Self: 'a;

    fn next_event(&mut self) -> Result<Option<ParseEvent<'de>>, Self::Error> {
        if let Some(err) = &self.pending_error {
            return Err(err.clone());
        }
        if self.idx >= self.events.len() {
            return Ok(None);
        }
        let event = self.events[self.idx].clone();
        self.idx += 1;
        Ok(Some(event))
    }

    fn peek_event(&mut self) -> Result<Option<ParseEvent<'de>>, Self::Error> {
        if let Some(err) = &self.pending_error {
            return Err(err.clone());
        }
        Ok(self.events.get(self.idx).cloned())
    }

    fn skip_value(&mut self) -> Result<(), Self::Error> {
        let mut struct_depth = 0usize;
        let mut pending_field_value = false;

        loop {
            let event = self.next_event()?.ok_or(HtmlError::UnexpectedEof)?;
            match event {
                ParseEvent::StructStart(_) | ParseEvent::SequenceStart(_) => {
                    pending_field_value = false;
                    struct_depth += 1;
                }
                ParseEvent::StructEnd | ParseEvent::SequenceEnd => {
                    if struct_depth == 0 {
                        break;
                    } else {
                        struct_depth -= 1;
                        if struct_depth == 0 && !pending_field_value {
                            break;
                        }
                    }
                }
                ParseEvent::Scalar(_) | ParseEvent::VariantTag(_) => {
                    if struct_depth == 0 && !pending_field_value {
                        break;
                    }
                    pending_field_value = false;
                }
                ParseEvent::FieldKey(_) | ParseEvent::OrderedField => {
                    pending_field_value = true;
                }
            }
        }
        Ok(())
    }

    fn begin_probe(&mut self) -> Result<Self::Probe<'_>, Self::Error> {
        let evidence = self.build_probe();
        Ok(HtmlProbe { evidence, idx: 0 })
    }
}

impl<'de> HtmlParser<'de> {
    /// Build field evidence by looking ahead at remaining events.
    fn build_probe(&self) -> Vec<FieldEvidence<'de>> {
        let mut evidence = Vec::new();

        if self.idx >= self.events.len() {
            return evidence;
        }

        if !matches!(
            self.events.get(self.idx),
            Some(ParseEvent::StructStart(ContainerKind::Element))
        ) {
            return evidence;
        }

        let mut i = self.idx + 1;
        let mut depth = 0usize;

        while i < self.events.len() {
            match &self.events[i] {
                ParseEvent::StructStart(_) | ParseEvent::SequenceStart(_) => {
                    depth += 1;
                    i += 1;
                }
                ParseEvent::StructEnd | ParseEvent::SequenceEnd => {
                    if depth == 0 {
                        break;
                    }
                    depth -= 1;
                    i += 1;
                }
                ParseEvent::FieldKey(key) if depth == 0 => {
                    let scalar_value = if let Some(next_event) = self.events.get(i + 1) {
                        match next_event {
                            ParseEvent::Scalar(sv) => Some(sv.clone()),
                            _ => None,
                        }
                    } else {
                        None
                    };

                    if let Some(sv) = scalar_value {
                        evidence.push(FieldEvidence::with_scalar_value(
                            key.name.clone(),
                            key.location,
                            None,
                            sv,
                            key.namespace.clone(),
                        ));
                    } else {
                        evidence.push(FieldEvidence::new(
                            key.name.clone(),
                            key.location,
                            None,
                            key.namespace.clone(),
                        ));
                    }
                    i += 1;
                }
                _ => {
                    i += 1;
                }
            }
        }

        evidence
    }
}

/// Probe stream for HTML evidence collection.
pub struct HtmlProbe<'de> {
    evidence: Vec<FieldEvidence<'de>>,
    idx: usize,
}

impl<'de> ProbeStream<'de> for HtmlProbe<'de> {
    type Error = HtmlError;

    fn next(&mut self) -> Result<Option<FieldEvidence<'de>>, Self::Error> {
        if self.idx >= self.evidence.len() {
            Ok(None)
        } else {
            let ev = self.evidence[self.idx].clone();
            self.idx += 1;
            Ok(Some(ev))
        }
    }
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
    ///
    /// Browsers preserve all text nodes (including whitespace-only ones) in the DOM.
    /// CSS controls rendering via `white-space` property. We match browser behavior
    /// by keeping everything - consumers can decide what to do with whitespace.
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

/// HTML void elements that cannot have children.
const VOID_ELEMENTS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];

fn is_void_element(name: &str) -> bool {
    VOID_ELEMENTS.contains(&name.to_ascii_lowercase().as_str())
}

/// Build ParseEvents from HTML input.
fn build_events<'de>(input: &'de [u8]) -> Result<Vec<ParseEvent<'de>>, HtmlError> {
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
                    // https://html.spec.whatwg.org/multipage/parsing.html#parsing-html-fragments
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
                    // Elements above the match are implicitly closed (HTML error recovery)
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
                // Capture the DOCTYPE name (e.g., "html" for <!DOCTYPE html>)
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
        // Insert doctype as the first attribute
        roots[0]
            .attributes
            .insert(0, ("doctype".to_string(), doctype.clone()));
    }

    // Generate events from the tree
    let mut events = Vec::new();

    if roots.is_empty() {
        // Empty document
        events.push(ParseEvent::StructStart(ContainerKind::Element));
        events.push(ParseEvent::StructEnd);
    } else if roots.len() == 1 {
        // Single root element
        emit_element_events(&roots[0], &mut events);
    } else {
        // Multiple roots - wrap in a virtual document element
        events.push(ParseEvent::StructStart(ContainerKind::Element));
        for root in &roots {
            let key = FieldKey::new(Cow::Owned(root.name.clone()), FieldLocationHint::Child);
            events.push(ParseEvent::FieldKey(key));
            emit_element_events(root, &mut events);
        }
        events.push(ParseEvent::StructEnd);
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

/// Emit ParseEvents from an Element.
fn emit_element_events<'de>(elem: &Element, events: &mut Vec<ParseEvent<'de>>) {
    let has_attrs = !elem.attributes.is_empty();
    let has_children = !elem.children.is_empty();

    // Case 1: No attributes, no children - emit struct with just _tag
    if !has_attrs && !has_children {
        events.push(ParseEvent::StructStart(ContainerKind::Element));
        // Always emit _tag so custom elements can capture the tag name
        let key = FieldKey::new(Cow::Borrowed("_tag"), FieldLocationHint::Tag);
        events.push(ParseEvent::FieldKey(key));
        events.push(ParseEvent::Scalar(ScalarValue::Str(Cow::Owned(
            elem.name.clone(),
        ))));
        events.push(ParseEvent::StructEnd);
        return;
    }

    // Case 2: Has attributes or children - emit as struct with _text children
    // The deserializer handles grouping repeated field names into sequences.
    events.push(ParseEvent::StructStart(ContainerKind::Element));

    // Always emit _tag first so custom elements can capture the tag name
    let key = FieldKey::new(Cow::Borrowed("_tag"), FieldLocationHint::Tag);
    events.push(ParseEvent::FieldKey(key));
    events.push(ParseEvent::Scalar(ScalarValue::Str(Cow::Owned(
        elem.name.clone(),
    ))));

    // Emit attributes as fields
    for (name, value) in &elem.attributes {
        let key = FieldKey::new(Cow::Owned(name.clone()), FieldLocationHint::Attribute);
        events.push(ParseEvent::FieldKey(key));
        events.push(ParseEvent::Scalar(ScalarValue::Str(Cow::Owned(
            value.clone(),
        ))));
    }

    // Emit children in order (preserving interleaved text/element ordering)
    for child in &elem.children {
        match child {
            ChildNode::Text(text) => {
                let key = FieldKey::new(Cow::Borrowed("_text"), FieldLocationHint::Text);
                events.push(ParseEvent::FieldKey(key));
                events.push(ParseEvent::Scalar(ScalarValue::Str(Cow::Owned(
                    text.clone(),
                ))));
            }
            ChildNode::Element(child_elem) => {
                let key = FieldKey::new(
                    Cow::Owned(child_elem.name.clone()),
                    FieldLocationHint::Child,
                );
                events.push(ParseEvent::FieldKey(key));
                emit_element_events(child_elem, events);
            }
        }
    }

    events.push(ParseEvent::StructEnd);
}

#[cfg(test)]
mod tests {
    use super::*;
    use facet::Facet;
    use facet_format::FormatDeserializer;

    #[test_log::test]
    fn test_simple_element() {
        let html = b"<div>hello</div>";
        let events = build_events(html).unwrap();
        // Elements now emit _tag first, then _text for content
        assert_eq!(
            events,
            vec![
                ParseEvent::StructStart(ContainerKind::Element),
                ParseEvent::FieldKey(FieldKey::new(Cow::Borrowed("_tag"), FieldLocationHint::Tag)),
                ParseEvent::Scalar(ScalarValue::Str(Cow::Owned("div".into()))),
                ParseEvent::FieldKey(FieldKey::new(
                    Cow::Borrowed("_text"),
                    FieldLocationHint::Text
                )),
                ParseEvent::Scalar(ScalarValue::Str(Cow::Owned("hello".into()))),
                ParseEvent::StructEnd,
            ]
        );
    }

    #[test_log::test]
    fn test_element_with_attribute() {
        let html = b"<div class=\"foo\">hello</div>";
        let events = build_events(html).unwrap();
        assert_eq!(
            events,
            vec![
                ParseEvent::StructStart(ContainerKind::Element),
                ParseEvent::FieldKey(FieldKey::new(Cow::Borrowed("_tag"), FieldLocationHint::Tag)),
                ParseEvent::Scalar(ScalarValue::Str(Cow::Owned("div".into()))),
                ParseEvent::FieldKey(FieldKey::new(
                    Cow::Owned("class".into()),
                    FieldLocationHint::Attribute
                )),
                ParseEvent::Scalar(ScalarValue::Str(Cow::Owned("foo".into()))),
                ParseEvent::FieldKey(FieldKey::new(
                    Cow::Owned("_text".into()),
                    FieldLocationHint::Text
                )),
                ParseEvent::Scalar(ScalarValue::Str(Cow::Owned("hello".into()))),
                ParseEvent::StructEnd,
            ]
        );
    }

    #[test_log::test]
    fn test_nested_elements() {
        let html = b"<div><span>inner</span></div>";
        let events = build_events(html).unwrap();
        // Nested elements now emit _tag, then child elements with their _tag
        assert_eq!(
            events,
            vec![
                ParseEvent::StructStart(ContainerKind::Element),
                ParseEvent::FieldKey(FieldKey::new(Cow::Borrowed("_tag"), FieldLocationHint::Tag)),
                ParseEvent::Scalar(ScalarValue::Str(Cow::Owned("div".into()))),
                ParseEvent::FieldKey(FieldKey::new(
                    Cow::Owned("span".into()),
                    FieldLocationHint::Child
                )),
                ParseEvent::StructStart(ContainerKind::Element),
                ParseEvent::FieldKey(FieldKey::new(Cow::Borrowed("_tag"), FieldLocationHint::Tag)),
                ParseEvent::Scalar(ScalarValue::Str(Cow::Owned("span".into()))),
                ParseEvent::FieldKey(FieldKey::new(
                    Cow::Borrowed("_text"),
                    FieldLocationHint::Text
                )),
                ParseEvent::Scalar(ScalarValue::Str(Cow::Owned("inner".into()))),
                ParseEvent::StructEnd,
                ParseEvent::StructEnd,
            ]
        );
    }

    #[test_log::test]
    fn test_void_element() {
        let html = b"<div><br><span>after</span></div>";
        let events = build_events(html).unwrap();
        // br is a void element, should be parsed correctly
        assert!(!events.is_empty());
    }

    #[test_log::test]
    fn test_deserialize_simple_struct() {
        #[derive(Debug, Facet, PartialEq)]
        struct Div {
            #[facet(default)]
            class: Option<String>,
        }

        let html = b"<div class=\"container\"></div>";
        let parser = HtmlParser::new(html);
        let mut deserializer = FormatDeserializer::new(parser);
        let result: Div = deserializer.deserialize().unwrap();
        assert_eq!(result.class, Some("container".into()));
    }

    #[test_log::test]
    fn test_deserialize_nested() {
        use facet_xml as xml;

        #[derive(Debug, Facet, PartialEq)]
        struct Outer {
            #[facet(default)]
            inner: Option<Inner>,
        }

        #[derive(Debug, Facet, PartialEq)]
        struct Inner {
            #[facet(default)]
            value: Option<Value>,
        }

        #[derive(Debug, Facet, PartialEq)]
        struct Value {
            #[facet(xml::text, default)]
            text: String,
        }

        let html = b"<outer><inner><value>hello</value></inner></outer>";
        let parser = HtmlParser::new(html);
        let mut deserializer = FormatDeserializer::new(parser);
        let result: Outer = deserializer.deserialize().unwrap();
        assert_eq!(
            result,
            Outer {
                inner: Some(Inner {
                    value: Some(Value {
                        text: "hello".into()
                    })
                })
            }
        );
    }

    #[test_log::test]
    fn test_deserialize_with_text() {
        use facet_xml as xml;

        #[derive(Debug, Facet, PartialEq)]
        struct Article {
            #[facet(default)]
            title: Option<TitleElement>,
            #[facet(default)]
            content: Option<ContentElement>,
        }

        #[derive(Debug, Facet, PartialEq)]
        struct TitleElement {
            #[facet(xml::text, default)]
            text: String,
        }

        #[derive(Debug, Facet, PartialEq)]
        struct ContentElement {
            #[facet(xml::text, default)]
            text: String,
        }

        let html = b"<article><title>Hello</title><content>World</content></article>";
        let parser = HtmlParser::new(html);
        let mut deserializer = FormatDeserializer::new(parser);
        let result: Article = deserializer.deserialize().unwrap();
        assert_eq!(
            result,
            Article {
                title: Some(TitleElement {
                    text: "Hello".into()
                }),
                content: Some(ContentElement {
                    text: "World".into()
                })
            }
        );
    }

    #[test_log::test]
    fn test_deserialize_multiple_attributes() {
        #[derive(Debug, Facet, PartialEq)]
        struct Link {
            #[facet(default)]
            href: Option<String>,
            #[facet(default)]
            target: Option<String>,
            #[facet(default)]
            rel: Option<String>,
        }

        let html = b"<a href=\"https://example.com\" target=\"_blank\" rel=\"noopener\"></a>";
        let parser = HtmlParser::new(html);
        let mut deserializer = FormatDeserializer::new(parser);
        let result: Link = deserializer.deserialize().unwrap();
        assert_eq!(
            result,
            Link {
                href: Some("https://example.com".into()),
                target: Some("_blank".into()),
                rel: Some("noopener".into())
            }
        );
    }

    #[test_log::test]
    fn test_deserialize_predefined_img() {
        use facet_html_dom::Img;

        let html = b"<img src=\"photo.jpg\" alt=\"A photo\" width=\"100\" height=\"200\">";
        let parser = HtmlParser::new(html);
        let mut deserializer = FormatDeserializer::new(parser);
        let result: Img = deserializer.deserialize().unwrap();
        assert_eq!(result.src, Some("photo.jpg".into()));
        assert_eq!(result.alt, Some("A photo".into()));
        assert_eq!(result.width, Some("100".into()));
        assert_eq!(result.height, Some("200".into()));
    }

    #[test_log::test]
    fn test_deserialize_predefined_a() {
        use facet_html_dom::{A, PhrasingContent};

        let html = b"<a href=\"https://example.com\" target=\"_blank\">Click me</a>";
        let parser = HtmlParser::new(html);
        let mut deserializer = FormatDeserializer::new(parser);
        let result: A = deserializer.deserialize().unwrap();
        assert_eq!(result.href, Some("https://example.com".into()));
        assert_eq!(result.target, Some("_blank".into()));
        assert_eq!(result.children.len(), 1);
        assert!(matches!(&result.children[0], PhrasingContent::Text(t) if t == "Click me"));
    }

    #[test_log::test]
    fn test_deserialize_predefined_div_with_class() {
        use facet_html_dom::{Div, FlowContent};

        let html = b"<div class=\"container\" id=\"main\">Hello World</div>";
        let parser = HtmlParser::new(html);
        let mut deserializer = FormatDeserializer::new(parser);
        let result: Div = deserializer.deserialize().unwrap();
        assert_eq!(result.attrs.class, Some("container".into()));
        assert_eq!(result.attrs.id, Some("main".into()));
        assert_eq!(result.children.len(), 1);
        assert!(matches!(&result.children[0], FlowContent::Text(t) if t == "Hello World"));
    }

    #[test_log::test]
    fn test_mixed_content_events() {
        // Test: <p>Hello <strong>world</strong> there</p>
        // Should produce events with text nodes in their correct positions
        // Whitespace is preserved exactly as in the source
        let html = b"<p>Hello <strong>world</strong> there</p>";
        let events = build_events(html).unwrap();

        // Should have:
        // StructStart (p)
        // FieldKey(_tag) -> "p"
        // FieldKey(_text) -> "Hello " (with trailing space)
        // FieldKey(strong) -> StructStart, FieldKey(_tag), "strong", FieldKey(_text), "world", StructEnd
        // FieldKey(_text) -> " there" (with leading space)
        // StructEnd
        assert_eq!(
            events,
            vec![
                ParseEvent::StructStart(ContainerKind::Element),
                ParseEvent::FieldKey(FieldKey::new(Cow::Borrowed("_tag"), FieldLocationHint::Tag)),
                ParseEvent::Scalar(ScalarValue::Str(Cow::Owned("p".into()))),
                ParseEvent::FieldKey(FieldKey::new(
                    Cow::Borrowed("_text"),
                    FieldLocationHint::Text
                )),
                ParseEvent::Scalar(ScalarValue::Str(Cow::Owned("Hello ".into()))),
                ParseEvent::FieldKey(FieldKey::new(
                    Cow::Owned("strong".into()),
                    FieldLocationHint::Child
                )),
                ParseEvent::StructStart(ContainerKind::Element),
                ParseEvent::FieldKey(FieldKey::new(Cow::Borrowed("_tag"), FieldLocationHint::Tag)),
                ParseEvent::Scalar(ScalarValue::Str(Cow::Owned("strong".into()))),
                ParseEvent::FieldKey(FieldKey::new(
                    Cow::Borrowed("_text"),
                    FieldLocationHint::Text
                )),
                ParseEvent::Scalar(ScalarValue::Str(Cow::Owned("world".into()))),
                ParseEvent::StructEnd,
                ParseEvent::FieldKey(FieldKey::new(
                    Cow::Borrowed("_text"),
                    FieldLocationHint::Text
                )),
                ParseEvent::Scalar(ScalarValue::Str(Cow::Owned(" there".into()))),
                ParseEvent::StructEnd,
            ]
        );
    }

    #[test_log::test]
    fn test_mixed_content_deserialization() {
        use facet_html_dom::{P, PhrasingContent};

        // Test: <p>Hello <strong>world</strong> there</p>
        // Whitespace is preserved exactly as in the source
        let html = b"<p>Hello <strong>world</strong> there</p>";
        let parser = HtmlParser::new(html);
        let mut deserializer = FormatDeserializer::new(parser);
        let result: P = deserializer.deserialize().unwrap();

        // The children should have the interleaved text and element nodes
        assert_eq!(result.children.len(), 3);
        assert!(matches!(&result.children[0], PhrasingContent::Text(t) if t == "Hello "));
        // Strong now has children, not a text field
        if let PhrasingContent::Strong(strong) = &result.children[1] {
            assert_eq!(strong.children.len(), 1);
            assert!(matches!(&strong.children[0], PhrasingContent::Text(t) if t == "world"));
        } else {
            panic!("Expected Strong element");
        }
        assert!(matches!(&result.children[2], PhrasingContent::Text(t) if t == " there"));
    }

    #[test_log::test]
    fn test_mixed_content_multiple_elements() {
        use facet_html_dom::{P, PhrasingContent};

        // Test: <p>Start <strong>bold</strong> middle <em>italic</em> end</p>
        // Whitespace is preserved exactly as in the source
        let html = b"<p>Start <strong>bold</strong> middle <em>italic</em> end</p>";
        let parser = HtmlParser::new(html);
        let mut deserializer = FormatDeserializer::new(parser);
        let result: P = deserializer.deserialize().unwrap();

        assert_eq!(result.children.len(), 5);
        assert!(matches!(&result.children[0], PhrasingContent::Text(t) if t == "Start "));
        // Strong and Em now have children, not text fields
        if let PhrasingContent::Strong(strong) = &result.children[1] {
            assert_eq!(strong.children.len(), 1);
            assert!(matches!(&strong.children[0], PhrasingContent::Text(t) if t == "bold"));
        } else {
            panic!("Expected Strong element");
        }
        assert!(matches!(&result.children[2], PhrasingContent::Text(t) if t == " middle "));
        if let PhrasingContent::Em(em) = &result.children[3] {
            assert_eq!(em.children.len(), 1);
            assert!(matches!(&em.children[0], PhrasingContent::Text(t) if t == "italic"));
        } else {
            panic!("Expected Em element");
        }
        assert!(matches!(&result.children[4], PhrasingContent::Text(t) if t == " end"));
    }

    #[test_log::test]
    fn test_deserialize_meta_charset() {
        use facet_html_dom::Meta;

        // Regression test for https://github.com/facet-rs/facet/issues/1527
        // meta charset="utf-8" was failing with:
        // "type mismatch: expected struct start, got Scalar(Str("utf-8"))"
        let html = b"<meta charset=\"utf-8\">";
        let parser = HtmlParser::new(html);
        let mut deserializer = FormatDeserializer::new(parser);
        let result: Meta = deserializer.deserialize().unwrap();
        assert_eq!(result.charset, Some("utf-8".into()));
    }

    #[test_log::test]
    fn test_deserialize_head_with_meta_charset() {
        use facet_html_dom::Head;

        // Regression test for https://github.com/facet-rs/facet/issues/1527
        // The bug occurs when meta is inside head
        let html = b"<head><meta charset=\"utf-8\"><title>Test</title></head>";
        let parser = HtmlParser::new(html);
        let mut deserializer = FormatDeserializer::new(parser);
        let result: Head = deserializer.deserialize().unwrap();

        // Head has children, use helper methods to access them
        let meta: Vec<_> = result.meta().collect();
        assert!(!meta.is_empty(), "Should have a meta element");
        assert_eq!(meta[0].charset, Some("utf-8".into()));
    }

    #[test_log::test]
    fn test_deserialize_full_html_document_with_meta_charset() {
        use facet_html_dom::Html;

        // Full reproduction from https://github.com/facet-rs/facet/issues/1527
        let html = br#"<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <title>Test Page</title>
</head>
<body>
    <p>Hello</p>
</body>
</html>"#;

        let parser = HtmlParser::new(html);
        let mut deserializer = FormatDeserializer::new(parser);
        let result: Html = deserializer.deserialize().unwrap();

        // Verify head was parsed correctly
        let head = result.head.as_ref().expect("Should have head");
        let meta: Vec<_> = head.meta().collect();
        assert!(!meta.is_empty(), "Should have meta elements");
        assert_eq!(meta[0].charset, Some("utf-8".into()));

        // Verify title
        let title = head.title().expect("Should have title");
        assert_eq!(title.text, "Test Page");

        // Verify body exists
        assert!(result.body.is_some(), "Should have body");
    }

    #[test_log::test]
    fn test_doctype_captured() {
        use facet_html_dom::Html;

        // Test that DOCTYPE is captured during parsing
        let html = br#"<!DOCTYPE html>
<html>
<head><title>Test</title></head>
<body></body>
</html>"#;

        let parser = HtmlParser::new(html);
        let mut deserializer = FormatDeserializer::new(parser);
        let result: Html = deserializer.deserialize().unwrap();

        // Verify DOCTYPE was captured
        assert_eq!(
            result.doctype,
            Some("html".to_string()),
            "DOCTYPE should be captured"
        );
    }

    #[test_log::test]
    fn test_doctype_not_present() {
        use facet_html_dom::Html;

        // Test that DOCTYPE is None when not present
        let html = br#"<html>
<head><title>Test</title></head>
<body></body>
</html>"#;

        let parser = HtmlParser::new(html);
        let mut deserializer = FormatDeserializer::new(parser);
        let result: Html = deserializer.deserialize().unwrap();

        // Verify DOCTYPE is None
        assert_eq!(
            result.doctype, None,
            "DOCTYPE should be None when not present"
        );
    }
}
