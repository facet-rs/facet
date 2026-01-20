//! Streaming DomParser implementation for XML using quick-xml.

extern crate alloc;

use alloc::borrow::Cow;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;
use std::io::Cursor;

use facet_dom::{DomEvent, DomParser};
use quick_xml::NsReader;
use quick_xml::escape::resolve_xml_entity;
use quick_xml::events::Event;
use quick_xml::name::ResolveResult;

/// XML parsing error.
#[derive(Debug, Clone)]
pub enum XmlError {
    /// Error from quick-xml.
    Parse(String),
    /// Unexpected end of input.
    UnexpectedEof,
    /// Unbalanced tags.
    UnbalancedTags,
    /// Invalid UTF-8.
    InvalidUtf8(core::str::Utf8Error),
}

impl fmt::Display for XmlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            XmlError::Parse(msg) => write!(f, "XML parse error: {}", msg),
            XmlError::UnexpectedEof => write!(f, "Unexpected end of XML"),
            XmlError::UnbalancedTags => write!(f, "Unbalanced XML tags"),
            XmlError::InvalidUtf8(e) => write!(f, "Invalid UTF-8 in XML: {}", e),
        }
    }
}

impl std::error::Error for XmlError {}

/// Streaming XML parser implementing `DomParser`.
pub struct XmlParser<'de> {
    reader: NsReader<Cursor<&'de [u8]>>,
    /// Buffer for quick-xml events
    buf: Vec<u8>,
    /// Buffer for peeked event
    peeked: Option<DomEvent<'de>>,
    /// Stack tracking element depth for skip_node
    depth: usize,
    /// Pending attributes from the current element
    pending_attrs: Vec<(Option<String>, String, String)>,
    /// Index into pending_attrs
    attr_idx: usize,
    /// State machine for event generation
    state: ParserState,
    /// True if current element is empty (self-closing)
    is_empty_element: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum ParserState {
    /// Ready to read next XML event
    Ready,
    /// Just emitted NodeStart, need to emit attributes
    EmittingAttrs,
    /// Done emitting attrs, need to emit ChildrenStart
    NeedChildrenStart,
    /// Inside element children
    InChildren,
    /// Need to emit ChildrenEnd before NodeEnd
    NeedChildrenEnd,
    /// Need to emit NodeEnd
    NeedNodeEnd,
    /// End of document
    Done,
}

impl<'de> XmlParser<'de> {
    /// Create a new streaming XML parser.
    pub fn new(input: &'de [u8]) -> Self {
        trace!(input_len = input.len(), "creating XML parser");

        let mut reader = NsReader::from_reader(Cursor::new(input));
        reader.config_mut().trim_text(true);

        Self {
            reader,
            buf: Vec::new(),
            peeked: None,
            depth: 0,
            pending_attrs: Vec::new(),
            attr_idx: 0,
            state: ParserState::Ready,
            is_empty_element: false,
        }
    }

    /// Read the next raw event from quick-xml and convert to DomEvent.
    fn read_next(&mut self) -> Result<Option<DomEvent<'de>>, XmlError> {
        loop {
            match self.state {
                ParserState::Done => return Ok(None),

                ParserState::EmittingAttrs => {
                    if self.attr_idx < self.pending_attrs.len() {
                        let (ns, name, value) = &self.pending_attrs[self.attr_idx];
                        let event = DomEvent::Attribute {
                            name: Cow::Owned(name.clone()),
                            value: Cow::Owned(value.clone()),
                            namespace: ns.clone().map(Cow::Owned),
                        };
                        self.attr_idx += 1;
                        return Ok(Some(event));
                    }
                    // Done with attrs
                    self.pending_attrs.clear();
                    self.attr_idx = 0;
                    self.state = ParserState::NeedChildrenStart;
                }

                ParserState::NeedChildrenStart => {
                    if self.is_empty_element {
                        self.state = ParserState::NeedChildrenEnd;
                        self.is_empty_element = false;
                    } else {
                        self.state = ParserState::InChildren;
                    }
                    return Ok(Some(DomEvent::ChildrenStart));
                }

                ParserState::NeedChildrenEnd => {
                    self.state = ParserState::NeedNodeEnd;
                    return Ok(Some(DomEvent::ChildrenEnd));
                }

                ParserState::NeedNodeEnd => {
                    self.depth -= 1;
                    self.state = if self.depth == 0 {
                        ParserState::Done
                    } else {
                        ParserState::InChildren
                    };
                    return Ok(Some(DomEvent::NodeEnd));
                }

                ParserState::Ready | ParserState::InChildren => {
                    self.buf.clear();
                    let (resolve, event) = self
                        .reader
                        .read_resolved_event_into(&mut self.buf)
                        .map_err(|e| XmlError::Parse(e.to_string()))?;

                    // Resolve element namespace upfront
                    let elem_ns = resolve_namespace(resolve)?;

                    match event {
                        Event::Start(ref e) | Event::Empty(ref e) => {
                            let is_empty = matches!(event, Event::Empty(_));

                            // Get element local name
                            let local_name = e.local_name();
                            let local = core::str::from_utf8(local_name.as_ref())
                                .map_err(XmlError::InvalidUtf8)?;
                            let local_owned = local.to_string();

                            // Collect attributes
                            self.pending_attrs.clear();
                            self.attr_idx = 0;

                            for attr in e.attributes() {
                                let attr = attr.map_err(|e| XmlError::Parse(e.to_string()))?;

                                // Skip xmlns declarations
                                let key = attr.key;
                                if key.as_ref() == b"xmlns" {
                                    continue;
                                }
                                if let Some(prefix) = key.prefix()
                                    && prefix.as_ref() == b"xmlns"
                                {
                                    continue;
                                }

                                let (attr_resolve, _) =
                                    self.reader.resolver().resolve_attribute(key);
                                let attr_ns = resolve_namespace(attr_resolve)?;
                                let attr_local_name = key.local_name();
                                let attr_local = core::str::from_utf8(attr_local_name.as_ref())
                                    .map_err(XmlError::InvalidUtf8)?;
                                let value = attr
                                    .unescape_value()
                                    .map_err(|e| XmlError::Parse(e.to_string()))?;

                                self.pending_attrs.push((
                                    attr_ns,
                                    attr_local.to_string(),
                                    value.into_owned(),
                                ));
                            }

                            self.depth += 1;
                            self.is_empty_element = is_empty;

                            if self.pending_attrs.is_empty() {
                                self.state = ParserState::NeedChildrenStart;
                            } else {
                                self.state = ParserState::EmittingAttrs;
                            }

                            return Ok(Some(DomEvent::NodeStart {
                                tag: Cow::Owned(local_owned),
                                namespace: elem_ns.map(Cow::Owned),
                            }));
                        }
                        Event::End(_) => {
                            self.state = ParserState::NeedChildrenEnd;
                        }
                        Event::Text(e) => {
                            let text = e.decode().map_err(|e| XmlError::Parse(e.to_string()))?;
                            let trimmed = text.trim();
                            if !trimmed.is_empty() {
                                return Ok(Some(DomEvent::Text(Cow::Owned(trimmed.to_string()))));
                            }
                        }
                        Event::CData(e) => {
                            let text =
                                core::str::from_utf8(e.as_ref()).map_err(XmlError::InvalidUtf8)?;
                            if !text.is_empty() {
                                return Ok(Some(DomEvent::Text(Cow::Owned(text.to_string()))));
                            }
                        }
                        Event::Comment(e) => {
                            let text =
                                core::str::from_utf8(e.as_ref()).map_err(XmlError::InvalidUtf8)?;
                            return Ok(Some(DomEvent::Comment(Cow::Owned(text.to_string()))));
                        }
                        Event::PI(e) => {
                            let content =
                                core::str::from_utf8(e.as_ref()).map_err(XmlError::InvalidUtf8)?;
                            let (target, data) = content
                                .split_once(char::is_whitespace)
                                .unwrap_or((content, ""));
                            return Ok(Some(DomEvent::ProcessingInstruction {
                                target: Cow::Owned(target.to_string()),
                                data: Cow::Owned(data.trim().to_string()),
                            }));
                        }
                        Event::Decl(_) => {
                            // XML declaration - skip
                        }
                        Event::DocType(_e) => {
                            // Skip the DocType until there is support in facet-dom. facet-html is able to inject the
                            // doctype onto a root node but that is a good option for XML.
                            // let text = core::str::from_utf8(e.as_ref()).map_err(XmlError::InvalidUtf8)?;
                            // return Ok(Some(DomEvent::Doctype(Cow::Owned(text.to_string()))));
                        }
                        Event::Eof => {
                            self.state = ParserState::Done;
                            return Ok(None);
                        }
                        Event::GeneralRef(e) => {
                            let raw = e.decode().map_err(|e| XmlError::Parse(e.to_string()))?;
                            let resolved = resolve_entity(&raw)?;
                            return Ok(Some(DomEvent::Text(Cow::Owned(resolved))));
                        }
                    }
                }
            }
        }
    }
}

impl<'de> DomParser<'de> for XmlParser<'de> {
    type Error = XmlError;

    fn next_event(&mut self) -> Result<Option<DomEvent<'de>>, Self::Error> {
        if let Some(event) = self.peeked.take() {
            return Ok(Some(event));
        }
        self.read_next()
    }

    fn peek_event(&mut self) -> Result<Option<&DomEvent<'de>>, Self::Error> {
        if self.peeked.is_none() {
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

    fn format_namespace(&self) -> Option<&'static str> {
        Some("xml")
    }
}

/// Resolve a namespace from quick-xml's ResolveResult.
fn resolve_namespace(resolve: ResolveResult<'_>) -> Result<Option<String>, XmlError> {
    match resolve {
        ResolveResult::Bound(ns) => Ok(Some(String::from_utf8_lossy(ns.as_ref()).into_owned())),
        ResolveResult::Unbound => Ok(None),
        ResolveResult::Unknown(_) => Ok(None),
    }
}

/// Resolve a general entity reference.
fn resolve_entity(raw: &str) -> Result<String, XmlError> {
    if let Some(resolved) = resolve_xml_entity(raw) {
        return Ok(resolved.into());
    }

    if let Some(rest) = raw.strip_prefix('#') {
        let code = if let Some(hex) = rest.strip_prefix('x').or_else(|| rest.strip_prefix('X')) {
            u32::from_str_radix(hex, 16)
                .map_err(|_| XmlError::Parse(format!("Invalid hex entity: #{}", rest)))?
        } else {
            rest.parse::<u32>()
                .map_err(|_| XmlError::Parse(format!("Invalid decimal entity: #{}", rest)))?
        };

        let ch = char::from_u32(code)
            .ok_or_else(|| XmlError::Parse(format!("Invalid Unicode: {}", code)))?;
        return Ok(ch.to_string());
    }

    Ok(format!("&{};", raw))
}
