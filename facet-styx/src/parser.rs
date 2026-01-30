//! Styx parser implementing the FormatParser trait.
//!
//! This module wraps the validated `Parser2` from `styx-parse` and converts its
//! events to `facet-format` parse events. This ensures that all Styx validation
//! (duplicate keys, mixed separators, invalid escapes, path validation, etc.)
//! is applied before deserialization.

use std::borrow::Cow;

use crate::trace;
use facet_core::Facet;
use facet_format::{
    ContainerKind, DeserializeErrorKind, FieldKey, FieldLocationHint, FormatParser, ParseError,
    ParseEvent, ParseEventKind, SavePoint, ScalarValue,
};
use facet_reflect::Span as ReflectSpan;
use styx_parse::{Event, ParseErrorKind, Parser, ScalarKind as StyxScalarKind, Span};

/// Streaming Styx parser implementing FormatParser.
///
/// This parser wraps `styx-parse::Parser2` which performs full validation
/// of the Styx syntax including:
/// - Duplicate key detection
/// - Mixed separator detection (commas vs newlines)
/// - Invalid escape sequence validation
/// - Dotted path validation (ReopenedPath, NestIntoTerminal)
/// - TooManyAtoms detection
#[derive(Clone)]
pub struct StyxParser<'de> {
    input: &'de str,
    inner: Parser<'de>,
    /// Peeked events queue (if any).
    peeked_events: Vec<ParseEvent<'de>>,
    /// Current span for error reporting.
    current_span: Option<Span>,
    /// Whether parsing is complete.
    complete: bool,
    /// Pending doc comments for the next field key.
    pending_doc: Vec<Cow<'de, str>>,
    /// Saved parser state for save/restore.
    saved_state: Option<Box<StyxParser<'de>>>,
    /// Whether we're at the implicit root level (for @schema skipping).
    at_implicit_root: bool,
    /// Depth of nested structures (for tracking when we leave root).
    depth: usize,
    /// Stack tracking whether each tag has seen a payload.
    /// When TagStart is seen, push false. When any payload event is seen, set top to true.
    /// When TagEnd is seen, if top is false, emit Scalar(Unit) for implicit unit.
    tag_has_payload_stack: Vec<bool>,
}

impl<'de> StyxParser<'de> {
    /// Create a new parser for the given source (document mode).
    pub fn new(source: &'de str) -> Self {
        Self {
            input: source,
            inner: Parser::new(source),
            peeked_events: Vec::new(),
            current_span: None,
            complete: false,
            tag_has_payload_stack: Vec::new(),
            pending_doc: Vec::new(),
            saved_state: None,
            at_implicit_root: true,
            depth: 0,
        }
    }

    /// Create a new parser in expression mode.
    ///
    /// Expression mode parses a single value rather than an implicit root object.
    /// Use this for parsing embedded values like default values in schemas.
    pub fn new_expr(source: &'de str) -> Self {
        Self {
            input: source,
            inner: Parser::new_expr(source),
            peeked_events: Vec::new(),
            current_span: None,
            complete: false,
            tag_has_payload_stack: Vec::new(),
            pending_doc: Vec::new(),
            saved_state: None,
            at_implicit_root: false, // Expression mode doesn't have implicit root
            depth: 0,
        }
    }

    /// Convert a Styx span to a facet_reflect span.
    fn to_reflect_span(&self, span: Span) -> ReflectSpan {
        ReflectSpan::new(span.start as usize, (span.end - span.start) as usize)
    }

    /// Get the text for a span.
    fn span_text(&self, span: Span) -> &'de str {
        &self.input[span.start as usize..span.end as usize]
    }

    /// Get the current span for event creation.
    fn event_span(&self) -> ReflectSpan {
        self.current_span
            .map(|s| self.to_reflect_span(s))
            .unwrap_or(ReflectSpan::new(0, 0))
    }

    /// Create a parse event with the current span.
    fn event(&self, kind: ParseEventKind<'de>) -> ParseEvent<'de> {
        ParseEvent::new(kind, self.event_span())
    }

    /// Mark that the current tag (if any) has seen a payload.
    fn mark_tag_has_payload(&mut self) {
        if let Some(last) = self.tag_has_payload_stack.last_mut() {
            *last = true;
        }
    }

    /// Create a parse error from a styx-parse error.
    fn make_error(&self, span: Span, kind: &ParseErrorKind) -> ParseError {
        let reflect_span = self.to_reflect_span(span);
        ParseError::new(
            reflect_span,
            DeserializeErrorKind::UnexpectedToken {
                got: kind.to_string().into(),
                expected: "valid syntax",
            },
        )
    }

    /// Parse a scalar value from text into a ScalarValue.
    fn parse_scalar(&self, value: Cow<'de, str>, kind: StyxScalarKind) -> ScalarValue<'de> {
        match kind {
            StyxScalarKind::Bare => {
                // Try to parse as number or bool
                if value == "true" {
                    ScalarValue::Bool(true)
                } else if value == "false" {
                    ScalarValue::Bool(false)
                } else if value == "null" {
                    ScalarValue::Null
                } else if let Ok(n) = value.parse::<i64>() {
                    ScalarValue::I64(n)
                } else if let Ok(n) = value.parse::<u64>() {
                    ScalarValue::U64(n)
                } else if let Ok(n) = value.parse::<f64>() {
                    ScalarValue::F64(n)
                } else {
                    // Bare identifier - treat as string
                    ScalarValue::Str(value)
                }
            }
            StyxScalarKind::Quoted | StyxScalarKind::Raw | StyxScalarKind::Heredoc => {
                // These are already unescaped by Parser2
                ScalarValue::Str(value)
            }
        }
    }

    /// Convert a styx-parse Event to facet-format ParseEvent(s).
    /// May queue additional events in peeked_events.
    /// Returns None if the event should be skipped (e.g., DocumentStart).
    fn convert_event(&mut self, event: Event<'de>) -> Result<Option<ParseEvent<'de>>, ParseError> {
        match event {
            Event::DocumentStart => {
                if self.at_implicit_root {
                    // Parser no longer emits ObjectStart for implicit root,
                    // so we synthesize StructStart here for the implicit root object.
                    self.depth += 1;
                    Ok(Some(
                        self.event(ParseEventKind::StructStart(ContainerKind::Object)),
                    ))
                } else {
                    // Expression mode - no implicit root, skip DocumentStart
                    Ok(None)
                }
            }

            Event::DocumentEnd => {
                if self.at_implicit_root {
                    // Parser no longer emits ObjectEnd for implicit root,
                    // so we synthesize StructEnd here for the implicit root object.
                    self.depth -= 1;
                    if self.depth == 0 {
                        self.at_implicit_root = false;
                    }
                    Ok(Some(self.event(ParseEventKind::StructEnd)))
                } else {
                    // Expression mode - no implicit root, skip DocumentEnd
                    Ok(None)
                }
            }

            Event::ObjectStart { span, .. } => {
                self.current_span = Some(span);
                self.depth += 1;
                self.mark_tag_has_payload();
                Ok(Some(
                    self.event(ParseEventKind::StructStart(ContainerKind::Object)),
                ))
            }

            Event::ObjectEnd { span } => {
                self.current_span = Some(span);
                self.depth -= 1;
                if self.depth == 0 {
                    self.at_implicit_root = false;
                }
                Ok(Some(self.event(ParseEventKind::StructEnd)))
            }

            Event::SequenceStart { span } => {
                self.current_span = Some(span);
                self.depth += 1;
                self.mark_tag_has_payload();
                Ok(Some(self.event(ParseEventKind::SequenceStart(
                    ContainerKind::Array,
                ))))
            }

            Event::SequenceEnd { span } => {
                self.current_span = Some(span);
                self.depth -= 1;
                Ok(Some(self.event(ParseEventKind::SequenceEnd)))
            }

            Event::EntryStart | Event::EntryEnd => {
                // These are structural markers not needed by facet-format
                Ok(None)
            }

            Event::Key {
                span,
                tag,
                payload,
                kind: _,
            } => {
                self.current_span = Some(span);

                // Handle @schema at implicit root - skip it
                if self.at_implicit_root && self.depth == 1 && tag == Some("schema") {
                    // Skip the @schema entry by consuming events until we're past it
                    self.skip_schema_value()?;
                    self.pending_doc.clear();
                    return Ok(None);
                }

                // Take any buffered doc comments
                let doc = std::mem::take(&mut self.pending_doc);

                let field_key = match (tag, payload) {
                    // Regular key: `name` or `"quoted name"`
                    (None, Some(name)) => {
                        FieldKey::with_doc(name, FieldLocationHint::KeyValue, doc)
                    }
                    // Tagged key: `@string`, `@int`, etc.
                    (Some(tag_name), None) => {
                        FieldKey::tagged_with_doc(tag_name, FieldLocationHint::KeyValue, doc)
                    }
                    // Unit key: `@` alone
                    (None, None) => FieldKey::unit_with_doc(FieldLocationHint::KeyValue, doc),
                    // Tagged key with payload - shouldn't happen for keys
                    (Some(tag_name), Some(_payload)) => {
                        // Treat as tagged key, ignore payload
                        FieldKey::tagged_with_doc(tag_name, FieldLocationHint::KeyValue, doc)
                    }
                };

                trace!(?field_key, "convert_event: FieldKey");
                Ok(Some(self.event(ParseEventKind::FieldKey(field_key))))
            }

            Event::Scalar {
                span,
                value,
                kind: _,
            } => {
                self.current_span = Some(span);
                // Determine scalar kind from value (Parser2 already unescaped it)
                // We need to figure out if it was bare or quoted from the raw text
                let text = self.span_text(span);
                let kind =
                    if text.starts_with('"') || text.starts_with("r#") || text.starts_with("<<") {
                        if text.starts_with('"') {
                            StyxScalarKind::Quoted
                        } else if text.starts_with("r#") {
                            StyxScalarKind::Raw
                        } else {
                            StyxScalarKind::Heredoc
                        }
                    } else {
                        StyxScalarKind::Bare
                    };
                let scalar = self.parse_scalar(value, kind);
                trace!(?scalar, "convert_event: Scalar");
                self.mark_tag_has_payload();
                Ok(Some(self.event(ParseEventKind::Scalar(scalar))))
            }

            Event::Unit { span } => {
                self.current_span = Some(span);
                self.mark_tag_has_payload();

                // Check if this Unit represents an actual @ token in the source
                // vs an implicit unit (key with no value).
                let is_at_token = self.span_text(span) == "@";

                if is_at_token && self.tag_has_payload_stack.is_empty() {
                    // Standalone @ is a unit tag - emit VariantTag(None) + Scalar(Unit)
                    trace!("convert_event: Unit (@) -> VariantTag(None) + Scalar(Unit)");
                    self.peeked_events
                        .push(self.event(ParseEventKind::Scalar(ScalarValue::Unit)));
                    Ok(Some(self.event(ParseEventKind::VariantTag(None))))
                } else {
                    // Either inside a tag payload, or an implicit unit (no value)
                    trace!("convert_event: Unit (implicit/payload) -> Scalar(Unit)");
                    Ok(Some(self.event(ParseEventKind::Scalar(ScalarValue::Unit))))
                }
            }

            Event::TagStart { span, name } => {
                self.current_span = Some(span);
                // Empty name means unit tag (@), which maps to VariantTag(None)
                let tag = if name.is_empty() { None } else { Some(name) };
                trace!(?tag, "convert_event: TagStart -> VariantTag");
                // Track that we're in a tag and haven't seen a payload yet
                self.tag_has_payload_stack.push(false);
                Ok(Some(self.event(ParseEventKind::VariantTag(tag))))
            }

            Event::TagEnd => {
                // Check if this tag had a payload
                if let Some(had_payload) = self.tag_has_payload_stack.pop()
                    && !had_payload
                {
                    // No payload was emitted - this is a unit tag, emit Scalar(Unit)
                    trace!("convert_event: TagEnd (unit tag) -> Scalar(Unit)");
                    return Ok(Some(self.event(ParseEventKind::Scalar(ScalarValue::Unit))));
                }
                // Tag had a payload, TagEnd doesn't need to emit anything
                Ok(None)
            }

            Event::Comment { .. } => {
                // Line comments are skipped
                Ok(None)
            }

            Event::DocComment { span, lines } => {
                self.current_span = Some(span);
                // Buffer doc comments for the next field key
                // Lines are already stripped of `/// ` prefix by the parser
                for line in lines {
                    self.pending_doc.push(Cow::Borrowed(line));
                }
                Ok(None)
            }

            Event::Error { span, kind } => {
                self.current_span = Some(span);
                Err(self.make_error(span, &kind))
            }
        }
    }

    /// Skip the value after @schema key.
    fn skip_schema_value(&mut self) -> Result<(), ParseError> {
        let mut depth = 0i32;
        loop {
            let event = self.inner.next_event();
            match event {
                Some(Event::ObjectStart { .. }) | Some(Event::SequenceStart { .. }) => {
                    depth += 1;
                }
                Some(Event::ObjectEnd { .. }) | Some(Event::SequenceEnd { .. }) => {
                    depth -= 1;
                    if depth <= 0 {
                        break;
                    }
                }
                Some(Event::Scalar { .. }) | Some(Event::Unit { .. }) => {
                    if depth == 0 {
                        break;
                    }
                }
                Some(Event::TagStart { .. }) => {
                    // Tag followed by payload - continue
                }
                Some(Event::TagEnd) => {
                    // After tag end, the payload should follow
                    if depth == 0 {
                        // Wait for the actual value
                    }
                }
                Some(Event::EntryStart) | Some(Event::EntryEnd) => {
                    if depth == 0 {
                        // EntryEnd marks end of the @schema entry
                        if matches!(event, Some(Event::EntryEnd)) {
                            break;
                        }
                    }
                }
                Some(Event::Error { span, kind }) => {
                    return Err(self.make_error(span, &kind));
                }
                Some(_) => {
                    // Continue
                }
                None => break,
            }
        }
        Ok(())
    }
}

impl<'de> FormatParser<'de> for StyxParser<'de> {
    fn next_event(&mut self) -> Result<Option<ParseEvent<'de>>, ParseError> {
        // Return queued event if any (FIFO - take from front)
        if !self.peeked_events.is_empty() {
            let event = self.peeked_events.remove(0);
            trace!(?event, "next_event: returning queued event");
            return Ok(Some(event));
        }

        if self.complete {
            trace!("next_event: parsing complete");
            return Ok(None);
        }

        // Get events from inner parser until we have one to return
        loop {
            let event = self.inner.next_event();
            trace!(?event, "next_event: got inner event");

            match event {
                Some(inner_event) => {
                    if let Some(converted) = self.convert_event(inner_event)? {
                        return Ok(Some(converted));
                    }
                    // Event was skipped, continue to next
                }
                None => {
                    self.complete = true;
                    return Ok(None);
                }
            }
        }
    }

    fn peek_event(&mut self) -> Result<Option<ParseEvent<'de>>, ParseError> {
        if self.peeked_events.is_empty()
            && let Some(event) = self.next_event()?
        {
            // Insert at front since next_event may have pushed follow-up events
            self.peeked_events.insert(0, event);
        }
        Ok(self.peeked_events.first().cloned())
    }

    fn skip_value(&mut self) -> Result<(), ParseError> {
        // Consume the next value, handling nested structures
        let mut depth = 0i32;
        loop {
            let event = self.next_event()?;
            trace!(?event, depth, "skip_value");
            match event.as_ref().map(|e| &e.kind) {
                Some(ParseEventKind::StructStart(_)) | Some(ParseEventKind::SequenceStart(_)) => {
                    depth += 1;
                }
                Some(ParseEventKind::StructEnd) | Some(ParseEventKind::SequenceEnd) => {
                    if depth == 0 {
                        // Safety: unexpected End at depth 0 (malformed input or bug)
                        break;
                    }
                    depth -= 1;
                    if depth == 0 {
                        // Normal case: matched the opening container
                        break;
                    }
                }
                Some(ParseEventKind::Scalar(_)) => {
                    if depth == 0 {
                        break;
                    }
                }
                Some(ParseEventKind::VariantTag(_)) => {
                    // VariantTag followed by payload - continue to consume the payload
                }
                Some(ParseEventKind::FieldKey(_)) | Some(ParseEventKind::OrderedField) => {
                    // Continue
                }
                None => break,
            }
        }
        Ok(())
    }

    fn save(&mut self) -> SavePoint {
        // Clone the current parser state (without the saved_state field to avoid recursion)
        let mut clone = self.clone();
        clone.saved_state = None;
        self.saved_state = Some(Box::new(clone));
        SavePoint(0)
    }

    fn restore(&mut self, _save_point: SavePoint) {
        if let Some(saved) = self.saved_state.take() {
            *self = *saved;
        }
    }

    fn current_span(&self) -> Option<facet_reflect::Span> {
        self.current_span
            .map(|s| facet_reflect::Span::new(s.start as usize, (s.end - s.start) as usize))
    }

    fn raw_capture_shape(&self) -> Option<&'static facet_core::Shape> {
        Some(crate::RawStyx::SHAPE)
    }

    fn input(&self) -> Option<&'de [u8]> {
        Some(self.input.as_bytes())
    }
}
