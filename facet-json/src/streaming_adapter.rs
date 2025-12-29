//! Streaming adapter that bridges buffer-based scanning to the token source trait.

extern crate alloc;

use alloc::borrow::Cow;
use alloc::rc::Rc;
use core::cell::RefCell;

use facet_reflect::Span;

use crate::adapter::{SpannedAdapterToken, Token, TokenSource};
use crate::error::{JsonError, JsonErrorKind};
use crate::scan_buffer::ScanBuffer;
use crate::scanner::{ParsedNumber, ScanErrorKind, Scanner, Token as ScanToken};

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
    #[allow(dead_code)]
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
    #[allow(dead_code)]
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
    #[allow(dead_code)]
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
