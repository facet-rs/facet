//! Streaming YAML parser implementing the FormatParser trait.
//!
//! This parser uses saphyr-parser's event-based API and translates YAML events
//! into the `ParseEvent` format expected by `facet-format`'s deserializer.

extern crate alloc;

use alloc::{
    borrow::Cow,
    format,
    string::{String, ToString},
    vec::Vec,
};

use facet_format::{
    ContainerKind, FieldEvidence, FieldKey, FieldLocationHint, FormatParser, ParseEvent,
    ProbeStream, ScalarValue,
};
use saphyr_parser::{Event, Parser, ScalarStyle, Span as SaphyrSpan, SpannedEventReceiver};

use crate::error::{SpanExt, YamlError, YamlErrorKind};
use facet_reflect::Span;

// ============================================================================
// Event wrapper with owned strings
// ============================================================================

/// A YAML event with owned string data and span information.
/// We convert from saphyr's borrowed events to owned so we can store them.
#[derive(Debug, Clone)]
#[allow(dead_code)] // Some variants/fields reserved for future anchor/alias support
enum OwnedEvent {
    StreamStart,
    StreamEnd,
    DocumentStart,
    DocumentEnd,
    Alias(usize),
    Scalar {
        value: String,
        style: ScalarStyle,
        anchor: usize,
    },
    SequenceStart {
        anchor: usize,
    },
    SequenceEnd,
    MappingStart {
        anchor: usize,
    },
    MappingEnd,
}

#[derive(Debug, Clone)]
struct SpannedEvent {
    event: OwnedEvent,
    span: SaphyrSpan,
}

// ============================================================================
// Event Collector
// ============================================================================

/// Collects all events from the parser upfront.
/// This is necessary because saphyr-parser doesn't support seeking/rewinding,
/// but we need to replay events for flatten deserialization.
struct EventCollector {
    events: Vec<SpannedEvent>,
}

impl EventCollector {
    fn new() -> Self {
        Self { events: Vec::new() }
    }
}

impl SpannedEventReceiver<'_> for EventCollector {
    fn on_event(&mut self, event: Event<'_>, span: SaphyrSpan) {
        let owned = match event {
            Event::StreamStart => OwnedEvent::StreamStart,
            Event::StreamEnd => OwnedEvent::StreamEnd,
            Event::DocumentStart(_) => OwnedEvent::DocumentStart,
            Event::DocumentEnd => OwnedEvent::DocumentEnd,
            Event::Alias(id) => OwnedEvent::Alias(id),
            Event::Scalar(value, style, anchor, _tag) => OwnedEvent::Scalar {
                value: value.into_owned(),
                style,
                anchor,
            },
            Event::SequenceStart(anchor, _tag) => OwnedEvent::SequenceStart { anchor },
            Event::SequenceEnd => OwnedEvent::SequenceEnd,
            Event::MappingStart(anchor, _tag) => OwnedEvent::MappingStart { anchor },
            Event::MappingEnd => OwnedEvent::MappingEnd,
            Event::Nothing => return, // Skip internal events
        };
        log::trace!("YAML event: {owned:?}");
        self.events.push(SpannedEvent { event: owned, span });
    }
}

// ============================================================================
// Parser State
// ============================================================================

/// Context for tracking where we are in the YAML structure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ContextState {
    /// Inside a mapping, expecting a key or end
    MappingKey,
    /// Inside a mapping, expecting a value
    MappingValue,
    /// Inside a sequence, expecting a value or end
    SequenceValue,
}

// ============================================================================
// YAML Parser
// ============================================================================

/// Streaming YAML parser backed by `saphyr-parser`.
///
/// This parser translates YAML's event stream into the `ParseEvent` format
/// expected by `facet-format`'s deserializer.
pub struct YamlParser<'de> {
    /// Original input string.
    input: &'de str,
    /// Pre-parsed events from saphyr-parser.
    events: Vec<SpannedEvent>,
    /// Current position in the event stream.
    pos: usize,
    /// Stack tracking nested containers.
    stack: Vec<ContextState>,
    /// Cached event for peek_event().
    event_peek: Option<ParseEvent<'de>>,
    /// Whether we've consumed the stream/document start events.
    started: bool,
    /// The span of the most recently consumed event (for error reporting).
    last_span: Option<Span>,
}

impl<'de> YamlParser<'de> {
    /// Create a new YAML parser from a string slice.
    pub fn new(input: &'de str) -> Result<Self, YamlError> {
        let mut collector = EventCollector::new();
        Parser::new_from_str(input)
            .load(&mut collector, true)
            .map_err(|e| {
                YamlError::without_span(YamlErrorKind::Parse(format!("{e}"))).with_source(input)
            })?;

        Ok(Self {
            input,
            events: collector.events,
            pos: 0,
            stack: Vec::new(),
            event_peek: None,
            started: false,
            last_span: None,
        })
    }

    /// Get the original input string.
    pub fn input(&self) -> &'de str {
        self.input
    }

    /// Consume and return the current event.
    fn next_raw(&mut self) -> Option<SpannedEvent> {
        if self.pos < self.events.len() {
            let event = self.events[self.pos].clone();
            self.last_span = Some(Span::from_saphyr_span(&event.span));
            self.pos += 1;
            Some(event)
        } else {
            None
        }
    }

    /// Skip stream/document start events.
    fn skip_preamble(&mut self) {
        while self.pos < self.events.len() {
            match &self.events[self.pos].event {
                OwnedEvent::StreamStart | OwnedEvent::DocumentStart => {
                    self.pos += 1;
                }
                _ => break,
            }
        }
        self.started = true;
    }

    /// Convert a YAML scalar to a ScalarValue.
    fn scalar_to_value(&self, value: &str, style: ScalarStyle) -> ScalarValue<'de> {
        // If quoted, always treat as string
        if matches!(style, ScalarStyle::SingleQuoted | ScalarStyle::DoubleQuoted) {
            return ScalarValue::Str(Cow::Owned(value.to_string()));
        }

        // Check for null
        if is_yaml_null(value) {
            return ScalarValue::Null;
        }

        // Check for boolean
        if let Some(b) = parse_yaml_bool(value) {
            return ScalarValue::Bool(b);
        }

        // Check for integer
        if let Ok(n) = value.parse::<i64>() {
            return ScalarValue::I64(n);
        }

        // Check for float
        if let Ok(f) = value.parse::<f64>() {
            return ScalarValue::F64(f);
        }

        // Default to string
        ScalarValue::Str(Cow::Owned(value.to_string()))
    }

    /// Produce the next parse event.
    fn produce_event(&mut self) -> Result<Option<ParseEvent<'de>>, YamlError> {
        // Skip preamble if we haven't started
        if !self.started {
            self.skip_preamble();
        }

        // Check current context to know what to expect
        let context = self.stack.last().copied();

        if self.pos >= self.events.len() {
            // EOF - we're done
            return Ok(None);
        }

        // Clone the event to avoid borrow issues
        let raw_event = self.events[self.pos].clone();

        match (&raw_event.event, context) {
            // Stream/Document end - skip and continue
            (OwnedEvent::StreamEnd, _) | (OwnedEvent::DocumentEnd, _) => {
                self.next_raw();
                self.produce_event()
            }

            // Mapping start
            (OwnedEvent::MappingStart { .. }, _) => {
                self.next_raw();
                // If we're in MappingValue context, this nested struct satisfies the value,
                // so transition parent back to MappingKey before pushing new context
                if let Some(ctx) = self.stack.last_mut()
                    && *ctx == ContextState::MappingValue
                {
                    *ctx = ContextState::MappingKey;
                }
                self.stack.push(ContextState::MappingKey);
                Ok(Some(ParseEvent::StructStart(ContainerKind::Object)))
            }

            // Mapping end
            (OwnedEvent::MappingEnd, _) => {
                self.next_raw();
                self.stack.pop();
                Ok(Some(ParseEvent::StructEnd))
            }

            // Sequence start
            (OwnedEvent::SequenceStart { .. }, _) => {
                self.next_raw();
                // If we're in MappingValue context, this sequence satisfies the value,
                // so transition parent back to MappingKey before pushing new context
                if let Some(ctx) = self.stack.last_mut()
                    && *ctx == ContextState::MappingValue
                {
                    *ctx = ContextState::MappingKey;
                }
                self.stack.push(ContextState::SequenceValue);
                Ok(Some(ParseEvent::SequenceStart(ContainerKind::Array)))
            }

            // Sequence end
            (OwnedEvent::SequenceEnd, _) => {
                self.next_raw();
                self.stack.pop();
                Ok(Some(ParseEvent::SequenceEnd))
            }

            // Scalar in mapping key position -> emit FieldKey
            (OwnedEvent::Scalar { value, .. }, Some(ContextState::MappingKey)) => {
                let key = value.clone();
                self.next_raw();
                // Transition to expecting value
                if let Some(ctx) = self.stack.last_mut() {
                    *ctx = ContextState::MappingValue;
                }
                Ok(Some(ParseEvent::FieldKey(FieldKey::new(
                    Cow::Owned(key),
                    FieldLocationHint::KeyValue,
                ))))
            }

            // Scalar in mapping value position -> emit Scalar and transition back to key
            (OwnedEvent::Scalar { value, style, .. }, Some(ContextState::MappingValue)) => {
                let value = value.clone();
                let style = *style;
                self.next_raw();
                // Transition back to expecting key
                if let Some(ctx) = self.stack.last_mut() {
                    *ctx = ContextState::MappingKey;
                }
                Ok(Some(ParseEvent::Scalar(
                    self.scalar_to_value(&value, style),
                )))
            }

            // Scalar in sequence -> emit Scalar
            (OwnedEvent::Scalar { value, style, .. }, Some(ContextState::SequenceValue)) => {
                let value = value.clone();
                let style = *style;
                self.next_raw();
                Ok(Some(ParseEvent::Scalar(
                    self.scalar_to_value(&value, style),
                )))
            }

            // Scalar at root level (no context) -> emit Scalar
            (OwnedEvent::Scalar { value, style, .. }, None) => {
                let value = value.clone();
                let style = *style;
                self.next_raw();
                Ok(Some(ParseEvent::Scalar(
                    self.scalar_to_value(&value, style),
                )))
            }

            // Alias - not fully supported yet
            (OwnedEvent::Alias(_), _) => {
                let span = Span::from_saphyr_span(&raw_event.span);
                Err(YamlError::new(
                    YamlErrorKind::Unsupported("YAML aliases are not yet supported".into()),
                    span,
                )
                .with_source(self.input))
            }

            // Unexpected combinations
            _ => {
                let span = Span::from_saphyr_span(&raw_event.span);
                Err(YamlError::new(
                    YamlErrorKind::UnexpectedEvent {
                        got: format!("{:?}", raw_event.event),
                        expected: "valid YAML structure",
                    },
                    span,
                )
                .with_source(self.input))
            }
        }
    }

    /// Skip the current value (for unknown fields).
    fn skip_current_value(&mut self) -> Result<(), YamlError> {
        if self.pos >= self.events.len() {
            return Ok(());
        }

        let raw_event = self.events[self.pos].clone();

        match &raw_event.event {
            OwnedEvent::Scalar { .. } => {
                self.next_raw();
                // Update context if in mapping value position
                if let Some(ctx) = self.stack.last_mut()
                    && *ctx == ContextState::MappingValue
                {
                    *ctx = ContextState::MappingKey;
                }
                Ok(())
            }
            OwnedEvent::MappingStart { .. } => {
                self.next_raw();
                let mut depth = 1;
                while depth > 0 {
                    let Some(event) = self.next_raw() else {
                        return Err(YamlError::without_span(YamlErrorKind::UnexpectedEof {
                            expected: "mapping end",
                        })
                        .with_source(self.input));
                    };
                    match &event.event {
                        OwnedEvent::MappingStart { .. } => depth += 1,
                        OwnedEvent::MappingEnd => depth -= 1,
                        OwnedEvent::SequenceStart { .. } => depth += 1,
                        OwnedEvent::SequenceEnd => depth -= 1,
                        _ => {}
                    }
                }
                // Update context if in mapping value position
                if let Some(ctx) = self.stack.last_mut()
                    && *ctx == ContextState::MappingValue
                {
                    *ctx = ContextState::MappingKey;
                }
                Ok(())
            }
            OwnedEvent::SequenceStart { .. } => {
                self.next_raw();
                let mut depth = 1;
                while depth > 0 {
                    let Some(event) = self.next_raw() else {
                        return Err(YamlError::without_span(YamlErrorKind::UnexpectedEof {
                            expected: "sequence end",
                        })
                        .with_source(self.input));
                    };
                    match &event.event {
                        OwnedEvent::MappingStart { .. } => depth += 1,
                        OwnedEvent::MappingEnd => depth -= 1,
                        OwnedEvent::SequenceStart { .. } => depth += 1,
                        OwnedEvent::SequenceEnd => depth -= 1,
                        _ => {}
                    }
                }
                // Update context if in mapping value position
                if let Some(ctx) = self.stack.last_mut()
                    && *ctx == ContextState::MappingValue
                {
                    *ctx = ContextState::MappingKey;
                }
                Ok(())
            }
            _ => {
                self.next_raw();
                Ok(())
            }
        }
    }

    /// Build probe evidence by scanning ahead without consuming.
    fn build_probe(&self) -> Result<Vec<FieldEvidence<'de>>, YamlError> {
        let mut evidence = Vec::new();
        let mut pos = self.pos;

        // Skip to MappingStart if we have one peeked
        if pos < self.events.len()
            && let OwnedEvent::MappingStart { .. } = &self.events[pos].event
        {
            pos += 1;
        }

        // Scan the mapping for keys
        let mut depth = 1;
        while pos < self.events.len() && depth > 0 {
            let event = &self.events[pos];
            match &event.event {
                OwnedEvent::MappingStart { .. } => {
                    depth += 1;
                    pos += 1;
                }
                OwnedEvent::MappingEnd => {
                    depth -= 1;
                    pos += 1;
                }
                OwnedEvent::SequenceStart { .. } => {
                    depth += 1;
                    pos += 1;
                }
                OwnedEvent::SequenceEnd => {
                    depth -= 1;
                    pos += 1;
                }
                OwnedEvent::Scalar { value, .. } if depth == 1 => {
                    // This is a key at the top level of the mapping
                    let key = Cow::Owned(value.clone());
                    pos += 1;

                    // Look at the value
                    if pos < self.events.len() {
                        let value_event = &self.events[pos];
                        let scalar_value = if let OwnedEvent::Scalar {
                            value: v, style: s, ..
                        } = &value_event.event
                        {
                            Some(self.scalar_to_value(v, *s))
                        } else {
                            None
                        };

                        if let Some(sv) = scalar_value {
                            evidence.push(FieldEvidence::with_scalar_value(
                                key,
                                FieldLocationHint::KeyValue,
                                None,
                                sv,
                                None,
                            ));
                        } else {
                            evidence.push(FieldEvidence::new(
                                key,
                                FieldLocationHint::KeyValue,
                                None,
                                None,
                            ));
                        }

                        // Skip the value
                        pos = self.skip_value_from(pos);
                    }
                }
                _ => {
                    pos += 1;
                }
            }
        }

        Ok(evidence)
    }

    /// Skip a value starting from `pos`, returning the position after the value.
    fn skip_value_from(&self, start: usize) -> usize {
        let mut pos = start;
        if pos >= self.events.len() {
            return pos;
        }

        match &self.events[pos].event {
            OwnedEvent::Scalar { .. } => pos + 1,
            OwnedEvent::MappingStart { .. } | OwnedEvent::SequenceStart { .. } => {
                let mut depth = 1;
                pos += 1;
                while pos < self.events.len() && depth > 0 {
                    match &self.events[pos].event {
                        OwnedEvent::MappingStart { .. } | OwnedEvent::SequenceStart { .. } => {
                            depth += 1;
                        }
                        OwnedEvent::MappingEnd | OwnedEvent::SequenceEnd => {
                            depth -= 1;
                        }
                        _ => {}
                    }
                    pos += 1;
                }
                pos
            }
            _ => pos + 1,
        }
    }
}

impl<'de> FormatParser<'de> for YamlParser<'de> {
    type Error = YamlError;
    type Probe<'a>
        = YamlProbe<'de>
    where
        Self: 'a;

    fn next_event(&mut self) -> Result<Option<ParseEvent<'de>>, Self::Error> {
        if let Some(event) = self.event_peek.take() {
            return Ok(Some(event));
        }
        self.produce_event()
    }

    fn peek_event(&mut self) -> Result<Option<ParseEvent<'de>>, Self::Error> {
        if let Some(event) = self.event_peek.clone() {
            return Ok(Some(event));
        }
        let event = self.produce_event()?;
        if let Some(ref e) = event {
            self.event_peek = Some(e.clone());
        }
        Ok(event)
    }

    fn skip_value(&mut self) -> Result<(), Self::Error> {
        debug_assert!(
            self.event_peek.is_none(),
            "skip_value called while an event is buffered"
        );
        self.skip_current_value()
    }

    fn begin_probe(&mut self) -> Result<Self::Probe<'_>, Self::Error> {
        let evidence = self.build_probe()?;
        Ok(YamlProbe { evidence, idx: 0 })
    }

    fn capture_raw(&mut self) -> Result<Option<&'de str>, Self::Error> {
        // YAML doesn't support raw capture (unlike JSON with RawJson)
        self.skip_value()?;
        Ok(None)
    }

    fn current_span(&self) -> Option<Span> {
        self.last_span
    }
}

/// Probe stream for YAML.
pub struct YamlProbe<'de> {
    evidence: Vec<FieldEvidence<'de>>,
    idx: usize,
}

impl<'de> ProbeStream<'de> for YamlProbe<'de> {
    type Error = YamlError;

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

// ============================================================================
// YAML-specific helpers
// ============================================================================

/// Check if a YAML value represents null.
fn is_yaml_null(value: &str) -> bool {
    matches!(
        value.to_lowercase().as_str(),
        "null" | "~" | "" | "nil" | "none"
    )
}

/// Parse a YAML boolean value.
fn parse_yaml_bool(value: &str) -> Option<bool> {
    match value.to_lowercase().as_str() {
        "true" | "yes" | "on" | "y" => Some(true),
        "false" | "no" | "off" | "n" => Some(false),
        _ => None,
    }
}
