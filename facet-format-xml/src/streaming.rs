//! Streaming XML deserialization using stackful coroutines.
//!
//! This module provides `from_reader` that can deserialize XML from any `Read`
//! source without requiring the entire input to be in memory.
//!
//! Note: XML structure requires buffering sibling elements to detect sequences
//! (when all children have the same name). This provides streaming I/O but
//! buffers at each element level for structure detection.

#![allow(unsafe_code)]

extern crate alloc;

use alloc::borrow::Cow;
use alloc::collections::BTreeMap;
use alloc::rc::Rc;
use alloc::string::String;
use alloc::vec::Vec;
use core::cell::RefCell;
use std::io::BufRead;

use corosensei::{Coroutine, CoroutineResult};
use facet_core::Facet;
use facet_format::{
    DeserializeError, FieldEvidence, FieldKey, FieldLocationHint, FormatDeserializer, FormatParser,
    ParseEvent, ProbeStream, ScalarValue,
};
use quick_xml::NsReader;
use quick_xml::events::Event;
use quick_xml::name::ResolveResult;

use crate::XmlError;

/// Buffer for streaming reads
struct StreamBuffer {
    data: Vec<u8>,
    filled: usize,
    eof: bool,
}

impl StreamBuffer {
    fn new() -> Self {
        Self {
            data: vec![0u8; 8192],
            filled: 0,
            eof: false,
        }
    }

    fn data(&self) -> &[u8] {
        &self.data[..self.filled]
    }

    fn is_eof(&self) -> bool {
        self.eof
    }

    fn reset(&mut self) {
        self.filled = 0;
    }

    #[cfg(feature = "std")]
    fn refill<R: std::io::Read>(&mut self, reader: &mut R) -> std::io::Result<usize> {
        if self.eof {
            return Ok(0);
        }
        self.reset();
        let n = reader.read(&mut self.data)?;
        self.filled = n;
        if n == 0 {
            self.eof = true;
        }
        Ok(n)
    }

    #[cfg(feature = "tokio")]
    async fn refill_tokio<R: tokio::io::AsyncRead + Unpin>(
        &mut self,
        reader: &mut R,
    ) -> std::io::Result<usize> {
        use tokio::io::AsyncReadExt;
        if self.eof {
            return Ok(0);
        }
        self.reset();
        let n = reader.read(&mut self.data).await?;
        self.filled = n;
        if n == 0 {
            self.eof = true;
        }
        Ok(n)
    }
}

/// A BufRead implementation that yields via coroutine when more data is needed.
struct YieldingReader<'y> {
    buffer: Rc<RefCell<StreamBuffer>>,
    pos: usize,
    yielder: &'y corosensei::Yielder<(), ()>,
}

impl<'y> YieldingReader<'y> {
    fn new(buffer: Rc<RefCell<StreamBuffer>>, yielder: &'y corosensei::Yielder<(), ()>) -> Self {
        Self {
            buffer,
            pos: 0,
            yielder,
        }
    }
}

impl<'y> std::io::Read for YieldingReader<'y> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let available = self.fill_buf()?;
        let to_copy = buf.len().min(available.len());
        buf[..to_copy].copy_from_slice(&available[..to_copy]);
        self.consume(to_copy);
        Ok(to_copy)
    }
}

impl<'y> std::io::BufRead for YieldingReader<'y> {
    fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
        loop {
            let buf = self.buffer.borrow();
            if self.pos < buf.filled {
                // SAFETY: We're returning a reference to data that won't move
                // because we hold the Rc and the buffer only grows
                let data = buf.data();
                let slice = &data[self.pos..];
                // We need to extend the lifetime - this is safe because the buffer
                // is held by Rc and won't be deallocated
                let slice: &[u8] = unsafe { core::mem::transmute(slice) };
                return Ok(slice);
            }
            if buf.is_eof() {
                return Ok(&[]);
            }
            drop(buf);
            // Need more data - yield to driver
            self.pos = 0;
            self.yielder.suspend(());
        }
    }

    fn consume(&mut self, amt: usize) {
        self.pos += amt;
    }
}

/// Qualified name with optional namespace.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct QName {
    local_name: String,
    namespace: Option<String>,
}

/// Parsed XML element with all its content.
#[derive(Debug, Clone)]
struct Element {
    name: QName,
    attributes: Vec<(QName, String)>,
    children: Vec<Element>,
    text: String,
}

/// Streaming XML parser that implements `FormatParser<'static>`.
pub struct StreamingXmlParser<'y> {
    reader: NsReader<YieldingReader<'y>>,
    xml_buf: Vec<u8>,
    /// Buffered events for replay
    event_buffer: Vec<ParseEvent<'static>>,
    buffer_idx: usize,
    /// Peeked event
    peeked: Option<ParseEvent<'static>>,
    /// Whether we've parsed the root element
    root_parsed: bool,
}

impl<'y> StreamingXmlParser<'y> {
    fn new(reader: NsReader<YieldingReader<'y>>) -> Self {
        Self {
            reader,
            xml_buf: Vec::new(),
            event_buffer: Vec::new(),
            buffer_idx: 0,
            peeked: None,
            root_parsed: false,
        }
    }

    /// Parse an element and all its children into an Element structure.
    fn parse_element(&mut self) -> Result<Option<Element>, XmlError> {
        loop {
            self.xml_buf.clear();
            let (resolve, event) = self
                .reader
                .read_resolved_event_into(&mut self.xml_buf)
                .map_err(|e| XmlError::ParseError(e.to_string()))?;

            let ns: Option<String> = match resolve {
                ResolveResult::Bound(ns) => Some(String::from_utf8_lossy(ns.as_ref()).into_owned()),
                ResolveResult::Unbound | ResolveResult::Unknown(_) => None,
            };

            match event {
                Event::Start(ref e) => {
                    let local = core::str::from_utf8(e.local_name().as_ref())
                        .map_err(|_| XmlError::InvalidUtf8)?
                        .to_string();

                    // Collect attributes
                    let mut attributes = Vec::new();
                    for attr in e.attributes() {
                        let attr = attr.map_err(|e| XmlError::ParseError(e.to_string()))?;
                        let key = attr.key;
                        // Skip xmlns declarations
                        if key.as_ref() == b"xmlns" {
                            continue;
                        }
                        if let Some(prefix) = key.prefix()
                            && prefix.as_ref() == b"xmlns"
                        {
                            continue;
                        }
                        let attr_local = core::str::from_utf8(key.local_name().as_ref())
                            .map_err(|_| XmlError::InvalidUtf8)?
                            .to_string();

                        // Get attribute namespace
                        let attr_ns = if let Some(_prefix) = key.prefix() {
                            // Look up namespace for prefixed attribute
                            match self.reader.resolve(key, true) {
                                (ResolveResult::Bound(ns), _) => {
                                    Some(String::from_utf8_lossy(ns.as_ref()).into_owned())
                                }
                                _ => None,
                            }
                        } else {
                            None // Unprefixed attributes have no namespace
                        };

                        let value = attr
                            .unescape_value()
                            .map_err(|e| XmlError::ParseError(e.to_string()))?
                            .into_owned();

                        attributes.push((
                            QName {
                                local_name: attr_local,
                                namespace: attr_ns,
                            },
                            value,
                        ));
                    }

                    // Parse children until we hit the end tag
                    let mut children = Vec::new();
                    let mut text = String::new();

                    loop {
                        // Peek at next event
                        self.xml_buf.clear();
                        let (child_resolve, child_event) = self
                            .reader
                            .read_resolved_event_into(&mut self.xml_buf)
                            .map_err(|e| XmlError::ParseError(e.to_string()))?;

                        match child_event {
                            Event::End(_) => break,
                            Event::Text(ref t) => {
                                let s = t
                                    .unescape()
                                    .map_err(|e| XmlError::ParseError(e.to_string()))?;
                                let trimmed = s.trim();
                                if !trimmed.is_empty() {
                                    if !text.is_empty() {
                                        text.push(' ');
                                    }
                                    text.push_str(trimmed);
                                }
                            }
                            Event::CData(ref c) => {
                                let s = core::str::from_utf8(c.as_ref())
                                    .map_err(|_| XmlError::InvalidUtf8)?;
                                let trimmed = s.trim();
                                if !trimmed.is_empty() {
                                    if !text.is_empty() {
                                        text.push(' ');
                                    }
                                    text.push_str(trimmed);
                                }
                            }
                            Event::Start(ref child_e) => {
                                // Recursively parse child element
                                let child_local =
                                    core::str::from_utf8(child_e.local_name().as_ref())
                                        .map_err(|_| XmlError::InvalidUtf8)?
                                        .to_string();

                                let child_ns: Option<String> = match child_resolve {
                                    ResolveResult::Bound(ns) => {
                                        Some(String::from_utf8_lossy(ns.as_ref()).into_owned())
                                    }
                                    ResolveResult::Unbound | ResolveResult::Unknown(_) => None,
                                };

                                // Collect child attributes
                                let mut child_attrs = Vec::new();
                                for attr in child_e.attributes() {
                                    let attr =
                                        attr.map_err(|e| XmlError::ParseError(e.to_string()))?;
                                    let key = attr.key;
                                    if key.as_ref() == b"xmlns" {
                                        continue;
                                    }
                                    if let Some(prefix) = key.prefix()
                                        && prefix.as_ref() == b"xmlns"
                                    {
                                        continue;
                                    }
                                    let attr_local =
                                        core::str::from_utf8(key.local_name().as_ref())
                                            .map_err(|_| XmlError::InvalidUtf8)?
                                            .to_string();
                                    let value = attr
                                        .unescape_value()
                                        .map_err(|e| XmlError::ParseError(e.to_string()))?
                                        .into_owned();
                                    child_attrs.push((
                                        QName {
                                            local_name: attr_local,
                                            namespace: None,
                                        },
                                        value,
                                    ));
                                }

                                // Parse child's content recursively
                                let mut child_children = Vec::new();
                                let mut child_text = String::new();
                                self.parse_element_content(&mut child_children, &mut child_text)?;

                                children.push(Element {
                                    name: QName {
                                        local_name: child_local,
                                        namespace: child_ns,
                                    },
                                    attributes: child_attrs,
                                    children: child_children,
                                    text: child_text,
                                });
                            }
                            Event::Empty(ref child_e) => {
                                let child_local =
                                    core::str::from_utf8(child_e.local_name().as_ref())
                                        .map_err(|_| XmlError::InvalidUtf8)?
                                        .to_string();

                                let child_ns: Option<String> = match child_resolve {
                                    ResolveResult::Bound(ns) => {
                                        Some(String::from_utf8_lossy(ns.as_ref()).into_owned())
                                    }
                                    ResolveResult::Unbound | ResolveResult::Unknown(_) => None,
                                };

                                let mut child_attrs = Vec::new();
                                for attr in child_e.attributes() {
                                    let attr =
                                        attr.map_err(|e| XmlError::ParseError(e.to_string()))?;
                                    let key = attr.key;
                                    if key.as_ref() == b"xmlns" {
                                        continue;
                                    }
                                    if let Some(prefix) = key.prefix()
                                        && prefix.as_ref() == b"xmlns"
                                    {
                                        continue;
                                    }
                                    let attr_local =
                                        core::str::from_utf8(key.local_name().as_ref())
                                            .map_err(|_| XmlError::InvalidUtf8)?
                                            .to_string();
                                    let value = attr
                                        .unescape_value()
                                        .map_err(|e| XmlError::ParseError(e.to_string()))?
                                        .into_owned();
                                    child_attrs.push((
                                        QName {
                                            local_name: attr_local,
                                            namespace: None,
                                        },
                                        value,
                                    ));
                                }

                                children.push(Element {
                                    name: QName {
                                        local_name: child_local,
                                        namespace: child_ns,
                                    },
                                    attributes: child_attrs,
                                    children: Vec::new(),
                                    text: String::new(),
                                });
                            }
                            Event::Eof => return Err(XmlError::UnbalancedTags),
                            Event::Comment(_)
                            | Event::PI(_)
                            | Event::Decl(_)
                            | Event::DocType(_) => {
                                continue;
                            }
                        }
                    }

                    return Ok(Some(Element {
                        name: QName {
                            local_name: local,
                            namespace: ns,
                        },
                        attributes,
                        children,
                        text,
                    }));
                }

                Event::Empty(ref e) => {
                    let local = core::str::from_utf8(e.local_name().as_ref())
                        .map_err(|_| XmlError::InvalidUtf8)?
                        .to_string();

                    let mut attributes = Vec::new();
                    for attr in e.attributes() {
                        let attr = attr.map_err(|e| XmlError::ParseError(e.to_string()))?;
                        let key = attr.key;
                        if key.as_ref() == b"xmlns" {
                            continue;
                        }
                        if let Some(prefix) = key.prefix()
                            && prefix.as_ref() == b"xmlns"
                        {
                            continue;
                        }
                        let attr_local = core::str::from_utf8(key.local_name().as_ref())
                            .map_err(|_| XmlError::InvalidUtf8)?
                            .to_string();
                        let value = attr
                            .unescape_value()
                            .map_err(|e| XmlError::ParseError(e.to_string()))?
                            .into_owned();
                        attributes.push((
                            QName {
                                local_name: attr_local,
                                namespace: None,
                            },
                            value,
                        ));
                    }

                    return Ok(Some(Element {
                        name: QName {
                            local_name: local,
                            namespace: ns,
                        },
                        attributes,
                        children: Vec::new(),
                        text: String::new(),
                    }));
                }

                Event::Eof => return Ok(None),
                Event::Comment(_)
                | Event::PI(_)
                | Event::Decl(_)
                | Event::DocType(_)
                | Event::Text(_)
                | Event::CData(_) => {
                    continue;
                }
                Event::End(_) => return Err(XmlError::UnbalancedTags),
            }
        }
    }

    /// Parse element content (children and text) until end tag.
    fn parse_element_content(
        &mut self,
        children: &mut Vec<Element>,
        text: &mut String,
    ) -> Result<(), XmlError> {
        loop {
            self.xml_buf.clear();
            let (resolve, event) = self
                .reader
                .read_resolved_event_into(&mut self.xml_buf)
                .map_err(|e| XmlError::ParseError(e.to_string()))?;

            match event {
                Event::End(_) => return Ok(()),
                Event::Text(ref t) => {
                    let s = t
                        .unescape()
                        .map_err(|e| XmlError::ParseError(e.to_string()))?;
                    let trimmed = s.trim();
                    if !trimmed.is_empty() {
                        if !text.is_empty() {
                            text.push(' ');
                        }
                        text.push_str(trimmed);
                    }
                }
                Event::CData(ref c) => {
                    let s = core::str::from_utf8(c.as_ref()).map_err(|_| XmlError::InvalidUtf8)?;
                    let trimmed = s.trim();
                    if !trimmed.is_empty() {
                        if !text.is_empty() {
                            text.push(' ');
                        }
                        text.push_str(trimmed);
                    }
                }
                Event::Start(ref e) => {
                    let local = core::str::from_utf8(e.local_name().as_ref())
                        .map_err(|_| XmlError::InvalidUtf8)?
                        .to_string();

                    let ns: Option<String> = match resolve {
                        ResolveResult::Bound(ns) => {
                            Some(String::from_utf8_lossy(ns.as_ref()).into_owned())
                        }
                        ResolveResult::Unbound | ResolveResult::Unknown(_) => None,
                    };

                    let mut attrs = Vec::new();
                    for attr in e.attributes() {
                        let attr = attr.map_err(|e| XmlError::ParseError(e.to_string()))?;
                        let key = attr.key;
                        if key.as_ref() == b"xmlns" {
                            continue;
                        }
                        if let Some(prefix) = key.prefix()
                            && prefix.as_ref() == b"xmlns"
                        {
                            continue;
                        }
                        let attr_local = core::str::from_utf8(key.local_name().as_ref())
                            .map_err(|_| XmlError::InvalidUtf8)?
                            .to_string();
                        let value = attr
                            .unescape_value()
                            .map_err(|e| XmlError::ParseError(e.to_string()))?
                            .into_owned();
                        attrs.push((
                            QName {
                                local_name: attr_local,
                                namespace: None,
                            },
                            value,
                        ));
                    }

                    let mut child_children = Vec::new();
                    let mut child_text = String::new();
                    self.parse_element_content(&mut child_children, &mut child_text)?;

                    children.push(Element {
                        name: QName {
                            local_name: local,
                            namespace: ns,
                        },
                        attributes: attrs,
                        children: child_children,
                        text: child_text,
                    });
                }
                Event::Empty(ref e) => {
                    let local = core::str::from_utf8(e.local_name().as_ref())
                        .map_err(|_| XmlError::InvalidUtf8)?
                        .to_string();

                    let ns: Option<String> = match resolve {
                        ResolveResult::Bound(ns) => {
                            Some(String::from_utf8_lossy(ns.as_ref()).into_owned())
                        }
                        ResolveResult::Unbound | ResolveResult::Unknown(_) => None,
                    };

                    let mut attrs = Vec::new();
                    for attr in e.attributes() {
                        let attr = attr.map_err(|e| XmlError::ParseError(e.to_string()))?;
                        let key = attr.key;
                        if key.as_ref() == b"xmlns" {
                            continue;
                        }
                        if let Some(prefix) = key.prefix()
                            && prefix.as_ref() == b"xmlns"
                        {
                            continue;
                        }
                        let attr_local = core::str::from_utf8(key.local_name().as_ref())
                            .map_err(|_| XmlError::InvalidUtf8)?
                            .to_string();
                        let value = attr
                            .unescape_value()
                            .map_err(|e| XmlError::ParseError(e.to_string()))?
                            .into_owned();
                        attrs.push((
                            QName {
                                local_name: attr_local,
                                namespace: None,
                            },
                            value,
                        ));
                    }

                    children.push(Element {
                        name: QName {
                            local_name: local,
                            namespace: ns,
                        },
                        attributes: attrs,
                        children: Vec::new(),
                        text: String::new(),
                    });
                }
                Event::Eof => return Err(XmlError::UnbalancedTags),
                Event::Comment(_) | Event::PI(_) | Event::Decl(_) | Event::DocType(_) => continue,
            }
        }
    }

    fn produce_event(&mut self) -> Result<ParseEvent<'static>, XmlError> {
        // First check buffered events
        if self.buffer_idx < self.event_buffer.len() {
            let event = self.event_buffer[self.buffer_idx].clone();
            self.buffer_idx += 1;
            if self.buffer_idx >= self.event_buffer.len() {
                self.event_buffer.clear();
                self.buffer_idx = 0;
            }
            return Ok(event);
        }

        // Parse root element if not done yet
        if !self.root_parsed {
            self.root_parsed = true;
            if let Some(elem) = self.parse_element()? {
                emit_element_events(&elem, &mut self.event_buffer);
                if !self.event_buffer.is_empty() {
                    let event = self.event_buffer[0].clone();
                    self.buffer_idx = 1;
                    if self.buffer_idx >= self.event_buffer.len() {
                        self.event_buffer.clear();
                        self.buffer_idx = 0;
                    }
                    return Ok(event);
                }
            }
        }

        Err(XmlError::UnexpectedEof)
    }
}

fn emit_scalar_from_text(text: &str) -> ParseEvent<'static> {
    if text.eq_ignore_ascii_case("null") {
        return ParseEvent::Scalar(ScalarValue::Null);
    }
    if let Ok(b) = text.parse::<bool>() {
        return ParseEvent::Scalar(ScalarValue::Bool(b));
    }
    if let Ok(i) = text.parse::<i64>() {
        return ParseEvent::Scalar(ScalarValue::I64(i));
    }
    if let Ok(u) = text.parse::<u64>() {
        return ParseEvent::Scalar(ScalarValue::U64(u));
    }
    if text.parse::<i128>().is_ok() || text.parse::<u128>().is_ok() {
        return ParseEvent::Scalar(ScalarValue::Str(Cow::Owned(text.to_string())));
    }
    if let Ok(f) = text.parse::<f64>() {
        return ParseEvent::Scalar(ScalarValue::F64(f));
    }
    ParseEvent::Scalar(ScalarValue::Str(Cow::Owned(text.to_string())))
}

/// Emit parse events for an element (mirrors parser.rs logic).
fn emit_element_events(elem: &Element, events: &mut Vec<ParseEvent<'static>>) {
    let has_attrs = !elem.attributes.is_empty();
    let has_children = !elem.children.is_empty();
    let text = elem.text.as_str();

    // Case 1: No attributes, no children - emit scalar from text
    if !has_attrs && !has_children {
        if text.is_empty() {
            // Empty element is an empty object (for unit structs)
            events.push(ParseEvent::StructStart);
            events.push(ParseEvent::StructEnd);
        } else {
            events.push(emit_scalar_from_text(text));
        }
        return;
    }

    // Case 2: No attributes, multiple children with same name - emit as array
    if !has_attrs && has_children && text.is_empty() && elem.children.len() > 1 {
        let first = &elem.children[0].name;
        if elem.children.iter().all(|child| &child.name == first) {
            events.push(ParseEvent::SequenceStart);
            for child in &elem.children {
                emit_element_events(child, events);
            }
            events.push(ParseEvent::SequenceEnd);
            return;
        }
    }

    // Case 3: Has attributes or mixed children - emit as struct
    events.push(ParseEvent::StructStart);

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

    // Group children by (local_name, namespace) to detect arrays
    let mut grouped: BTreeMap<(&str, Option<&str>), Vec<&Element>> = BTreeMap::new();
    for child in &elem.children {
        let key = (
            child.name.local_name.as_str(),
            child.name.namespace.as_deref(),
        );
        grouped.entry(key).or_default().push(child);
    }

    // Emit children as fields
    for ((local_name, namespace), children) in grouped {
        let mut key = FieldKey::new(Cow::Owned(local_name.to_string()), FieldLocationHint::Child);
        if let Some(ns) = namespace {
            key = key.with_namespace(Cow::Owned(ns.to_string()));
        }
        events.push(ParseEvent::FieldKey(key));

        if children.len() == 1 {
            emit_element_events(children[0], events);
        } else {
            // Multiple children with same name -> array
            events.push(ParseEvent::SequenceStart);
            for child in children {
                emit_element_events(child, events);
            }
            events.push(ParseEvent::SequenceEnd);
        }
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

impl<'y> FormatParser<'static> for StreamingXmlParser<'y> {
    type Error = XmlError;
    type Probe<'a>
        = StreamingXmlProbe
    where
        Self: 'a;

    fn next_event(&mut self) -> Result<ParseEvent<'static>, Self::Error> {
        if let Some(event) = self.peeked.take() {
            return Ok(event);
        }
        self.produce_event()
    }

    fn peek_event(&mut self) -> Result<ParseEvent<'static>, Self::Error> {
        if let Some(ref event) = self.peeked {
            return Ok(event.clone());
        }
        let event = self.produce_event()?;
        self.peeked = Some(event.clone());
        Ok(event)
    }

    fn skip_value(&mut self) -> Result<(), Self::Error> {
        let mut depth = 0usize;
        loop {
            let event = self.next_event()?;
            match event {
                ParseEvent::StructStart | ParseEvent::SequenceStart => depth += 1,
                ParseEvent::StructEnd | ParseEvent::SequenceEnd => {
                    if depth == 0 {
                        break;
                    }
                    depth -= 1;
                }
                ParseEvent::Scalar(_) | ParseEvent::VariantTag(_) => {
                    if depth == 0 {
                        break;
                    }
                }
                ParseEvent::FieldKey(_) => {
                    depth += 1;
                }
            }
        }
        Ok(())
    }

    fn begin_probe(&mut self) -> Result<Self::Probe<'_>, Self::Error> {
        // Ensure we have parsed and buffered events
        if !self.root_parsed {
            self.root_parsed = true;
            if let Some(elem) = self.parse_element()? {
                emit_element_events(&elem, &mut self.event_buffer);
            }
        }

        // Build field evidence by looking ahead in the buffer
        let evidence = self.build_probe();
        Ok(StreamingXmlProbe { evidence, idx: 0 })
    }
}

impl<'y> StreamingXmlParser<'y> {
    /// Build field evidence by looking ahead at remaining events.
    fn build_probe(&self) -> Vec<FieldEvidence<'static>> {
        let mut evidence = Vec::new();

        // Get the events we're looking at
        let events: Vec<&ParseEvent<'static>> = if let Some(ref peeked) = self.peeked {
            // Include the peeked event plus remaining buffer
            core::iter::once(peeked)
                .chain(self.event_buffer[self.buffer_idx..].iter())
                .collect()
        } else {
            self.event_buffer[self.buffer_idx..].iter().collect()
        };

        if events.is_empty() {
            return evidence;
        }

        // Check if we're about to read a struct
        if !matches!(events.first(), Some(ParseEvent::StructStart)) {
            return evidence;
        }

        // Scan the struct's fields
        let mut i = 1;
        let mut depth = 0usize;

        while i < events.len() {
            match events[i] {
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
                    let scalar_value = if let Some(next_event) = events.get(i + 1) {
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

/// Probe for streaming XML parser.
pub struct StreamingXmlProbe {
    evidence: Vec<FieldEvidence<'static>>,
    idx: usize,
}

impl ProbeStream<'static> for StreamingXmlProbe {
    type Error = XmlError;

    fn next(&mut self) -> Result<Option<FieldEvidence<'static>>, Self::Error> {
        if self.idx >= self.evidence.len() {
            Ok(None)
        } else {
            let ev = self.evidence[self.idx].clone();
            self.idx += 1;
            Ok(Some(ev))
        }
    }
}

/// Deserialize XML from a synchronous reader.
#[cfg(feature = "std")]
pub fn from_reader<R, T>(mut reader: R) -> Result<T, DeserializeError<XmlError>>
where
    R: std::io::Read,
    T: Facet<'static>,
{
    let buffer = Rc::new(RefCell::new(StreamBuffer::new()));
    let buffer_for_coroutine = buffer.clone();

    // Initial fill
    {
        let mut buf = buffer.borrow_mut();
        let n = buf.refill(&mut reader).map_err(|e| {
            DeserializeError::Parser(XmlError::ParseError(format!("IO error: {}", e)))
        })?;
        if n == 0 {
            return Err(DeserializeError::Parser(XmlError::UnexpectedEof));
        }
    }

    let mut coroutine: Coroutine<(), (), Result<T, DeserializeError<XmlError>>> =
        Coroutine::new(move |yielder, ()| {
            let yielding_reader = YieldingReader::new(buffer_for_coroutine, yielder);
            let mut ns_reader = NsReader::from_reader(yielding_reader);
            ns_reader.config_mut().trim_text(true);
            let parser = StreamingXmlParser::new(ns_reader);
            let mut de = FormatDeserializer::new_owned(parser);
            de.deserialize_root::<T>()
        });

    loop {
        match coroutine.resume(()) {
            CoroutineResult::Yield(()) => {
                let mut buf = buffer.borrow_mut();
                let _n = buf.refill(&mut reader).map_err(|e| {
                    DeserializeError::Parser(XmlError::ParseError(format!("IO error: {}", e)))
                })?;
            }
            CoroutineResult::Return(result) => {
                return result;
            }
        }
    }
}

/// Deserialize XML from an async reader (tokio).
#[cfg(feature = "tokio")]
#[allow(clippy::await_holding_refcell_ref)]
pub async fn from_async_reader_tokio<R, T>(mut reader: R) -> Result<T, DeserializeError<XmlError>>
where
    R: tokio::io::AsyncRead + Unpin,
    T: Facet<'static>,
{
    let buffer = Rc::new(RefCell::new(StreamBuffer::new()));
    let buffer_for_coroutine = buffer.clone();

    // Initial fill
    {
        let mut buf = buffer.borrow_mut();
        let n = buf.refill_tokio(&mut reader).await.map_err(|e| {
            DeserializeError::Parser(XmlError::ParseError(format!("IO error: {}", e)))
        })?;
        if n == 0 {
            return Err(DeserializeError::Parser(XmlError::UnexpectedEof));
        }
    }

    let mut coroutine: Coroutine<(), (), Result<T, DeserializeError<XmlError>>> =
        Coroutine::new(move |yielder, ()| {
            let yielding_reader = YieldingReader::new(buffer_for_coroutine, yielder);
            let mut ns_reader = NsReader::from_reader(yielding_reader);
            ns_reader.config_mut().trim_text(true);
            let parser = StreamingXmlParser::new(ns_reader);
            let mut de = FormatDeserializer::new_owned(parser);
            de.deserialize_root::<T>()
        });

    loop {
        match coroutine.resume(()) {
            CoroutineResult::Yield(()) => {
                let mut buf = buffer.borrow_mut();
                let _n = buf.refill_tokio(&mut reader).await.map_err(|e| {
                    DeserializeError::Parser(XmlError::ParseError(format!("IO error: {}", e)))
                })?;
            }
            CoroutineResult::Return(result) => {
                return result;
            }
        }
    }
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;
    use facet::Facet;
    use std::io::Cursor;

    #[test]
    fn test_from_reader_simple() {
        #[derive(Facet, Debug, PartialEq)]
        struct Person {
            name: String,
            age: u32,
        }

        let xml = b"<person><name>Alice</name><age>30</age></person>";
        let reader = Cursor::new(&xml[..]);
        let person: Person = from_reader(reader).unwrap();

        assert_eq!(person.name, "Alice");
        assert_eq!(person.age, 30);
    }

    #[test]
    fn test_from_reader_nested() {
        #[derive(Facet, Debug, PartialEq)]
        struct Inner {
            value: i32,
        }

        #[derive(Facet, Debug, PartialEq)]
        struct Outer {
            inner: Inner,
        }

        let xml = b"<outer><inner><value>42</value></inner></outer>";
        let reader = Cursor::new(&xml[..]);
        let result: Outer = from_reader(reader).unwrap();

        assert_eq!(result.inner.value, 42);
    }
}
