extern crate alloc;

use alloc::borrow::Cow;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use facet_format::{
    ContainerKind, FieldEvidence, FieldKey, FieldLocationHint, FormatParser, ParseEvent,
    ProbeStream, ScalarValue,
};
use quick_xml::NsReader;
use quick_xml::escape::resolve_xml_entity;
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
    InvalidUtf8(core::str::Utf8Error),
    MultipleRoots,
}

impl fmt::Display for XmlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            XmlError::ParseError(msg) => write!(f, "XML parse error: {}", msg),
            XmlError::UnexpectedEof => write!(f, "Unexpected end of XML"),
            XmlError::UnbalancedTags => write!(f, "Unbalanced XML tags"),
            XmlError::InvalidUtf8(e) => write!(f, "Invalid UTF-8 in XML: {}", e),
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
        // Track how many "pending field values" we have at each struct depth.
        // When we see FieldKey, we expect a value to follow.
        // When that value is consumed (Scalar or StructEnd/SequenceEnd), we're done with that field.
        let mut struct_depth = 0usize;
        let mut pending_field_value = false;

        loop {
            let event = self.next_event()?.ok_or(XmlError::UnexpectedEof)?;
            match event {
                ParseEvent::StructStart(_) | ParseEvent::SequenceStart(_) => {
                    // If we were waiting for a field value, this struct/seq IS that value
                    pending_field_value = false;
                    struct_depth += 1;
                }
                ParseEvent::StructEnd | ParseEvent::SequenceEnd => {
                    if struct_depth == 0 {
                        // We were skipping a struct/seq value and now it's closed
                        break;
                    } else {
                        struct_depth -= 1;
                        // If we just closed the top-level value, we're done
                        if struct_depth == 0 && !pending_field_value {
                            break;
                        }
                    }
                }
                ParseEvent::Scalar(_) | ParseEvent::VariantTag(_) => {
                    if struct_depth == 0 && !pending_field_value {
                        // This scalar IS the value we were asked to skip
                        break;
                    }
                    // If we were waiting for a field value, this scalar is it
                    pending_field_value = false;
                }
                ParseEvent::FieldKey(_) | ParseEvent::OrderedField => {
                    // A field key means a value will follow
                    pending_field_value = true;
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

        if !matches!(
            self.events.get(self.idx),
            Some(ParseEvent::StructStart(ContainerKind::Element))
        ) {
            return evidence;
        }

        // Scan the struct's fields
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

/// Resolve a general entity reference to its character value.
/// Handles both named entities (lt, gt, amp, etc.) and numeric entities (&#10;, &#x09;, etc.)
fn resolve_entity(raw: &str) -> Result<String, XmlError> {
    // Try named entity first (e.g., "lt" -> "<")
    if let Some(resolved) = resolve_xml_entity(raw) {
        return Ok(resolved.into());
    }

    // Try numeric entity (e.g., "#10" -> "\n", "#x09" -> "\t")
    if let Some(rest) = raw.strip_prefix('#') {
        let code = if let Some(hex) = rest.strip_prefix('x').or_else(|| rest.strip_prefix('X')) {
            // Hexadecimal numeric entity
            u32::from_str_radix(hex, 16).map_err(|_| {
                XmlError::ParseError(format!("Invalid hex numeric entity: #{}", rest))
            })?
        } else {
            // Decimal numeric entity
            rest.parse::<u32>().map_err(|_| {
                XmlError::ParseError(format!("Invalid decimal numeric entity: #{}", rest))
            })?
        };

        let ch = char::from_u32(code)
            .ok_or_else(|| XmlError::ParseError(format!("Invalid Unicode code point: {}", code)))?;
        return Ok(ch.to_string());
    }

    // Unknown entity - return as-is with & and ;
    Ok(format!("&{};", raw))
}

#[derive(Debug, Clone)]
struct Element {
    name: QName,
    attributes: Vec<(QName, String)>,
    children: Vec<Element>,
    text: String,
}

impl Element {
    const fn new(name: QName, attributes: Vec<(QName, String)>) -> Self {
        Self {
            name,
            attributes,
            children: Vec::new(),
            text: String::new(),
        }
    }

    fn push_text(&mut self, text: &str) {
        self.push_text_impl(text, true);
    }

    fn push_text_raw(&mut self, text: &str) {
        self.push_text_impl(text, false);
    }

    fn push_text_impl(&mut self, text: &str, should_trim: bool) {
        let content = if should_trim { text.trim() } else { text };
        if content.is_empty() {
            return;
        }
        self.text.push_str(content);
    }
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
                    .map_err(XmlError::InvalidUtf8)?
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

                    let (attr_resolve, _) = reader.resolver().resolve_attribute(key);
                    let attr_ns = resolve_namespace(attr_resolve)?;
                    let attr_local = core::str::from_utf8(key.local_name().as_ref())
                        .map_err(XmlError::InvalidUtf8)?
                        .to_string();
                    let attr_qname = match attr_ns {
                        Some(uri) => QName::with_ns(uri, attr_local),
                        None => QName::local(attr_local),
                    };
                    let value = attr
                        .decode_and_unescape_value(reader.decoder())
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
                        .decode()
                        .map_err(|err| XmlError::ParseError(err.to_string()))?;
                    current.push_text(text.as_ref());
                }
            }
            Event::CData(e) => {
                if let Some(current) = stack.last_mut() {
                    let text = core::str::from_utf8(e.as_ref()).map_err(XmlError::InvalidUtf8)?;
                    current.push_text(text);
                }
            }
            Event::GeneralRef(e) => {
                // General entity references (e.g., &lt;, &gt;, &amp;, &#10;, etc.)
                // These are now reported separately in quick-xml 0.38+
                if let Some(current) = stack.last_mut() {
                    let raw = e
                        .decode()
                        .map_err(|err| XmlError::ParseError(err.to_string()))?;
                    let resolved = resolve_entity(&raw)?;
                    // Don't trim entity references - they may be intentional whitespace/control chars
                    current.push_text_raw(&resolved);
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
    let mut events = Vec::new();
    emit_element_events(&root, &mut events);
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

/// Emit ParseEvents directly from an Element, without intermediate XmlValue.
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

    // Case 2: No attributes, multiple children with same name - emit as array
    if !has_attrs && has_children && text.is_empty() && elem.children.len() > 1 {
        let first = &elem.children[0].name;
        if elem.children.iter().all(|child| &child.name == first) {
            events.push(ParseEvent::SequenceStart(ContainerKind::Element));
            for child in &elem.children {
                emit_element_events(child, events);
            }
            events.push(ParseEvent::SequenceEnd);
            return;
        }
    }

    // Case 3: Has attributes or mixed children - emit as struct
    events.push(ParseEvent::StructStart(ContainerKind::Element));

    // Emit attributes as fields
    for (qname, value) in &elem.attributes {
        let mut key = FieldKey::new(
            Cow::Owned(qname.local_name.clone()),
            FieldLocationHint::Attribute,
        );
        if let Some(ns) = &qname.namespace {
            key = key.with_namespace(Cow::Owned(ns.clone()));
        }
        events.push(ParseEvent::FieldKey(key));
        // Attributes are always strings
        events.push(ParseEvent::Scalar(ScalarValue::Str(Cow::Owned(
            value.clone(),
        ))));
    }

    // Emit children in order (preserving document order for xml::elements support)
    // The deserializer is responsible for grouping same-named children into arrays
    // or collecting them into xml::elements fields.
    for child in &elem.children {
        let mut key = FieldKey::new(
            Cow::Owned(child.name.local_name.clone()),
            FieldLocationHint::Child,
        );
        if let Some(ns) = &child.name.namespace {
            key = key.with_namespace(Cow::Owned(ns.clone()));
        }
        events.push(ParseEvent::FieldKey(key));
        emit_element_events(child, events);
    }

    // Emit text content if present (mixed content)
    if !text.is_empty() {
        let key = FieldKey::new(Cow::Borrowed("_text"), FieldLocationHint::Text);
        events.push(ParseEvent::FieldKey(key));
        events.push(ParseEvent::Scalar(ScalarValue::Str(Cow::Owned(
            text.to_string(),
        ))));
    }

    events.push(ParseEvent::StructEnd);
}

/// Emit text content as a stringly-typed scalar.
///
/// XML text content is inherently ambiguous - `<value>42</value>` could be an integer,
/// float, or string depending on the target type. We emit `StringlyTyped` and let the
/// deserializer determine the actual type based on what it's deserializing into.
fn emit_scalar_from_text<'de>(text: &str, events: &mut Vec<ParseEvent<'de>>) {
    events.push(ParseEvent::Scalar(ScalarValue::StringlyTyped(Cow::Owned(
        text.to_string(),
    ))));
}
