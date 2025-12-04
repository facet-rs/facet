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

/// Token adapter for slice-based parsing.
///
/// Wraps a Scanner and provides `next()` and `skip()` methods.
///
/// The const generic `BORROW` controls string handling:
/// - `BORROW=true`: strings without escapes are borrowed (`Cow::Borrowed`)
/// - `BORROW=false`: all strings are owned (`Cow::Owned`)
pub struct SliceAdapter<'input, const BORROW: bool> {
    buffer: &'input [u8],
    scanner: Scanner,
}

impl<'input, const BORROW: bool> SliceAdapter<'input, BORROW> {
    /// Create a new adapter for slice-based parsing.
    pub fn new(buffer: &'input [u8]) -> Self {
        Self {
            buffer,
            scanner: Scanner::new(),
        }
    }

    /// Get the next token with decoded content.
    ///
    /// Strings are decoded (escapes processed) and returned as Cow<str>.
    /// Numbers are parsed into appropriate numeric types.
    pub fn next_token(&mut self) -> Result<SpannedAdapterToken<'input>, AdapterError> {
        let spanned = self.scanner.next_token(self.buffer)?;

        let token = match spanned.token {
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
                has_escapes,
            } => {
                let s = if BORROW && !has_escapes {
                    // Can borrow directly from input
                    scanner::decode_string(self.buffer, start, end, false)?
                } else {
                    // Must produce owned string (either BORROW=false or has escapes)
                    Cow::Owned(scanner::decode_string_owned(self.buffer, start, end)?)
                };
                Token::String(s)
            }
            ScanToken::Number { start, end, hint } => {
                let parsed = scanner::parse_number(self.buffer, start, end, hint)?;
                match parsed {
                    ParsedNumber::U64(n) => Token::U64(n),
                    ParsedNumber::I64(n) => Token::I64(n),
                    ParsedNumber::U128(n) => Token::U128(n),
                    ParsedNumber::I128(n) => Token::I128(n),
                    ParsedNumber::F64(n) => Token::F64(n),
                }
            }
            ScanToken::Eof => Token::Eof,
            ScanToken::NeedMore { .. } => {
                // For slice-based parsing, NeedMore means unexpected EOF
                return Err(AdapterError {
                    kind: AdapterErrorKind::Scan(ScanErrorKind::UnexpectedEof("in token")),
                    span: spanned.span,
                });
            }
        };

        Ok(SpannedAdapterToken {
            token,
            span: spanned.span,
        })
    }

    /// Skip a JSON value without decoding.
    ///
    /// Returns the span of the skipped value.
    /// No string allocations occur.
    pub fn skip(&mut self) -> Result<Span, AdapterError> {
        let start_spanned = self.scanner.next_token(self.buffer)?;
        let start_offset = start_spanned.span.offset;

        match start_spanned.token {
            ScanToken::ObjectStart => {
                // Skip until matching ObjectEnd
                let mut depth = 1;
                let mut end_span = start_spanned.span;
                while depth > 0 {
                    let spanned = self.scanner.next_token(self.buffer)?;
                    end_span = spanned.span;
                    match spanned.token {
                        ScanToken::ObjectStart => depth += 1,
                        ScanToken::ObjectEnd => depth -= 1,
                        ScanToken::NeedMore { .. } => {
                            return Err(AdapterError {
                                kind: AdapterErrorKind::Scan(ScanErrorKind::UnexpectedEof(
                                    "in object",
                                )),
                                span: spanned.span,
                            });
                        }
                        _ => {}
                    }
                }
                Ok(Span::new(
                    start_offset,
                    end_span.offset + end_span.len - start_offset,
                ))
            }
            ScanToken::ArrayStart => {
                // Skip until matching ArrayEnd
                let mut depth = 1;
                let mut end_span = start_spanned.span;
                while depth > 0 {
                    let spanned = self.scanner.next_token(self.buffer)?;
                    end_span = spanned.span;
                    match spanned.token {
                        ScanToken::ArrayStart => depth += 1,
                        ScanToken::ArrayEnd => depth -= 1,
                        ScanToken::NeedMore { .. } => {
                            return Err(AdapterError {
                                kind: AdapterErrorKind::Scan(ScanErrorKind::UnexpectedEof(
                                    "in array",
                                )),
                                span: spanned.span,
                            });
                        }
                        _ => {}
                    }
                }
                Ok(Span::new(
                    start_offset,
                    end_span.offset + end_span.len - start_offset,
                ))
            }
            // Scalars: just return their span
            ScanToken::String { .. }
            | ScanToken::Number { .. }
            | ScanToken::True
            | ScanToken::False
            | ScanToken::Null => Ok(start_spanned.span),
            ScanToken::Eof => Err(AdapterError {
                kind: AdapterErrorKind::Scan(ScanErrorKind::UnexpectedEof("expected value")),
                span: start_spanned.span,
            }),
            ScanToken::NeedMore { .. } => Err(AdapterError {
                kind: AdapterErrorKind::Scan(ScanErrorKind::UnexpectedEof("expected value")),
                span: start_spanned.span,
            }),
            // Colon/Comma are not values
            ScanToken::Colon | ScanToken::ObjectEnd | ScanToken::ArrayEnd | ScanToken::Comma => {
                Err(AdapterError {
                    kind: AdapterErrorKind::Scan(ScanErrorKind::UnexpectedChar(':')),
                    span: start_spanned.span,
                })
            }
        }
    }

    /// Get the current position in the buffer.
    pub fn position(&self) -> usize {
        self.scanner.pos()
    }

    /// Get the underlying buffer.
    pub fn buffer(&self) -> &'input [u8] {
        self.buffer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}

#[cfg(all(test, feature = "bolero-inline-tests"))]
#[allow(clippy::while_let_loop, clippy::same_item_push)]
mod fuzz_tests {
    use super::*;
    use bolero::check;

    /// Fuzz the adapter with arbitrary bytes - should never panic
    #[test]
    fn fuzz_adapter_arbitrary_bytes() {
        check!().for_each(|input: &[u8]| {
            let mut adapter = SliceAdapter::<true>::new(input);
            loop {
                match adapter.next_token() {
                    Ok(spanned) => {
                        if matches!(spanned.token, Token::Eof) {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });
    }

    /// Fuzz adapter.skip() with arbitrary bytes
    #[test]
    fn fuzz_adapter_skip() {
        check!().for_each(|input: &[u8]| {
            let mut adapter = SliceAdapter::<true>::new(input);
            // Try to skip - should handle anything gracefully
            let _ = adapter.skip();
        });
    }

    /// Fuzz with JSON-like objects, alternating next/skip
    #[test]
    fn fuzz_adapter_next_skip_alternating() {
        check!().for_each(|input: &[u8]| {
            let mut adapter = SliceAdapter::<true>::new(input);
            let mut use_skip = false;
            loop {
                let result = if use_skip {
                    adapter.skip().map(|_| ())
                } else {
                    adapter
                        .next_token()
                        .map(|t| if matches!(t.token, Token::Eof) {})
                };
                match result {
                    Ok(()) => {}
                    Err(_) => break,
                }
                use_skip = !use_skip;

                // Safety: check if we're at EOF by peeking
                let mut peek_adapter = SliceAdapter::<true>::new(&input[adapter.position()..]);
                if matches!(peek_adapter.next_token(), Ok(t) if matches!(t.token, Token::Eof)) {
                    break;
                }
            }
        });
    }

    /// Fuzz BORROW=false path
    #[test]
    fn fuzz_adapter_no_borrow() {
        check!().for_each(|input: &[u8]| {
            let mut adapter = SliceAdapter::<false>::new(input);
            loop {
                match adapter.next_token() {
                    Ok(spanned) => {
                        // Verify strings are always Owned when BORROW=false
                        if let Token::String(cow) = &spanned.token {
                            assert!(matches!(cow, Cow::Owned(_)));
                        }
                        if matches!(spanned.token, Token::Eof) {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });
    }

    /// Fuzz with strings containing various escape sequences
    #[test]
    fn fuzz_adapter_string_escapes() {
        check!().for_each(|content: &[u8]| {
            // Build {"key": "...content..."}
            let mut input = Vec::new();
            input.extend_from_slice(br#"{"k":""#);
            input.extend_from_slice(content);
            input.extend_from_slice(br#""}"#);

            let mut adapter = SliceAdapter::<true>::new(&input);
            loop {
                match adapter.next_token() {
                    Ok(t) if matches!(t.token, Token::Eof) => break,
                    Ok(_) => {}
                    Err(_) => break,
                }
            }
        });
    }

    /// Fuzz skip on nested structures
    #[test]
    fn fuzz_adapter_skip_nested() {
        check!().for_each(|input: &[u8]| {
            // Use first byte as depth indicator
            let depth = (input.first().copied().unwrap_or(1) as usize % 50) + 1;

            // Build nested array: [[[[...]]]]
            let mut nested = Vec::new();
            for _ in 0..depth {
                nested.push(b'[');
            }
            nested.extend_from_slice(b"null");
            for _ in 0..depth {
                nested.push(b']');
            }

            let mut adapter = SliceAdapter::<true>::new(&nested);
            let result = adapter.skip();
            assert!(result.is_ok());
            let span = result.unwrap();
            assert_eq!(span.len, nested.len());
        });
    }
}
