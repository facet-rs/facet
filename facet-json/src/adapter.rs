#![allow(clippy::result_large_err)]
//! Token adapter that bridges Scanner to the deserializer.
//!
//! The adapter provides two methods:
//! - `next_token()` - returns decoded token content
//! - `skip()` - skips a value without allocation, returns span
//!
//! This design allows the deserializer to avoid allocations when:
//! - Skipping unknown fields
//! - Capturing RawJson (just need span)

use alloc::borrow::Cow;

use facet_reflect::Span;

use crate::scanner::{self, ParsedNumber, ScanError, ScanErrorKind, Scanner, Token as ScanToken};

/// Token with decoded content, ready for deserialization.
#[derive(Debug, Clone, PartialEq)]
pub enum Token<'input> {
    /// `{`
    ObjectStart,
    /// `}`
    ObjectEnd,
    /// `[`
    ArrayStart,
    /// `]`
    ArrayEnd,
    /// `:`
    Colon,
    /// `,`
    Comma,
    /// `null`
    Null,
    /// `true`
    True,
    /// `false`
    False,
    /// String value (decoded)
    String(Cow<'input, str>),
    /// Unsigned 64-bit integer
    U64(u64),
    /// Signed 64-bit integer
    I64(i64),
    /// Unsigned 128-bit integer
    U128(u128),
    /// Signed 128-bit integer
    I128(i128),
    /// 64-bit float
    F64(f64),
    /// End of input
    Eof,
}

/// Spanned token with location information.
#[derive(Debug, Clone)]
pub struct SpannedAdapterToken<'input> {
    /// The token
    pub token: Token<'input>,
    /// Source span
    pub span: Span,
}

/// Adapter error (wraps scanner errors).
#[derive(Debug, Clone)]
pub struct AdapterError {
    /// The error kind
    pub kind: AdapterErrorKind,
    /// Source span
    pub span: Span,
}

/// Types of adapter errors.
#[derive(Debug, Clone)]
pub enum AdapterErrorKind {
    /// Scanner error
    Scan(ScanErrorKind),
    /// Need more data (for streaming)
    NeedMore,
}

impl From<ScanError> for AdapterError {
    fn from(e: ScanError) -> Self {
        AdapterError {
            kind: AdapterErrorKind::Scan(e.kind),
            span: e.span,
        }
    }
}

/// Default chunk size for windowed scanning (small for testing boundary conditions)
pub const DEFAULT_CHUNK_SIZE: usize = 4;

/// Token adapter for slice-based parsing with fixed-size windowing.
///
/// Uses a sliding window approach:
/// - Scanner sees only `chunk_size` bytes at a time
/// - On `NeedMore`, window grows (extends end) to include more bytes
/// - On token complete, window slides (moves start) past consumed bytes
/// - No data is copied - window is just a view into the original slice
///
/// The const generic `BORROW` controls string handling:
/// - `BORROW=true`: strings without escapes are borrowed (`Cow::Borrowed`)
/// - `BORROW=false`: all strings are owned (`Cow::Owned`)
///
/// The const generic `TRUSTED_UTF8` controls UTF-8 validation:
/// - `TRUSTED_UTF8=true`: skip UTF-8 validation (input came from `&str`)
/// - `TRUSTED_UTF8=false`: validate UTF-8 (input came from `&[u8]`)
pub struct SliceAdapter<'input, const BORROW: bool, const TRUSTED_UTF8: bool = false> {
    /// Full original input (for borrowing strings)
    input: &'input [u8],
    /// Start of current window in input
    window_start: usize,
    /// End of current window in input
    window_end: usize,
    /// Chunk size for window growth
    chunk_size: usize,
    /// The scanner
    scanner: Scanner,
}

impl<'input, const BORROW: bool, const TRUSTED_UTF8: bool>
    SliceAdapter<'input, BORROW, TRUSTED_UTF8>
{
    /// Create a new adapter with the default chunk size (4 bytes).
    pub fn new(input: &'input [u8]) -> Self {
        Self::with_chunk_size(input, DEFAULT_CHUNK_SIZE)
    }

    /// Create a new adapter with a custom chunk size.
    pub fn with_chunk_size(input: &'input [u8], chunk_size: usize) -> Self {
        let initial_end = chunk_size.min(input.len());
        Self {
            input,
            window_start: 0,
            window_end: initial_end,
            chunk_size,
            scanner: Scanner::new(),
        }
    }

    /// Create a new adapter starting at a specific offset.
    ///
    /// This is used to reset the parser position (for save/restore functionality
    /// and JIT deserialization). The spans remain absolute positions in the
    /// original input.
    pub fn new_with_offset(input: &'input [u8], offset: usize) -> Self {
        let offset = offset.min(input.len());
        let initial_end = (offset + DEFAULT_CHUNK_SIZE).min(input.len());
        Self {
            input,
            window_start: offset,
            window_end: initial_end,
            chunk_size: DEFAULT_CHUNK_SIZE,
            scanner: Scanner::new(),
        }
    }

    /// Get the current window into the input.
    #[inline]
    fn current_window(&self) -> &'input [u8] {
        &self.input[self.window_start..self.window_end]
    }

    /// Grow the window by one chunk (or to end of input).
    #[inline]
    fn grow_window(&mut self) {
        self.window_end = (self.window_end + self.chunk_size).min(self.input.len());
    }

    /// Slide the window forward past consumed bytes, reset scanner.
    #[inline]
    fn slide_window(&mut self, consumed_in_window: usize) {
        self.window_start += consumed_in_window;
        self.window_end = (self.window_start + self.chunk_size).min(self.input.len());
        self.scanner.set_pos(0);
    }

    /// Check if we've reached the end of input.
    #[inline]
    const fn at_end_of_input(&self) -> bool {
        self.window_end >= self.input.len()
    }

    /// Get the next token with decoded content.
    ///
    /// Strings are decoded (escapes processed) and returned as `Cow<str>`.
    /// Numbers are parsed into appropriate numeric types.
    ///
    /// Uses windowed scanning: on `NeedMore`, grows the window and retries.
    /// Spans are absolute positions in the original input.
    pub fn next_token(&mut self) -> Result<SpannedAdapterToken<'input>, AdapterError> {
        loop {
            let window = self.current_window();
            let spanned = match self.scanner.next_token(window) {
                Ok(s) => s,
                Err(e) => {
                    // Translate error span to absolute position
                    return Err(AdapterError {
                        kind: AdapterErrorKind::Scan(e.kind),
                        span: Span::new(self.window_start + e.span.offset, e.span.len),
                    });
                }
            };

            match spanned.token {
                ScanToken::NeedMore { .. } => {
                    // Need more data - grow window if possible
                    if self.at_end_of_input() {
                        // True EOF - try to finalize any pending token (e.g., number at EOF)
                        let window = self.current_window();
                        let finalized = match self.scanner.finalize_at_eof(window) {
                            Ok(f) => f,
                            Err(e) => {
                                return Err(AdapterError {
                                    kind: AdapterErrorKind::Scan(e.kind),
                                    span: Span::new(self.window_start + e.span.offset, e.span.len),
                                });
                            }
                        };

                        // Handle the finalized token
                        let consumed = self.scanner.pos();
                        let absolute_span = Span::new(
                            self.window_start + finalized.span.offset,
                            finalized.span.len,
                        );

                        let token = self.materialize_token(&finalized)?;
                        self.slide_window(consumed);

                        return Ok(SpannedAdapterToken {
                            token,
                            span: absolute_span,
                        });
                    }
                    self.grow_window();
                    continue;
                }
                ScanToken::Eof => {
                    // Scanner hit end of window
                    if self.at_end_of_input() {
                        // True EOF
                        return Ok(SpannedAdapterToken {
                            token: Token::Eof,
                            span: Span::new(self.window_start + spanned.span.offset, 0),
                        });
                    }
                    // End of window but more input available - slide forward
                    self.slide_window(self.scanner.pos());
                    continue;
                }
                _ => {
                    // Complete token - materialize and return
                    let consumed = self.scanner.pos();
                    let absolute_span =
                        Span::new(self.window_start + spanned.span.offset, spanned.span.len);

                    let token = self.materialize_token(&spanned)?;

                    // Slide window past this token for next call
                    self.slide_window(consumed);

                    return Ok(SpannedAdapterToken {
                        token,
                        span: absolute_span,
                    });
                }
            }
        }
    }

    /// Materialize a scanned token into a decoded token.
    ///
    /// Positions in `spanned` are relative to current window.
    /// We borrow/decode from the original input slice using absolute positions.
    fn materialize_token(
        &self,
        spanned: &scanner::SpannedToken,
    ) -> Result<Token<'input>, AdapterError> {
        match &spanned.token {
            ScanToken::ObjectStart => Ok(Token::ObjectStart),
            ScanToken::ObjectEnd => Ok(Token::ObjectEnd),
            ScanToken::ArrayStart => Ok(Token::ArrayStart),
            ScanToken::ArrayEnd => Ok(Token::ArrayEnd),
            ScanToken::Colon => Ok(Token::Colon),
            ScanToken::Comma => Ok(Token::Comma),
            ScanToken::Null => Ok(Token::Null),
            ScanToken::True => Ok(Token::True),
            ScanToken::False => Ok(Token::False),
            ScanToken::String {
                start,
                end,
                has_escapes,
            } => {
                // Convert to absolute positions in original input
                let abs_start = self.window_start + start;
                let abs_end = self.window_start + end;

                let s = if BORROW && !*has_escapes {
                    // Borrow directly from original input (zero-copy)
                    if TRUSTED_UTF8 {
                        // SAFETY: Caller guarantees input is valid UTF-8 (came from &str)
                        unsafe {
                            scanner::decode_string_unchecked(self.input, abs_start, abs_end, false)?
                        }
                    } else {
                        scanner::decode_string(self.input, abs_start, abs_end, false)?
                    }
                } else {
                    // Must produce owned string (has escapes or BORROW=false)
                    Cow::Owned(scanner::decode_string_owned(
                        self.input, abs_start, abs_end,
                    )?)
                };
                Ok(Token::String(s))
            }
            ScanToken::Number { start, end, hint } => {
                // Convert to absolute positions
                let abs_start = self.window_start + start;
                let abs_end = self.window_start + end;

                let parsed = scanner::parse_number(self.input, abs_start, abs_end, *hint)?;
                Ok(match parsed {
                    ParsedNumber::U64(n) => Token::U64(n),
                    ParsedNumber::I64(n) => Token::I64(n),
                    ParsedNumber::U128(n) => Token::U128(n),
                    ParsedNumber::I128(n) => Token::I128(n),
                    ParsedNumber::F64(n) => Token::F64(n),
                })
            }
            ScanToken::Eof | ScanToken::NeedMore { .. } => {
                unreachable!("Eof and NeedMore handled in next_token loop")
            }
        }
    }

    /// Skip a JSON value without decoding.
    ///
    /// Returns the span of the skipped value (absolute positions).
    /// No string allocations occur.
    pub fn skip(&mut self) -> Result<Span, AdapterError> {
        // Get the first token using windowing
        let first_token = self.next_token_for_skip()?;
        let abs_start = first_token.span.offset;

        match first_token.token {
            SkipToken::ObjectStart => {
                // Skip until matching ObjectEnd
                let mut depth = 1;
                let mut abs_end = first_token.span.offset + first_token.span.len;
                while depth > 0 {
                    let t = self.next_token_for_skip()?;
                    abs_end = t.span.offset + t.span.len;
                    match t.token {
                        SkipToken::ObjectStart => depth += 1,
                        SkipToken::ObjectEnd => depth -= 1,
                        _ => {}
                    }
                }
                Ok(Span::new(abs_start, abs_end - abs_start))
            }
            SkipToken::ArrayStart => {
                // Skip until matching ArrayEnd
                let mut depth = 1;
                let mut abs_end = first_token.span.offset + first_token.span.len;
                while depth > 0 {
                    let t = self.next_token_for_skip()?;
                    abs_end = t.span.offset + t.span.len;
                    match t.token {
                        SkipToken::ArrayStart => depth += 1,
                        SkipToken::ArrayEnd => depth -= 1,
                        _ => {}
                    }
                }
                Ok(Span::new(abs_start, abs_end - abs_start))
            }
            // Scalars: just return their span
            SkipToken::Scalar => Ok(first_token.span),
            SkipToken::Invalid(ch) => Err(AdapterError {
                kind: AdapterErrorKind::Scan(ScanErrorKind::UnexpectedChar(ch)),
                span: first_token.span,
            }),
            SkipToken::Eof => Err(AdapterError {
                kind: AdapterErrorKind::Scan(ScanErrorKind::UnexpectedEof("expected value")),
                span: first_token.span,
            }),
            // These shouldn't appear as first token when skipping a value
            SkipToken::ObjectEnd => Err(AdapterError {
                kind: AdapterErrorKind::Scan(ScanErrorKind::UnexpectedChar('}')),
                span: first_token.span,
            }),
            SkipToken::ArrayEnd => Err(AdapterError {
                kind: AdapterErrorKind::Scan(ScanErrorKind::UnexpectedChar(']')),
                span: first_token.span,
            }),
        }
    }

    /// Internal: get next token for skip operation (handles windowing).
    fn next_token_for_skip(&mut self) -> Result<SpannedSkipToken, AdapterError> {
        loop {
            let window = self.current_window();
            let spanned = match self.scanner.next_token(window) {
                Ok(s) => s,
                Err(e) => {
                    return Err(AdapterError {
                        kind: AdapterErrorKind::Scan(e.kind),
                        span: Span::new(self.window_start + e.span.offset, e.span.len),
                    });
                }
            };

            match spanned.token {
                ScanToken::NeedMore { .. } => {
                    if self.at_end_of_input() {
                        // True EOF - try to finalize any pending token
                        let window = self.current_window();
                        let finalized = match self.scanner.finalize_at_eof(window) {
                            Ok(f) => f,
                            Err(e) => {
                                return Err(AdapterError {
                                    kind: AdapterErrorKind::Scan(e.kind),
                                    span: Span::new(self.window_start + e.span.offset, e.span.len),
                                });
                            }
                        };

                        let consumed = self.scanner.pos();
                        let abs_span = Span::new(
                            self.window_start + finalized.span.offset,
                            finalized.span.len,
                        );

                        let skip_token = match finalized.token {
                            ScanToken::ObjectStart => SkipToken::ObjectStart,
                            ScanToken::ObjectEnd => SkipToken::ObjectEnd,
                            ScanToken::ArrayStart => SkipToken::ArrayStart,
                            ScanToken::ArrayEnd => SkipToken::ArrayEnd,
                            ScanToken::String { .. }
                            | ScanToken::Number { .. }
                            | ScanToken::True
                            | ScanToken::False
                            | ScanToken::Null => SkipToken::Scalar,
                            ScanToken::Colon => SkipToken::Invalid(':'),
                            ScanToken::Comma => SkipToken::Invalid(','),
                            ScanToken::Eof => SkipToken::Eof,
                            ScanToken::NeedMore { .. } => unreachable!(),
                        };

                        self.slide_window(consumed);
                        return Ok(SpannedSkipToken {
                            token: skip_token,
                            span: abs_span,
                        });
                    }
                    self.grow_window();
                    continue;
                }
                ScanToken::Eof => {
                    if self.at_end_of_input() {
                        return Ok(SpannedSkipToken {
                            token: SkipToken::Eof,
                            span: Span::new(self.window_start + spanned.span.offset, 0),
                        });
                    }
                    self.slide_window(self.scanner.pos());
                    continue;
                }
                _ => {
                    let consumed = self.scanner.pos();
                    let abs_span =
                        Span::new(self.window_start + spanned.span.offset, spanned.span.len);

                    let skip_token = match spanned.token {
                        ScanToken::ObjectStart => SkipToken::ObjectStart,
                        ScanToken::ObjectEnd => SkipToken::ObjectEnd,
                        ScanToken::ArrayStart => SkipToken::ArrayStart,
                        ScanToken::ArrayEnd => SkipToken::ArrayEnd,
                        ScanToken::String { .. }
                        | ScanToken::Number { .. }
                        | ScanToken::True
                        | ScanToken::False
                        | ScanToken::Null => SkipToken::Scalar,
                        ScanToken::Colon => SkipToken::Invalid(':'),
                        ScanToken::Comma => SkipToken::Invalid(','),
                        ScanToken::Eof | ScanToken::NeedMore { .. } => unreachable!(),
                    };

                    self.slide_window(consumed);
                    return Ok(SpannedSkipToken {
                        token: skip_token,
                        span: abs_span,
                    });
                }
            }
        }
    }

    /// Get the current absolute position in the input.
    #[allow(dead_code)]
    pub const fn position(&self) -> usize {
        self.window_start + self.scanner.pos()
    }

    /// Get the underlying input slice.
    #[allow(dead_code)]
    pub const fn input(&self) -> &'input [u8] {
        self.input
    }
}

/// Simplified token type for skip operations (no need to decode content).
#[derive(Debug, Clone, Copy)]
enum SkipToken {
    ObjectStart,
    ObjectEnd,
    ArrayStart,
    ArrayEnd,
    Scalar,        // String, Number, true, false, null
    Invalid(char), // colon, comma, etc.
    Eof,
}

/// Spanned skip token.
#[derive(Debug)]
struct SpannedSkipToken {
    token: SkipToken,
    span: Span,
}

// ============================================================================
// TokenSource trait - unifies slice and streaming adapters
// ============================================================================

#[cfg(feature = "streaming")]
use crate::error::{JsonError, JsonErrorKind};

/// Trait for token sources that can be used by the deserializer.
///
/// This trait abstracts over both:
/// - `SliceAdapter<'input>` implements `TokenSource<'input>` - can borrow from input
/// - `StreamingAdapter` implements `TokenSource<'static>` - always owned
///
/// The lifetime parameter `'input` is the lifetime of the input data,
/// NOT the lifetime of `self`. This is why we don't need GATs.
#[cfg(feature = "streaming")]
pub trait TokenSource<'input> {
    /// Get the next token.
    fn next_token(&mut self) -> Result<SpannedAdapterToken<'input>, JsonError>;

    /// Skip a JSON value without fully decoding it.
    /// Returns the span of the skipped value.
    fn skip(&mut self) -> Result<Span, JsonError>;

    /// Get the raw input bytes, if available.
    ///
    /// Returns `Some` for slice-based adapters, `None` for streaming.
    /// Used for features that need direct input access (RawJson, flatten replay).
    #[allow(dead_code)]
    fn input_bytes(&self) -> Option<&'input [u8]> {
        None
    }

    /// Create a new adapter starting from the given offset in the input.
    ///
    /// Returns `Some` for slice-based adapters, `None` for streaming.
    /// Used for flatten replay.
    #[allow(dead_code)]
    fn at_offset(&self, offset: usize) -> Option<Self>
    where
        Self: Sized,
    {
        let _ = offset;
        None
    }
}

#[cfg(feature = "streaming")]
impl<'input, const BORROW: bool> TokenSource<'input> for SliceAdapter<'input, BORROW> {
    fn next_token(&mut self) -> Result<SpannedAdapterToken<'input>, JsonError> {
        SliceAdapter::next_token(self).map_err(|e| JsonError {
            kind: JsonErrorKind::Scan(match e.kind {
                AdapterErrorKind::Scan(s) => s,
                AdapterErrorKind::NeedMore => {
                    crate::scanner::ScanErrorKind::UnexpectedEof("need more data")
                }
            }),
            span: Some(e.span),
            source_code: None,
        })
    }

    fn skip(&mut self) -> Result<Span, JsonError> {
        SliceAdapter::skip(self).map_err(|e| JsonError {
            kind: JsonErrorKind::Scan(match e.kind {
                AdapterErrorKind::Scan(s) => s,
                AdapterErrorKind::NeedMore => {
                    crate::scanner::ScanErrorKind::UnexpectedEof("need more data")
                }
            }),
            span: Some(e.span),
            source_code: None,
        })
    }

    fn input_bytes(&self) -> Option<&'input [u8]> {
        Some(self.input)
    }

    fn at_offset(&self, offset: usize) -> Option<Self> {
        Some(SliceAdapter::new(&self.input[offset..]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use facet_testhelpers::test;

    #[test]
    fn test_next_simple() {
        let json = br#"{"name": "test", "value": 42}"#;
        let mut adapter = SliceAdapter::<true>::new(json);

        // {
        let t = adapter.next_token().unwrap();
        assert!(matches!(t.token, Token::ObjectStart));

        // "name"
        let t = adapter.next_token().unwrap();
        assert_eq!(t.token, Token::String(Cow::Borrowed("name")));

        // :
        let t = adapter.next_token().unwrap();
        assert!(matches!(t.token, Token::Colon));

        // "test"
        let t = adapter.next_token().unwrap();
        assert_eq!(t.token, Token::String(Cow::Borrowed("test")));

        // ,
        let t = adapter.next_token().unwrap();
        assert!(matches!(t.token, Token::Comma));

        // "value"
        let t = adapter.next_token().unwrap();
        assert_eq!(t.token, Token::String(Cow::Borrowed("value")));

        // :
        let t = adapter.next_token().unwrap();
        assert!(matches!(t.token, Token::Colon));

        // 42
        let t = adapter.next_token().unwrap();
        assert_eq!(t.token, Token::U64(42));

        // }
        let t = adapter.next_token().unwrap();
        assert!(matches!(t.token, Token::ObjectEnd));

        // EOF
        let t = adapter.next_token().unwrap();
        assert!(matches!(t.token, Token::Eof));
    }

    #[test]
    fn test_next_with_escapes() {
        let json = br#""hello\nworld""#;
        let mut adapter = SliceAdapter::<true>::new(json);

        let t = adapter.next_token().unwrap();
        // Has escapes, so it's Owned
        assert_eq!(
            t.token,
            Token::String(Cow::Owned("hello\nworld".to_string()))
        );
    }

    #[test]
    fn test_skip_scalar() {
        let json = br#"{"skip": 12345, "keep": "value"}"#;
        let mut adapter = SliceAdapter::<true>::new(json);

        // {
        adapter.next_token().unwrap();
        // "skip"
        adapter.next_token().unwrap();
        // :
        adapter.next_token().unwrap();

        // Skip the number - no allocation!
        let span = adapter.skip().unwrap();
        assert_eq!(&json[span.offset..span.offset + span.len], b"12345");

        // ,
        let t = adapter.next_token().unwrap();
        assert!(matches!(t.token, Token::Comma));

        // Continue with "keep"
        let t = adapter.next_token().unwrap();
        assert_eq!(t.token, Token::String(Cow::Borrowed("keep")));
    }

    #[test]
    fn test_skip_object() {
        let json = br#"{"skip": {"nested": {"deep": true}}, "keep": 1}"#;
        let mut adapter = SliceAdapter::<true>::new(json);

        // {
        adapter.next_token().unwrap();
        // "skip"
        adapter.next_token().unwrap();
        // :
        adapter.next_token().unwrap();

        // Skip the entire nested object
        let span = adapter.skip().unwrap();
        assert_eq!(
            &json[span.offset..span.offset + span.len],
            br#"{"nested": {"deep": true}}"#
        );

        // ,
        let t = adapter.next_token().unwrap();
        assert!(matches!(t.token, Token::Comma));

        // "keep"
        let t = adapter.next_token().unwrap();
        assert_eq!(t.token, Token::String(Cow::Borrowed("keep")));
    }

    #[test]
    fn test_skip_array() {
        let json = br#"{"skip": [1, [2, 3], 4], "keep": true}"#;
        let mut adapter = SliceAdapter::<true>::new(json);

        // {
        adapter.next_token().unwrap();
        // "skip"
        adapter.next_token().unwrap();
        // :
        adapter.next_token().unwrap();

        // Skip the entire array
        let span = adapter.skip().unwrap();
        assert_eq!(
            &json[span.offset..span.offset + span.len],
            br#"[1, [2, 3], 4]"#
        );

        // ,
        adapter.next_token().unwrap();

        // "keep"
        let t = adapter.next_token().unwrap();
        assert_eq!(t.token, Token::String(Cow::Borrowed("keep")));

        // :
        adapter.next_token().unwrap();

        // true
        let t = adapter.next_token().unwrap();
        assert!(matches!(t.token, Token::True));
    }

    #[test]
    fn test_skip_string_no_allocation() {
        // When skipping a string, even one with escapes, we shouldn't allocate
        let json = br#"{"skip": "hello\nworld\twith\rescapes", "keep": 1}"#;
        let mut adapter = SliceAdapter::<true>::new(json);

        // {
        adapter.next_token().unwrap();
        // "skip"
        adapter.next_token().unwrap();
        // :
        adapter.next_token().unwrap();

        // Skip the string - the span covers the whole string including quotes
        let span = adapter.skip().unwrap();
        assert_eq!(
            &json[span.offset..span.offset + span.len],
            br#""hello\nworld\twith\rescapes""#
        );
    }

    #[test]
    fn test_borrow_false_always_owned() {
        let json = br#""no escapes here""#;
        let mut adapter = SliceAdapter::<false>::new(json);

        let t = adapter.next_token().unwrap();
        // Even without escapes, BORROW=false means Owned
        assert!(matches!(t.token, Token::String(Cow::Owned(_))));
    }

    #[test]
    fn test_borrow_true_borrows_when_possible() {
        let json = br#""no escapes here""#;
        let mut adapter = SliceAdapter::<true>::new(json);

        let t = adapter.next_token().unwrap();
        // No escapes + BORROW=true means Borrowed
        assert!(matches!(t.token, Token::String(Cow::Borrowed(_))));
    }

    #[test]
    fn test_windowed_parsing_long_string() {
        // Test that strings longer than the chunk size (4 bytes) work correctly
        // This exercises the NeedMore handling
        let json = br#""hello world""#; // 13 bytes, much longer than chunk_size=4
        let mut adapter = SliceAdapter::<true>::new(json);

        let t = adapter.next_token().unwrap();
        assert_eq!(t.token, Token::String(Cow::Borrowed("hello world")));
        // Span should cover the entire string including quotes
        assert_eq!(t.span.offset, 0);
        assert_eq!(t.span.len, 13);
    }

    #[test]
    fn test_windowed_parsing_number_at_eof() {
        // Test that numbers at EOF are finalized correctly
        let json = b"-123"; // 4 bytes, exactly chunk_size
        let mut adapter = SliceAdapter::<true>::new(json);

        let t = adapter.next_token().unwrap();
        assert_eq!(t.token, Token::I64(-123));
    }

    #[test]
    fn test_windowed_parsing_complex_object() {
        // Test a complex object that spans many chunks
        let json = br#"{"name": "hello world", "value": 12345, "nested": {"a": 1}}"#;
        let mut adapter = SliceAdapter::<true>::new(json);

        // {
        assert!(matches!(
            adapter.next_token().unwrap().token,
            Token::ObjectStart
        ));
        // "name"
        assert_eq!(
            adapter.next_token().unwrap().token,
            Token::String(Cow::Borrowed("name"))
        );
        // :
        assert!(matches!(adapter.next_token().unwrap().token, Token::Colon));
        // "hello world"
        assert_eq!(
            adapter.next_token().unwrap().token,
            Token::String(Cow::Borrowed("hello world"))
        );
        // ,
        assert!(matches!(adapter.next_token().unwrap().token, Token::Comma));
        // "value"
        assert_eq!(
            adapter.next_token().unwrap().token,
            Token::String(Cow::Borrowed("value"))
        );
        // :
        assert!(matches!(adapter.next_token().unwrap().token, Token::Colon));
        // 12345
        assert_eq!(adapter.next_token().unwrap().token, Token::U64(12345));
        // ,
        assert!(matches!(adapter.next_token().unwrap().token, Token::Comma));
        // "nested"
        assert_eq!(
            adapter.next_token().unwrap().token,
            Token::String(Cow::Borrowed("nested"))
        );
        // :
        assert!(matches!(adapter.next_token().unwrap().token, Token::Colon));
        // {
        assert!(matches!(
            adapter.next_token().unwrap().token,
            Token::ObjectStart
        ));
        // "a"
        assert_eq!(
            adapter.next_token().unwrap().token,
            Token::String(Cow::Borrowed("a"))
        );
        // :
        assert!(matches!(adapter.next_token().unwrap().token, Token::Colon));
        // 1
        assert_eq!(adapter.next_token().unwrap().token, Token::U64(1));
        // }
        assert!(matches!(
            adapter.next_token().unwrap().token,
            Token::ObjectEnd
        ));
        // }
        assert!(matches!(
            adapter.next_token().unwrap().token,
            Token::ObjectEnd
        ));
        // EOF
        assert!(matches!(adapter.next_token().unwrap().token, Token::Eof));
    }
}
