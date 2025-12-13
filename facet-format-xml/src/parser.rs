extern crate alloc;

use alloc::borrow::Cow;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use facet_format::{
    FieldEvidence, FieldKey, FieldLocationHint, FormatParser, ParseEvent, ProbeStream, ScalarValue,
};
use quick_xml::NsReader;
use quick_xml::events::Event;
use quick_xml::name::ResolveResult;
use std::io::Cursor;

/// A qualified XML name with optional namespace URI.
///
/// In XML, elements and attributes can be in a namespace. The namespace is
/// identified by a URI, not the prefix used in the document. For example,
/// `android:label` and `a:label` are the same if both prefixes resolve to
/// the same namespace URI.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)] // Will be used in Phase 2
struct QName {
    /// The namespace URI, or `None` for "no namespace".
    ///
    /// - Elements without a prefix and no default `xmlns` are in no namespace.
    /// - Attributes without a prefix are always in no namespace (even with default xmlns).
    /// - Elements/attributes with a prefix have their namespace resolved via xmlns declarations.
    namespace: Option<String>,
    /// The local name (without prefix).
    local_name: String,
}

#[allow(dead_code)] // Will be used in Phase 2
impl QName {
    /// Create a qualified name with no namespace.
    fn local(name: impl Into<String>) -> Self {
        Self {
            namespace: None,
            local_name: name.into(),
        }
    }

    /// Create a qualified name with a namespace.
    fn with_ns(namespace: impl Into<String>, local_name: impl Into<String>) -> Self {
        Self {
            namespace: Some(namespace.into()),
            local_name: local_name.into(),
        }
    }

    /// Check if this name matches a local name with an optional expected namespace.
    ///
    /// If `expected_ns` is `None`, matches any name with the given local name.
    /// If `expected_ns` is `Some(ns)`, only matches if both local name and namespace match.
    fn matches(&self, local_name: &str, expected_ns: Option<&str>) -> bool {
        if self.local_name != local_name {
            return false;
        }
        match expected_ns {
            None => true, // No namespace constraint - match any namespace (or none)
            Some(ns) => self.namespace.as_deref() == Some(ns),
        }
    }
}

impl fmt::Display for QName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.namespace {
            Some(ns) => write!(f, "{{{}}}{}", ns, self.local_name),
            None => write!(f, "{}", self.local_name),
        }
    }
}

pub struct XmlParser<'de> {
    events: Vec<ParseEvent<'de>>,
    idx: usize,
    pending_error: Option<XmlError>,
}

impl<'de> XmlParser<'de> {
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

#[derive(Debug, Clone)]
pub enum XmlError {
    ParseError(alloc::string::String),
    UnexpectedEof,
    UnbalancedTags,
    InvalidUtf8,
    MultipleRoots,
}

impl fmt::Display for XmlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            XmlError::ParseError(msg) => write!(f, "XML parse error: {}", msg),
            XmlError::UnexpectedEof => write!(f, "Unexpected end of XML"),
            XmlError::UnbalancedTags => write!(f, "Unbalanced XML tags"),
            XmlError::InvalidUtf8 => write!(f, "Invalid UTF-8 in XML"),
            XmlError::MultipleRoots => write!(f, "XML document has multiple root elements"),
        }
    }
}

impl<'de> FormatParser<'de> for XmlParser<'de> {
    type Error = XmlError;
    type Probe<'a>
        = XmlProbe<'de>
    where
        Self: 'a;

    fn next_event(&mut self) -> Result<ParseEvent<'de>, Self::Error> {
        if let Some(err) = &self.pending_error {
            return Err(err.clone());
        }
        if self.idx >= self.events.len() {
            return Err(XmlError::UnexpectedEof);
        }
        let event = self.events[self.idx].clone();
        self.idx += 1;
        Ok(event)
    }

    fn peek_event(&mut self) -> Result<ParseEvent<'de>, Self::Error> {
        if let Some(err) = &self.pending_error {
            return Err(err.clone());
        }
        self.events
            .get(self.idx)
            .cloned()
            .ok_or(XmlError::UnexpectedEof)
    }

    fn skip_value(&mut self) -> Result<(), Self::Error> {
        let mut depth = 0usize;
        loop {
            let event = self.next_event()?;
            match event {
                ParseEvent::StructStart | ParseEvent::SequenceStart => {
                    depth += 1;
                }
                ParseEvent::StructEnd | ParseEvent::SequenceEnd => {
                    if depth == 0 {
                        break;
                    } else {
                        depth -= 1;
                    }
                }
                ParseEvent::Scalar(_) | ParseEvent::VariantTag(_) => {
                    if depth == 0 {
                        break;
                    }
                }
                ParseEvent::FieldKey(_) => {
                    // Value will follow; treat as entering one more depth level.
                    depth += 1;
                }
            }
        }
        Ok(())
    }

    fn begin_probe(&mut self) -> Result<Self::Probe<'_>, Self::Error> {
        // Look ahead in the remaining events to build field evidence
        let evidence = self.build_probe();
        Ok(XmlProbe { evidence, idx: 0 })
    }
}

impl<'de> XmlParser<'de> {
    /// Build field evidence by looking ahead at remaining events.
    fn build_probe(&self) -> Vec<FieldEvidence<'de>> {
        let mut evidence = Vec::new();

        // Check if we're about to read a struct
        if self.idx >= self.events.len() {
            return evidence;
        }

        if !matches!(self.events.get(self.idx), Some(ParseEvent::StructStart)) {
            return evidence;
        }

        // Scan the struct's fields
        let mut i = self.idx + 1;
        let mut depth = 0usize;

        while i < self.events.len() {
            match &self.events[i] {
                ParseEvent::StructStart | ParseEvent::SequenceStart => {
                    depth += 1;
                    i += 1;
                }
                ParseEvent::StructEnd | ParseEvent::SequenceEnd => {
                    if depth == 0 {
                        // End of the struct we're probing
                        break;
                    }
                    depth -= 1;
                    i += 1;
                }
                ParseEvent::FieldKey(key) if depth == 0 => {
                    // This is a top-level field in the struct we're probing
                    // Look at the next event to see if it's a scalar
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

pub struct XmlProbe<'de> {
    evidence: Vec<FieldEvidence<'de>>,
    idx: usize,
}

impl<'de> ProbeStream<'de> for XmlProbe<'de> {
    type Error = XmlError;

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

/// Resolve a namespace from quick-xml's ResolveResult.
fn resolve_namespace(resolve: ResolveResult<'_>) -> Result<Option<String>, XmlError> {
    match resolve {
        ResolveResult::Bound(ns) => Ok(Some(String::from_utf8_lossy(ns.as_ref()).into_owned())),
        ResolveResult::Unbound => Ok(None),
        ResolveResult::Unknown(_) => {
            // Unknown prefix - treat as no namespace
            Ok(None)
        }
    }
}

#[derive(Debug, Clone)]
struct Element {
    name: QName,
    attributes: Vec<(QName, String)>,
    children: Vec<Element>,
    text: String,
}

impl Element {
    fn new(name: QName, attributes: Vec<(QName, String)>) -> Self {
        Self {
            name,
            attributes,
            children: Vec::new(),
            text: String::new(),
        }
    }

    fn push_text(&mut self, text: &str) {
        if text.trim().is_empty() {
            return;
        }
        if !self.text.is_empty() {
            self.text.push(' ');
        }
        self.text.push_str(text.trim());
    }
}

#[derive(Debug, Clone)]
enum XmlValue {
    Null,
    Bool(bool),
    I64(i64),
    U64(u64),
    F64(f64),
    String(String),
    Array(Vec<XmlValue>),
    Object(Vec<XmlField>),
}

#[derive(Debug, Clone)]
struct XmlField {
    name: String,
    namespace: Option<String>,
    location: FieldLocationHint,
    value: XmlValue,
}

fn build_events<'de>(input: &'de [u8]) -> Result<Vec<ParseEvent<'de>>, XmlError> {
    let mut reader = NsReader::from_reader(Cursor::new(input));
    reader.config_mut().trim_text(true);

    let mut buf = Vec::new();
    let mut stack: Vec<Element> = Vec::new();
    let mut root: Option<Element> = None;

    loop {
        buf.clear();
        let (resolve, event) = reader
            .read_resolved_event_into(&mut buf)
            .map_err(|e| XmlError::ParseError(e.to_string()))?;

        match event {
            Event::Start(ref e) | Event::Empty(ref e) => {
                // Resolve element namespace
                let ns = resolve_namespace(resolve)?;
                let local = core::str::from_utf8(e.local_name().as_ref())
                    .map_err(|_| XmlError::InvalidUtf8)?
                    .to_string();
                let name = match ns {
                    Some(uri) => QName::with_ns(uri, local),
                    None => QName::local(local),
                };

                // Resolve attribute namespaces
                let mut attributes = Vec::new();
                for attr in e.attributes() {
                    let attr = attr.map_err(|e| XmlError::ParseError(e.to_string()))?;

                    // Skip xmlns declarations (xmlns and xmlns:*)
                    let key = attr.key;
                    if key.as_ref() == b"xmlns" {
                        continue; // Skip default namespace declaration
                    }
                    if let Some(prefix) = key.prefix()
                        && prefix.as_ref() == b"xmlns"
                    {
                        continue; // Skip prefixed namespace declarations
                    }

                    let (attr_resolve, _) = reader.resolve_attribute(key);
                    let attr_ns = resolve_namespace(attr_resolve)?;
                    let attr_local = core::str::from_utf8(key.local_name().as_ref())
                        .map_err(|_| XmlError::InvalidUtf8)?
                        .to_string();
                    let attr_qname = match attr_ns {
                        Some(uri) => QName::with_ns(uri, attr_local),
                        None => QName::local(attr_local),
                    };
                    let value = attr
                        .unescape_value()
                        .map_err(|e| XmlError::ParseError(e.to_string()))?
                        .into_owned();
                    attributes.push((attr_qname, value));
                }

                let elem = Element::new(name, attributes);

                if matches!(event, Event::Start(_)) {
                    stack.push(elem);
                } else {
                    // Empty element
                    attach_element(stack.as_mut_slice(), elem, &mut root)?;
                }
            }
            Event::End(_) => {
                let elem = stack.pop().ok_or(XmlError::UnbalancedTags)?;
                attach_element(stack.as_mut_slice(), elem, &mut root)?;
            }
            Event::Text(e) => {
                if let Some(current) = stack.last_mut() {
                    let text = e
                        .unescape()
                        .map_err(|err| XmlError::ParseError(err.to_string()))?;
                    current.push_text(text.as_ref());
                }
            }
            Event::CData(e) => {
                if let Some(current) = stack.last_mut() {
                    let text =
                        core::str::from_utf8(e.as_ref()).map_err(|_| XmlError::InvalidUtf8)?;
                    current.push_text(text);
                }
            }
            Event::Decl(_) | Event::Comment(_) | Event::PI(_) | Event::DocType(_) => {}
            Event::Eof => break,
        }
    }

    if !stack.is_empty() {
        return Err(XmlError::UnbalancedTags);
    }

    let root = root.ok_or(XmlError::UnexpectedEof)?;
    let value = element_to_value(&root);
    let mut events = Vec::new();
    emit_value_events(&value, &mut events);
    Ok(events)
}

fn attach_element(
    stack: &mut [Element],
    elem: Element,
    root: &mut Option<Element>,
) -> Result<(), XmlError> {
    if let Some(parent) = stack.last_mut() {
        parent.children.push(elem);
    } else if root.is_none() {
        *root = Some(elem);
    } else {
        return Err(XmlError::MultipleRoots);
    }
    Ok(())
}

fn element_to_value(elem: &Element) -> XmlValue {
    let text = elem.text.trim();
    let has_attrs = !elem.attributes.is_empty();
    let has_children = !elem.children.is_empty();

    if !has_attrs && !has_children {
        if text.is_empty() {
            // Empty element is an empty object, not null
            // This allows unit structs to deserialize correctly
            return XmlValue::Object(vec![]);
        }
        return parse_scalar(text);
    }

    if !has_attrs && has_children && text.is_empty() && elem.children.len() > 1 {
        let first = &elem.children[0].name;
        // Check if all children have the same name (both local name and namespace)
        if elem.children.iter().all(|child| &child.name == first) {
            let items = elem
                .children
                .iter()
                .map(element_to_value)
                .collect::<Vec<_>>();
            return XmlValue::Array(items);
        }
    }

    let mut fields = Vec::new();
    for (qname, value) in &elem.attributes {
        fields.push(XmlField {
            name: qname.local_name.clone(),
            namespace: qname.namespace.clone(),
            location: FieldLocationHint::Attribute,
            value: XmlValue::String(value.clone()),
        });
    }

    // Group children by (local_name, namespace) tuple to handle same local name with different namespaces
    let mut grouped: BTreeMap<(&str, Option<&str>), Vec<XmlValue>> = BTreeMap::new();
    for child in &elem.children {
        let key = (
            child.name.local_name.as_str(),
            child.name.namespace.as_deref(),
        );
        grouped
            .entry(key)
            .or_default()
            .push(element_to_value(child));
    }

    for ((local_name, namespace), mut values) in grouped {
        let value = if values.len() == 1 {
            values.pop().unwrap()
        } else {
            XmlValue::Array(values)
        };
        fields.push(XmlField {
            name: local_name.to_string(),
            namespace: namespace.map(String::from),
            location: FieldLocationHint::Child,
            value,
        });
    }

    if !text.is_empty() {
        if fields.is_empty() {
            return parse_scalar(text);
        }
        fields.push(XmlField {
            name: "_text".into(),
            namespace: None,
            location: FieldLocationHint::Text,
            value: XmlValue::String(text.to_string()),
        });
    }

    XmlValue::Object(fields)
}

fn parse_scalar(text: &str) -> XmlValue {
    if text.eq_ignore_ascii_case("null") {
        return XmlValue::Null;
    }
    if let Ok(b) = text.parse::<bool>() {
        return XmlValue::Bool(b);
    }
    if let Ok(i) = text.parse::<i64>() {
        return XmlValue::I64(i);
    }
    if let Ok(u) = text.parse::<u64>() {
        return XmlValue::U64(u);
    }
    if let Ok(f) = text.parse::<f64>() {
        return XmlValue::F64(f);
    }
    XmlValue::String(text.to_string())
}

fn emit_value_events<'de>(value: &XmlValue, events: &mut Vec<ParseEvent<'de>>) {
    match value {
        XmlValue::Null => events.push(ParseEvent::Scalar(ScalarValue::Null)),
        XmlValue::Bool(b) => events.push(ParseEvent::Scalar(ScalarValue::Bool(*b))),
        XmlValue::I64(n) => events.push(ParseEvent::Scalar(ScalarValue::I64(*n))),
        XmlValue::U64(n) => events.push(ParseEvent::Scalar(ScalarValue::U64(*n))),
        XmlValue::F64(n) => events.push(ParseEvent::Scalar(ScalarValue::F64(*n))),
        XmlValue::String(s) => {
            events.push(ParseEvent::Scalar(ScalarValue::Str(Cow::Owned(s.clone()))))
        }
        XmlValue::Array(items) => {
            events.push(ParseEvent::SequenceStart);
            for item in items {
                emit_value_events(item, events);
            }
            events.push(ParseEvent::SequenceEnd);
        }
        XmlValue::Object(fields) => {
            events.push(ParseEvent::StructStart);
            for field in fields {
                let mut key = FieldKey::new(Cow::Owned(field.name.clone()), field.location);
                if let Some(ns) = &field.namespace {
                    key = key.with_namespace(Cow::Owned(ns.clone()));
                }
                events.push(ParseEvent::FieldKey(key));
                emit_value_events(&field.value, events);
            }
            events.push(ParseEvent::StructEnd);
        }
    }
}
