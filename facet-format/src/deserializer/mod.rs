//! # Format Deserializer
//!
//! This module provides a generic deserializer that drives format-specific parsers
//! (JSON, TOML, etc.) directly into facet's `Partial` builder.
//!
//! ## Error Handling Philosophy
//!
//! Good error messages are critical for developer experience. When deserialization
//! fails, users need to know **where** the error occurred (both in the input and
//! in the type structure) and **what** went wrong. This module enforces several
//! invariants to ensure high-quality error messages.
//!
//! ### Always Include a Span
//!
//! Every error should include a `Span` pointing to the location in the input where
//! the error occurred. This allows tools to highlight the exact position:
//!
//! ```text
//! error: expected integer, got string
//!   --> config.toml:15:12
//!    |
//! 15 |     count = "not a number"
//!    |             ^^^^^^^^^^^^^^
//! ```
//!
//! The deserializer tracks `last_span` which is updated after consuming each event.
//! When constructing errors manually, always use `self.last_span`. The `SpanGuard`
//! RAII type sets a thread-local span that the `From<ReflectError>` impl uses
//! automatically.
//!
//! ### Always Include a Path
//!
//! Every error should include a `Path` showing the location in the type structure.
//! This is especially important for nested types where the span alone doesn't tell
//! you which field failed:
//!
//! ```text
//! error: missing required field `email`
//!   --> config.toml:10:5
//!    |
//! 10 |     [users.alice]
//!    |     ^^^^^^^^^^^^^
//!    |
//!    = path: config.users["alice"].contact
//! ```
//!
//! When constructing errors, use `wip.path()` to get the current path through the
//! type structure. The `Partial` tracks this automatically as you descend into
//! fields, list items, map keys, etc.
//!
//! ### Error Construction Patterns
//!
//! **For errors during deserialization (when `wip` is available):**
//!
//! ```ignore
//! return Err(DeserializeError {
//!     span: Some(self.last_span),
//!     path: Some(wip.path()),
//!     kind: DeserializeErrorKind::UnexpectedToken { ... },
//! });
//! ```
//!
//! **For errors from `Partial` methods (via `?` operator):**
//!
//! The `From<ReflectError>` impl automatically captures the span from the
//! thread-local `SpanGuard` and the path from the `ReflectError`. Just use `?`:
//!
//! ```ignore
//! let _guard = SpanGuard::new(self.last_span);
//! wip = wip.begin_field("name")?;  // Error automatically has span + path
//! ```
//!
//! **For errors with `DeserializeErrorKind::with_span()`:**
//!
//! When you only have an error kind and span (no `wip` for path):
//!
//! ```ignore
//! return Err(DeserializeErrorKind::UnexpectedEof { expected: "value" }
//!     .with_span(self.last_span));
//! ```
//!
//! Note: `with_span()` sets `path: None`. Prefer the full struct when you have a path.
//!
//! ### ReflectError Conversion
//!
//! Errors from `facet-reflect` (the `Partial` API) are converted via `From<ReflectError>`.
//! This impl:
//!
//! 1. Captures the span from the thread-local `CURRENT_SPAN` (set by `SpanGuard`)
//! 2. Preserves the path from `ReflectError::path`
//! 3. Wraps the error kind in `DeserializeErrorKind::Reflect`
//!
//! This means you must have an active `SpanGuard` when calling `Partial` methods
//! that might fail. The guard is typically created at the start of each
//! deserialization method:
//!
//! ```ignore
//! pub fn deserialize_struct(&mut self, wip: Partial) -> Result<...> {
//!     let _guard = SpanGuard::new(self.last_span);
//!     // ... Partial methods can now use ? and errors will have spans
//! }
//! ```
//!
//! ## Method Chaining with `.with()`
//!
//! The `Partial` API provides a `.with()` method for cleaner chaining when you
//! need to call deserializer methods (which take `&mut self`) in the middle of
//! a chain:
//!
//! ```ignore
//! // Instead of:
//! wip = wip.begin_field("name")?;
//! wip = self.deserialize_into(wip)?;
//! wip = wip.end()?;
//!
//! // Use:
//! wip = wip
//!     .begin_field("name")?
//!     .with(|w| self.deserialize_into(w))?
//!     .end()?;
//! ```
//!
//! This keeps the "begin/deserialize/end" pattern visually grouped and makes
//! the nesting structure clearer.

use std::collections::VecDeque;
use std::marker::PhantomData;
use std::sync::Arc;

use bumpalo::Bump;
use facet_core::{Facet, Shape};
use facet_reflect::{HeapValue, Partial, Span, typeplan::TypePlan};
use facet_solver::{KeyResult, Schema, Solver};

use crate::{FormatParser, ParseEvent};

mod error;
pub use error::*;

/// Convenience setters for string etc.
mod setters;

/// Entry point for deserialization
mod entry;

/// Deserialization of dynamic values
mod dynamic;

/// Enum handling
mod eenum;

/// Smart pointers (Box, Arc, etc.)
mod pointer;

/// Check if a scalar matches a target shape
mod scalar_matches;

/// Simple struct deserialization (no flatten)
mod struct_simple;

/// Not-so-simple struct deserialization (flatten)
mod struct_with_flatten;

/// Path navigation for flattened struct deserialization
mod path_navigator;

/// Default size of the event buffer for batched parsing.
pub const DEFAULT_EVENT_BUFFER_SIZE: usize = 512;

/// Generic deserializer that drives a format-specific parser directly into `Partial`.
///
/// The const generic `BORROW` controls whether string data can be borrowed:
/// - `BORROW=true`: strings without escapes are borrowed from input
/// - `BORROW=false`: all strings are owned
///
/// The lifetime `'parser` is the lifetime of the parser itself, which may be shorter
/// than `'input` (e.g., for streaming parsers that produce owned data but contain
/// references to internal state).
///
/// The lifetime `'bump` is the lifetime of the bump allocator used for TypePlan and
/// other internal allocations during deserialization.
pub struct FormatDeserializer<'parser, 'input, 'bump, const BORROW: bool> {
    parser: &'parser mut dyn FormatParser<'input>,

    /// Bump allocator for TypePlan and internal allocations.
    bump: &'bump Bump,

    /// The span of the most recently consumed event (for error reporting).
    last_span: Span,

    /// Buffer for batched event reading (push back, pop front).
    event_buffer: VecDeque<ParseEvent<'input>>,
    /// Maximum number of events to buffer at once.
    buffer_capacity: usize,

    /// Events recorded since save() was called (for restore).
    recording: Option<Vec<ParseEvent<'input>>>,

    /// Whether the parser is non-self-describing (postcard, etc.).
    /// For these formats, we bypass buffering entirely because hints
    /// clear the parser's peeked event and must take effect immediately.
    /// Computed once at construction time.
    is_non_self_describing: bool,

    _marker: PhantomData<&'input ()>,
}

impl<'parser, 'input, 'bump> FormatDeserializer<'parser, 'input, 'bump, true> {
    /// Create a new deserializer that can borrow strings from input.
    pub fn new(bump: &'bump Bump, parser: &'parser mut dyn FormatParser<'input>) -> Self {
        Self::with_buffer_capacity(bump, parser, DEFAULT_EVENT_BUFFER_SIZE)
    }

    /// Create a new deserializer with a custom buffer capacity.
    pub fn with_buffer_capacity(
        bump: &'bump Bump,
        parser: &'parser mut dyn FormatParser<'input>,
        buffer_capacity: usize,
    ) -> Self {
        let is_non_self_describing = !parser.is_self_describing();
        Self {
            parser,
            bump,
            last_span: Span { offset: 0, len: 0 },
            event_buffer: VecDeque::with_capacity(buffer_capacity),
            buffer_capacity,
            recording: None,
            is_non_self_describing,
            _marker: PhantomData,
        }
    }
}

impl<'parser, 'input, 'bump> FormatDeserializer<'parser, 'input, 'bump, false> {
    /// Create a new deserializer that produces owned strings.
    pub fn new_owned(bump: &'bump Bump, parser: &'parser mut dyn FormatParser<'input>) -> Self {
        Self::with_buffer_capacity_owned(bump, parser, DEFAULT_EVENT_BUFFER_SIZE)
    }

    /// Create a new deserializer with a custom buffer capacity.
    pub fn with_buffer_capacity_owned(
        bump: &'bump Bump,
        parser: &'parser mut dyn FormatParser<'input>,
        buffer_capacity: usize,
    ) -> Self {
        let is_non_self_describing = !parser.is_self_describing();
        Self {
            parser,
            bump,
            last_span: Span { offset: 0, len: 0 },
            event_buffer: VecDeque::with_capacity(buffer_capacity),
            buffer_capacity,
            recording: None,
            is_non_self_describing,
            _marker: PhantomData,
        }
    }
}

impl<'parser, 'input, 'bump, const BORROW: bool>
    FormatDeserializer<'parser, 'input, 'bump, BORROW>
{
    /// Borrow the inner parser mutably.
    pub fn parser_mut(&mut self) -> &mut dyn FormatParser<'input> {
        self.parser
    }
}

impl<'parser, 'input, 'bump> FormatDeserializer<'parser, 'input, 'bump, true> {
    /// Deserialize the next value in the stream into `T`, allowing borrowed strings.
    pub fn deserialize<T>(&mut self) -> Result<T, DeserializeError>
    where
        T: Facet<'input>,
    {
        let plan = TypePlan::<T>::build(self.bump)?;
        let wip: Partial<'input, 'bump, true> = plan.partial()?;
        let partial = self.deserialize_into(wip)?;
        // SpanGuard must cover build() and materialize() which can fail with ReflectError.
        // Created AFTER deserialize_into so last_span points to the final token.
        let _guard = SpanGuard::new(self.last_span);
        let heap_value: HeapValue<'input, true> = partial.build()?;
        Ok(heap_value.materialize::<T>()?)
    }

    /// Deserialize the next value in the stream into `T` (for backward compatibility).
    pub fn deserialize_root<T>(&mut self) -> Result<T, DeserializeError>
    where
        T: Facet<'input>,
    {
        self.deserialize()
    }

    /// Deserialize using deferred mode, allowing interleaved field initialization.
    ///
    /// This is required for formats like TOML that allow table reopening, where
    /// fields of a nested struct may be set, then fields of a sibling, then more
    /// fields of the original struct.
    pub fn deserialize_deferred<T>(&mut self) -> Result<T, DeserializeError>
    where
        T: Facet<'input>,
    {
        let plan = TypePlan::<T>::build(self.bump)?;
        let wip: Partial<'input, 'bump, true> = plan.partial()?;
        let wip = wip.begin_deferred()?;
        let partial = self.deserialize_into(wip)?;

        // SpanGuard must cover finish_deferred(), build() and materialize() which can fail with ReflectError.
        // Created AFTER deserialize_into so last_span points to the final token.
        let _guard = SpanGuard::new(self.last_span);
        let partial = partial.finish_deferred()?;
        let heap_value: HeapValue<'input, true> = partial.build()?;
        Ok(heap_value.materialize::<T>()?)
    }
}

impl<'parser, 'input, 'bump> FormatDeserializer<'parser, 'input, 'bump, false> {
    /// Deserialize the next value in the stream into `T`, using owned strings.
    pub fn deserialize<T>(&mut self) -> Result<T, DeserializeError>
    where
        T: Facet<'static>,
    {
        // Get format namespace for format-specific proxy resolution in TypePlan
        let format_ns = self.parser.format_namespace();

        let plan = TypePlan::<T>::build_for_format(self.bump, format_ns)?;
        // SAFETY: partial_owned produces Partial<'static, 'bump, false>, but our deserializer
        // expects 'input. Since BORROW=false means we never borrow from input anyway,
        // this is safe. We also transmute the HeapValue back to 'static before materializing.
        #[allow(unsafe_code)]
        let wip: Partial<'input, 'bump, false> = unsafe {
            core::mem::transmute::<Partial<'static, 'bump, false>, Partial<'input, 'bump, false>>(
                plan.partial_owned()?,
            )
        };

        let partial = self.deserialize_into(wip)?;

        // SpanGuard must cover build() and materialize() which can fail with ReflectError.
        // Created AFTER deserialize_into so last_span points to the final token.
        let _guard = SpanGuard::new(self.last_span);
        let heap_value: HeapValue<'input, false> = partial.build()?;

        // SAFETY: HeapValue<'input, false> contains no borrowed data because BORROW=false.
        // The transmute only changes the phantom lifetime marker.
        #[allow(unsafe_code)]
        let heap_value: HeapValue<'static, false> = unsafe {
            core::mem::transmute::<HeapValue<'input, false>, HeapValue<'static, false>>(heap_value)
        };

        Ok(heap_value.materialize::<T>()?)
    }

    /// Deserialize the next value in the stream into `T` (for backward compatibility).
    pub fn deserialize_root<T>(&mut self) -> Result<T, DeserializeError>
    where
        T: Facet<'static>,
    {
        self.deserialize()
    }

    /// Deserialize using deferred mode, allowing interleaved field initialization.
    ///
    /// This is required for formats like TOML that allow table reopening, where
    /// fields of a nested struct may be set, then fields of a sibling, then more
    /// fields of the original struct.
    pub fn deserialize_deferred<T>(&mut self) -> Result<T, DeserializeError>
    where
        T: Facet<'static>,
    {
        // Get format namespace for format-specific proxy resolution in TypePlan
        let format_ns = self.parser.format_namespace();

        let plan = TypePlan::<T>::build_for_format(self.bump, format_ns)?;
        // SAFETY: partial_owned produces Partial<'static, 'bump, false>, but our deserializer
        // expects 'input. Since BORROW=false means we never borrow from input anyway,
        // this is safe. We also transmute the HeapValue back to 'static before materializing.
        #[allow(unsafe_code)]
        let wip: Partial<'input, 'bump, false> = unsafe {
            core::mem::transmute::<Partial<'static, 'bump, false>, Partial<'input, 'bump, false>>(
                plan.partial_owned()?,
            )
        };
        let wip = wip.begin_deferred()?;
        let partial = self.deserialize_into(wip)?;

        // SpanGuard must cover finish_deferred(), build() and materialize() which can fail with ReflectError.
        // Created AFTER deserialize_into so last_span points to the final token.
        let _guard = SpanGuard::new(self.last_span);
        let partial = partial.finish_deferred()?;
        let heap_value: HeapValue<'input, false> = partial.build()?;

        // SAFETY: HeapValue<'input, false> contains no borrowed data because BORROW=false.
        // The transmute only changes the phantom lifetime marker.
        #[allow(unsafe_code)]
        let heap_value: HeapValue<'static, false> = unsafe {
            core::mem::transmute::<HeapValue<'input, false>, HeapValue<'static, false>>(heap_value)
        };

        Ok(heap_value.materialize::<T>()?)
    }

    /// Deserialize using an explicit source shape for parser hints.
    ///
    /// This is useful for non-self-describing formats like postcard where you need
    /// to decode data that was serialized using a specific type, but you only have
    /// the shape information at runtime (not the concrete type).
    ///
    /// The target type `T` should typically be a `DynamicValue` like `facet_value::Value`.
    pub fn deserialize_with_shape<T>(
        &mut self,
        source_shape: &'static Shape,
    ) -> Result<T, DeserializeError>
    where
        T: Facet<'static>,
    {
        // Get format namespace for format-specific proxy resolution in TypePlan
        let format_ns = self.parser.format_namespace();

        let plan = TypePlan::<T>::build_for_format(self.bump, format_ns)?;
        #[allow(unsafe_code)]
        let wip: Partial<'input, 'bump, false> = unsafe {
            core::mem::transmute::<Partial<'static, 'bump, false>, Partial<'input, 'bump, false>>(
                plan.partial_owned()?,
            )
        };

        let partial = self.deserialize_into_with_shape(wip, source_shape)?;

        // SpanGuard must cover build() and materialize() which can fail with ReflectError.
        // Created AFTER deserialize_into so last_span points to the final token.
        let _guard = SpanGuard::new(self.last_span);
        let heap_value: HeapValue<'input, false> = partial.build()?;

        #[allow(unsafe_code)]
        let heap_value: HeapValue<'static, false> = unsafe {
            core::mem::transmute::<HeapValue<'input, false>, HeapValue<'static, false>>(heap_value)
        };

        Ok(heap_value.materialize::<T>()?)
    }
}

impl<'parser, 'input, 'bump, const BORROW: bool>
    FormatDeserializer<'parser, 'input, 'bump, BORROW>
{
    /// Refill the event buffer from the parser.
    #[inline]
    fn refill_buffer(&mut self) -> Result<(), ParseError> {
        self.parser
            .next_events(&mut self.event_buffer, self.buffer_capacity)?;
        Ok(())
    }

    /// Check if parser is non-self-describing.
    #[inline(always)]
    fn is_non_self_describing(&self) -> bool {
        self.is_non_self_describing
    }

    /// Read the next event, returning an error if EOF is reached.
    #[inline]
    fn expect_event(
        &mut self,
        expected: &'static str,
    ) -> Result<ParseEvent<'input>, DeserializeError> {
        // For non-self-describing formats, bypass buffering entirely
        // because hints clear the parser's peeked event and must take effect immediately
        if self.is_non_self_describing() {
            let event = self.parser.next_event()?.ok_or_else(|| {
                DeserializeErrorKind::UnexpectedEof { expected }.with_span(self.last_span)
            })?;
            trace!(?event, expected, "expect_event (direct): got event");
            self.last_span = event.span;
            return Ok(event);
        }

        // Refill if empty
        if self.event_buffer.is_empty() {
            self.refill_buffer()?;
        }

        let event = self.event_buffer.pop_front().ok_or_else(|| {
            DeserializeErrorKind::UnexpectedEof { expected }.with_span(self.last_span)
        })?;

        // Record event if we're in save mode
        if let Some(ref mut rec) = self.recording {
            rec.push(event.clone());
        }

        trace!(?event, expected, "expect_event: got event");
        self.last_span = event.span;
        Ok(event)
    }

    /// Save the current position for later restore.
    #[inline]
    fn save(&mut self) {
        self.recording = Some(Vec::new());
    }

    /// Restore to the saved position, replaying recorded events.
    #[inline]
    fn restore(&mut self) {
        if let Some(recorded) = self.recording.take() {
            // Prepend recorded events to front of buffer (in reverse order)
            for event in recorded.into_iter().rev() {
                self.event_buffer.push_front(event);
            }
        }
    }

    /// Peek at the next event, returning an error if EOF is reached.
    #[inline]
    fn expect_peek(
        &mut self,
        expected: &'static str,
    ) -> Result<ParseEvent<'input>, DeserializeError> {
        self.peek_event_opt()?.ok_or_else(|| {
            DeserializeErrorKind::UnexpectedEof { expected }.with_span(self.last_span)
        })
    }

    /// Peek at the next event, returning None if EOF is reached.
    #[inline]
    fn peek_event_opt(&mut self) -> Result<Option<ParseEvent<'input>>, DeserializeError> {
        // For non-self-describing formats, bypass buffering entirely
        if self.is_non_self_describing() {
            let event = self.parser.peek_event()?;
            if let Some(ref _e) = event {
                trace!(?_e, "peek_event_opt (direct): peeked event");
            }
            return Ok(event);
        }

        // Refill if empty
        if self.event_buffer.is_empty() {
            self.refill_buffer()?;
        }

        let event = self.event_buffer.front().cloned();
        if let Some(ref _e) = event {
            trace!(?_e, "peek_event_opt: peeked event");
        }
        Ok(event)
    }

    /// Count buffered sequence items without consuming events.
    ///
    /// Scans the event buffer to count how many items exist at depth 0.
    /// Returns the count found so far - this is a lower bound useful for
    /// pre-reserving Vec capacity.
    ///
    /// If the full sequence is buffered (ends with `SequenceEnd`), this
    /// returns the exact count. Otherwise it returns a partial count.
    #[inline]
    pub(crate) fn count_buffered_sequence_items(&self) -> usize {
        use crate::ParseEventKind;

        let mut count = 0usize;
        let mut depth = 0i32;

        for event in &self.event_buffer {
            match &event.kind {
                ParseEventKind::StructStart(_) | ParseEventKind::SequenceStart(_) => {
                    if depth == 0 {
                        // Starting a new item at depth 0
                        count += 1;
                    }
                    depth += 1;
                }
                ParseEventKind::StructEnd | ParseEventKind::SequenceEnd => {
                    depth -= 1;
                    if depth < 0 {
                        // Found the closing SequenceEnd for our list
                        return count;
                    }
                }
                ParseEventKind::Scalar(_) if depth == 0 => {
                    // Scalar at depth 0 is a list item
                    count += 1;
                }
                _ => {}
            }
        }

        // Return partial count - still useful for reserve
        count
    }

    /// Skip the current value using the buffer, returning start and end offsets.
    #[inline]
    fn skip_value_with_span(&mut self) -> Result<(usize, usize), DeserializeError> {
        use crate::ParseEventKind;

        // Peek to get the start offset
        let first_event = self.expect_peek("value to skip")?;
        let start_offset = first_event.span.offset as usize;
        #[allow(unused_assignments)]
        let mut end_offset = 0usize;

        let mut depth = 0i32;
        loop {
            let event = self.expect_event("value to skip")?;
            // Track the end of each event
            end_offset = event.span.end();

            match &event.kind {
                ParseEventKind::StructStart(_) | ParseEventKind::SequenceStart(_) => {
                    depth += 1;
                }
                ParseEventKind::StructEnd | ParseEventKind::SequenceEnd => {
                    depth -= 1;
                    if depth <= 0 {
                        return Ok((start_offset, end_offset));
                    }
                }
                ParseEventKind::Scalar(_) if depth == 0 => {
                    return Ok((start_offset, end_offset));
                }
                _ => {}
            }
        }
    }

    /// Skip the current value using the buffer.
    #[inline]
    fn skip_value(&mut self) -> Result<(), DeserializeError> {
        self.skip_value_with_span()?;
        Ok(())
    }

    /// Capture the raw bytes of the current value without parsing it.
    #[inline]
    fn capture_raw(&mut self) -> Result<Option<&'input str>, DeserializeError> {
        let Some(input) = self.parser.input() else {
            // Parser doesn't provide raw input access
            self.skip_value()?;
            return Ok(None);
        };

        let (start, end) = self.skip_value_with_span()?;

        // Slice the input
        if end <= input.len() {
            // SAFETY: We trust the parser's spans to be valid UTF-8 boundaries
            let raw = core::str::from_utf8(&input[start..end]).map_err(|_| {
                DeserializeErrorKind::InvalidValue {
                    message: "raw capture contains invalid UTF-8".into(),
                }
                .with_span(self.last_span)
            })?;
            Ok(Some(raw))
        } else {
            Ok(None)
        }
    }

    /// Read the next event, returning None if EOF is reached.
    #[inline]
    fn next_event_opt(&mut self) -> Result<Option<ParseEvent<'input>>, DeserializeError> {
        // Refill if empty
        if self.event_buffer.is_empty() {
            self.refill_buffer()?;
        }

        let Some(event) = self.event_buffer.pop_front() else {
            return Ok(None);
        };

        // Record event if we're in save mode
        if let Some(ref mut rec) = self.recording {
            rec.push(event.clone());
        }

        self.last_span = event.span;
        Ok(Some(event))
    }

    /// Attempt to solve which enum variant matches the input.
    ///
    /// This uses save/restore to read ahead and determine the variant without
    /// consuming the events permanently. After this returns, the position
    /// is restored so the actual deserialization can proceed.
    pub(crate) fn solve_variant(
        &mut self,
        shape: &'static facet_core::Shape,
    ) -> Result<Option<crate::SolveOutcome>, crate::SolveVariantError> {
        let schema = Arc::new(Schema::build_auto(shape)?);
        let mut solver = Solver::new(&schema);

        // Save position and start recording events
        self.save();

        let mut depth = 0i32;
        let mut in_struct = false;
        let mut expecting_value = false;

        let result = loop {
            let event = self.next_event_opt().map_err(|e| {
                crate::SolveVariantError::Parser(ParseError::new(
                    e.span.unwrap_or(self.last_span),
                    e.kind,
                ))
            })?;

            let Some(event) = event else {
                // EOF reached
                self.restore();
                return Ok(None);
            };

            match event.kind {
                crate::ParseEventKind::StructStart(_) => {
                    depth += 1;
                    if depth == 1 {
                        in_struct = true;
                    }
                }
                crate::ParseEventKind::StructEnd => {
                    depth -= 1;
                    if depth == 0 {
                        // Done with top-level struct
                        break None;
                    }
                }
                crate::ParseEventKind::SequenceStart(_) => {
                    depth += 1;
                }
                crate::ParseEventKind::SequenceEnd => {
                    depth -= 1;
                }
                crate::ParseEventKind::FieldKey(ref key) => {
                    if depth == 1 && in_struct {
                        // Top-level field - feed to solver
                        if let Some(name) = key.name() {
                            match solver.see_key(name.clone()) {
                                KeyResult::Solved(handle) => {
                                    break Some(handle);
                                }
                                KeyResult::Unknown
                                | KeyResult::Unambiguous { .. }
                                | KeyResult::Ambiguous { .. } => {}
                            }
                        }
                        expecting_value = true;
                    }
                }
                crate::ParseEventKind::Scalar(_)
                | crate::ParseEventKind::OrderedField
                | crate::ParseEventKind::VariantTag(_) => {
                    if expecting_value {
                        expecting_value = false;
                    }
                }
            }
        };

        // Restore position regardless of outcome
        self.restore();

        match result {
            Some(handle) => {
                let idx = handle.index();
                Ok(Some(crate::SolveOutcome {
                    schema,
                    resolution_index: idx,
                }))
            }
            None => Ok(None),
        }
    }

    /// Make an error using the last span, the current path of the given wip.
    fn mk_err(
        &self,
        wip: &Partial<'input, 'bump, BORROW>,
        kind: DeserializeErrorKind,
    ) -> DeserializeError {
        DeserializeError {
            span: Some(self.last_span),
            path: Some(wip.path()),
            kind,
        }
    }
}
