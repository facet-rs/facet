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
    ContainerKind, DeserializeError, DeserializeErrorKind, FieldKey, FieldLocationHint,
    FormatDeserializer, FormatParser, ParseError, ParseEvent, SavePoint, ScalarValue,
};
use facet_reflect::Span;

use crate::adapter::{SpannedAdapterToken, Token as AdapterToken, TokenSource};
use crate::error::JsonError;
use crate::scan_buffer::ScanBuffer;
use crate::streaming_adapter::StreamingAdapter;

/// Convert an I/O error to a ParseError.
/// Uses span (0, 0) since I/O errors occur during buffer operations, not parsing.
fn io_error_to_parse_error(e: std::io::Error) -> ParseError {
    ParseError::new(
        Span::new(0, 0),
        DeserializeErrorKind::Io {
            message: e.to_string().into(),
        },
    )
}

/// Convert a JsonError to a ParseError.
fn json_error_to_parse_error(e: JsonError) -> ParseError {
    use crate::error::JsonErrorKind;

    let span = e.span.unwrap_or(Span::new(0, 0));
    let kind = match e.kind {
        JsonErrorKind::UnexpectedEof { expected } => {
            DeserializeErrorKind::UnexpectedEof { expected }
        }
        JsonErrorKind::UnexpectedToken { got, expected } => DeserializeErrorKind::UnexpectedToken {
            got: got.into(),
            expected,
        },
        JsonErrorKind::Scan(scan_err) => match scan_err {
            crate::scanner::ScanErrorKind::UnexpectedChar(ch) => {
                DeserializeErrorKind::UnexpectedChar {
                    ch,
                    expected: "valid JSON token",
                }
            }
            crate::scanner::ScanErrorKind::UnexpectedEof(expected) => {
                DeserializeErrorKind::UnexpectedEof { expected }
            }
            crate::scanner::ScanErrorKind::InvalidUtf8 => DeserializeErrorKind::InvalidUtf8 {
                context: [0u8; 16],
                context_len: 0,
            },
        },
        JsonErrorKind::ScanWithContext { error, .. } => match error {
            crate::scanner::ScanErrorKind::UnexpectedChar(ch) => {
                DeserializeErrorKind::UnexpectedChar {
                    ch,
                    expected: "valid JSON token",
                }
            }
            crate::scanner::ScanErrorKind::UnexpectedEof(expected) => {
                DeserializeErrorKind::UnexpectedEof { expected }
            }
            crate::scanner::ScanErrorKind::InvalidUtf8 => DeserializeErrorKind::InvalidUtf8 {
                context: [0u8; 16],
                context_len: 0,
            },
        },
        JsonErrorKind::TypeMismatch { expected, got } => DeserializeErrorKind::UnexpectedToken {
            got: got.into(),
            expected,
        },
        JsonErrorKind::InvalidValue { message } => DeserializeErrorKind::InvalidValue {
            message: message.into(),
        },
        JsonErrorKind::InvalidUtf8 => DeserializeErrorKind::InvalidUtf8 {
            context: [0u8; 16],
            context_len: 0,
        },
        JsonErrorKind::Io(msg) => DeserializeErrorKind::Io {
            message: msg.into(),
        },
        // These shouldn't occur in parser context, but handle them anyway
        JsonErrorKind::UnknownField { field, .. } => DeserializeErrorKind::UnknownField {
            field: field.into(),
            suggestion: None,
        },
        JsonErrorKind::MissingField { field, .. } => DeserializeErrorKind::Bug {
            error: alloc::format!("missing field '{}' in streaming parser", field).into(),
            context: "streaming JSON parser",
        },
        JsonErrorKind::Reflect(e) => DeserializeErrorKind::Reflect {
            kind: e.kind,
            context: "streaming JSON parser",
        },
        JsonErrorKind::NumberOutOfRange { value, target_type } => {
            DeserializeErrorKind::NumberOutOfRange {
                value: value.into(),
                target_type,
            }
        }
        JsonErrorKind::DuplicateKey { key } => DeserializeErrorKind::DuplicateField {
            field: key.into(),
            first_span: None,
        },
        JsonErrorKind::Solver(msg) => DeserializeErrorKind::Solver {
            message: msg.into(),
        },
    };
    ParseError::new(span, kind)
}

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
pub fn from_reader<R, T>(mut reader: R) -> Result<T, DeserializeError>
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
        let n = buf.refill(&mut reader).map_err(io_error_to_parse_error)?;
        if n == 0 {
            return Err(ParseError::new(
                Span::new(0, 0),
                DeserializeErrorKind::UnexpectedEof {
                    expected: "JSON value",
                },
            )
            .into());
        }
    }

    // Create coroutine that runs the deserializer
    let mut coroutine: Coroutine<(), (), Result<T, DeserializeError>> =
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

                let _n = buf.refill(&mut reader).map_err(io_error_to_parse_error)?;
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
pub async fn from_async_reader_tokio<R, T>(mut reader: R) -> Result<T, DeserializeError>
where
    R: tokio::io::AsyncRead + Unpin,
    T: Facet<'static>,
{
    let buffer = Rc::new(RefCell::new(ScanBuffer::new()));
    let buffer_for_coroutine = buffer.clone();

    // Initial fill
    {
        let mut buf = buffer.borrow_mut();
        let n = buf
            .refill_tokio(&mut reader)
            .await
            .map_err(io_error_to_parse_error)?;
        if n == 0 {
            return Err(ParseError::new(
                Span::new(0, 0),
                DeserializeErrorKind::UnexpectedEof {
                    expected: "JSON value",
                },
            )
            .into());
        }
    }

    let mut coroutine: Coroutine<(), (), Result<T, DeserializeError>> =
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
                let _n = buf
                    .refill_tokio(&mut reader)
                    .await
                    .map_err(io_error_to_parse_error)?;
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
pub async fn from_async_reader_futures<R, T>(mut reader: R) -> Result<T, DeserializeError>
where
    R: futures_io::AsyncRead + Unpin,
    T: Facet<'static>,
{
    let buffer = Rc::new(RefCell::new(ScanBuffer::new()));
    let buffer_for_coroutine = buffer.clone();

    // Initial fill
    {
        let mut buf = buffer.borrow_mut();
        let n = buf
            .refill_futures(&mut reader)
            .await
            .map_err(io_error_to_parse_error)?;
        if n == 0 {
            return Err(ParseError::new(
                Span::new(0, 0),
                DeserializeErrorKind::UnexpectedEof {
                    expected: "JSON value",
                },
            )
            .into());
        }
    }

    let mut coroutine: Coroutine<(), (), Result<T, DeserializeError>> =
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
                let _n = buf
                    .refill_futures(&mut reader)
                    .await
                    .map_err(io_error_to_parse_error)?;
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
    /// Counter for save points
    save_counter: u64,
    /// Events recorded since save() was called
    recording: Option<Vec<ParseEvent<'static>>>,
    /// Events to replay before producing new ones
    replay_buffer: Vec<ParseEvent<'static>>,
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
            save_counter: 0,
            recording: None,
            replay_buffer: Vec::new(),
        }
    }

    fn next_token(&mut self) -> Result<SpannedAdapterToken<'static>, ParseError> {
        self.adapter.next_token().map_err(json_error_to_parse_error)
    }

    fn unexpected(
        &self,
        token: &SpannedAdapterToken<'static>,
        expected: &'static str,
    ) -> ParseError {
        ParseError::new(
            token.span,
            DeserializeErrorKind::UnexpectedToken {
                got: alloc::format!("{:?}", token.token).into(),
                expected,
            },
        )
    }

    fn expect_colon(&mut self) -> Result<(), ParseError> {
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
    ) -> Result<ParseEvent<'static>, ParseError> {
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
            AdapterToken::Eof => Err(ParseError::new(
                token.span,
                DeserializeErrorKind::UnexpectedEof { expected: "value" },
            )),
        }
    }

    fn produce_event(&mut self) -> Result<Option<ParseEvent<'static>>, ParseError> {
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
                            return Err(ParseError::new(
                                token.span,
                                DeserializeErrorKind::UnexpectedEof {
                                    expected: "field name or '}'",
                                },
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
                            return Err(ParseError::new(
                                token.span,
                                DeserializeErrorKind::UnexpectedEof {
                                    expected: "',' or '}'",
                                },
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
                            return Err(ParseError::new(
                                token.span,
                                DeserializeErrorKind::UnexpectedEof {
                                    expected: "value or ']'",
                                },
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
                            return Err(ParseError::new(
                                token.span,
                                DeserializeErrorKind::UnexpectedEof {
                                    expected: "',' or ']'",
                                },
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

    fn skip_value_internal(&mut self) -> Result<(), ParseError> {
        self.adapter
            .skip()
            .map(|_| ())
            .map_err(json_error_to_parse_error)
    }

    /// Skip a value while replaying from buffer.
    fn skip_value_buffered(&mut self) -> Result<(), ParseError> {
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
    fn next_event(&mut self) -> Result<Option<ParseEvent<'static>>, ParseError> {
        // First check replay buffer (from restore)
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

        // Replay from event_buffer if available (legacy probe buffering)
        if self.buffer_idx < self.event_buffer.len() {
            let event = self.event_buffer[self.buffer_idx].clone();
            self.buffer_idx += 1;
            // Record if we're in save mode
            if let Some(ref mut rec) = self.recording {
                rec.push(event.clone());
            }
            return Ok(Some(event));
        }

        let event = self.produce_event()?;
        // Record if we're in save mode
        if let Some(ref mut rec) = self.recording
            && let Some(ref e) = event
        {
            rec.push(e.clone());
        }
        Ok(event)
    }

    fn peek_event(&mut self) -> Result<Option<ParseEvent<'static>>, ParseError> {
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

    fn skip_value(&mut self) -> Result<(), ParseError> {
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

    fn format_namespace(&self) -> Option<&'static str> {
        Some("json")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use facet::Facet;
    use facet_testhelpers::test;
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
        let res: Result<Simple, DeserializeError> = from_reader(reader);

        let err = res.unwrap_err();
        // if let DeserializeError::Parser(e) = err {
        //     assert_eq!(e.span.unwrap().offset, 14);
        // } else {
        panic!("Expected parser error, got {:?}", err);
        // }
    }
}
