//! Streaming JSON deserialization using stackful coroutines.
//!
//! This module provides `from_reader` and `from_async_reader` functions that
//! can deserialize JSON from any `Read` or `AsyncRead` source without requiring
//! the entire input to be in memory.
//!
//! # Architecture
//!
//! The streaming deserializer uses [corosensei] stackful coroutines to suspend
//! the recursive descent parser when more data is needed:
//!
//! ```text
//! Driver (sync or async)              Coroutine
//! ─────────────────────               ─────────
//!      │                                  │
//!      │ resume(())                       │
//!      ├─────────────────────────────────►│
//!      │                                  │ deserialize...
//!      │                                  │ need more data
//!      │◄─────────────────────────────────┤ yield(())
//!      │ refill buffer                    │
//!      │ resume(())                       │
//!      ├─────────────────────────────────►│
//!      │                                  │ continue...
//!      │◄─────────────────────────────────┤ return(result)
//!      │ done!                            │
//! ```
//!
//! [corosensei]: https://docs.rs/corosensei

use alloc::borrow::Cow;
use alloc::rc::Rc;
use core::cell::RefCell;

use corosensei::{Coroutine, CoroutineResult};
use facet::Facet;
use facet_reflect::{ReflectError, Span};

use crate::adapter::{SpannedAdapterToken, Token, TokenSource};
use crate::deserialize::JsonDeserializer;
use crate::scan_buffer::ScanBuffer;
use crate::scanner::{ParsedNumber, ScanErrorKind, Scanner, Token as ScanToken};
use crate::{JsonError, JsonErrorKind};
use facet_reflect::Partial;

#[inline(never)]
#[cold]
fn reflect_err(e: ReflectError) -> JsonError {
    JsonError {
        kind: JsonErrorKind::Reflect(e),
        span: None,
        source_code: None,
    }
}

#[inline(never)]
#[cold]
fn io_err<E: ToString>(e: E) -> JsonError {
    JsonError {
        kind: JsonErrorKind::Io(e.to_string()),
        span: None,
        source_code: None,
    }
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
/// use facet_json_legacy::from_reader;
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
pub fn from_reader<R, T>(mut reader: R) -> Result<T, JsonError>
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
        let n = buf.refill(&mut reader).map_err(io_err::<std::io::Error>)?;
        if n == 0 {
            return Err(JsonError {
                kind: JsonErrorKind::Scan(ScanErrorKind::UnexpectedEof("empty input")),
                span: None,
                source_code: None,
            });
        }
    }

    // Create coroutine that runs the deserializer
    let mut coroutine: Coroutine<(), (), Result<T, JsonError>> =
        Coroutine::new(move |yielder, ()| {
            // Create streaming adapter with shared buffer
            let adapter = StreamingAdapter::new(buffer_for_coroutine, yielder);
            from_streaming_adapter::<_, T>(adapter)
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

                let _n = buf.refill(&mut reader).map_err(io_err::<std::io::Error>)?;

                // EOF is handled by the adapter via is_eof()
            }
            CoroutineResult::Return(result) => {
                return result;
            }
        }
    }
}

/// Deserialize JSON from an async reader (tokio).
#[cfg(feature = "tokio")]
#[allow(clippy::await_holding_refcell_ref)] // Safe: single-threaded coroutine, buffer not accessed elsewhere during await
pub async fn from_async_reader_tokio<R, T>(mut reader: R) -> Result<T, JsonError>
where
    R: tokio::io::AsyncRead + Unpin,
    T: Facet<'static>,
{
    // Shared buffer between coroutine and driver
    let buffer = Rc::new(RefCell::new(ScanBuffer::new()));
    let buffer_for_coroutine = buffer.clone();

    // Initial fill
    {
        let mut buf = buffer.borrow_mut();
        let n = buf
            .refill_tokio(&mut reader)
            .await
            .map_err(io_err::<tokio::io::Error>)?;
        if n == 0 {
            return Err(JsonError {
                kind: JsonErrorKind::Scan(ScanErrorKind::UnexpectedEof("empty input")),
                span: None,
                source_code: None,
            });
        }
    }

    // Create coroutine that runs the deserializer
    let mut coroutine: Coroutine<(), (), Result<T, JsonError>> =
        Coroutine::new(move |yielder, ()| {
            let adapter = StreamingAdapter::new(buffer_for_coroutine, yielder);
            from_streaming_adapter::<_, T>(adapter)
        });

    // Drive the coroutine
    loop {
        match coroutine.resume(()) {
            CoroutineResult::Yield(()) => {
                // Coroutine needs more data - refill buffer
                let mut buf = buffer.borrow_mut();

                if buf.filled() == buf.capacity() {
                    buf.grow();
                }

                let _n = buf
                    .refill_tokio(&mut reader)
                    .await
                    .map_err(io_err::<tokio::io::Error>)?;
            }
            CoroutineResult::Return(result) => {
                return result;
            }
        }
    }
}

/// Deserialize JSON from an async reader (futures-io).
#[cfg(feature = "futures-io")]
#[allow(clippy::await_holding_refcell_ref)] // Safe: single-threaded coroutine, buffer not accessed elsewhere during await
pub async fn from_async_reader_futures<R, T>(mut reader: R) -> Result<T, JsonError>
where
    R: futures_io::AsyncRead + Unpin,
    T: Facet<'static>,
{
    // Shared buffer between coroutine and driver
    let buffer = Rc::new(RefCell::new(ScanBuffer::new()));
    let buffer_for_coroutine = buffer.clone();

    // Initial fill
    {
        let mut buf = buffer.borrow_mut();
        let n = buf
            .refill_futures(&mut reader)
            .await
            .map_err(io_err::<futures_io::Error>)?;
        if n == 0 {
            return Err(JsonError {
                kind: JsonErrorKind::Scan(ScanErrorKind::UnexpectedEof("empty input")),
                span: None,
                source_code: None,
            });
        }
    }

    // Create coroutine that runs the deserializer
    let mut coroutine: Coroutine<(), (), Result<T, JsonError>> =
        Coroutine::new(move |yielder, ()| {
            let adapter = StreamingAdapter::new(buffer_for_coroutine, yielder);
            from_streaming_adapter::<_, T>(adapter)
        });

    // Drive the coroutine
    loop {
        match coroutine.resume(()) {
            CoroutineResult::Yield(()) => {
                // Coroutine needs more data - refill buffer
                let mut buf = buffer.borrow_mut();

                if buf.filled() == buf.capacity() {
                    buf.grow();
                }

                let _n = buf
                    .refill_futures(&mut reader)
                    .await
                    .map_err(io_err::<futures_io::Error>)?;
            }
            CoroutineResult::Return(result) => {
                return result;
            }
        }
    }
}

// ============================================================================
// Core streaming deserializer - uses generic JsonDeserializer
// ============================================================================

/// Deserialize JSON from any TokenSource<'static>.
///
/// This is the core deserialization function for streaming. It uses the generic
/// `JsonDeserializer` with `BORROW=false` since streaming always produces owned data.
pub fn from_streaming_adapter<A, T>(adapter: A) -> Result<T, JsonError>
where
    A: TokenSource<'static>,
    T: Facet<'static>,
{
    // Allocate a Partial<'static, false> - owned mode, no borrowing.
    let wip: Partial<'static, false> = Partial::alloc_owned::<T>().map_err(reflect_err)?;

    // Use the generic JsonDeserializer with BORROW=false
    let mut deserializer: JsonDeserializer<'static, false, A> =
        JsonDeserializer::from_adapter(adapter);
    let partial = deserializer.deserialize_into(wip)?;

    let heap_value = partial.build().map_err(reflect_err)?;

    heap_value.materialize::<T>().map_err(reflect_err)
}

/// Spanned token for streaming deserialization.
///
/// Uses `Token<'static>` since streaming always produces owned strings
/// (the underlying buffer may be refilled/modified between tokens).
#[derive(Debug, Clone)]
pub struct SpannedStreamingToken {
    /// The token (always owned strings)
    pub token: Token<'static>,
    /// Source span (absolute position in input stream)
    pub span: Span,
}

impl SpannedStreamingToken {
    /// Convert to a SpannedAdapterToken (same type, just for clarity)
    pub fn to_adapter_token(&self) -> SpannedAdapterToken<'static> {
        SpannedAdapterToken {
            token: self.token.clone(),
            span: self.span,
        }
    }
}

/// Streaming adapter that wraps a shared buffer and can yield to the driver.
///
/// This adapter is used inside the coroutine. When it needs more data, it
/// suspends the coroutine via the yielder, and the driver refills the buffer.
pub struct StreamingAdapter<'y> {
    /// Shared buffer (via `Rc<RefCell>`)
    buffer: Rc<RefCell<ScanBuffer>>,
    /// Yielder to suspend coroutine when more data needed
    yielder: &'y corosensei::Yielder<(), ()>,
    /// Scanner state
    scanner: Scanner,
    /// Total bytes processed (for absolute positions)
    bytes_processed: usize,
    /// Peeked token for lookahead
    peeked: Option<SpannedStreamingToken>,
}

impl<'y> StreamingAdapter<'y> {
    /// Create a new streaming adapter.
    pub fn new(buffer: Rc<RefCell<ScanBuffer>>, yielder: &'y corosensei::Yielder<(), ()>) -> Self {
        Self {
            buffer,
            yielder,
            scanner: Scanner::new(),
            bytes_processed: 0,
            peeked: None,
        }
    }

    /// Peek at the next token without consuming it.
    pub fn peek(&mut self) -> Result<&SpannedStreamingToken, JsonError> {
        if self.peeked.is_none() {
            self.peeked = Some(self.next_token_internal()?);
        }
        Ok(self.peeked.as_ref().unwrap())
    }

    /// Consume and return the next token.
    pub fn next_token(&mut self) -> Result<SpannedStreamingToken, JsonError> {
        if let Some(token) = self.peeked.take() {
            Ok(token)
        } else {
            self.next_token_internal()
        }
    }

    /// Internal implementation of next_token, yielding to the driver if more data is needed.
    fn next_token_internal(&mut self) -> Result<SpannedStreamingToken, JsonError> {
        loop {
            let buf = self.buffer.borrow();
            let data = buf.data();
            let is_eof = buf.is_eof();

            match self.scanner.next_token(data) {
                Ok(spanned) => match spanned.token {
                    ScanToken::NeedMore { .. } => {
                        drop(buf); // Release borrow before yielding

                        if is_eof {
                            // True EOF - try to finalize
                            let buf = self.buffer.borrow();
                            let finalized = self.scanner.finalize_at_eof(buf.data());
                            drop(buf);

                            match finalized {
                                Ok(f) => {
                                    let abs_span =
                                        Span::new(self.bytes_processed + f.span.offset, f.span.len);
                                    let token = self.materialize_token(&f.token, abs_span)?;
                                    self.advance_past(f.span.offset + f.span.len);
                                    return Ok(token);
                                }
                                Err(e) => {
                                    return Err(JsonError {
                                        kind: JsonErrorKind::Scan(e.kind),
                                        span: Some(Span::new(
                                            self.bytes_processed + e.span.offset,
                                            e.span.len,
                                        )),
                                        source_code: None,
                                    });
                                }
                            }
                        }

                        // Yield to driver to refill buffer
                        self.yielder.suspend(());
                        continue;
                    }
                    ScanToken::Eof => {
                        let abs_span = Span::new(self.bytes_processed + spanned.span.offset, 0);
                        drop(buf);

                        if is_eof {
                            return Ok(SpannedStreamingToken {
                                token: Token::Eof,
                                span: abs_span,
                            });
                        }

                        // End of current buffer data, but not true EOF
                        // Reset scanner and yield for more data
                        self.advance_past(self.scanner.pos());
                        self.scanner.set_pos(0);
                        self.yielder.suspend(());
                        continue;
                    }
                    _ => {
                        // Complete token - compute absolute span
                        let abs_span =
                            Span::new(self.bytes_processed + spanned.span.offset, spanned.span.len);
                        let token = self.materialize_token(&spanned.token, abs_span)?;
                        let consumed = self.scanner.pos();
                        drop(buf);

                        self.advance_past(consumed);
                        return Ok(token);
                    }
                },
                Err(e) => {
                    return Err(JsonError {
                        kind: JsonErrorKind::Scan(e.kind),
                        span: Some(Span::new(self.bytes_processed + e.span.offset, e.span.len)),
                        source_code: None,
                    });
                }
            }
        }
    }

    /// Skip a JSON value without fully decoding it.
    pub fn skip(&mut self) -> Result<Span, JsonError> {
        let first = self.next_token()?;
        let start_span = first.span;

        match first.token {
            Token::ObjectStart => {
                let mut depth = 1;
                while depth > 0 {
                    let t = self.next_token()?;
                    match t.token {
                        Token::ObjectStart => depth += 1,
                        Token::ObjectEnd => depth -= 1,
                        Token::Eof => {
                            return Err(JsonError {
                                kind: JsonErrorKind::Scan(ScanErrorKind::UnexpectedEof(
                                    "in object",
                                )),
                                span: None,
                                source_code: None,
                            });
                        }
                        _ => {}
                    }
                }
                Ok(start_span)
            }
            Token::ArrayStart => {
                let mut depth = 1;
                while depth > 0 {
                    let t = self.next_token()?;
                    match t.token {
                        Token::ArrayStart => depth += 1,
                        Token::ArrayEnd => depth -= 1,
                        Token::Eof => {
                            return Err(JsonError {
                                kind: JsonErrorKind::Scan(ScanErrorKind::UnexpectedEof("in array")),
                                span: None,
                                source_code: None,
                            });
                        }
                        _ => {}
                    }
                }
                Ok(start_span)
            }
            Token::Eof => Err(JsonError {
                kind: JsonErrorKind::Scan(ScanErrorKind::UnexpectedEof("expected value")),
                span: None,
                source_code: None,
            }),
            // Scalars - already consumed
            _ => Ok(start_span),
        }
    }

    /// Materialize a scanner token into an owned token with span.
    fn materialize_token(
        &self,
        token: &ScanToken,
        span: Span,
    ) -> Result<SpannedStreamingToken, JsonError> {
        let buf = self.buffer.borrow();
        let data = buf.data();

        let owned_token = match token {
            ScanToken::ObjectStart => Token::ObjectStart,
            ScanToken::ObjectEnd => Token::ObjectEnd,
            ScanToken::ArrayStart => Token::ArrayStart,
            ScanToken::ArrayEnd => Token::ArrayEnd,
            ScanToken::Colon => Token::Colon,
            ScanToken::Comma => Token::Comma,
            ScanToken::Null => Token::Null,
            ScanToken::True => Token::True,
            ScanToken::False => Token::False,
            ScanToken::String {
                start,
                end,
                has_escapes: _,
            } => {
                // Always decode to owned string (buffer may change)
                let s = crate::scanner::decode_string_owned(data, *start, *end).map_err(|e| {
                    JsonError {
                        kind: JsonErrorKind::Scan(e.kind),
                        span: Some(Span::new(self.bytes_processed + e.span.offset, e.span.len)),
                        source_code: None,
                    }
                })?;
                Token::String(Cow::Owned(s))
            }
            ScanToken::Number { start, end, hint } => {
                let parsed =
                    crate::scanner::parse_number(data, *start, *end, *hint).map_err(|e| {
                        JsonError {
                            kind: JsonErrorKind::Scan(e.kind),
                            span: Some(Span::new(self.bytes_processed + e.span.offset, e.span.len)),
                            source_code: None,
                        }
                    })?;
                match parsed {
                    ParsedNumber::U64(n) => Token::U64(n),
                    ParsedNumber::I64(n) => Token::I64(n),
                    ParsedNumber::U128(n) => Token::U128(n),
                    ParsedNumber::I128(n) => Token::I128(n),
                    ParsedNumber::F64(n) => Token::F64(n),
                }
            }
            ScanToken::Eof => Token::Eof,
            ScanToken::NeedMore { .. } => unreachable!("NeedMore handled in next_token loop"),
        };

        Ok(SpannedStreamingToken {
            token: owned_token,
            span,
        })
    }

    /// Advance past consumed bytes, resetting buffer if all data consumed.
    fn advance_past(&mut self, consumed: usize) {
        self.bytes_processed += consumed;

        let mut buf = self.buffer.borrow_mut();
        if consumed >= buf.filled() {
            // All data consumed - reset buffer for fresh data
            buf.reset();
            self.scanner.set_pos(0);
        } else {
            // Partial consumption - this shouldn't happen with our model
            // since we process complete tokens, but handle it anyway
            self.scanner.set_pos(consumed);
        }
    }

    /// Get the current absolute position.
    pub fn position(&self) -> usize {
        self.bytes_processed + self.scanner.pos()
    }
}

impl<'y> TokenSource<'static> for StreamingAdapter<'y> {
    fn next_token(&mut self) -> Result<SpannedAdapterToken<'static>, JsonError> {
        // StreamingAdapter already produces owned tokens (Token<'static>)
        // Call the inherent method, not the trait method
        let token = StreamingAdapter::next_token(self)?;
        Ok(SpannedAdapterToken {
            token: token.token,
            span: token.span,
        })
    }

    fn skip(&mut self) -> Result<Span, JsonError> {
        // Call the inherent skip method
        StreamingAdapter::skip(self)
    }

    // input_bytes() and at_offset() return None (default) for streaming
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_from_reader_simple() {
        #[derive(facet::Facet, Debug, PartialEq)]
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
    fn test_from_reader_large_string() {
        // Test with a string larger than the default buffer
        let long_string = "x".repeat(20000);
        let json = format!(r#"{{"data": "{long_string}"}}"#);

        #[derive(facet::Facet, Debug)]
        struct Data {
            data: String,
        }

        let reader = Cursor::new(json.as_bytes());
        let result: Data = from_reader(reader).unwrap();
        assert_eq!(result.data, long_string);
    }

    #[test]
    fn test_from_reader_nested() {
        #[derive(facet::Facet, Debug, PartialEq)]
        struct Inner {
            value: i32,
        }

        #[derive(facet::Facet, Debug, PartialEq)]
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
