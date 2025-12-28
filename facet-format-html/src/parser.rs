extern crate alloc;

use alloc::borrow::Cow;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use facet_format::{
    ContainerKind, FieldEvidence, FieldKey, FieldLocationHint, FormatParser, ParseEvent,
    ProbeStream, ScalarValue,
};
use html5gum::{Token, Tokenizer};

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

/// An HTML element in the DOM tree.
#[derive(Debug, Clone)]
struct Element {
    /// Tag name (lowercase).
    name: String,
    /// Attributes as (name, value) pairs.
    attributes: Vec<(String, String)>,
    /// Child elements.
    children: Vec<Element>,
    /// Text content (accumulated from text nodes).
    text: String,
}

impl Element {
    fn new(name: String, attributes: Vec<(String, String)>) -> Self {
        Self {
            name,
            attributes,
            children: Vec::new(),
            text: String::new(),
        }
    }

    fn push_text(&mut self, text: &str) {
        // Normalize whitespace: collapse multiple whitespace to single space
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return;
        }
        if !self.text.is_empty() {
            self.text.push(' ');
        }
        self.text.push_str(trimmed);
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

    let tokenizer = Tokenizer::new(input_str);
    let mut stack: Vec<Element> = Vec::new();
    let mut roots: Vec<Element> = Vec::new();

    for token_result in tokenizer {
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
                    // Push onto stack to collect children
                    stack.push(elem);
                }
            }
            Token::EndTag(tag) => {
                let end_name = String::from_utf8_lossy(&tag.name).to_ascii_lowercase();

                // Pop elements until we find a matching start tag
                // This handles malformed HTML gracefully
                while let Some(elem) = stack.pop() {
                    if elem.name == end_name {
                        attach_element(&mut stack, elem, &mut roots);
                        break;
                    } else {
                        // Implicitly close this element (HTML error recovery)
                        attach_element(&mut stack, elem, &mut roots);
                    }
                }
            }
            Token::String(text) => {
                let text_str = String::from_utf8_lossy(&text);
                if let Some(current) = stack.last_mut() {
                    current.push_text(&text_str);
                }
                // Text outside elements is ignored
            }
            Token::Doctype(_) | Token::Comment(_) | Token::Error(_) => {
                // Ignore doctype, comments, and errors
            }
        }
    }

    // Close any remaining open elements
    while let Some(elem) = stack.pop() {
        attach_element(&mut stack, elem, &mut roots);
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
        parent.children.push(elem);
    } else {
        roots.push(elem);
    }
}

/// Emit ParseEvents from an Element.
fn emit_element_events<'de>(elem: &Element, events: &mut Vec<ParseEvent<'de>>) {
    let text = elem.text.trim();
    let has_attrs = !elem.attributes.is_empty();
    let has_children = !elem.children.is_empty();

    // Case 1: No attributes, no children - emit scalar from text
    if !has_attrs && !has_children {
        if text.is_empty() {
            // Empty element is an empty object (for unit structs)
            events.push(ParseEvent::StructStart(ContainerKind::Element));
            events.push(ParseEvent::StructEnd);
        } else {
            emit_scalar_from_text(text, events);
        }
        return;
    }

    // Case 2: Has attributes or children - emit as struct
    // The deserializer handles grouping repeated field names into sequences.
    events.push(ParseEvent::StructStart(ContainerKind::Element));

    // Emit attributes as fields
    for (name, value) in &elem.attributes {
        let key = FieldKey::new(Cow::Owned(name.clone()), FieldLocationHint::Attribute);
        events.push(ParseEvent::FieldKey(key));
        events.push(ParseEvent::Scalar(ScalarValue::Str(Cow::Owned(
            value.clone(),
        ))));
    }

    // Emit children
    for child in &elem.children {
        let key = FieldKey::new(Cow::Owned(child.name.clone()), FieldLocationHint::Child);
        events.push(ParseEvent::FieldKey(key));
        emit_element_events(child, events);
    }

    // Emit text content if present
    if !text.is_empty() {
        let key = FieldKey::new(Cow::Borrowed("_text"), FieldLocationHint::Text);
        events.push(ParseEvent::FieldKey(key));
        events.push(ParseEvent::Scalar(ScalarValue::Str(Cow::Owned(
            text.to_string(),
        ))));
    }

    events.push(ParseEvent::StructEnd);
}

/// Parse text and emit appropriate scalar event.
fn emit_scalar_from_text<'de>(text: &str, events: &mut Vec<ParseEvent<'de>>) {
    // Try to parse as various types
    if text.eq_ignore_ascii_case("null") {
        events.push(ParseEvent::Scalar(ScalarValue::Null));
        return;
    }
    if let Ok(b) = text.parse::<bool>() {
        events.push(ParseEvent::Scalar(ScalarValue::Bool(b)));
        return;
    }
    if let Ok(i) = text.parse::<i64>() {
        events.push(ParseEvent::Scalar(ScalarValue::I64(i)));
        return;
    }
    if let Ok(u) = text.parse::<u64>() {
        events.push(ParseEvent::Scalar(ScalarValue::U64(u)));
        return;
    }
    if text.parse::<i128>().is_ok() || text.parse::<u128>().is_ok() {
        events.push(ParseEvent::Scalar(ScalarValue::Str(Cow::Owned(
            text.to_string(),
        ))));
        return;
    }
    if let Ok(f) = text.parse::<f64>() {
        events.push(ParseEvent::Scalar(ScalarValue::F64(f)));
        return;
    }
    events.push(ParseEvent::Scalar(ScalarValue::Str(Cow::Owned(
        text.to_string(),
    ))));
}

#[cfg(test)]
mod tests {
    use super::*;
    use facet::Facet;
    use facet_format::FormatDeserializer;

    #[test]
    fn test_simple_element() {
        let html = b"<div>hello</div>";
        let events = build_events(html).unwrap();
        assert_eq!(
            events,
            vec![ParseEvent::Scalar(ScalarValue::Str(Cow::Owned(
                "hello".into()
            )))]
        );
    }

    #[test]
    fn test_element_with_attribute() {
        let html = b"<div class=\"foo\">hello</div>";
        let events = build_events(html).unwrap();
        assert_eq!(
            events,
            vec![
                ParseEvent::StructStart(ContainerKind::Element),
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

    #[test]
    fn test_nested_elements() {
        let html = b"<div><span>inner</span></div>";
        let events = build_events(html).unwrap();
        assert_eq!(
            events,
            vec![
                ParseEvent::StructStart(ContainerKind::Element),
                ParseEvent::FieldKey(FieldKey::new(
                    Cow::Owned("span".into()),
                    FieldLocationHint::Child
                )),
                ParseEvent::Scalar(ScalarValue::Str(Cow::Owned("inner".into()))),
                ParseEvent::StructEnd,
            ]
        );
    }

    #[test]
    fn test_void_element() {
        let html = b"<div><br><span>after</span></div>";
        let events = build_events(html).unwrap();
        // br is a void element, should be parsed correctly
        assert!(!events.is_empty());
    }

    #[test]
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

    #[test]
    fn test_deserialize_nested() {
        #[derive(Debug, Facet, PartialEq)]
        struct Outer {
            #[facet(default)]
            inner: Option<Inner>,
        }

        #[derive(Debug, Facet, PartialEq)]
        struct Inner {
            #[facet(default)]
            value: Option<String>,
        }

        let html = b"<outer><inner><value>hello</value></inner></outer>";
        let parser = HtmlParser::new(html);
        let mut deserializer = FormatDeserializer::new(parser);
        let result: Outer = deserializer.deserialize().unwrap();
        assert_eq!(
            result,
            Outer {
                inner: Some(Inner {
                    value: Some("hello".into())
                })
            }
        );
    }

    #[test]
    fn test_deserialize_with_text() {
        #[derive(Debug, Facet, PartialEq)]
        struct Article {
            #[facet(default)]
            title: Option<String>,
            #[facet(default)]
            content: Option<String>,
        }

        let html = b"<article><title>Hello</title><content>World</content></article>";
        let parser = HtmlParser::new(html);
        let mut deserializer = FormatDeserializer::new(parser);
        let result: Article = deserializer.deserialize().unwrap();
        assert_eq!(
            result,
            Article {
                title: Some("Hello".into()),
                content: Some("World".into())
            }
        );
    }

    #[test]
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

    #[test]
    fn test_deserialize_predefined_img() {
        use crate::elements::Img;

        let html = b"<img src=\"photo.jpg\" alt=\"A photo\" width=\"100\" height=\"200\">";
        let parser = HtmlParser::new(html);
        let mut deserializer = FormatDeserializer::new(parser);
        let result: Img = deserializer.deserialize().unwrap();
        assert_eq!(result.src, Some("photo.jpg".into()));
        assert_eq!(result.alt, Some("A photo".into()));
        assert_eq!(result.width, Some("100".into()));
        assert_eq!(result.height, Some("200".into()));
    }

    #[test]
    fn test_deserialize_predefined_a() {
        use crate::elements::A;

        let html = b"<a href=\"https://example.com\" target=\"_blank\">Click me</a>";
        let parser = HtmlParser::new(html);
        let mut deserializer = FormatDeserializer::new(parser);
        let result: A = deserializer.deserialize().unwrap();
        assert_eq!(result.href, Some("https://example.com".into()));
        assert_eq!(result.target, Some("_blank".into()));
        assert_eq!(result.text, "Click me");
    }

    #[test]
    fn test_deserialize_predefined_div_with_class() {
        use crate::elements::Div;

        let html = b"<div class=\"container\" id=\"main\">Hello World</div>";
        let parser = HtmlParser::new(html);
        let mut deserializer = FormatDeserializer::new(parser);
        let result: Div = deserializer.deserialize().unwrap();
        assert_eq!(result.attrs.class, Some("container".into()));
        assert_eq!(result.attrs.id, Some("main".into()));
        assert_eq!(result.text, "Hello World");
    }
}
