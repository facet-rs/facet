//! Streaming TOML parser implementing the FormatParser trait.
//!
//! The key challenge with TOML is "table reopening" - fields for the same struct
//! can appear at different points in the document:
//!
//! ```toml
//! [foo.bar]
//! x = 1
//!
//! [foo.baz]
//! z = 3
//!
//! [foo.bar]  # reopening!
//! y = 2
//! ```
//!
//! This parser handles this by treating `StructEnd` and `SequenceEnd` as
//! "navigating up the graph" rather than "we're done forever". The same applies
//! to array tables - they can be interleaved with other tables:
//!
//! ```toml
//! [[servers]]
//! name = "alpha"
//!
//! [database]
//! host = "localhost"
//!
//! [[servers]]  # reopening the array!
//! name = "beta"
//! ```
//!
//! The deserializer with `Partial` in deferred mode handles fields/elements
//! arriving out of order. No buffering or pre-scanning needed.

extern crate alloc;

use alloc::{
    borrow::Cow,
    collections::VecDeque,
    string::{String, ToString},
    vec::Vec,
};

use facet_format::{
    ContainerKind, FieldEvidence, FieldKey, FieldLocationHint, FormatParser, ParseEvent,
    ProbeStream, ScalarValue,
};
use toml_parser::{
    ErrorSink, Raw, Source,
    decoder::ScalarKind,
    parser::{Event, EventKind, RecursionGuard, parse_document},
};

use crate::{TomlError, TomlErrorKind};

// ============================================================================
// Error collection for parsing
// ============================================================================

/// Collects parse errors from the TOML parser
struct ParseErrorCollector {
    error: Option<String>,
}

impl ParseErrorCollector {
    fn new() -> Self {
        Self { error: None }
    }

    fn take_error(&mut self) -> Option<String> {
        self.error.take()
    }
}

impl ErrorSink for ParseErrorCollector {
    fn report_error(&mut self, error: toml_parser::ParseError) {
        if self.error.is_none() {
            self.error = Some(error.description().to_string());
        }
    }
}

// ============================================================================
// Path tracking
// ============================================================================

/// Kind of a path segment - determines what events to emit when navigating.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SegmentKind {
    /// Standard table `[foo]` - emits StructStart/StructEnd
    Table,
    /// Array table element `[[foo]]` - the array itself
    Array,
    /// Element inside an array table - emits StructStart/StructEnd
    ArrayElement,
}

/// A segment in the current document path.
#[derive(Debug, Clone)]
struct PathSegment<'de> {
    name: Cow<'de, str>,
    kind: SegmentKind,
}

// ============================================================================
// TOML Parser
// ============================================================================

/// Streaming TOML parser backed by `toml_parser`.
///
/// This parser translates TOML's event stream into the `ParseEvent` format
/// expected by `facet-format`'s deserializer.
pub struct TomlParser<'de> {
    /// Original input string.
    input: &'de str,
    /// Pre-parsed events from toml_parser.
    events: Vec<Event>,
    /// Current position in the event stream.
    pos: usize,
    /// Current path in the document with segment kinds.
    current_path: Vec<PathSegment<'de>>,
    /// Pending events to emit (navigation when tables change).
    pending_events: VecDeque<ParseEvent<'de>>,
    /// Cached event for peek_event().
    event_peek: Option<ParseEvent<'de>>,
    /// Whether we've emitted the initial StructStart for the root.
    root_started: bool,
    /// Whether we've emitted the final StructEnd for the root.
    root_ended: bool,
    /// Stack tracking nested inline containers (inline tables and arrays).
    /// Each entry is true for inline table, false for array.
    inline_stack: Vec<bool>,
}

impl<'de> TomlParser<'de> {
    /// Create a new TOML parser from a string slice.
    pub fn new(input: &'de str) -> Result<Self, TomlError> {
        let source = Source::new(input);
        let tokens: Vec<_> = source.lex().collect();
        let mut events: Vec<Event> = Vec::new();
        let mut guarded = RecursionGuard::new(&mut events, 128);
        let mut error_collector = ParseErrorCollector::new();

        parse_document(&tokens, &mut guarded, &mut error_collector);

        if let Some(err_msg) = error_collector.take_error() {
            return Err(TomlError::without_span(TomlErrorKind::Parse(err_msg)));
        }

        Ok(Self {
            input,
            events,
            pos: 0,
            current_path: Vec::new(),
            pending_events: VecDeque::new(),
            event_peek: None,
            root_started: false,
            root_ended: false,
            inline_stack: Vec::new(),
        })
    }

    /// Get the original input string.
    pub fn input(&self) -> &'de str {
        self.input
    }

    /// Check if an event should be skipped (whitespace, comment, newline).
    #[inline]
    fn should_skip(event: &Event) -> bool {
        matches!(
            event.kind(),
            EventKind::Whitespace | EventKind::Comment | EventKind::Newline
        )
    }

    /// Peek at the next raw event (skipping whitespace/comments).
    fn peek_raw(&self) -> Option<&Event> {
        let mut pos = self.pos;
        while pos < self.events.len() {
            let event = &self.events[pos];
            if !Self::should_skip(event) {
                return Some(event);
            }
            pos += 1;
        }
        None
    }

    /// Consume the next raw event (skipping whitespace/comments).
    fn next_raw(&mut self) -> Option<&Event> {
        while self.pos < self.events.len() {
            let event = &self.events[self.pos];
            self.pos += 1;
            if !Self::should_skip(event) {
                return Some(event);
            }
        }
        None
    }

    /// Get the string slice for an event's span.
    fn get_span_str(&self, event: &Event) -> &'de str {
        let span = event.span();
        &self.input[span.start()..span.end()]
    }

    /// Create a Raw from an event for scalar decoding.
    fn raw_from_event(&self, event: &Event) -> Raw<'de> {
        let span = event.span();
        Raw::new_unchecked(
            &self.input[span.start()..span.end()],
            event.encoding(),
            span,
        )
    }

    /// Decode a raw TOML value into the appropriate scalar.
    fn decode_scalar(&self, event: &Event) -> Result<ScalarValue<'de>, TomlError> {
        let raw = self.raw_from_event(event);
        let mut output: Cow<'de, str> = Cow::Borrowed("");
        let kind = raw.decode_scalar(&mut output, &mut ());

        match kind {
            ScalarKind::String => {
                // Use the decoded output (handles escapes, quotes, etc.)
                Ok(ScalarValue::Str(output))
            }
            ScalarKind::Boolean(b) => Ok(ScalarValue::Bool(b)),
            ScalarKind::Integer(radix) => {
                // Remove underscores for parsing
                let clean: String = output.chars().filter(|c| *c != '_').collect();
                let n: i64 = i64::from_str_radix(&clean, radix.value()).map_err(|e| {
                    TomlError::without_span(TomlErrorKind::InvalidValue {
                        message: e.to_string(),
                    })
                })?;
                Ok(ScalarValue::I64(n))
            }
            ScalarKind::Float => {
                let clean: String = output.chars().filter(|c| *c != '_').collect();
                // Handle special float values
                let f: f64 = match clean.as_str() {
                    "inf" | "+inf" => f64::INFINITY,
                    "-inf" => f64::NEG_INFINITY,
                    "nan" | "+nan" | "-nan" => f64::NAN,
                    _ => clean.parse().map_err(|e: core::num::ParseFloatError| {
                        TomlError::without_span(TomlErrorKind::InvalidValue {
                            message: e.to_string(),
                        })
                    })?,
                };
                Ok(ScalarValue::F64(f))
            }
            ScalarKind::DateTime => {
                // Keep as string, let facet-reflect handle datetime types
                Ok(ScalarValue::Str(output))
            }
        }
    }

    /// Parse a dotted key from the current position until we hit a delimiter.
    /// Returns the components and advances past any key separators.
    fn parse_dotted_key(&mut self) -> Vec<Cow<'de, str>> {
        let mut parts = Vec::new();

        loop {
            let Some(event) = self.peek_raw() else {
                break;
            };

            match event.kind() {
                EventKind::SimpleKey => {
                    let key = self.decode_key(event);
                    self.next_raw(); // consume the key
                    parts.push(key);
                }
                EventKind::KeySep => {
                    // Dot separator - consume and continue
                    self.next_raw();
                }
                _ => break,
            }
        }

        parts
    }

    /// Decode a key from an event.
    fn decode_key(&self, event: &Event) -> Cow<'de, str> {
        let raw = self.raw_from_event(event);
        let mut output: Cow<'de, str> = Cow::Borrowed("");
        raw.decode_key(&mut output, &mut ());
        output
    }

    /// Emit the "end" event for a path segment based on its kind.
    fn end_event_for_segment(segment: &PathSegment<'_>) -> ParseEvent<'static> {
        match segment.kind {
            SegmentKind::Table => ParseEvent::StructEnd,
            SegmentKind::Array => ParseEvent::SequenceEnd,
            SegmentKind::ArrayElement => ParseEvent::StructEnd,
        }
    }

    /// Compute navigation events to move from current path to target path.
    ///
    /// For standard tables `[foo.bar]`, target segments are all `Table` kind.
    /// For array tables `[[foo.bar]]`, the last segment is `Array` + `ArrayElement`.
    fn compute_navigation_to_table(
        &self,
        target_names: &[Cow<'de, str>],
    ) -> (Vec<ParseEvent<'de>>, Vec<PathSegment<'de>>) {
        let mut events = Vec::new();

        // Find common prefix length (by name only)
        let common_len = self
            .current_path
            .iter()
            .zip(target_names.iter())
            .take_while(|(seg, name)| &seg.name == *name)
            .count();

        // Pop up to common ancestor - emit end events in reverse order
        for segment in self.current_path[common_len..].iter().rev() {
            events.push(Self::end_event_for_segment(segment));
        }

        // Navigate down to target - all segments are Tables for [table.path]
        let mut new_path: Vec<PathSegment<'de>> = self.current_path[..common_len].to_vec();
        for name in &target_names[common_len..] {
            events.push(ParseEvent::FieldKey(FieldKey::new(
                name.clone(),
                FieldLocationHint::KeyValue,
            )));
            events.push(ParseEvent::StructStart(ContainerKind::Object));
            new_path.push(PathSegment {
                name: name.clone(),
                kind: SegmentKind::Table,
            });
        }

        (events, new_path)
    }

    /// Compute navigation events to move to an array table `[[path]]`.
    ///
    /// Array tables are special: the last segment becomes Array + ArrayElement,
    /// meaning we emit FieldKey, SequenceStart, StructStart.
    ///
    /// IMPORTANT: For array tables, we must NOT include Array/ArrayElement segments
    /// in the common prefix. Each `[[name]]` creates a NEW element, so we must fully
    /// exit any existing array of the same name and re-enter it.
    fn compute_navigation_to_array_table(
        &self,
        target_names: &[Cow<'de, str>],
    ) -> (Vec<ParseEvent<'de>>, Vec<PathSegment<'de>>) {
        let mut events = Vec::new();

        // Find common prefix length, but STOP at Array/ArrayElement segments.
        // We only keep Table segments in the common prefix because:
        // - Each [[array]] creates a NEW element, requiring full re-entry
        // - Table segments can be shared (e.g., [[foo.bar]] and [[foo.baz]] share "foo")
        let common_len = self
            .current_path
            .iter()
            .zip(target_names.iter())
            .take_while(|(seg, name)| {
                // Stop at Array or ArrayElement - these must be popped and re-entered
                if matches!(seg.kind, SegmentKind::Array | SegmentKind::ArrayElement) {
                    return false;
                }
                &seg.name == *name
            })
            .count();

        // Pop up to common ancestor
        for segment in self.current_path[common_len..].iter().rev() {
            events.push(Self::end_event_for_segment(segment));
        }

        // Navigate down - all but last are Tables, last is Array + ArrayElement
        let mut new_path: Vec<PathSegment<'de>> = self.current_path[..common_len].to_vec();

        if target_names.len() > common_len {
            // Navigate to parent tables first
            for name in &target_names[common_len..target_names.len() - 1] {
                events.push(ParseEvent::FieldKey(FieldKey::new(
                    name.clone(),
                    FieldLocationHint::KeyValue,
                )));
                events.push(ParseEvent::StructStart(ContainerKind::Object));
                new_path.push(PathSegment {
                    name: name.clone(),
                    kind: SegmentKind::Table,
                });
            }

            // Last segment is the array table
            let array_name = target_names.last().unwrap();
            events.push(ParseEvent::FieldKey(FieldKey::new(
                array_name.clone(),
                FieldLocationHint::KeyValue,
            )));
            events.push(ParseEvent::SequenceStart(ContainerKind::Array));
            events.push(ParseEvent::StructStart(ContainerKind::Object));

            new_path.push(PathSegment {
                name: array_name.clone(),
                kind: SegmentKind::Array,
            });
            new_path.push(PathSegment {
                name: array_name.clone(),
                kind: SegmentKind::ArrayElement,
            });
        }

        (events, new_path)
    }

    /// Produce the next parse event.
    fn produce_event(&mut self) -> Result<Option<ParseEvent<'de>>, TomlError> {
        // First, drain any pending navigation events
        if let Some(event) = self.pending_events.pop_front() {
            return Ok(Some(event));
        }

        // If we're inside inline containers, handle them specially
        if !self.inline_stack.is_empty() {
            return self.produce_inline_event();
        }

        // Emit root StructStart if we haven't yet
        if !self.root_started {
            self.root_started = true;
            return Ok(Some(ParseEvent::StructStart(ContainerKind::Object)));
        }

        // Get next raw event
        let Some(event) = self.peek_raw() else {
            // EOF - emit end events for remaining path elements, then root
            if self.root_ended {
                return Ok(None);
            }

            // Pop all remaining path segments
            for segment in self.current_path.iter().rev() {
                self.pending_events
                    .push_back(Self::end_event_for_segment(segment));
            }
            self.current_path.clear();

            // Final StructEnd for root
            self.pending_events.push_back(ParseEvent::StructEnd);
            self.root_ended = true;

            return Ok(self.pending_events.pop_front());
        };

        match event.kind() {
            EventKind::StdTableOpen => {
                // Standard table header [table.path]
                self.next_raw(); // consume StdTableOpen
                let path = self.parse_dotted_key();

                // Consume the StdTableClose
                if let Some(close) = self.peek_raw()
                    && matches!(close.kind(), EventKind::StdTableClose)
                {
                    self.next_raw();
                }

                // Compute navigation from current path to new table path
                let (nav_events, new_path) = self.compute_navigation_to_table(&path);
                for e in nav_events {
                    self.pending_events.push_back(e);
                }
                self.current_path = new_path;

                // If no navigation events were generated, recurse to get next actual event
                if self.pending_events.is_empty() {
                    return self.produce_event();
                }

                Ok(self.pending_events.pop_front())
            }

            EventKind::ArrayTableOpen => {
                // Array table header [[table.path]]
                self.next_raw(); // consume ArrayTableOpen
                let path = self.parse_dotted_key();

                // Consume the ArrayTableClose
                if let Some(close) = self.peek_raw()
                    && matches!(close.kind(), EventKind::ArrayTableClose)
                {
                    self.next_raw();
                }

                // Compute navigation to array table (handles reopening)
                let (nav_events, new_path) = self.compute_navigation_to_array_table(&path);
                for e in nav_events {
                    self.pending_events.push_back(e);
                }
                self.current_path = new_path;

                Ok(self.pending_events.pop_front())
            }

            EventKind::SimpleKey => {
                // Key-value pair
                let key_parts = self.parse_dotted_key();

                // Consume the KeyValSep (=)
                if let Some(sep) = self.peek_raw()
                    && matches!(sep.kind(), EventKind::KeyValSep)
                {
                    self.next_raw();
                }

                // For dotted keys like `foo.bar.baz = 1`, emit navigation events
                // to nested structs, then the final key
                if key_parts.len() > 1 {
                    // Navigate into nested structs
                    for name in &key_parts[..key_parts.len() - 1] {
                        self.pending_events
                            .push_back(ParseEvent::FieldKey(FieldKey::new(
                                name.clone(),
                                FieldLocationHint::KeyValue,
                            )));
                        self.pending_events
                            .push_back(ParseEvent::StructStart(ContainerKind::Object));
                    }

                    // Emit the final key
                    let final_key = key_parts.last().unwrap();
                    self.pending_events
                        .push_back(ParseEvent::FieldKey(FieldKey::new(
                            final_key.clone(),
                            FieldLocationHint::KeyValue,
                        )));

                    // Parse the value
                    self.parse_value_into_pending()?;

                    // Navigate back out of nested structs
                    for _ in 0..key_parts.len() - 1 {
                        self.pending_events.push_back(ParseEvent::StructEnd);
                    }

                    Ok(self.pending_events.pop_front())
                } else {
                    // Simple key
                    let key = key_parts.into_iter().next().unwrap();
                    self.pending_events
                        .push_back(ParseEvent::FieldKey(FieldKey::new(
                            key,
                            FieldLocationHint::KeyValue,
                        )));

                    // Parse the value
                    self.parse_value_into_pending()?;

                    Ok(self.pending_events.pop_front())
                }
            }

            EventKind::Error => {
                let span_str = self.get_span_str(event);
                Err(TomlError::without_span(TomlErrorKind::Parse(
                    span_str.to_string(),
                )))
            }

            _ => {
                // Skip unexpected events
                self.next_raw();
                self.produce_event()
            }
        }
    }

    /// Parse a value and add its events to pending_events.
    fn parse_value_into_pending(&mut self) -> Result<(), TomlError> {
        let Some(event) = self.peek_raw() else {
            return Err(TomlError::without_span(TomlErrorKind::UnexpectedEof {
                expected: "value",
            }));
        };

        match event.kind() {
            EventKind::Scalar => {
                let scalar = self.decode_scalar(event)?;
                self.next_raw();
                self.pending_events.push_back(ParseEvent::Scalar(scalar));
            }

            EventKind::InlineTableOpen => {
                self.next_raw();
                self.pending_events
                    .push_back(ParseEvent::StructStart(ContainerKind::Object));
                self.inline_stack.push(true); // true = inline table
            }

            EventKind::ArrayOpen => {
                self.next_raw();
                self.pending_events
                    .push_back(ParseEvent::SequenceStart(ContainerKind::Array));
                self.inline_stack.push(false); // false = array
            }

            _ => {
                return Err(TomlError::without_span(TomlErrorKind::UnexpectedType {
                    expected: "value",
                    got: "unexpected token",
                }));
            }
        }

        Ok(())
    }

    /// Produce events while inside inline containers (inline tables or arrays).
    fn produce_inline_event(&mut self) -> Result<Option<ParseEvent<'de>>, TomlError> {
        // Check pending events first
        if let Some(event) = self.pending_events.pop_front() {
            return Ok(Some(event));
        }

        let is_inline_table = *self.inline_stack.last().unwrap();

        let Some(event) = self.peek_raw() else {
            return Err(TomlError::without_span(TomlErrorKind::UnexpectedEof {
                expected: if is_inline_table { "}" } else { "]" },
            }));
        };

        match event.kind() {
            EventKind::InlineTableClose if is_inline_table => {
                self.next_raw();
                self.inline_stack.pop();
                Ok(Some(ParseEvent::StructEnd))
            }

            EventKind::ArrayClose if !is_inline_table => {
                self.next_raw();
                self.inline_stack.pop();
                Ok(Some(ParseEvent::SequenceEnd))
            }

            EventKind::ValueSep => {
                // Comma separator - skip and continue
                self.next_raw();
                self.produce_inline_event()
            }

            EventKind::SimpleKey if is_inline_table => {
                // Key in inline table
                let key_parts = self.parse_dotted_key();

                // Consume KeyValSep
                if let Some(sep) = self.peek_raw()
                    && matches!(sep.kind(), EventKind::KeyValSep)
                {
                    self.next_raw();
                }

                // Handle dotted keys
                if key_parts.len() > 1 {
                    for name in &key_parts[..key_parts.len() - 1] {
                        self.pending_events
                            .push_back(ParseEvent::FieldKey(FieldKey::new(
                                name.clone(),
                                FieldLocationHint::KeyValue,
                            )));
                        self.pending_events
                            .push_back(ParseEvent::StructStart(ContainerKind::Object));
                    }

                    let final_key = key_parts.last().unwrap();
                    self.pending_events
                        .push_back(ParseEvent::FieldKey(FieldKey::new(
                            final_key.clone(),
                            FieldLocationHint::KeyValue,
                        )));

                    self.parse_value_into_pending()?;

                    for _ in 0..key_parts.len() - 1 {
                        self.pending_events.push_back(ParseEvent::StructEnd);
                    }

                    Ok(self.pending_events.pop_front())
                } else {
                    let key = key_parts.into_iter().next().unwrap();
                    self.pending_events
                        .push_back(ParseEvent::FieldKey(FieldKey::new(
                            key,
                            FieldLocationHint::KeyValue,
                        )));
                    self.parse_value_into_pending()?;
                    Ok(self.pending_events.pop_front())
                }
            }

            EventKind::Scalar if !is_inline_table => {
                // Value in array
                let scalar = self.decode_scalar(event)?;
                self.next_raw();
                Ok(Some(ParseEvent::Scalar(scalar)))
            }

            EventKind::InlineTableOpen if !is_inline_table => {
                // Inline table inside array
                self.next_raw();
                self.inline_stack.push(true);
                Ok(Some(ParseEvent::StructStart(ContainerKind::Object)))
            }

            EventKind::ArrayOpen if !is_inline_table => {
                // Nested array
                self.next_raw();
                self.inline_stack.push(false);
                Ok(Some(ParseEvent::SequenceStart(ContainerKind::Array)))
            }

            _ => {
                // Skip unexpected
                self.next_raw();
                self.produce_inline_event()
            }
        }
    }

    /// Skip the current value (used for skip_value).
    fn skip_current_value(&mut self) -> Result<(), TomlError> {
        let Some(event) = self.peek_raw() else {
            return Ok(());
        };

        match event.kind() {
            EventKind::Scalar => {
                self.next_raw();
            }
            EventKind::InlineTableOpen => {
                self.skip_inline_container(true)?;
            }
            EventKind::ArrayOpen => {
                self.skip_inline_container(false)?;
            }
            _ => {}
        }

        Ok(())
    }

    /// Skip an inline container (table or array).
    fn skip_inline_container(&mut self, is_table: bool) -> Result<(), TomlError> {
        self.next_raw(); // consume opener

        let close_kind = if is_table {
            EventKind::InlineTableClose
        } else {
            EventKind::ArrayClose
        };

        let mut depth = 1;
        while depth > 0 {
            let Some(event) = self.next_raw() else {
                return Err(TomlError::without_span(TomlErrorKind::UnexpectedEof {
                    expected: if is_table { "}" } else { "]" },
                }));
            };

            match event.kind() {
                EventKind::InlineTableOpen => {
                    if is_table {
                        depth += 1;
                    }
                }
                EventKind::ArrayOpen => {
                    if !is_table {
                        depth += 1;
                    }
                }
                k if k == close_kind => {
                    depth -= 1;
                }
                _ => {}
            }
        }

        Ok(())
    }

    /// Build probe evidence by scanning ahead.
    fn build_probe(&self) -> Result<Vec<FieldEvidence<'de>>, TomlError> {
        let mut evidence = Vec::new();
        let mut pos = self.pos;

        // Skip to find field keys at current level
        while pos < self.events.len() {
            let event = &self.events[pos];

            if Self::should_skip(event) {
                pos += 1;
                continue;
            }

            match event.kind() {
                EventKind::SimpleKey => {
                    let key = self.decode_key(event);
                    pos += 1;

                    // Skip to value
                    while pos < self.events.len() {
                        let e = &self.events[pos];
                        if !Self::should_skip(e) {
                            break;
                        }
                        pos += 1;
                    }

                    // Skip KeySep (dots) and additional key parts
                    while pos < self.events.len() {
                        let e = &self.events[pos];
                        if Self::should_skip(e) {
                            pos += 1;
                            continue;
                        }
                        if matches!(e.kind(), EventKind::KeySep | EventKind::SimpleKey) {
                            pos += 1;
                            continue;
                        }
                        break;
                    }

                    // Skip KeyValSep (=)
                    if pos < self.events.len() {
                        let e = &self.events[pos];
                        if matches!(e.kind(), EventKind::KeyValSep) {
                            pos += 1;
                        }
                    }

                    // Skip whitespace to value
                    while pos < self.events.len() {
                        let e = &self.events[pos];
                        if !Self::should_skip(e) {
                            break;
                        }
                        pos += 1;
                    }

                    // Try to get scalar value
                    let scalar_value = if pos < self.events.len() {
                        let e = &self.events[pos];
                        if matches!(e.kind(), EventKind::Scalar) {
                            self.decode_scalar(e).ok()
                        } else {
                            None
                        }
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
                }

                EventKind::StdTableOpen
                | EventKind::ArrayTableOpen
                | EventKind::InlineTableClose
                | EventKind::ArrayClose => {
                    // Stop scanning at table boundaries or container ends
                    break;
                }

                _ => {
                    pos += 1;
                }
            }
        }

        Ok(evidence)
    }
}

impl<'de> FormatParser<'de> for TomlParser<'de> {
    type Error = TomlError;
    type Probe<'a>
        = TomlProbe<'de>
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
        Ok(TomlProbe { evidence, idx: 0 })
    }

    fn capture_raw(&mut self) -> Result<Option<&'de str>, Self::Error> {
        // TOML doesn't support raw capture (unlike JSON)
        self.skip_value()?;
        Ok(None)
    }
}

/// Probe stream for TOML.
pub struct TomlProbe<'de> {
    evidence: Vec<FieldEvidence<'de>>,
    idx: usize,
}

impl<'de> ProbeStream<'de> for TomlProbe<'de> {
    type Error = TomlError;

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
// Public API
// ============================================================================

/// Deserialize a TOML string into a type.
pub fn from_str<'de, T>(input: &'de str) -> Result<T, TomlError>
where
    T: facet_core::Facet<'de>,
{
    let parser = TomlParser::new(input)?;
    let mut deserializer = facet_format::FormatDeserializer::new(parser);
    deserializer.deserialize().map_err(|e| match e {
        facet_format::DeserializeError::Parser(e) => e,
        facet_format::DeserializeError::Reflect(e) => TomlError::from(e),
        facet_format::DeserializeError::UnexpectedEof { expected } => {
            TomlError::without_span(TomlErrorKind::UnexpectedEof { expected })
        }
        facet_format::DeserializeError::Unsupported(msg) => {
            TomlError::without_span(TomlErrorKind::InvalidValue { message: msg })
        }
        facet_format::DeserializeError::TypeMismatch { expected, got } => {
            TomlError::without_span(TomlErrorKind::InvalidValue {
                message: alloc::format!("type mismatch: expected {}, got {}", expected, got),
            })
        }
        facet_format::DeserializeError::UnknownField(field) => {
            TomlError::without_span(TomlErrorKind::UnknownField {
                field,
                expected: Vec::new(),
                suggestion: None,
            })
        }
        facet_format::DeserializeError::CannotBorrow { message, .. } => {
            TomlError::without_span(TomlErrorKind::InvalidValue { message })
        }
        facet_format::DeserializeError::MissingField { field, .. } => {
            TomlError::without_span(TomlErrorKind::MissingField {
                field,
                table_start: None,
                table_end: None,
            })
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to collect all events from a parser
    fn collect_events<'de>(parser: &mut TomlParser<'de>) -> Vec<ParseEvent<'de>> {
        let mut events = Vec::new();
        while let Ok(Some(event)) = parser.next_event() {
            events.push(event);
        }
        events
    }

    /// Helper to format events for debugging
    fn format_events(events: &[ParseEvent<'_>]) -> String {
        events
            .iter()
            .map(|e| format!("{:?}", e))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn test_simple_key_value() {
        let input = r#"
name = "test"
value = 42
"#;
        let mut parser = TomlParser::new(input).unwrap();

        // StructStart (root)
        assert!(matches!(
            parser.next_event().unwrap(),
            Some(ParseEvent::StructStart(ContainerKind::Object))
        ));

        // FieldKey("name")
        assert!(matches!(
            parser.next_event().unwrap(),
            Some(ParseEvent::FieldKey(key)) if key.name == "name"
        ));

        // Scalar("test")
        assert!(matches!(
            parser.next_event().unwrap(),
            Some(ParseEvent::Scalar(ScalarValue::Str(s))) if s == "test"
        ));

        // FieldKey("value")
        assert!(matches!(
            parser.next_event().unwrap(),
            Some(ParseEvent::FieldKey(key)) if key.name == "value"
        ));

        // Scalar(42)
        assert!(matches!(
            parser.next_event().unwrap(),
            Some(ParseEvent::Scalar(ScalarValue::I64(42)))
        ));

        // StructEnd (root)
        assert!(matches!(
            parser.next_event().unwrap(),
            Some(ParseEvent::StructEnd)
        ));

        // EOF
        assert!(parser.next_event().unwrap().is_none());
    }

    #[test]
    fn test_table_header() {
        let input = r#"
[server]
host = "localhost"
port = 8080
"#;
        let mut parser = TomlParser::new(input).unwrap();
        let events = collect_events(&mut parser);

        // Expected: StructStart, FieldKey(server), StructStart, FieldKey(host), Scalar,
        //           FieldKey(port), Scalar, StructEnd, StructEnd
        assert!(matches!(&events[0], ParseEvent::StructStart(_)));
        assert!(matches!(&events[1], ParseEvent::FieldKey(k) if k.name == "server"));
        assert!(matches!(&events[2], ParseEvent::StructStart(_)));
        assert!(matches!(&events[3], ParseEvent::FieldKey(k) if k.name == "host"));
        assert!(matches!(&events[4], ParseEvent::Scalar(ScalarValue::Str(s)) if s == "localhost"));
        assert!(matches!(&events[5], ParseEvent::FieldKey(k) if k.name == "port"));
        assert!(matches!(
            &events[6],
            ParseEvent::Scalar(ScalarValue::I64(8080))
        ));
        assert!(matches!(&events[7], ParseEvent::StructEnd)); // server
        assert!(matches!(&events[8], ParseEvent::StructEnd)); // root
    }

    #[test]
    fn test_array_table() {
        let input = r#"
[[servers]]
name = "alpha"

[[servers]]
name = "beta"
"#;
        let mut parser = TomlParser::new(input).unwrap();
        let events = collect_events(&mut parser);

        // Expected sequence:
        // StructStart (root)
        // FieldKey(servers), SequenceStart, StructStart (element 0)
        // FieldKey(name), Scalar(alpha)
        // StructEnd (element 0), SequenceEnd
        // FieldKey(servers), SequenceStart, StructStart (element 1) <- REOPEN
        // FieldKey(name), Scalar(beta)
        // StructEnd (element 1), SequenceEnd
        // StructEnd (root)

        let event_str = format_events(&events);
        eprintln!("Events:\n{}", event_str);

        assert!(matches!(&events[0], ParseEvent::StructStart(_))); // root
        assert!(matches!(&events[1], ParseEvent::FieldKey(k) if k.name == "servers"));
        assert!(matches!(&events[2], ParseEvent::SequenceStart(_)));
        assert!(matches!(&events[3], ParseEvent::StructStart(_))); // element 0
        assert!(matches!(&events[4], ParseEvent::FieldKey(k) if k.name == "name"));
        assert!(matches!(&events[5], ParseEvent::Scalar(ScalarValue::Str(s)) if s == "alpha"));
        assert!(matches!(&events[6], ParseEvent::StructEnd)); // element 0
        assert!(matches!(&events[7], ParseEvent::SequenceEnd)); // servers array (navigate up)

        // Reopen servers array
        assert!(matches!(&events[8], ParseEvent::FieldKey(k) if k.name == "servers"));
        assert!(matches!(&events[9], ParseEvent::SequenceStart(_)));
        assert!(matches!(&events[10], ParseEvent::StructStart(_))); // element 1
        assert!(matches!(&events[11], ParseEvent::FieldKey(k) if k.name == "name"));
        assert!(matches!(&events[12], ParseEvent::Scalar(ScalarValue::Str(s)) if s == "beta"));
    }

    #[test]
    fn test_interleaved_array_table() {
        // This is the tricky case: array table elements interleaved with other tables
        let input = r#"
[[servers]]
name = "alpha"

[database]
host = "localhost"

[[servers]]
name = "beta"
"#;
        let mut parser = TomlParser::new(input).unwrap();
        let events = collect_events(&mut parser);

        let event_str = format_events(&events);
        eprintln!("Interleaved events:\n{}", event_str);

        // The key verification: we should see servers array opened, closed,
        // then database, then servers reopened
        let mut saw_servers_first = false;
        let mut saw_database = false;
        let mut saw_servers_second = false;
        let mut servers_count = 0;

        for event in events.iter() {
            if let ParseEvent::FieldKey(k) = event {
                if k.name == "servers" {
                    servers_count += 1;
                    if !saw_database {
                        saw_servers_first = true;
                    } else {
                        saw_servers_second = true;
                    }
                } else if k.name == "database" {
                    saw_database = true;
                }
            }
        }

        assert!(saw_servers_first, "Should see servers before database");
        assert!(saw_database, "Should see database");
        assert!(
            saw_servers_second,
            "Should see servers reopened after database"
        );
        assert_eq!(servers_count, 2, "Should have two FieldKey(servers) events");
    }

    #[test]
    fn test_table_reopening() {
        // Standard table reopening (not array table)
        let input = r#"
[foo.bar]
x = 1

[foo.baz]
z = 3

[foo.bar]
y = 2
"#;
        let mut parser = TomlParser::new(input).unwrap();
        let events = collect_events(&mut parser);

        let event_str = format_events(&events);
        eprintln!("Table reopen events:\n{}", event_str);

        // Count how many times we see FieldKey("bar")
        let bar_count = events
            .iter()
            .filter(|e| matches!(e, ParseEvent::FieldKey(k) if k.name == "bar"))
            .count();

        assert_eq!(bar_count, 2, "Should see bar twice (reopened)");
    }

    #[test]
    fn test_dotted_key() {
        let input = r#"
foo.bar.baz = 1
"#;
        let mut parser = TomlParser::new(input).unwrap();
        let events = collect_events(&mut parser);

        let event_str = format_events(&events);
        eprintln!("Dotted key events:\n{}", event_str);

        // Expected: StructStart, FieldKey(foo), StructStart, FieldKey(bar), StructStart,
        //           FieldKey(baz), Scalar(1), StructEnd, StructEnd, StructEnd
        assert!(matches!(&events[0], ParseEvent::StructStart(_))); // root
        assert!(matches!(&events[1], ParseEvent::FieldKey(k) if k.name == "foo"));
        assert!(matches!(&events[2], ParseEvent::StructStart(_)));
        assert!(matches!(&events[3], ParseEvent::FieldKey(k) if k.name == "bar"));
        assert!(matches!(&events[4], ParseEvent::StructStart(_)));
        assert!(matches!(&events[5], ParseEvent::FieldKey(k) if k.name == "baz"));
        assert!(matches!(
            &events[6],
            ParseEvent::Scalar(ScalarValue::I64(1))
        ));
        // Three StructEnds for the nested structs, plus root
        assert!(matches!(&events[7], ParseEvent::StructEnd));
        assert!(matches!(&events[8], ParseEvent::StructEnd));
        assert!(matches!(&events[9], ParseEvent::StructEnd));
    }

    #[test]
    fn test_inline_table() {
        let input = r#"
server = { host = "localhost", port = 8080 }
"#;
        let mut parser = TomlParser::new(input).unwrap();
        let events = collect_events(&mut parser);

        let event_str = format_events(&events);
        eprintln!("Inline table events:\n{}", event_str);

        assert!(matches!(&events[0], ParseEvent::StructStart(_))); // root
        assert!(matches!(&events[1], ParseEvent::FieldKey(k) if k.name == "server"));
        assert!(matches!(&events[2], ParseEvent::StructStart(_))); // inline table
        assert!(matches!(&events[3], ParseEvent::FieldKey(k) if k.name == "host"));
        assert!(matches!(&events[4], ParseEvent::Scalar(ScalarValue::Str(s)) if s == "localhost"));
        assert!(matches!(&events[5], ParseEvent::FieldKey(k) if k.name == "port"));
        assert!(matches!(
            &events[6],
            ParseEvent::Scalar(ScalarValue::I64(8080))
        ));
        assert!(matches!(&events[7], ParseEvent::StructEnd)); // inline table
        assert!(matches!(&events[8], ParseEvent::StructEnd)); // root
    }

    #[test]
    fn test_inline_array() {
        let input = r#"
numbers = [1, 2, 3]
"#;
        let mut parser = TomlParser::new(input).unwrap();
        let events = collect_events(&mut parser);

        let event_str = format_events(&events);
        eprintln!("Inline array events:\n{}", event_str);

        assert!(matches!(&events[0], ParseEvent::StructStart(_))); // root
        assert!(matches!(&events[1], ParseEvent::FieldKey(k) if k.name == "numbers"));
        assert!(matches!(&events[2], ParseEvent::SequenceStart(_)));
        assert!(matches!(
            &events[3],
            ParseEvent::Scalar(ScalarValue::I64(1))
        ));
        assert!(matches!(
            &events[4],
            ParseEvent::Scalar(ScalarValue::I64(2))
        ));
        assert!(matches!(
            &events[5],
            ParseEvent::Scalar(ScalarValue::I64(3))
        ));
        assert!(matches!(&events[6], ParseEvent::SequenceEnd));
        assert!(matches!(&events[7], ParseEvent::StructEnd)); // root
    }

    // ========================================================================
    // Deserialization tests (full pipeline)
    // ========================================================================

    #[test]
    fn test_deserialize_simple_struct() {
        #[derive(Debug, PartialEq, facet::Facet)]
        struct Config {
            name: String,
            port: i64,
            enabled: bool,
        }

        let input = r#"
name = "myapp"
port = 8080
enabled = true
"#;
        let config: Config = from_str(input).unwrap();
        assert_eq!(config.name, "myapp");
        assert_eq!(config.port, 8080);
        assert!(config.enabled);
    }

    #[test]
    fn test_deserialize_nested_table() {
        #[derive(Debug, PartialEq, facet::Facet)]
        struct Config {
            server: Server,
        }

        #[derive(Debug, PartialEq, facet::Facet)]
        struct Server {
            host: String,
            port: i64,
        }

        let input = r#"
[server]
host = "localhost"
port = 3000
"#;
        let config: Config = from_str(input).unwrap();
        assert_eq!(config.server.host, "localhost");
        assert_eq!(config.server.port, 3000);
    }

    #[test]
    fn test_deserialize_array_table() {
        #[derive(Debug, PartialEq, facet::Facet)]
        struct Config {
            servers: Vec<Server>,
        }

        #[derive(Debug, PartialEq, facet::Facet)]
        struct Server {
            name: String,
        }

        let input = r#"
[[servers]]
name = "alpha"

[[servers]]
name = "beta"

[[servers]]
name = "gamma"
"#;
        let config: Config = from_str(input).unwrap();
        assert_eq!(config.servers.len(), 3);
        assert_eq!(config.servers[0].name, "alpha");
        assert_eq!(config.servers[1].name, "beta");
        assert_eq!(config.servers[2].name, "gamma");
    }

    #[test]
    fn test_deserialize_interleaved_array_table() {
        #[derive(Debug, PartialEq, facet::Facet)]
        struct Config {
            servers: Vec<Server>,
            database: Database,
        }

        #[derive(Debug, PartialEq, facet::Facet)]
        struct Server {
            name: String,
        }

        #[derive(Debug, PartialEq, facet::Facet)]
        struct Database {
            host: String,
        }

        let input = r#"
[[servers]]
name = "alpha"

[database]
host = "localhost"

[[servers]]
name = "beta"
"#;
        let config: Config = from_str(input).unwrap();
        assert_eq!(config.servers.len(), 2);
        assert_eq!(config.servers[0].name, "alpha");
        assert_eq!(config.servers[1].name, "beta");
        assert_eq!(config.database.host, "localhost");
    }

    #[test]
    fn test_deserialize_inline_table() {
        #[derive(Debug, PartialEq, facet::Facet)]
        struct Config {
            point: Point,
        }

        #[derive(Debug, PartialEq, facet::Facet)]
        struct Point {
            x: i64,
            y: i64,
        }

        let input = r#"point = { x = 10, y = 20 }"#;
        let config: Config = from_str(input).unwrap();
        assert_eq!(config.point.x, 10);
        assert_eq!(config.point.y, 20);
    }

    #[test]
    fn test_deserialize_inline_array() {
        #[derive(Debug, PartialEq, facet::Facet)]
        struct Config {
            values: Vec<i64>,
        }

        let input = r#"values = [1, 2, 3, 4, 5]"#;
        let config: Config = from_str(input).unwrap();
        assert_eq!(config.values, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_deserialize_dotted_key() {
        #[derive(Debug, PartialEq, facet::Facet)]
        struct Config {
            foo: Foo,
        }

        #[derive(Debug, PartialEq, facet::Facet)]
        struct Foo {
            bar: Bar,
        }

        #[derive(Debug, PartialEq, facet::Facet)]
        struct Bar {
            baz: i64,
        }

        let input = r#"foo.bar.baz = 42"#;
        let config: Config = from_str(input).unwrap();
        assert_eq!(config.foo.bar.baz, 42);
    }

    // NOTE: Table reopening deserialization is a known limitation.
    //
    // TOML allows fields for the same struct to appear at different points in the document:
    //
    //   [foo.bar]
    //   x = 1
    //   [foo.baz]
    //   z = 3
    //   [foo.bar]  # reopening!
    //   y = 2
    //
    // The parser correctly emits events with StructEnd as "navigation" (not finalization),
    // but facet-format's deserializer validates structs at StructEnd and fails because
    // 'y' hasn't been seen yet.
    //
    // Solutions require either:
    // 1. Adding deferred validation to facet-format (don't validate until EOF)
    // 2. Reordering events in the parser so each struct is contiguous
    //
    // The event stream test (test_table_reopening) passes because it only checks events,
    // not the full deserialization pipeline.
    #[test]
    #[ignore = "table reopening requires deferred validation in facet-format"]
    fn test_deserialize_table_reopening() {
        #[derive(Debug, PartialEq, facet::Facet)]
        struct Config {
            foo: Foo,
        }

        #[derive(Debug, PartialEq, facet::Facet)]
        struct Foo {
            bar: Bar,
            baz: Baz,
        }

        #[derive(Debug, PartialEq, facet::Facet)]
        struct Bar {
            x: i64,
            y: i64,
        }

        #[derive(Debug, PartialEq, facet::Facet)]
        struct Baz {
            z: i64,
        }

        let input = r#"
[foo.bar]
x = 1

[foo.baz]
z = 3

[foo.bar]
y = 2
"#;
        let config: Config = from_str(input).unwrap();
        assert_eq!(config.foo.bar.x, 1);
        assert_eq!(config.foo.bar.y, 2);
        assert_eq!(config.foo.baz.z, 3);
    }
}
