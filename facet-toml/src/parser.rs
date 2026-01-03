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
    /// Each entry is (is_inline_table, deferred_struct_ends) where:
    /// - is_inline_table: true for inline table, false for array
    /// - deferred_struct_ends: number of StructEnd events to emit when this container closes
    inline_stack: Vec<(bool, usize)>,
    /// The span of the most recently consumed scalar value (for error reporting).
    last_scalar_span: Option<facet_reflect::Span>,
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
            last_scalar_span: None,
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
    ///
    /// Special handling: When inside an array element (Array + ArrayElement pair),
    /// and the target path starts with the array's name, we stay in the current
    /// array element rather than exiting it. This handles cases like:
    /// ```toml
    /// [[item]]
    /// foo = 1
    /// [item.nested_item]  # nested_item is inside the current item element
    /// bar = 2
    /// ```
    fn compute_navigation_to_table(
        &self,
        target_names: &[Cow<'de, str>],
    ) -> (Vec<ParseEvent<'de>>, Vec<PathSegment<'de>>) {
        let mut events = Vec::new();

        // Find how many segments match, with special handling for Array+ArrayElement pairs.
        // An Array+ArrayElement pair in current_path corresponds to ONE segment in target_names.
        let mut current_idx = 0;
        let mut target_idx = 0;

        while current_idx < self.current_path.len() && target_idx < target_names.len() {
            let seg = &self.current_path[current_idx];
            let target_name = &target_names[target_idx];

            if seg.name != *target_name {
                break;
            }

            current_idx += 1;

            // If this was an Array segment and the next is its ArrayElement, include both
            // (but only advance target_idx once - both segments correspond to one target name)
            if matches!(seg.kind, SegmentKind::Array) && current_idx < self.current_path.len() {
                let next_seg = &self.current_path[current_idx];
                if matches!(next_seg.kind, SegmentKind::ArrayElement) && next_seg.name == seg.name {
                    current_idx += 1;
                }
            }

            target_idx += 1;
        }

        // Pop up to common ancestor - emit end events in reverse order
        for segment in self.current_path[current_idx..].iter().rev() {
            events.push(Self::end_event_for_segment(segment));
        }

        // Navigate down to target - all segments are Tables for [table.path]
        let mut new_path: Vec<PathSegment<'de>> = self.current_path[..current_idx].to_vec();
        for name in &target_names[target_idx..] {
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
    /// There are two cases to handle:
    /// 1. `[[item]]` after `[[item]]` - same array, new element. We must exit the
    ///    old element and re-enter the array with a new element.
    /// 2. `[[item.subarray]]` after `[[item]]` - nested array. We stay in the
    ///    current array element and add a nested array inside it.
    ///
    /// The distinction is whether the target path goes DEEPER than just matching
    /// the current array context.
    fn compute_navigation_to_array_table(
        &self,
        target_names: &[Cow<'de, str>],
    ) -> (Vec<ParseEvent<'de>>, Vec<PathSegment<'de>>) {
        let mut events = Vec::new();

        // Find how many segments match, with special handling for Array+ArrayElement pairs.
        let mut current_idx = 0;
        let mut target_idx = 0;

        while current_idx < self.current_path.len() && target_idx < target_names.len() {
            let seg = &self.current_path[current_idx];
            let target_name = &target_names[target_idx];

            if seg.name != *target_name {
                break;
            }

            // Check if this is an Array segment
            if matches!(seg.kind, SegmentKind::Array) {
                // Check if we're navigating DEEPER (more target segments after this)
                // or just reopening the same array (this is the last target segment)
                let more_targets_after = target_idx + 1 < target_names.len();

                if more_targets_after {
                    // Nested path like [[item.subarray]] - stay in the array element
                    current_idx += 1;
                    // Skip the ArrayElement too if it follows
                    if current_idx < self.current_path.len() {
                        let next_seg = &self.current_path[current_idx];
                        if matches!(next_seg.kind, SegmentKind::ArrayElement)
                            && next_seg.name == seg.name
                        {
                            current_idx += 1;
                        }
                    }
                    target_idx += 1;
                } else {
                    // Same array, new element like [[item]] then [[item]]
                    // Stop here - we need to exit and re-enter this array
                    break;
                }
            } else if matches!(seg.kind, SegmentKind::ArrayElement) {
                // Skip ArrayElement if we encounter it directly (should be handled with its Array)
                break;
            } else {
                // Table segment - include in common prefix
                current_idx += 1;
                target_idx += 1;
            }
        }

        // Pop up to common ancestor
        for segment in self.current_path[current_idx..].iter().rev() {
            events.push(Self::end_event_for_segment(segment));
        }

        // Navigate down - all but last are Tables, last is Array + ArrayElement
        let mut new_path: Vec<PathSegment<'de>> = self.current_path[..current_idx].to_vec();

        if target_names.len() > target_idx {
            // Navigate to parent tables first
            for name in &target_names[target_idx..target_names.len() - 1] {
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

                    // Track inline stack depth before parsing value
                    let inline_depth_before = self.inline_stack.len();

                    // Parse the value
                    self.parse_value_into_pending()?;

                    // Check if we entered an inline container (array or inline table)
                    let entered_inline_container = self.inline_stack.len() > inline_depth_before;

                    if entered_inline_container {
                        // Defer the StructEnd events until the inline container closes
                        let num_deferred = key_parts.len() - 1;
                        if let Some((_, deferred_closes)) = self.inline_stack.last_mut() {
                            *deferred_closes += num_deferred;
                        }
                    } else {
                        // Navigate back out of nested structs immediately (for scalar values)
                        for _ in 0..key_parts.len() - 1 {
                            self.pending_events.push_back(ParseEvent::StructEnd);
                        }
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
                // Track span for error reporting
                let span = event.span();
                self.last_scalar_span = Some(facet_reflect::Span::new(
                    span.start(),
                    span.end() - span.start(),
                ));
                self.next_raw();
                self.pending_events.push_back(ParseEvent::Scalar(scalar));
            }

            EventKind::InlineTableOpen => {
                self.next_raw();
                self.pending_events
                    .push_back(ParseEvent::StructStart(ContainerKind::Object));
                self.inline_stack.push((true, 0)); // true = inline table, 0 deferred closes
            }

            EventKind::ArrayOpen => {
                self.next_raw();
                self.pending_events
                    .push_back(ParseEvent::SequenceStart(ContainerKind::Array));
                self.inline_stack.push((false, 0)); // false = array, 0 deferred closes
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

        let (is_inline_table, _deferred_closes) = *self.inline_stack.last().unwrap();

        let Some(event) = self.peek_raw() else {
            return Err(TomlError::without_span(TomlErrorKind::UnexpectedEof {
                expected: if is_inline_table { "}" } else { "]" },
            }));
        };

        match event.kind() {
            EventKind::InlineTableClose if is_inline_table => {
                self.next_raw();
                let (_, deferred_closes) = self.inline_stack.pop().unwrap();
                // Emit the StructEnd for the inline table
                self.pending_events.push_back(ParseEvent::StructEnd);
                // Then emit any deferred StructEnd events from dotted keys
                for _ in 0..deferred_closes {
                    self.pending_events.push_back(ParseEvent::StructEnd);
                }
                Ok(self.pending_events.pop_front())
            }

            EventKind::ArrayClose if !is_inline_table => {
                self.next_raw();
                let (_, deferred_closes) = self.inline_stack.pop().unwrap();
                // Emit the SequenceEnd for the array
                self.pending_events.push_back(ParseEvent::SequenceEnd);
                // Then emit any deferred StructEnd events from dotted keys
                for _ in 0..deferred_closes {
                    self.pending_events.push_back(ParseEvent::StructEnd);
                }
                Ok(self.pending_events.pop_front())
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

                    // Track inline stack depth before parsing value
                    let inline_depth_before = self.inline_stack.len();

                    self.parse_value_into_pending()?;

                    // Check if we entered an inline container (array or inline table)
                    let entered_inline_container = self.inline_stack.len() > inline_depth_before;

                    if entered_inline_container {
                        // Defer the StructEnd events until the inline container closes
                        let num_deferred = key_parts.len() - 1;
                        if let Some((_, deferred_closes)) = self.inline_stack.last_mut() {
                            *deferred_closes += num_deferred;
                        }
                    } else {
                        // Navigate back out of nested structs immediately (for scalar values)
                        for _ in 0..key_parts.len() - 1 {
                            self.pending_events.push_back(ParseEvent::StructEnd);
                        }
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
                // Track span for error reporting
                let span = event.span();
                self.last_scalar_span = Some(facet_reflect::Span::new(
                    span.start(),
                    span.end() - span.start(),
                ));
                self.next_raw();
                Ok(Some(ParseEvent::Scalar(scalar)))
            }

            EventKind::InlineTableOpen if !is_inline_table => {
                // Inline table inside array
                self.next_raw();
                self.inline_stack.push((true, 0));
                Ok(Some(ParseEvent::StructStart(ContainerKind::Object)))
            }

            EventKind::ArrayOpen if !is_inline_table => {
                // Nested array
                self.next_raw();
                self.inline_stack.push((false, 0));
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
    ///
    /// This operates at the parse event level, not the raw TOML token level.
    /// It must handle:
    /// - Scalars: consume one Scalar event
    /// - Structs: consume StructStart, all contents, and StructEnd
    /// - Sequences: consume SequenceStart, all contents, and SequenceEnd
    fn skip_current_value(&mut self) -> Result<(), TomlError> {
        // Peek at the next parse event (not raw token)
        let Some(event) = self.produce_event()? else {
            return Ok(());
        };

        match event {
            ParseEvent::Scalar(_) => {
                // Scalar value - already consumed by produce_event
                Ok(())
            }
            ParseEvent::StructStart(_) => {
                // Need to skip the entire struct
                let mut depth = 1;
                while depth > 0 {
                    let Some(event) = self.produce_event()? else {
                        return Err(TomlError::without_span(TomlErrorKind::UnexpectedEof {
                            expected: "struct end",
                        }));
                    };
                    match event {
                        ParseEvent::StructStart(_) => depth += 1,
                        ParseEvent::StructEnd => depth -= 1,
                        _ => {}
                    }
                }
                Ok(())
            }
            ParseEvent::SequenceStart(_) => {
                // Need to skip the entire sequence
                let mut depth = 1;
                while depth > 0 {
                    let Some(event) = self.produce_event()? else {
                        return Err(TomlError::without_span(TomlErrorKind::UnexpectedEof {
                            expected: "sequence end",
                        }));
                    };
                    match event {
                        ParseEvent::SequenceStart(_) => depth += 1,
                        ParseEvent::SequenceEnd => depth -= 1,
                        _ => {}
                    }
                }
                Ok(())
            }
            _ => {
                // Unexpected event type - shouldn't happen in well-formed input
                Ok(())
            }
        }
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

    fn current_span(&self) -> Option<facet_reflect::Span> {
        self.last_scalar_span
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::from_str;

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
    fn test_issue_1399_array_of_tables_only_parses_last_entry() {
        // Regression test for #1399: array-of-tables should collect all entries, not just the last one
        // The bug is specifically with Option<Vec<T>>, not Vec<T>
        #[derive(Debug, PartialEq, facet::Facet)]
        struct Lockfile {
            version: Option<u32>,
            package: Option<Vec<Package>>,
        }

        #[derive(Debug, PartialEq, facet::Facet)]
        struct Package {
            name: String,
            version: String,
        }

        let input = r#"
version = 4

[[package]]
name = "myapp"
version = "0.1.0"

[[package]]
name = "aho-corasick"
version = "1.1.2"
"#;
        let lockfile: Lockfile = from_str(input).unwrap();

        assert_eq!(lockfile.version, Some(4));

        let packages = lockfile.package.expect("package field should be Some");
        assert_eq!(
            packages.len(),
            2,
            "Should parse both package entries, not just the last one"
        );

        assert_eq!(packages[0].name, "myapp");
        assert_eq!(packages[0].version, "0.1.0");

        assert_eq!(packages[1].name, "aho-corasick");
        assert_eq!(packages[1].version, "1.1.2");
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

    // Table reopening: TOML allows fields for the same struct to appear at different
    // points in the document. This works because facet-toml uses deferred mode, which
    // stores frames when we navigate away and restores them when we re-enter.
    #[test]
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
