//! Streaming JSON deserialization using stackful coroutines.
//!
//! This module provides `from_reader` that can deserialize JSON from any `Read`
//! source without requiring the entire input to be in memory.

extern crate alloc;

use alloc::borrow::Cow;
use alloc::rc::Rc;
use alloc::vec::Vec;
use core::cell::RefCell;

use corosensei::{Coroutine, CoroutineResult};
use facet_core::Facet;
use facet_format::{
    ContainerKind, DeserializeError, FieldEvidence, FieldKey, FieldLocationHint,
    FormatDeserializer, FormatParser, ParseEvent, ProbeStream, ScalarValue,
};

use crate::adapter::{SpannedAdapterToken, Token as AdapterToken, TokenSource};
use crate::error::{JsonError, JsonErrorKind};
use crate::scan_buffer::ScanBuffer;
use crate::streaming_adapter::StreamingAdapter;

/// Deserialize JSON from a synchronous reader.
///
/// This function streams the JSON input, reading chunks as needed rather than
/// requiring the entire input to be in memory.
///
/// # Example
///
/// ```ignore
/// use std::io::Cursor;
/// use facet::Facet;
/// use facet_json::from_reader;
///
/// #[derive(Facet, Debug)]
/// struct Person {
///     name: String,
///     age: u32,
/// }
///
/// let json = br#"{"name": "Alice", "age": 30}"#;
/// let reader = Cursor::new(json);
/// let person: Person = from_reader(reader).unwrap();
/// ```
#[cfg(feature = "std")]
pub fn from_reader<R, T>(mut reader: R) -> Result<T, DeserializeError<JsonError>>
where
    R: std::io::Read,
    T: Facet<'static>,
{
    // Shared buffer between coroutine and driver
    let buffer = Rc::new(RefCell::new(ScanBuffer::new()));
    let buffer_for_coroutine = buffer.clone();

    // Initial fill
    {
        let mut buf = buffer.borrow_mut();
        let n = buf.refill(&mut reader).map_err(|e| {
            DeserializeError::Parser(JsonError::without_span(JsonErrorKind::Io(e.to_string())))
        })?;
        if n == 0 {
            return Err(DeserializeError::Parser(JsonError::without_span(
                JsonErrorKind::UnexpectedEof {
                    expected: "JSON value",
                },
            )));
        }
    }

    // Create coroutine that runs the deserializer
    let mut coroutine: Coroutine<(), (), Result<T, DeserializeError<JsonError>>> =
        Coroutine::new(move |yielder, ()| {
            let adapter = StreamingAdapter::new(buffer_for_coroutine, yielder);
            let parser = StreamingJsonParser::new(adapter);
            let mut de = FormatDeserializer::new_owned(parser);
            de.deserialize_root::<T>()
        });

    // Drive the coroutine
    loop {
        match coroutine.resume(()) {
            CoroutineResult::Yield(()) => {
                // Coroutine needs more data - refill buffer
                let mut buf = buffer.borrow_mut();

                // If buffer is full, grow it
                if buf.filled() == buf.capacity() {
                    buf.grow();
                }

                let _n = buf.refill(&mut reader).map_err(|e| {
                    DeserializeError::Parser(JsonError::without_span(JsonErrorKind::Io(
                        e.to_string(),
                    )))
                })?;
            }
            CoroutineResult::Return(result) => {
                return result;
            }
        }
    }
}

/// Deserialize JSON from an async reader (tokio).
///
/// This function streams the JSON input asynchronously, reading chunks as needed.
#[cfg(feature = "tokio")]
#[allow(clippy::await_holding_refcell_ref)]
pub async fn from_async_reader_tokio<R, T>(mut reader: R) -> Result<T, DeserializeError<JsonError>>
where
    R: tokio::io::AsyncRead + Unpin,
    T: Facet<'static>,
{
    let buffer = Rc::new(RefCell::new(ScanBuffer::new()));
    let buffer_for_coroutine = buffer.clone();

    // Initial fill
    {
        let mut buf = buffer.borrow_mut();
        let n = buf.refill_tokio(&mut reader).await.map_err(|e| {
            DeserializeError::Parser(JsonError::without_span(JsonErrorKind::Io(e.to_string())))
        })?;
        if n == 0 {
            return Err(DeserializeError::Parser(JsonError::without_span(
                JsonErrorKind::UnexpectedEof {
                    expected: "JSON value",
                },
            )));
        }
    }

    let mut coroutine: Coroutine<(), (), Result<T, DeserializeError<JsonError>>> =
        Coroutine::new(move |yielder, ()| {
            let adapter = StreamingAdapter::new(buffer_for_coroutine, yielder);
            let parser = StreamingJsonParser::new(adapter);
            let mut de = FormatDeserializer::new_owned(parser);
            de.deserialize_root::<T>()
        });

    loop {
        match coroutine.resume(()) {
            CoroutineResult::Yield(()) => {
                let mut buf = buffer.borrow_mut();
                if buf.filled() == buf.capacity() {
                    buf.grow();
                }
                let _n = buf.refill_tokio(&mut reader).await.map_err(|e| {
                    DeserializeError::Parser(JsonError::without_span(JsonErrorKind::Io(
                        e.to_string(),
                    )))
                })?;
            }
            CoroutineResult::Return(result) => {
                return result;
            }
        }
    }
}

/// Deserialize JSON from an async reader (futures-io).
///
/// This function streams the JSON input asynchronously, reading chunks as needed.
#[cfg(feature = "futures-io")]
#[allow(clippy::await_holding_refcell_ref)]
pub async fn from_async_reader_futures<R, T>(
    mut reader: R,
) -> Result<T, DeserializeError<JsonError>>
where
    R: futures_io::AsyncRead + Unpin,
    T: Facet<'static>,
{
    let buffer = Rc::new(RefCell::new(ScanBuffer::new()));
    let buffer_for_coroutine = buffer.clone();

    // Initial fill
    {
        let mut buf = buffer.borrow_mut();
        let n = buf.refill_futures(&mut reader).await.map_err(|e| {
            DeserializeError::Parser(JsonError::without_span(JsonErrorKind::Io(e.to_string())))
        })?;
        if n == 0 {
            return Err(DeserializeError::Parser(JsonError::without_span(
                JsonErrorKind::UnexpectedEof {
                    expected: "JSON value",
                },
            )));
        }
    }

    let mut coroutine: Coroutine<(), (), Result<T, DeserializeError<JsonError>>> =
        Coroutine::new(move |yielder, ()| {
            let adapter = StreamingAdapter::new(buffer_for_coroutine, yielder);
            let parser = StreamingJsonParser::new(adapter);
            let mut de = FormatDeserializer::new_owned(parser);
            de.deserialize_root::<T>()
        });

    loop {
        match coroutine.resume(()) {
            CoroutineResult::Yield(()) => {
                let mut buf = buffer.borrow_mut();
                if buf.filled() == buf.capacity() {
                    buf.grow();
                }
                let _n = buf.refill_futures(&mut reader).await.map_err(|e| {
                    DeserializeError::Parser(JsonError::without_span(JsonErrorKind::Io(
                        e.to_string(),
                    )))
                })?;
            }
            CoroutineResult::Return(result) => {
                return result;
            }
        }
    }
}

/// Streaming JSON parser that implements `FormatParser<'static>`.
///
/// Wraps a `TokenSource<'static>` (like `StreamingAdapter`) and converts
/// tokens to `ParseEvent`s.
pub struct StreamingJsonParser<A> {
    adapter: A,
    stack: Vec<ContextState>,
    event_peek: Option<ParseEvent<'static>>,
    root_started: bool,
    root_complete: bool,
    /// Buffered events for replay after probing
    event_buffer: Vec<ParseEvent<'static>>,
    /// Index into event_buffer for replay
    buffer_idx: usize,
}

#[derive(Debug, Clone)]
enum ContextState {
    Object(ObjectState),
    Array(ArrayState),
}

#[derive(Debug, Clone)]
enum ObjectState {
    KeyOrEnd,
    Value,
    CommaOrEnd,
}

#[derive(Debug, Clone)]
enum ArrayState {
    ValueOrEnd,
    CommaOrEnd,
}

impl<A: TokenSource<'static>> StreamingJsonParser<A> {
    /// Create a new streaming parser wrapping a token source.
    pub fn new(adapter: A) -> Self {
        Self {
            adapter,
            stack: Vec::new(),
            event_peek: None,
            root_started: false,
            root_complete: false,
            event_buffer: Vec::new(),
            buffer_idx: 0,
        }
    }

    fn next_token(&mut self) -> Result<SpannedAdapterToken<'static>, JsonError> {
        self.adapter.next_token()
    }

    fn unexpected(
        &self,
        token: &SpannedAdapterToken<'static>,
        expected: &'static str,
    ) -> JsonError {
        JsonError::new(
            JsonErrorKind::UnexpectedToken {
                got: alloc::format!("{:?}", token.token),
                expected,
            },
            token.span,
        )
    }

    fn expect_colon(&mut self) -> Result<(), JsonError> {
        let token = self.next_token()?;
        if !matches!(token.token, AdapterToken::Colon) {
            return Err(self.unexpected(&token, "':'"));
        }
        Ok(())
    }

    fn finish_value_in_parent(&mut self) {
        if let Some(context) = self.stack.last_mut() {
            match context {
                ContextState::Object(state) => *state = ObjectState::CommaOrEnd,
                ContextState::Array(state) => *state = ArrayState::CommaOrEnd,
            }
        } else if self.root_started {
            self.root_complete = true;
        }
    }

    fn parse_value_start_with_token(
        &mut self,
        first: Option<SpannedAdapterToken<'static>>,
    ) -> Result<ParseEvent<'static>, JsonError> {
        let token = match first {
            Some(tok) => tok,
            None => self.next_token()?,
        };

        self.root_started = true;

        match token.token {
            AdapterToken::ObjectStart => {
                self.stack.push(ContextState::Object(ObjectState::KeyOrEnd));
                Ok(ParseEvent::StructStart(ContainerKind::Object))
            }
            AdapterToken::ArrayStart => {
                self.stack.push(ContextState::Array(ArrayState::ValueOrEnd));
                Ok(ParseEvent::SequenceStart(ContainerKind::Array))
            }
            AdapterToken::String(s) => {
                let event = ParseEvent::Scalar(ScalarValue::Str(s));
                self.finish_value_in_parent();
                Ok(event)
            }
            AdapterToken::True => {
                self.finish_value_in_parent();
                Ok(ParseEvent::Scalar(ScalarValue::Bool(true)))
            }
            AdapterToken::False => {
                self.finish_value_in_parent();
                Ok(ParseEvent::Scalar(ScalarValue::Bool(false)))
            }
            AdapterToken::Null => {
                self.finish_value_in_parent();
                Ok(ParseEvent::Scalar(ScalarValue::Null))
            }
            AdapterToken::U64(n) => {
                self.finish_value_in_parent();
                Ok(ParseEvent::Scalar(ScalarValue::U64(n)))
            }
            AdapterToken::I64(n) => {
                self.finish_value_in_parent();
                Ok(ParseEvent::Scalar(ScalarValue::I64(n)))
            }
            AdapterToken::U128(n) => {
                self.finish_value_in_parent();
                Ok(ParseEvent::Scalar(ScalarValue::Str(Cow::Owned(
                    n.to_string(),
                ))))
            }
            AdapterToken::I128(n) => {
                self.finish_value_in_parent();
                Ok(ParseEvent::Scalar(ScalarValue::Str(Cow::Owned(
                    n.to_string(),
                ))))
            }
            AdapterToken::F64(n) => {
                self.finish_value_in_parent();
                Ok(ParseEvent::Scalar(ScalarValue::F64(n)))
            }
            AdapterToken::ObjectEnd | AdapterToken::ArrayEnd => {
                Err(self.unexpected(&token, "value"))
            }
            AdapterToken::Comma | AdapterToken::Colon => Err(self.unexpected(&token, "value")),
            AdapterToken::Eof => Err(JsonError::new(
                JsonErrorKind::UnexpectedEof { expected: "value" },
                token.span,
            )),
        }
    }

    fn produce_event(&mut self) -> Result<Option<ParseEvent<'static>>, JsonError> {
        loop {
            match self.determine_action() {
                NextAction::ObjectKey => {
                    let token = self.next_token()?;
                    match token.token {
                        AdapterToken::ObjectEnd => {
                            self.stack.pop();
                            self.finish_value_in_parent();
                            return Ok(Some(ParseEvent::StructEnd));
                        }
                        AdapterToken::String(name) => {
                            self.expect_colon()?;
                            if let Some(ContextState::Object(state)) = self.stack.last_mut() {
                                *state = ObjectState::Value;
                            }
                            return Ok(Some(ParseEvent::FieldKey(FieldKey::new(
                                name,
                                FieldLocationHint::KeyValue,
                            ))));
                        }
                        AdapterToken::Eof => {
                            return Err(JsonError::new(
                                JsonErrorKind::UnexpectedEof {
                                    expected: "field name or '}'",
                                },
                                token.span,
                            ));
                        }
                        _ => return Err(self.unexpected(&token, "field name or '}'")),
                    }
                }
                NextAction::ObjectValue => {
                    return self.parse_value_start_with_token(None).map(Some);
                }
                NextAction::ObjectComma => {
                    let token = self.next_token()?;
                    match token.token {
                        AdapterToken::Comma => {
                            if let Some(ContextState::Object(state)) = self.stack.last_mut() {
                                *state = ObjectState::KeyOrEnd;
                            }
                            continue;
                        }
                        AdapterToken::ObjectEnd => {
                            self.stack.pop();
                            self.finish_value_in_parent();
                            return Ok(Some(ParseEvent::StructEnd));
                        }
                        AdapterToken::Eof => {
                            return Err(JsonError::new(
                                JsonErrorKind::UnexpectedEof {
                                    expected: "',' or '}'",
                                },
                                token.span,
                            ));
                        }
                        _ => return Err(self.unexpected(&token, "',' or '}'")),
                    }
                }
                NextAction::ArrayValue => {
                    let token = self.next_token()?;
                    match token.token {
                        AdapterToken::ArrayEnd => {
                            self.stack.pop();
                            self.finish_value_in_parent();
                            return Ok(Some(ParseEvent::SequenceEnd));
                        }
                        AdapterToken::Eof => {
                            return Err(JsonError::new(
                                JsonErrorKind::UnexpectedEof {
                                    expected: "value or ']'",
                                },
                                token.span,
                            ));
                        }
                        AdapterToken::Comma | AdapterToken::Colon => {
                            return Err(self.unexpected(&token, "value or ']'"));
                        }
                        _ => {
                            return self.parse_value_start_with_token(Some(token)).map(Some);
                        }
                    }
                }
                NextAction::ArrayComma => {
                    let token = self.next_token()?;
                    match token.token {
                        AdapterToken::Comma => {
                            if let Some(ContextState::Array(state)) = self.stack.last_mut() {
                                *state = ArrayState::ValueOrEnd;
                            }
                            continue;
                        }
                        AdapterToken::ArrayEnd => {
                            self.stack.pop();
                            self.finish_value_in_parent();
                            return Ok(Some(ParseEvent::SequenceEnd));
                        }
                        AdapterToken::Eof => {
                            return Err(JsonError::new(
                                JsonErrorKind::UnexpectedEof {
                                    expected: "',' or ']'",
                                },
                                token.span,
                            ));
                        }
                        _ => return Err(self.unexpected(&token, "',' or ']'")),
                    }
                }
                NextAction::RootValue => {
                    return self.parse_value_start_with_token(None).map(Some);
                }
                NextAction::RootFinished => {
                    // No more events - EOF
                    return Ok(None);
                }
            }
        }
    }

    fn determine_action(&self) -> NextAction {
        if let Some(context) = self.stack.last() {
            match context {
                ContextState::Object(state) => match state {
                    ObjectState::KeyOrEnd => NextAction::ObjectKey,
                    ObjectState::Value => NextAction::ObjectValue,
                    ObjectState::CommaOrEnd => NextAction::ObjectComma,
                },
                ContextState::Array(state) => match state {
                    ArrayState::ValueOrEnd => NextAction::ArrayValue,
                    ArrayState::CommaOrEnd => NextAction::ArrayComma,
                },
            }
        } else if self.root_complete {
            NextAction::RootFinished
        } else {
            NextAction::RootValue
        }
    }

    fn skip_value_internal(&mut self) -> Result<(), JsonError> {
        self.adapter.skip().map(|_| ())
    }

    /// Skip a value while replaying from buffer.
    fn skip_value_buffered(&mut self) -> Result<(), JsonError> {
        if self.buffer_idx >= self.event_buffer.len() {
            return Ok(());
        }

        let event = &self.event_buffer[self.buffer_idx];
        self.buffer_idx += 1;

        match event {
            ParseEvent::StructStart(_) | ParseEvent::SequenceStart(_) => {
                // Skip the entire container
                let mut depth = 1;
                while depth > 0 && self.buffer_idx < self.event_buffer.len() {
                    match &self.event_buffer[self.buffer_idx] {
                        ParseEvent::StructStart(_) | ParseEvent::SequenceStart(_) => depth += 1,
                        ParseEvent::StructEnd | ParseEvent::SequenceEnd => depth -= 1,
                        _ => {}
                    }
                    self.buffer_idx += 1;
                }
            }
            _ => {
                // Scalar value - already skipped by incrementing buffer_idx
            }
        }

        Ok(())
    }

    /// Build probe evidence by scanning ahead through the current object.
    /// Buffers all events for later replay.
    fn build_probe_buffered(&mut self) -> Result<Vec<FieldEvidence<'static>>, JsonError> {
        // Save parser state
        let saved_stack = self.stack.clone();
        let saved_root_started = self.root_started;
        let saved_root_complete = self.root_complete;
        let saved_event_peek = self.event_peek.clone();

        // Clear any previous buffer
        self.event_buffer.clear();
        self.buffer_idx = 0;

        // Check if we've already peeked a StructStart
        let already_inside_object = matches!(saved_event_peek, Some(ParseEvent::StructStart(_)));

        if already_inside_object {
            // Take the peeked StructStart and add it to buffer
            // Don't restore it later since it's now in the buffer
            if let Some(event) = self.event_peek.take() {
                self.event_buffer.push(event);
            }
        } else {
            // Expect an object start
            let event = self.produce_event()?;
            if let Some(e) = event.clone() {
                self.event_buffer.push(e);
            }
            if !matches!(event, Some(ParseEvent::StructStart(_))) {
                // Not an object, return empty evidence - but restore state first
                self.stack = saved_stack;
                self.root_started = saved_root_started;
                self.root_complete = saved_root_complete;
                self.event_peek = saved_event_peek;
                self.event_buffer.clear();
                return Ok(Vec::new());
            }
        }

        let mut evidence = Vec::new();
        let mut depth: usize = 1; // Track nesting depth

        loop {
            let event = self.produce_event()?;
            if let Some(ref e) = event {
                self.event_buffer.push(e.clone());
            }

            match event {
                Some(ParseEvent::StructEnd) => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        break;
                    }
                }
                Some(ParseEvent::StructStart(_)) => {
                    depth += 1;
                }
                Some(ParseEvent::SequenceStart(_)) => {
                    depth += 1;
                }
                Some(ParseEvent::SequenceEnd) => {
                    depth = depth.saturating_sub(1);
                }
                Some(ParseEvent::FieldKey(ref key)) if depth == 1 => {
                    // Top-level field in the object we're probing
                    let field_name = key.name.clone();

                    // Get the value
                    let value_event = self.produce_event()?;
                    if let Some(ref e) = value_event {
                        self.event_buffer.push(e.clone());
                    }

                    // Extract scalar value if possible
                    let scalar_value = match value_event {
                        Some(ParseEvent::Scalar(ref sv)) => Some(sv.clone()),
                        Some(ParseEvent::StructStart(_)) => {
                            // Skip the nested structure
                            self.skip_nested_container(&mut depth)?;
                            None
                        }
                        Some(ParseEvent::SequenceStart(_)) => {
                            // Skip the nested sequence
                            self.skip_nested_container(&mut depth)?;
                            None
                        }
                        _ => None,
                    };

                    if let Some(sv) = scalar_value {
                        evidence.push(FieldEvidence::with_scalar_value(
                            field_name,
                            FieldLocationHint::KeyValue,
                            None,
                            sv,
                            None,
                        ));
                    } else {
                        evidence.push(FieldEvidence::new(
                            field_name,
                            FieldLocationHint::KeyValue,
                            None,
                            None,
                        ));
                    }
                }
                Some(ParseEvent::FieldKey(_)) => {
                    // Nested field, skip the value
                    let value_event = self.produce_event()?;
                    if let Some(ref e) = value_event {
                        self.event_buffer.push(e.clone());
                    }

                    match value_event {
                        Some(ParseEvent::StructStart(_)) | Some(ParseEvent::SequenceStart(_)) => {
                            self.skip_nested_container(&mut depth)?;
                        }
                        _ => {}
                    }
                }
                None => {
                    return Err(JsonError::without_span(JsonErrorKind::UnexpectedEof {
                        expected: "object end",
                    }));
                }
                _ => {}
            }
        }

        // Restore parser state
        self.stack = saved_stack;
        self.root_started = saved_root_started;
        self.root_complete = saved_root_complete;
        // Only restore event_peek if we didn't move it to the buffer
        if !already_inside_object {
            self.event_peek = saved_event_peek;
        }

        Ok(evidence)
    }

    /// Skip a nested container while buffering events and tracking depth.
    fn skip_nested_container(&mut self, depth: &mut usize) -> Result<(), JsonError> {
        let mut local_depth = 1; // We're entering a container
        loop {
            let event = self.produce_event()?;
            if let Some(ref e) = event {
                self.event_buffer.push(e.clone());
            }

            match event {
                Some(ParseEvent::StructStart(_)) | Some(ParseEvent::SequenceStart(_)) => {
                    local_depth += 1;
                    *depth += 1;
                }
                Some(ParseEvent::StructEnd) | Some(ParseEvent::SequenceEnd) => {
                    if local_depth > 0 {
                        local_depth -= 1;
                    }
                    if local_depth == 0 {
                        return Ok(());
                    }
                }
                None => {
                    return Err(JsonError::without_span(JsonErrorKind::UnexpectedEof {
                        expected: "container end",
                    }));
                }
                _ => {}
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NextAction {
    ObjectKey,
    ObjectValue,
    ObjectComma,
    ArrayValue,
    ArrayComma,
    RootValue,
    RootFinished,
}

impl<A: TokenSource<'static>> FormatParser<'static> for StreamingJsonParser<A> {
    type Error = JsonError;
    type Probe<'a>
        = StreamingProbe
    where
        Self: 'a;

    fn next_event(&mut self) -> Result<Option<ParseEvent<'static>>, Self::Error> {
        if let Some(event) = self.event_peek.take() {
            return Ok(Some(event));
        }

        // Replay from buffer if available
        if self.buffer_idx < self.event_buffer.len() {
            let event = self.event_buffer[self.buffer_idx].clone();
            self.buffer_idx += 1;
            return Ok(Some(event));
        }

        self.produce_event()
    }

    fn peek_event(&mut self) -> Result<Option<ParseEvent<'static>>, Self::Error> {
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

        // If we're replaying from buffer, skip through buffered events instead
        if self.buffer_idx < self.event_buffer.len() {
            self.skip_value_buffered()?;
        } else {
            self.skip_value_internal()?;
        }
        self.finish_value_in_parent();
        Ok(())
    }

    fn begin_probe(&mut self) -> Result<Self::Probe<'_>, Self::Error> {
        let evidence = self.build_probe_buffered()?;
        Ok(StreamingProbe { evidence, idx: 0 })
    }
}

/// Probe for streaming parser with buffered evidence.
pub struct StreamingProbe {
    evidence: Vec<FieldEvidence<'static>>,
    idx: usize,
}

impl ProbeStream<'static> for StreamingProbe {
    type Error = JsonError;

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

#[cfg(test)]
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

        let json = br#"{"name": "Alice", "age": 30}"#;
        let reader = Cursor::new(&json[..]);
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
            list: Vec<i32>,
        }

        let json = br#"{"inner": {"value": 42}, "list": [1, 2, 3]}"#;
        let reader = Cursor::new(&json[..]);
        let result: Outer = from_reader(reader).unwrap();

        assert_eq!(result.inner.value, 42);
        assert_eq!(result.list, vec![1, 2, 3]);
    }

    #[test]
    fn test_from_reader_internally_tagged() {
        #[derive(Facet, Debug, PartialEq)]
        #[facet(tag = "type")]
        #[repr(C)]
        enum Shape {
            Circle { radius: f64 },
            Rectangle { width: f64, height: f64 },
        }

        let json = br#"{"type": "Circle", "radius": 5.0}"#;
        let reader = Cursor::new(&json[..]);
        let result: Shape = from_reader(reader).unwrap();

        match result {
            Shape::Circle { radius } => assert_eq!(radius, 5.0),
            _ => panic!("Expected Circle variant"),
        }
    }

    #[test]
    fn test_from_reader_span_offset() {
        #[derive(Facet, Debug, PartialEq)]
        struct Simple {
            a: i32,
        }

        let json = br#"{"a": 1, "b"  }"#;
        // 012345678901234
        // {"a": 1, "b"  }
        //               ^ offset 14 (the '}')

        let reader = Cursor::new(&json[..]);
        let res: Result<Simple, DeserializeError<JsonError>> = from_reader(reader);

        let err = res.unwrap_err();
        if let DeserializeError::Parser(e) = err {
            assert_eq!(e.span.unwrap().offset, 14);
        } else {
            panic!("Expected parser error, got {:?}", err);
        }
    }
}
