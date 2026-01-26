//! Streaming YAML parser implementing the FormatParser trait.
//!
//! This parser uses saphyr-parser's event-based API and translates YAML events
//! into the `ParseEvent` format expected by `facet-format`'s deserializer.

extern crate alloc;

use alloc::{borrow::Cow, format, vec::Vec};

use facet_format::{
    ContainerKind, FieldKey, FieldLocationHint, FormatParser, ParseEvent, SavePoint, ScalarValue,
};
use saphyr_parser::{Event, Parser, ScalarStyle, Span as SaphyrSpan, StrInput};

use crate::error::{SpanExt, YamlError, YamlErrorKind};
use facet_reflect::Span;

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
    /// The underlying saphyr parser.
    parser: Parser<'de, StrInput<'de>>,
    /// Stack tracking nested containers.
    stack: Vec<ContextState>,
    /// Cached event for peek_event().
    event_peek: Option<ParseEvent<'de>>,
    /// Whether we've consumed the stream/document start events.
    started: bool,
    /// The span of the most recently consumed event (for error reporting).
    last_span: Option<Span>,
    /// Counter for save points.
    save_counter: u64,
    /// Events recorded since save() was called.
    recording: Option<Vec<ParseEvent<'de>>>,
    /// Events to replay before producing new ones.
    replay_buffer: Vec<ParseEvent<'de>>,
}

impl<'de> YamlParser<'de> {
    /// Create a new YAML parser from a string slice.
    pub fn new(input: &'de str) -> Result<Self, YamlError> {
        Ok(Self {
            input,
            parser: Parser::new_from_str(input),
            stack: Vec::new(),
            event_peek: None,
            started: false,
            last_span: None,
            save_counter: 0,
            recording: None,
            replay_buffer: Vec::new(),
        })
    }

    /// Get the original input string.
    pub const fn input(&self) -> &'de str {
        self.input
    }

    /// Get the next raw event from saphyr, updating span tracking.
    fn next_raw_event(&mut self) -> Result<Option<(Event<'de>, SaphyrSpan)>, YamlError> {
        match self.parser.next_event() {
            Some(Ok((event, span))) => {
                self.last_span = Some(Span::from_saphyr_span(&span));
                Ok(Some((event, span)))
            }
            Some(Err(e)) => Err(
                YamlError::without_span(YamlErrorKind::Parse(format!("{e}")))
                    .with_source(self.input),
            ),
            None => Ok(None),
        }
    }

    /// Skip stream/document start events.
    fn skip_preamble(&mut self) -> Result<(), YamlError> {
        if self.started {
            return Ok(());
        }
        self.started = true;

        // Skip StreamStart
        if let Some((Event::StreamStart, _)) = self.next_raw_event()? {
            // Good
        }

        // Skip DocumentStart if present
        // We need to peek - but saphyr has peek() too
        if let Some(Ok((Event::DocumentStart(_), _))) = self.parser.peek() {
            self.next_raw_event()?;
        }

        Ok(())
    }

    /// Produce a ParseEvent from the underlying saphyr parser.
    fn produce_event(&mut self) -> Result<Option<ParseEvent<'de>>, YamlError> {
        self.skip_preamble()?;

        let (event, _span) = match self.next_raw_event()? {
            Some(ev) => ev,
            None => return Ok(None),
        };

        match event {
            Event::StreamStart | Event::DocumentStart(_) => {
                // Should have been skipped by preamble
                self.produce_event()
            }
            Event::StreamEnd | Event::DocumentEnd => {
                // End of document - return None
                Ok(None)
            }
            Event::MappingStart(_anchor, _tag) => {
                self.stack.push(ContextState::MappingKey);
                Ok(Some(ParseEvent::StructStart(ContainerKind::Object)))
            }
            Event::MappingEnd => {
                self.stack.pop();
                // If the parent was expecting a value, transition back to expecting a key
                if let Some(ctx @ ContextState::MappingValue) = self.stack.last_mut() {
                    *ctx = ContextState::MappingKey;
                }
                Ok(Some(ParseEvent::StructEnd))
            }
            Event::SequenceStart(_anchor, _tag) => {
                self.stack.push(ContextState::SequenceValue);
                Ok(Some(ParseEvent::SequenceStart(ContainerKind::Array)))
            }
            Event::SequenceEnd => {
                self.stack.pop();
                // If the parent was expecting a value, transition back to expecting a key
                if let Some(ctx @ ContextState::MappingValue) = self.stack.last_mut() {
                    *ctx = ContextState::MappingKey;
                }
                Ok(Some(ParseEvent::SequenceEnd))
            }
            Event::Scalar(value, style, _anchor, _tag) => {
                // Check if we're expecting a mapping key
                if let Some(ctx @ ContextState::MappingKey) = self.stack.last_mut() {
                    // This scalar is a key
                    *ctx = ContextState::MappingValue;
                    Ok(Some(ParseEvent::FieldKey(FieldKey::new(
                        value,
                        FieldLocationHint::KeyValue,
                    ))))
                } else {
                    // This scalar is a value
                    if let Some(ctx @ ContextState::MappingValue) = self.stack.last_mut() {
                        *ctx = ContextState::MappingKey;
                    }
                    Ok(Some(ParseEvent::Scalar(self.scalar_to_value(value, style))))
                }
            }
            Event::Alias(_id) => {
                // For now, treat aliases as null (proper anchor support would be more complex)
                if let Some(ctx @ ContextState::MappingValue) = self.stack.last_mut() {
                    *ctx = ContextState::MappingKey;
                }
                Ok(Some(ParseEvent::Scalar(ScalarValue::Null)))
            }
            Event::Nothing => {
                // Internal event, skip
                self.produce_event()
            }
        }
    }

    /// Convert a YAML scalar to a ScalarValue.
    fn scalar_to_value(&self, value: Cow<'de, str>, style: ScalarStyle) -> ScalarValue<'de> {
        // Quoted strings are always strings
        if matches!(style, ScalarStyle::SingleQuoted | ScalarStyle::DoubleQuoted) {
            return ScalarValue::Str(value);
        }

        // Check for null
        if is_yaml_null(&value) {
            return ScalarValue::Null;
        }

        // Check for boolean
        if let Some(b) = parse_yaml_bool(&value) {
            return ScalarValue::Bool(b);
        }

        // Check for integer
        if let Ok(n) = value.parse::<i64>() {
            return ScalarValue::I64(n);
        }
        if let Ok(n) = value.parse::<u64>() {
            return ScalarValue::U64(n);
        }

        // Check for float
        if let Ok(f) = value.parse::<f64>() {
            return ScalarValue::F64(f);
        }

        // Special float values
        match value.as_ref() {
            ".inf" | ".Inf" | ".INF" => return ScalarValue::F64(f64::INFINITY),
            "-.inf" | "-.Inf" | "-.INF" => return ScalarValue::F64(f64::NEG_INFINITY),
            ".nan" | ".NaN" | ".NAN" => return ScalarValue::F64(f64::NAN),
            _ => {}
        }

        // Default to string
        ScalarValue::Str(value)
    }

    /// Skip the current value (handles nested structures).
    /// This uses next_event_internal to properly handle replay buffers.
    fn skip_current_value(&mut self) -> Result<(), YamlError> {
        let mut depth = 0i32;

        loop {
            let event = self.next_event_internal()?;
            match event {
                Some(ParseEvent::StructStart(_) | ParseEvent::SequenceStart(_)) => {
                    depth += 1;
                }
                Some(ParseEvent::StructEnd | ParseEvent::SequenceEnd) => {
                    depth -= 1;
                    if depth <= 0 {
                        return Ok(());
                    }
                }
                Some(ParseEvent::Scalar(_)) if depth == 0 => {
                    return Ok(());
                }
                Some(_) => {}
                None => return Ok(()),
            }
        }
    }

    /// Internal next_event that handles replay buffer and recording.
    fn next_event_internal(&mut self) -> Result<Option<ParseEvent<'de>>, YamlError> {
        // First check replay buffer
        if let Some(event) = self.replay_buffer.pop() {
            return Ok(Some(event));
        }

        // Then check peeked event
        if let Some(event) = self.event_peek.take() {
            // Record if we're in save mode
            if let Some(ref mut rec) = self.recording {
                rec.push(event.clone());
            }
            return Ok(Some(event));
        }

        // Produce new event
        let event = self.produce_event()?;
        // Record if we're in save mode
        if let Some(ref mut rec) = self.recording
            && let Some(ref e) = event
        {
            rec.push(e.clone());
        }
        Ok(event)
    }
}

impl<'de> FormatParser<'de> for YamlParser<'de> {
    type Error = YamlError;

    fn next_event(&mut self) -> Result<Option<ParseEvent<'de>>, Self::Error> {
        self.next_event_internal()
    }

    fn peek_event(&mut self) -> Result<Option<ParseEvent<'de>>, Self::Error> {
        // First check replay buffer (peek at last element without removing)
        if let Some(event) = self.replay_buffer.last().cloned() {
            return Ok(Some(event));
        }
        // Then check already-peeked event
        if let Some(event) = self.event_peek.clone() {
            return Ok(Some(event));
        }
        // Finally, produce new event and cache it
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

    fn save(&mut self) -> SavePoint {
        self.save_counter += 1;
        self.recording = Some(Vec::new());
        SavePoint(self.save_counter)
    }

    fn restore(&mut self, _save_point: SavePoint) {
        if let Some(mut recorded) = self.recording.take() {
            // Reverse so we can pop from the end
            recorded.reverse();
            // Prepend to replay buffer (in case there's already stuff there)
            recorded.append(&mut self.replay_buffer);
            self.replay_buffer = recorded;
        }
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
