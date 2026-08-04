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
    ParseEvent, ParseEventKind, SavePoint, ScalarTypeHint, ScalarValue,
};
use facet_reflect::Span as ReflectSpan;
use styx_parse::{Event, EventKind, ParseErrorKind, Parser, ScalarKind as StyxScalarKind, Span};

mod inner {
    use super::*;

    /// Wrapper around styx-parse::Parser that converts errors to Results.
    ///
    /// This ensures error handling happens in ONE place instead of being scattered
    /// across multiple match arms.
    #[derive(Clone)]
    pub struct InnerParser<'de> {
        inner: Parser<'de>,
        input: &'de str,
    }

    impl<'de> InnerParser<'de> {
        pub fn new(input: &'de str) -> Self {
            Self {
                inner: Parser::new(input),
                input,
            }
        }

        pub fn new_expr(input: &'de str) -> Self {
            Self {
                inner: Parser::new_expr(input),
                input,
            }
        }

        pub fn input(&self) -> &'de str {
            self.input
        }

        fn to_reflect_span(&self, span: Span) -> ReflectSpan {
            ReflectSpan::new(span.start as usize, (span.end - span.start) as usize)
        }

        fn make_error(&self, span: Span, kind: &ParseErrorKind) -> ParseError {
            ParseError::new(
                self.to_reflect_span(span),
                DeserializeErrorKind::UnexpectedToken {
                    got: kind.to_string().into(),
                    expected: "valid syntax",
                },
            )
        }

        pub fn next_event(&mut self) -> Result<Option<Event<'de>>, ParseError> {
            let next_ev = self.inner.next_event();
            trace!(?next_ev, "inner_wrapper");

            match next_ev {
                Some(Event {
                    span,
                    kind: EventKind::Error { kind },
                }) => Err(self.make_error(span, &kind)),
                Some(event) => Ok(Some(event)),
                None => Ok(None),
            }
        }
    }
}

use inner::InnerParser;

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
    inner: InnerParser<'de>,
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
    /// Hint for the next scalar type expected by the deserializer.
    scalar_type_hint: Option<ScalarTypeHint>,
    /// A scalar the deserializer asked for as a number, whose text is not one
    /// in Styx: its span, its text, and the form that was wanted. Raised by
    /// the next `next_event` rather than at the point of discovery, because
    /// interpretation can happen while the event is already cached (see
    /// `hint_scalar_type`) where there is nothing to return an error to.
    /// Held as parts because `ParseError` is not `Clone` and this parser is.
    pending_interp_error: Option<(ReflectSpan, String, &'static str)>,
}

impl<'de> StyxParser<'de> {
    /// Create a new parser for the given source (document mode).
    pub fn new(source: &'de str) -> Self {
        Self {
            inner: InnerParser::new(source),
            peeked_events: Vec::new(),
            current_span: None,
            complete: false,
            tag_has_payload_stack: Vec::new(),
            scalar_type_hint: None,
            pending_interp_error: None,
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
            inner: InnerParser::new_expr(source),
            peeked_events: Vec::new(),
            current_span: None,
            complete: false,
            tag_has_payload_stack: Vec::new(),
            scalar_type_hint: None,
            pending_interp_error: None,
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
        let input = self.inner.input();
        &input[span.start as usize..span.end as usize]
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

    /// Parse a scalar value from text into a ScalarValue.
    /// Uses the scalar_type_hint to determine how to parse the value.
    ///
    /// In styx, ALL scalars are syntactically strings - whether bare, quoted, raw, or heredoc.
    /// The target type (via hint) determines how the string is interpreted.
    fn parse_scalar(&mut self, value: Cow<'de, str>, _kind: StyxScalarKind) -> ScalarValue<'de> {
        // Take the hint (it's consumed after use)
        let hint = self.scalar_type_hint.take();
        self.interpret_or_reject(value, hint)
    }

    /// The Styx grammar a numeric hint asks for, for the rejection message.
    /// `None` for hints that impose no numeric form (a string, bytes, …).
    fn numeric_form(hint: ScalarTypeHint) -> Option<&'static str> {
        match hint {
            ScalarTypeHint::I8
            | ScalarTypeHint::I16
            | ScalarTypeHint::I32
            | ScalarTypeHint::I64
            | ScalarTypeHint::I128
            | ScalarTypeHint::Isize
            | ScalarTypeHint::U8
            | ScalarTypeHint::U16
            | ScalarTypeHint::U32
            | ScalarTypeHint::U64
            | ScalarTypeHint::U128
            | ScalarTypeHint::Usize => Some("an integer"),
            ScalarTypeHint::F32 | ScalarTypeHint::F64 => Some("a float"),
            _ => None,
        }
    }

    /// Interpret `value` under `hint`, remembering a rejection.
    ///
    /// A numeric hint that does not interpret is a REJECTION, not a pass to
    /// the next reader. Leaving it as a string would hand the decision to the
    /// target type's `FromStr`, whose grammar is a different one — that is how
    /// `Infinity` and `NaN` used to decode into an `f64` field although
    /// `interp[interp.float.special]` defines a closed, case-sensitive set.
    fn interpret_or_reject(
        &mut self,
        value: Cow<'de, str>,
        hint: Option<ScalarTypeHint>,
    ) -> ScalarValue<'de> {
        let interpreted = Self::interpret_scalar(value, hint);
        if let (Some(hint), ScalarValue::Str(text)) = (hint, &interpreted)
            && let Some(expected) = Self::numeric_form(hint)
        {
            self.pending_interp_error =
                Some((self.event_span(), text.as_ref().to_string(), expected));
        }
        interpreted
    }

    /// Interpret one scalar's text under a target-type hint. Split out of
    /// [`Self::parse_scalar`] because a hint can also arrive *after* the
    /// scalar was converted (see [`Self::hint_scalar_type`]), and both entry
    /// points must reach exactly the same rules.
    fn interpret_scalar(value: Cow<'de, str>, hint: Option<ScalarTypeHint>) -> ScalarValue<'de> {
        // All scalar kinds are treated the same - the hint determines interpretation
        match hint {
            Some(ScalarTypeHint::String) | None => ScalarValue::Str(value),
            Some(ScalarTypeHint::Bool) => {
                if value == "true" {
                    ScalarValue::Bool(true)
                } else if value == "false" {
                    ScalarValue::Bool(false)
                } else {
                    // Invalid bool, return as string and let deserializer error
                    ScalarValue::Str(value)
                }
            }
            // The numeric forms are `interp[interp.int.*]` and
            // `interp[interp.float.*]`, not Rust's `str::parse` — they admit
            // readability underscores and radix prefixes, and they hold the
            // special float names to the spec's closed case-sensitive set.
            Some(
                ScalarTypeHint::I8
                | ScalarTypeHint::I16
                | ScalarTypeHint::I32
                | ScalarTypeHint::I64
                | ScalarTypeHint::I128
                | ScalarTypeHint::Isize,
            ) => match crate::scalar_interp::parse_i64(&value) {
                Some(n) => ScalarValue::I64(n),
                None => ScalarValue::Str(value),
            },
            Some(
                ScalarTypeHint::U8
                | ScalarTypeHint::U16
                | ScalarTypeHint::U32
                | ScalarTypeHint::U64
                | ScalarTypeHint::U128
                | ScalarTypeHint::Usize,
            ) => match crate::scalar_interp::parse_u64(&value) {
                Some(n) => ScalarValue::U64(n),
                None => ScalarValue::Str(value),
            },
            Some(ScalarTypeHint::F32 | ScalarTypeHint::F64) => {
                match crate::scalar_interp::parse_f64(&value) {
                    Some(n) => ScalarValue::F64(n),
                    None => ScalarValue::Str(value),
                }
            }
            Some(ScalarTypeHint::Char) => {
                let mut chars = value.chars();
                if let (Some(c), None) = (chars.next(), chars.next()) {
                    ScalarValue::Char(c)
                } else {
                    ScalarValue::Str(value)
                }
            }
            Some(ScalarTypeHint::Bytes) => ScalarValue::Str(value),
            // facet's ScalarTypeHint is #[non_exhaustive]; an unrecognized hint falls back
            // to the string form (styx scalars are syntactically strings — the deserializer
            // reinterprets them per the target type).
            Some(_) => ScalarValue::Str(value),
        }
    }

    /// Convert a styx-parse Event to facet-format ParseEvent(s).
    /// May queue additional events in peeked_events.
    /// Returns None if the event should be skipped (e.g., DocumentStart).
    fn convert_event(&mut self, event: Event<'de>) -> Result<Option<ParseEvent<'de>>, ParseError> {
        let span = event.span;
        match event.kind {
            EventKind::DocumentStart => {
                if self.at_implicit_root {
                    Ok(None)
                } else {
                    // Expression mode - no implicit root, skip DocumentStart
                    Ok(None)
                }
            }

            EventKind::DocumentEnd => {
                if self.at_implicit_root {
                    Ok(None)
                } else {
                    // Expression mode - no implicit root, skip DocumentEnd
                    Ok(None)
                }
            }

            EventKind::ObjectStart => {
                self.current_span = Some(span);
                self.depth += 1;
                self.mark_tag_has_payload();
                Ok(Some(
                    self.event(ParseEventKind::StructStart(ContainerKind::Object)),
                ))
            }

            EventKind::ObjectEnd => {
                self.current_span = Some(span);
                self.depth -= 1;
                if self.depth == 0 {
                    self.at_implicit_root = false;
                }
                Ok(Some(self.event(ParseEventKind::StructEnd)))
            }

            EventKind::SequenceStart => {
                self.current_span = Some(span);
                self.depth += 1;
                self.mark_tag_has_payload();
                Ok(Some(self.event(ParseEventKind::SequenceStart(
                    ContainerKind::Array,
                ))))
            }

            EventKind::SequenceEnd => {
                self.current_span = Some(span);
                self.depth -= 1;
                Ok(Some(self.event(ParseEventKind::SequenceEnd)))
            }

            EventKind::EntryStart | EventKind::EntryEnd => {
                // These are structural markers not needed by facet-format
                Ok(None)
            }

            EventKind::Key {
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
                    // Tagged key with payload: `@tag"payload"`
                    (Some(tag_name), Some(payload)) => FieldKey::tagged_with_name_and_doc(
                        tag_name,
                        payload,
                        FieldLocationHint::KeyValue,
                        doc,
                    ),
                };

                trace!(?field_key, "convert_event: FieldKey");
                Ok(Some(self.event(ParseEventKind::FieldKey(field_key))))
            }

            EventKind::Scalar { value, kind: _ } => {
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

            EventKind::Unit => {
                self.current_span = Some(span);
                self.mark_tag_has_payload();

                // A bare `@` is Styx's unit value. Named tags produce
                // `VariantTag`, but the nameless form must remain a scalar so
                // `Option<T>` can consume it as `None`.
                trace!("convert_event: Unit -> Scalar(Unit)");
                Ok(Some(self.event(ParseEventKind::Scalar(ScalarValue::Unit))))
            }

            EventKind::TagStart { name } => {
                self.current_span = Some(span);
                self.mark_tag_has_payload();
                // Empty name means unit tag (@), which maps to VariantTag(None)
                let tag = if name.is_empty() { None } else { Some(name) };
                trace!(?tag, "convert_event: TagStart -> VariantTag");
                // Track that we're in a tag and haven't seen a payload yet
                self.tag_has_payload_stack.push(false);
                Ok(Some(self.event(ParseEventKind::VariantTag(tag))))
            }

            EventKind::TagEnd => {
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

            EventKind::Comment { .. } => {
                // Line comments are skipped
                Ok(None)
            }

            EventKind::DocComment { lines } => {
                self.current_span = Some(span);
                // Buffer doc comments for the next field key
                // Lines are already stripped of `/// ` prefix by the parser
                for line in lines {
                    self.pending_doc.push(Cow::Borrowed(line));
                }
                Ok(None)
            }

            EventKind::Error { .. } => {
                // This should never happen - InnerParser converts errors to Results
                unreachable!("Error events should be handled by InnerParser")
            }
        }
    }

    /// Skip the value after @schema key.
    fn skip_schema_value(&mut self) -> Result<(), ParseError> {
        let mut depth = 0i32;
        loop {
            let event = self.inner.next_event()?;
            match event {
                Some(Event {
                    kind: EventKind::ObjectStart,
                    ..
                })
                | Some(Event {
                    kind: EventKind::SequenceStart,
                    ..
                }) => {
                    depth += 1;
                }
                Some(Event {
                    kind: EventKind::ObjectEnd,
                    ..
                })
                | Some(Event {
                    kind: EventKind::SequenceEnd,
                    ..
                }) => {
                    depth -= 1;
                    if depth <= 0 {
                        break;
                    }
                }
                Some(Event {
                    kind: EventKind::Scalar { .. },
                    ..
                })
                | Some(Event {
                    kind: EventKind::Unit,
                    ..
                }) => {
                    if depth == 0 {
                        break;
                    }
                }
                Some(Event {
                    kind: EventKind::TagStart { .. },
                    ..
                }) => {
                    // Tag followed by payload - continue
                }
                Some(Event {
                    kind: EventKind::TagEnd,
                    ..
                }) => {
                    // After tag end, the payload should follow
                    if depth == 0 {
                        // Wait for the actual value
                    }
                }
                Some(Event {
                    kind: EventKind::EntryStart,
                    ..
                }) => {
                    // Continue
                }
                Some(Event {
                    kind: EventKind::EntryEnd,
                    ..
                }) => {
                    if depth == 0 {
                        // EntryEnd marks end of the @schema entry
                        break;
                    }
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
        // A scalar the deserializer asked for as a number, whose text is not
        // one, fails HERE — the point where there is an error channel.
        if let Some((span, text, expected)) = self.pending_interp_error.take() {
            return Err(ParseError::new(
                span,
                DeserializeErrorKind::UnexpectedToken {
                    got: text.into(),
                    expected,
                },
            ));
        }

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
            match self.inner.next_event()? {
                Some(inner_event) => {
                    trace!(?inner_event);
                    if let Some(converted_event) = self.convert_event(inner_event)? {
                        trace!(?converted_event);
                        return Ok(Some(converted_event));
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
                // facet's ParseEventKind is #[non_exhaustive]; skip unknown event kinds.
                Some(_) => {}
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
        Some(self.inner.input().as_bytes())
    }

    /// Record the target type the next scalar should be read as.
    ///
    /// A hint may arrive AFTER the scalar it describes has already been
    /// converted: `peek_event` pulls an event through `convert_event` — with
    /// no hint yet — and caches it, and the deserializer peeks to decide what
    /// it is looking at before it announces the type it wants. Storing the
    /// hint for "the next scalar" would therefore apply it to the wrong
    /// scalar, or to none: a cached `Scalar` is already past.
    ///
    /// Styx scalars are opaque text, so nothing is lost by re-reading one —
    /// the cached value still carries the source text. A hint that lands on
    /// an already-converted scalar reinterprets it in place, and only a hint
    /// with nothing to reinterpret is stored for the scalar still to come.
    /// Styx is self-describing about STRUCTURE and deliberately opaque about
    /// SCALARS (`interp[interp.coerce.none]`: "the target type determines
    /// interpretation"). Without this the deserializer would skip
    /// `hint_scalar_type` as it does for JSON, every scalar would reach it as
    /// a string, and numbers would be read by the target type's `FromStr`
    /// rather than by `interp[interp.int.*]`/`interp[interp.float.*]`.
    fn needs_scalar_hints(&self) -> bool {
        true
    }

    fn hint_scalar_type(&mut self, hint: ScalarTypeHint) {
        if let Some(ParseEvent {
            kind: ParseEventKind::Scalar(ScalarValue::Str(text)),
            ..
        }) = self.peeked_events.first()
        {
            let text = text.clone();
            let reinterpreted = self.interpret_or_reject(text, Some(hint));
            self.peeked_events[0].kind = ParseEventKind::Scalar(reinterpreted);
            return;
        }
        self.scalar_type_hint = Some(hint);
    }
}
