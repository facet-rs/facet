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
/// use facet_format_json::from_reader;
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
}

#[derive(Debug)]
enum ContextState {
    Object(ObjectState),
    Array(ArrayState),
}

#[derive(Debug)]
enum ObjectState {
    KeyOrEnd,
    Value,
    CommaOrEnd,
}

#[derive(Debug)]
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
        self.skip_value_internal()?;
        self.finish_value_in_parent();
        Ok(())
    }

    fn begin_probe(&mut self) -> Result<Self::Probe<'_>, Self::Error> {
        // Streaming doesn't support probing - return empty evidence
        Ok(StreamingProbe)
    }
}

/// Empty probe for streaming (probing not supported in streaming mode).
pub struct StreamingProbe;

impl ProbeStream<'static> for StreamingProbe {
    type Error = JsonError;

    fn next(&mut self) -> Result<Option<FieldEvidence<'static>>, Self::Error> {
        Ok(None)
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
}
