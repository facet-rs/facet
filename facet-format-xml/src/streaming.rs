//! Streaming XML deserialization using stackful coroutines.
//!
//! This module provides `from_reader` that can deserialize XML from any `Read`
//! source without requiring the entire input to be in memory.
//!
//! The parser emits events incrementally as XML is parsed. Sequence detection
//! (multiple children with same name) is handled by the deserializer, not the parser.

#![allow(unsafe_code)]

extern crate alloc;

use alloc::borrow::Cow;
use alloc::rc::Rc;
use alloc::string::String;
use alloc::vec::Vec;
use core::cell::RefCell;
use std::io::BufRead;

use corosensei::{Coroutine, CoroutineResult};
use facet_core::Facet;
use facet_format::{
    ContainerKind, DeserializeError, FieldEvidence, FieldKey, FieldLocationHint,
    FormatDeserializer, FormatParser, ParseEvent, ProbeStream, ScalarValue,
};
use quick_xml::NsReader;
use quick_xml::escape::resolve_xml_entity;
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

/// State for an element being parsed.
struct ElementState {
    /// Accumulated text content
    text: String,
    /// Whether we've emitted StructStart for this element
    started: bool,
    /// Pending attributes (held until we decide if this is a struct)
    pending_attrs: Vec<(String, String)>,
}

/// Streaming XML parser that implements `FormatParser<'static>`.
///
/// Events are emitted incrementally as XML is parsed:
/// - `<element>` → StructStart, then attribute FieldKeys
/// - `<child>` → FieldKey(child_name), then child's StructStart...
/// - `</element>` → _text field if text, then StructEnd
pub struct StreamingXmlParser<'y> {
    reader: NsReader<YieldingReader<'y>>,
    xml_buf: Vec<u8>,
    /// Stack of element states
    element_stack: Vec<ElementState>,
    /// Buffered events for replay
    event_buffer: Vec<ParseEvent<'static>>,
    buffer_idx: usize,
    /// Peeked event
    peeked: Option<ParseEvent<'static>>,
}

impl<'y> StreamingXmlParser<'y> {
    fn new(reader: NsReader<YieldingReader<'y>>) -> Self {
        Self {
            reader,
            xml_buf: Vec::new(),
            element_stack: Vec::new(),
            event_buffer: Vec::new(),
            buffer_idx: 0,
            peeked: None,
        }
    }

    /// Ensure the parent element (if any) has been started as a struct.
    /// Called when we're about to emit a child element.
    fn ensure_parent_started(&mut self) {
        if let Some(parent) = self.element_stack.last_mut()
            && !parent.started
        {
            // Parent needs to become a struct now
            self.event_buffer
                .push(ParseEvent::StructStart(ContainerKind::Element));
            for (name, value) in parent.pending_attrs.drain(..) {
                let key = FieldKey::new(Cow::Owned(name), FieldLocationHint::Attribute);
                self.event_buffer.push(ParseEvent::FieldKey(key));
                self.event_buffer
                    .push(ParseEvent::Scalar(ScalarValue::Str(Cow::Owned(value))));
            }
            parent.started = true;
        }
    }

    /// Push a new element state (deferred - doesn't emit StructStart yet).
    fn push_element(&mut self, attrs: Vec<(String, String)>) {
        self.element_stack.push(ElementState {
            text: String::new(),
            started: false,
            pending_attrs: attrs,
        });
    }

    /// Collect attributes from a quick-xml element, skipping xmlns declarations.
    /// Note: Does not resolve namespaces for attributes (would require separate borrow).
    fn collect_attrs_simple(
        e: &quick_xml::events::BytesStart<'_>,
    ) -> Result<Vec<(String, String)>, XmlError> {
        let mut attrs = Vec::new();
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

            let name = core::str::from_utf8(key.local_name().as_ref())
                .map_err(|_| XmlError::InvalidUtf8)?
                .to_string();

            let value = attr
                .unescape_value()
                .map_err(|e| XmlError::ParseError(e.to_string()))?
                .into_owned();

            attrs.push((name, value));
        }
        Ok(attrs)
    }

    fn produce_event(&mut self) -> Result<Option<ParseEvent<'static>>, XmlError> {
        // First check buffered events
        if self.buffer_idx < self.event_buffer.len() {
            let event = self.event_buffer[self.buffer_idx].clone();
            self.buffer_idx += 1;
            if self.buffer_idx >= self.event_buffer.len() {
                self.event_buffer.clear();
                self.buffer_idx = 0;
            }
            return Ok(Some(event));
        }

        self.produce_event_from_xml()
    }

    /// Read events directly from XML, bypassing the buffer.
    /// Used for buffering ahead when probing.
    fn produce_event_from_xml(&mut self) -> Result<Option<ParseEvent<'static>>, XmlError> {
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

                    let attrs = Self::collect_attrs_simple(e)?;

                    // If we're inside an element, that parent must become a struct
                    if !self.element_stack.is_empty() {
                        self.ensure_parent_started();
                        let mut key =
                            FieldKey::new(Cow::Owned(local.clone()), FieldLocationHint::Child);
                        if let Some(ref ns) = ns {
                            key = key.with_namespace(Cow::Owned(ns.clone()));
                        }
                        self.event_buffer.push(ParseEvent::FieldKey(key));
                    }

                    // Push this element (deferred - don't emit StructStart yet)
                    self.push_element(attrs);

                    // Return first buffered event if any
                    if !self.event_buffer.is_empty() {
                        let ev = self.event_buffer[0].clone();
                        self.buffer_idx = 1;
                        if self.buffer_idx >= self.event_buffer.len() {
                            self.event_buffer.clear();
                            self.buffer_idx = 0;
                        }
                        return Ok(Some(ev));
                    }
                    continue;
                }

                Event::Empty(ref e) => {
                    let local = core::str::from_utf8(e.local_name().as_ref())
                        .map_err(|_| XmlError::InvalidUtf8)?
                        .to_string();

                    let attrs = Self::collect_attrs_simple(e)?;

                    // If we're inside an element, that parent must become a struct
                    if !self.element_stack.is_empty() {
                        self.ensure_parent_started();
                        let mut key =
                            FieldKey::new(Cow::Owned(local.clone()), FieldLocationHint::Child);
                        if let Some(ref ns) = ns {
                            key = key.with_namespace(Cow::Owned(ns.clone()));
                        }
                        self.event_buffer.push(ParseEvent::FieldKey(key));
                    }

                    // Empty element with attrs: emit as struct
                    // Empty element without attrs: emit as empty struct (or could be unit)
                    self.event_buffer
                        .push(ParseEvent::StructStart(ContainerKind::Element));
                    for (name, value) in attrs {
                        let key = FieldKey::new(Cow::Owned(name), FieldLocationHint::Attribute);
                        self.event_buffer.push(ParseEvent::FieldKey(key));
                        self.event_buffer
                            .push(ParseEvent::Scalar(ScalarValue::Str(Cow::Owned(value))));
                    }
                    self.event_buffer.push(ParseEvent::StructEnd);

                    // Return first buffered event
                    if !self.event_buffer.is_empty() {
                        let ev = self.event_buffer[0].clone();
                        self.buffer_idx = 1;
                        if self.buffer_idx >= self.event_buffer.len() {
                            self.event_buffer.clear();
                            self.buffer_idx = 0;
                        }
                        return Ok(Some(ev));
                    }
                    continue;
                }

                Event::Text(ref e) => {
                    let text = e
                        .decode()
                        .map_err(|e| XmlError::ParseError(e.to_string()))?;
                    let trimmed = text.trim();
                    if !trimmed.is_empty()
                        && let Some(state) = self.element_stack.last_mut()
                    {
                        if !state.text.is_empty() {
                            state.text.push(' ');
                        }
                        state.text.push_str(trimmed);
                    }
                    continue;
                }

                Event::CData(ref e) => {
                    let text =
                        core::str::from_utf8(e.as_ref()).map_err(|_| XmlError::InvalidUtf8)?;
                    let trimmed = text.trim();
                    if !trimmed.is_empty()
                        && let Some(state) = self.element_stack.last_mut()
                    {
                        if !state.text.is_empty() {
                            state.text.push(' ');
                        }
                        state.text.push_str(trimmed);
                    }
                    continue;
                }

                Event::End(_) => {
                    let state = self.element_stack.pop().ok_or(XmlError::UnbalancedTags)?;

                    if state.started {
                        // Element was started as struct (had children)
                        // Emit text as _text field if present
                        if !state.text.is_empty() {
                            self.event_buffer.push(ParseEvent::FieldKey(FieldKey::new(
                                Cow::Borrowed("_text"),
                                FieldLocationHint::Text,
                            )));
                            self.event_buffer.push(emit_scalar_from_text(&state.text));
                        }
                        self.event_buffer.push(ParseEvent::StructEnd);
                    } else if !state.pending_attrs.is_empty() {
                        // Element has attributes but no children - emit as struct
                        self.event_buffer
                            .push(ParseEvent::StructStart(ContainerKind::Element));
                        for (name, value) in state.pending_attrs {
                            let key = FieldKey::new(Cow::Owned(name), FieldLocationHint::Attribute);
                            self.event_buffer.push(ParseEvent::FieldKey(key));
                            self.event_buffer
                                .push(ParseEvent::Scalar(ScalarValue::Str(Cow::Owned(value))));
                        }
                        if !state.text.is_empty() {
                            self.event_buffer.push(ParseEvent::FieldKey(FieldKey::new(
                                Cow::Borrowed("_text"),
                                FieldLocationHint::Text,
                            )));
                            self.event_buffer.push(emit_scalar_from_text(&state.text));
                        }
                        self.event_buffer.push(ParseEvent::StructEnd);
                    } else {
                        // Element had no children and no attributes - emit as scalar or empty struct
                        if !state.text.is_empty() {
                            self.event_buffer.push(emit_scalar_from_text(&state.text));
                        } else {
                            self.event_buffer
                                .push(ParseEvent::StructStart(ContainerKind::Element));
                            self.event_buffer.push(ParseEvent::StructEnd);
                        }
                    }

                    // Return first buffered event
                    if !self.event_buffer.is_empty() {
                        let ev = self.event_buffer[0].clone();
                        self.buffer_idx = 1;
                        if self.buffer_idx >= self.event_buffer.len() {
                            self.event_buffer.clear();
                            self.buffer_idx = 0;
                        }
                        return Ok(Some(ev));
                    }
                    continue;
                }

                Event::GeneralRef(ref e) => {
                    // General entity references (e.g., &lt;, &gt;, &amp;, &#10;, etc.)
                    // These are now reported separately in quick-xml 0.38+
                    let raw = e
                        .decode()
                        .map_err(|e| XmlError::ParseError(e.to_string()))?;
                    let resolved = resolve_entity(&raw)?;
                    if let Some(state) = self.element_stack.last_mut() {
                        state.text.push_str(&resolved);
                    }
                    continue;
                }

                Event::Eof => {
                    if !self.element_stack.is_empty() {
                        return Err(XmlError::UnbalancedTags);
                    }
                    // Clean EOF - document is complete
                    return Ok(None);
                }

                Event::Decl(_) | Event::Comment(_) | Event::PI(_) | Event::DocType(_) => {
                    continue;
                }
            }
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

impl<'y> FormatParser<'static> for StreamingXmlParser<'y> {
    type Error = XmlError;
    type Probe<'a>
        = StreamingXmlProbe
    where
        Self: 'a;

    fn next_event(&mut self) -> Result<Option<ParseEvent<'static>>, Self::Error> {
        if let Some(event) = self.peeked.take() {
            return Ok(Some(event));
        }
        self.produce_event()
    }

    fn peek_event(&mut self) -> Result<Option<ParseEvent<'static>>, Self::Error> {
        if let Some(ref event) = self.peeked {
            return Ok(Some(event.clone()));
        }
        let event = self.produce_event()?;
        if let Some(ref e) = event {
            self.peeked = Some(e.clone());
        }
        Ok(event)
    }

    fn skip_value(&mut self) -> Result<(), Self::Error> {
        let mut depth = 0usize;
        loop {
            let event = self.next_event()?.ok_or(XmlError::UnexpectedEof)?;
            match event {
                ParseEvent::StructStart(_) | ParseEvent::SequenceStart(_) => depth += 1,
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
                ParseEvent::FieldKey(_) | ParseEvent::OrderedField => {
                    // Don't increment depth for FieldKey/OrderedField - it doesn't start a new value
                }
            }
        }
        Ok(())
    }

    fn begin_probe(&mut self) -> Result<Self::Probe<'_>, Self::Error> {
        // Buffer events until we have the full struct (including StructEnd at depth 0)
        self.buffer_until_struct_end()?;

        // Build field evidence from buffered events
        let evidence = self.build_probe();
        Ok(StreamingXmlProbe { evidence, idx: 0 })
    }
}

impl<'y> StreamingXmlParser<'y> {
    /// Buffer events until we see StructEnd at depth 0.
    /// This is called when probing needs to see all fields of the current struct.
    fn buffer_until_struct_end(&mut self) -> Result<(), XmlError> {
        // We need to collect events from current position until we see the closing StructEnd.
        // The tricky part is that produce_event() consumes from the buffer, so we need to
        // re-buffer the events we consume.
        let mut probe_events: Vec<ParseEvent<'static>> = Vec::new();

        // First, move any existing buffered events (including peeked) to probe_events
        if let Some(peeked) = self.peeked.take() {
            probe_events.push(peeked);
        }
        for event in self.event_buffer.drain(self.buffer_idx..) {
            probe_events.push(event);
        }
        self.buffer_idx = 0;
        self.event_buffer.clear();

        // Count depth in what we already have
        let mut depth = 0i32;
        for event in &probe_events {
            match event {
                ParseEvent::StructStart(_) | ParseEvent::SequenceStart(_) => depth += 1,
                ParseEvent::StructEnd | ParseEvent::SequenceEnd => {
                    depth -= 1;
                    if depth < 0 {
                        // We already have the closing StructEnd - put events back and return
                        self.event_buffer = probe_events;
                        self.buffer_idx = 0;
                        return Ok(());
                    }
                }
                _ => {}
            }
        }

        // Determine the target depth for stopping:
        // - If we start from scratch (depth=0 and first event is StructStart), stop when depth returns to 0
        // - If we're inside a struct (depth=0 but StructStart already consumed), stop when depth goes to -1
        let mut first_event = true;
        let mut started_from_struct_start = false;

        // Need to read more events from XML until we see the closing StructEnd
        loop {
            // Use produce_event which handles the XML parsing properly
            let event = self.produce_event()?.ok_or(XmlError::UnexpectedEof)?;

            // Track if we started from StructStart (probing before consuming root)
            if first_event {
                first_event = false;
                if matches!(event, ParseEvent::StructStart(ContainerKind::Element)) {
                    started_from_struct_start = true;
                }
            }

            let is_end = matches!(event, ParseEvent::StructEnd | ParseEvent::SequenceEnd);

            match &event {
                ParseEvent::StructStart(_) | ParseEvent::SequenceStart(_) => depth += 1,
                ParseEvent::StructEnd | ParseEvent::SequenceEnd => depth -= 1,
                _ => {}
            }

            probe_events.push(event);

            // Exit condition depends on how we started:
            // - If started from StructStart: exit when depth returns to 0
            // - Otherwise: exit when depth goes to -1
            let target_depth = if started_from_struct_start { 0 } else { -1 };
            if is_end && depth <= target_depth {
                // We've collected up to and including the closing StructEnd
                // Put all events back into the buffer for normal consumption
                self.event_buffer = probe_events;
                self.buffer_idx = 0;
                return Ok(());
            }
        }
    }

    /// Build field evidence by looking ahead at remaining events.
    fn build_probe(&self) -> Vec<FieldEvidence<'static>> {
        let mut evidence = Vec::new();

        // Get the events we're looking at (peeked + remaining buffer)
        let events: Vec<&ParseEvent<'static>> = if let Some(ref peeked) = self.peeked {
            core::iter::once(peeked)
                .chain(self.event_buffer[self.buffer_idx..].iter())
                .collect()
        } else {
            self.event_buffer[self.buffer_idx..].iter().collect()
        };

        if events.is_empty() {
            return evidence;
        }

        // Determine the target depth for finding top-level fields:
        // - If first event is StructStart, find fields at depth 1 (inside the struct)
        // - Otherwise, we're already inside a struct, find fields at depth 0
        let target_depth = if matches!(
            events.first(),
            Some(ParseEvent::StructStart(ContainerKind::Element))
        ) {
            1
        } else {
            0
        };

        let mut i = 0;
        let mut depth = 0usize;

        while i < events.len() {
            match events[i] {
                ParseEvent::StructStart(_) | ParseEvent::SequenceStart(_) => {
                    depth += 1;
                    i += 1;
                }
                ParseEvent::StructEnd | ParseEvent::SequenceEnd => {
                    if depth == 0 || (target_depth == 1 && depth == 1) {
                        break;
                    }
                    depth -= 1;
                    i += 1;
                }
                ParseEvent::FieldKey(key) if depth == target_depth => {
                    // This is a top-level field in the struct we're probing
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

    #[test]
    fn test_from_reader_list() {
        let xml = b"<numbers><value>1</value><value>2</value><value>3</value></numbers>";
        let reader = Cursor::new(&xml[..]);
        let result: Vec<u64> = from_reader(reader).unwrap();

        assert_eq!(result, vec![1, 2, 3]);
    }

    #[test]
    fn test_from_reader_internally_tagged_enum() {
        #[derive(Facet, Debug, PartialEq)]
        #[facet(tag = "type")]
        #[repr(C)]
        enum Shape {
            Circle { radius: f64 },
            Rectangle { width: f64, height: f64 },
        }

        let xml = b"<shape><type>Circle</type><radius>5.0</radius></shape>";
        let reader = Cursor::new(&xml[..]);
        let result: Shape = from_reader(reader).unwrap();

        assert_eq!(result, Shape::Circle { radius: 5.0 });
    }
}
