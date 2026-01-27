//! Low-level JSON scanner that finds token boundaries without materializing strings.
//!
//! The scanner's job is to identify where tokens are in a buffer, not to interpret them.
//! String content is returned as indices + a `has_escapes` flag. The deserializer
//! decides whether to decode escapes based on the target type.
//!
//! This design enables:
//! - Zero-copy borrowed strings (when no escapes)
//! - Streaming from `std::io::Read` with buffer refills
//! - Skipping values without allocation (RawJson, unknown fields)

use core::str;

use facet_reflect::Span;

/// Token kinds with minimal data - strings/numbers are just indices into the buffer.
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
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
    /// A string literal - indices point to content (excluding quotes)
    String {
        /// Start index of string content (after opening quote)
        start: usize,
        /// End index of string content (before closing quote)
        end: usize,
        /// True if the string contains escape sequences that need processing
        has_escapes: bool,
    },
    /// A number literal - indices point to the raw number text
    Number {
        /// Start index of number
        start: usize,
        /// End index of number
        end: usize,
        /// Hint about number format
        hint: NumberHint,
    },
    /// End of input reached
    Eof,
    /// Buffer exhausted mid-token - need refill for streaming
    NeedMore {
        /// How many bytes were consumed before hitting the boundary
        consumed: usize,
    },
}

/// Hint about number format to guide parsing
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NumberHint {
    /// Unsigned integer (no sign, no decimal, no exponent)
    Unsigned,
    /// Signed integer (has `-` prefix, no decimal, no exponent)
    Signed,
    /// Floating point (has `.` or `e`/`E`)
    Float,
}

/// Spanned token with location information
#[derive(Debug, Clone, PartialEq)]
pub struct SpannedToken {
    /// The token
    pub token: Token,
    /// Source span
    pub span: Span,
}

/// Scanner error
#[derive(Debug, Clone, PartialEq)]
pub struct ScanError {
    /// The error kind
    pub kind: ScanErrorKind,
    /// Source span
    pub span: Span,
}

/// Types of scanner errors
#[derive(Debug, Clone, PartialEq)]
pub enum ScanErrorKind {
    /// Unexpected character
    UnexpectedChar(char),
    /// Unexpected end of input (with context)
    UnexpectedEof(&'static str),
    /// Invalid UTF-8
    InvalidUtf8,
}

/// Result type for scanner operations
pub type ScanResult = Result<SpannedToken, ScanError>;

/// JSON scanner state machine.
///
/// The scanner operates on a byte buffer and tracks position. For streaming,
/// the buffer can be refilled when `Token::NeedMore` is returned.
pub struct Scanner {
    /// Current position in the buffer
    pos: usize,
    /// State for resuming after NeedMore (for streaming)
    state: ScanState,
}

/// Internal state for resuming mid-token after buffer refill
#[derive(Debug, Clone, Default)]
enum ScanState {
    #[default]
    Ready,
    /// In the middle of scanning a string
    InString {
        start: usize,
        has_escapes: bool,
        escape_next: bool,
    },
    /// In the middle of scanning a number
    InNumber { start: usize, hint: NumberHint },
    /// In the middle of scanning a literal (true/false/null)
    InLiteral {
        start: usize,
        expected: &'static [u8],
        matched: usize,
    },
}

impl Scanner {
    /// Create a new scanner starting at position 0
    pub const fn new() -> Self {
        Self {
            pos: 0,
            state: ScanState::Ready,
        }
    }

    /// Create a scanner starting at a specific position
    #[allow(dead_code)]
    pub const fn at_position(pos: usize) -> Self {
        Self {
            pos,
            state: ScanState::Ready,
        }
    }

    /// Current position in the buffer
    pub const fn pos(&self) -> usize {
        self.pos
    }

    /// Set position (used after buffer operations)
    pub const fn set_pos(&mut self, pos: usize) {
        self.pos = pos;
    }

    /// Finalize any pending token at true EOF.
    ///
    /// Call this when the scanner returned `NeedMore` but no more data is available.
    /// Returns the completed token if one is pending (e.g., a number at EOF),
    /// or an error if the token is incomplete (e.g., unterminated string).
    pub fn finalize_at_eof(&mut self, buf: &[u8]) -> ScanResult {
        match core::mem::take(&mut self.state) {
            ScanState::Ready => {
                // Nothing pending
                Ok(SpannedToken {
                    token: Token::Eof,
                    span: Span::new(self.pos, 0),
                })
            }
            ScanState::InNumber { start, hint } => {
                // Number is complete at EOF (numbers don't need closing delimiter)
                let end = self.pos;
                if end == start || (end == start + 1 && buf.get(start) == Some(&b'-')) {
                    return Err(ScanError {
                        kind: ScanErrorKind::UnexpectedEof("in number"),
                        span: Span::new(start, end - start),
                    });
                }
                Ok(SpannedToken {
                    token: Token::Number { start, end, hint },
                    span: Span::new(start, end - start),
                })
            }
            ScanState::InString { start, .. } => {
                // Unterminated string
                Err(ScanError {
                    kind: ScanErrorKind::UnexpectedEof("in string"),
                    span: Span::new(start, self.pos - start),
                })
            }
            ScanState::InLiteral {
                start,
                expected,
                matched,
            } => {
                // Check if the literal is complete
                if matched == expected.len() {
                    let token = match expected {
                        b"true" => Token::True,
                        b"false" => Token::False,
                        b"null" => Token::Null,
                        _ => unreachable!(),
                    };
                    Ok(SpannedToken {
                        token,
                        span: Span::new(start, expected.len()),
                    })
                } else {
                    Err(ScanError {
                        kind: ScanErrorKind::UnexpectedEof("in literal"),
                        span: Span::new(start, self.pos - start),
                    })
                }
            }
        }
    }

    /// Scan the next token from the buffer.
    ///
    /// Returns `Token::NeedMore` if the buffer is exhausted mid-token,
    /// allowing the caller to refill and retry.
    pub fn next_token(&mut self, buf: &[u8]) -> ScanResult {
        // If we have pending state from a previous NeedMore, resume
        match core::mem::take(&mut self.state) {
            ScanState::Ready => {}
            ScanState::InString {
                start,
                has_escapes,
                escape_next,
            } => {
                return self.resume_string(buf, start, has_escapes, escape_next);
            }
            ScanState::InNumber { start, hint } => {
                return self.resume_number(buf, start, hint);
            }
            ScanState::InLiteral {
                start,
                expected,
                matched,
            } => {
                return self.resume_literal(buf, start, expected, matched);
            }
        }

        self.skip_whitespace(buf);

        let start = self.pos;
        let Some(&byte) = buf.get(self.pos) else {
            return Ok(SpannedToken {
                token: Token::Eof,
                span: Span::new(self.pos, 0),
            });
        };

        match byte {
            b'{' => {
                self.pos += 1;
                Ok(SpannedToken {
                    token: Token::ObjectStart,
                    span: Span::new(start, 1),
                })
            }
            b'}' => {
                self.pos += 1;
                Ok(SpannedToken {
                    token: Token::ObjectEnd,
                    span: Span::new(start, 1),
                })
            }
            b'[' => {
                self.pos += 1;
                Ok(SpannedToken {
                    token: Token::ArrayStart,
                    span: Span::new(start, 1),
                })
            }
            b']' => {
                self.pos += 1;
                Ok(SpannedToken {
                    token: Token::ArrayEnd,
                    span: Span::new(start, 1),
                })
            }
            b':' => {
                self.pos += 1;
                Ok(SpannedToken {
                    token: Token::Colon,
                    span: Span::new(start, 1),
                })
            }
            b',' => {
                self.pos += 1;
                Ok(SpannedToken {
                    token: Token::Comma,
                    span: Span::new(start, 1),
                })
            }
            b'"' => self.scan_string(buf, start),
            b'-' | b'0'..=b'9' => self.scan_number(buf, start),
            b't' => self.scan_literal(buf, start, b"true", Token::True),
            b'f' => self.scan_literal(buf, start, b"false", Token::False),
            b'n' => self.scan_literal(buf, start, b"null", Token::Null),
            _ => Err(ScanError {
                kind: ScanErrorKind::UnexpectedChar(byte as char),
                span: Span::new(start, 1),
            }),
        }
    }

    fn skip_whitespace(&mut self, buf: &[u8]) {
        let mut pos = self.pos;
        while let Some(&b) = buf.get(pos) {
            match b {
                b' ' | b'\t' | b'\n' | b'\r' => pos += 1,
                _ => break,
            }
        }
        self.pos = pos;
    }

    /// Scan a string, finding its boundaries and noting if it has escapes.
    fn scan_string(&mut self, buf: &[u8], start: usize) -> ScanResult {
        // Skip opening quote
        self.pos += 1;
        let content_start = self.pos;

        self.scan_string_content(buf, start, content_start, false, false)
    }

    fn resume_string(
        &mut self,
        buf: &[u8],
        start: usize,
        has_escapes: bool,
        escape_next: bool,
    ) -> ScanResult {
        let content_start = start + 1; // After opening quote
        self.scan_string_content(buf, start, content_start, has_escapes, escape_next)
    }

    fn scan_string_content(
        &mut self,
        buf: &[u8],
        start: usize,
        content_start: usize,
        mut has_escapes: bool,
        mut escape_next: bool,
    ) -> ScanResult {
        // SIMD-friendly fast path: scan 16 bytes at a time looking for quotes/backslashes
        const STEP_SIZE: usize = 16;
        type Window = u128;
        type Chunk = [u8; STEP_SIZE];

        // SIMD fast path: only if we're not in escape mode
        if !escape_next {
            loop {
                if let Some(Ok(chunk)) = buf
                    .get(self.pos..)
                    .and_then(|s| s.get(..STEP_SIZE))
                    .map(Chunk::try_from)
                {
                    let window = Window::from_ne_bytes(chunk);
                    let has_quote = contains_byte(window, b'"');
                    let has_backslash = contains_byte(window, b'\\');

                    if !has_quote && !has_backslash {
                        // Fast path: no special chars in this chunk
                        self.pos += STEP_SIZE;
                        continue;
                    }
                }
                // Fall through to byte-by-byte scanning
                break;
            }
        }

        // Byte-by-byte scanning
        while let Some(&byte) = buf.get(self.pos) {
            if escape_next {
                // Previous char was backslash, skip this char
                escape_next = false;
                self.pos += 1;

                // Handle \uXXXX - need to skip 4 more hex digits
                if byte == b'u' {
                    // Check if we have 4 more bytes
                    if self.pos + 4 > buf.len() {
                        // Need more data
                        self.state = ScanState::InString {
                            start,
                            has_escapes: true,
                            escape_next: false,
                        };
                        return Ok(SpannedToken {
                            token: Token::NeedMore { consumed: start },
                            span: Span::new(start, self.pos - start),
                        });
                    }
                    self.pos += 4;

                    // Check for surrogate pair (\uXXXX\uXXXX)
                    if self.pos + 2 <= buf.len()
                        && buf.get(self.pos) == Some(&b'\\')
                        && buf.get(self.pos + 1) == Some(&b'u')
                    {
                        if self.pos + 6 > buf.len() {
                            // Need more data for second surrogate
                            self.state = ScanState::InString {
                                start,
                                has_escapes: true,
                                escape_next: false,
                            };
                            return Ok(SpannedToken {
                                token: Token::NeedMore { consumed: start },
                                span: Span::new(start, self.pos - start),
                            });
                        }
                        // Skip \uXXXX
                        self.pos += 6;
                    }
                }
                continue;
            }

            match byte {
                b'"' => {
                    // Found closing quote
                    let content_end = self.pos;
                    self.pos += 1; // Skip closing quote

                    return Ok(SpannedToken {
                        token: Token::String {
                            start: content_start,
                            end: content_end,
                            has_escapes,
                        },
                        span: Span::new(start, self.pos - start),
                    });
                }
                b'\\' => {
                    has_escapes = true;
                    escape_next = true;
                    self.pos += 1;
                }
                _ => {
                    self.pos += 1;
                }
            }
        }

        // Reached end of buffer without closing quote
        if escape_next || self.pos > start {
            // Mid-string, need more data
            self.state = ScanState::InString {
                start,
                has_escapes,
                escape_next,
            };
            Ok(SpannedToken {
                token: Token::NeedMore { consumed: start },
                span: Span::new(start, self.pos - start),
            })
        } else {
            Err(ScanError {
                kind: ScanErrorKind::UnexpectedEof("in string"),
                span: Span::new(start, self.pos - start),
            })
        }
    }

    /// Scan a number, finding its boundaries and determining its type hint.
    fn scan_number(&mut self, buf: &[u8], start: usize) -> ScanResult {
        let mut hint = NumberHint::Unsigned;

        if buf.get(self.pos) == Some(&b'-') {
            hint = NumberHint::Signed;
            self.pos += 1;
        }

        self.scan_number_content(buf, start, hint)
    }

    fn resume_number(&mut self, buf: &[u8], start: usize, hint: NumberHint) -> ScanResult {
        // Reset position to start of number and re-scan with the larger buffer.
        // Needed since we might have stopped in an exponent. We also need to handle
        // negative numbers by ignoring the leading - (can use the hint since it may have
        // changed from the exponent)
        self.pos = start;
        if buf.get(self.pos) == Some(&b'-') {
            self.pos += 1;
        }
        self.scan_number_content(buf, start, hint)
    }

    fn scan_number_content(
        &mut self,
        buf: &[u8],
        start: usize,
        mut hint: NumberHint,
    ) -> ScanResult {
        let mut pos = self.pos;

        // Integer part
        while let Some(&b) = buf.get(pos) {
            if b.is_ascii_digit() {
                pos += 1;
            } else {
                break;
            }
        }

        // Check for decimal part
        if buf.get(pos) == Some(&b'.') {
            hint = NumberHint::Float;
            pos += 1;

            // Fractional digits
            while let Some(&b) = buf.get(pos) {
                if b.is_ascii_digit() {
                    pos += 1;
                } else {
                    break;
                }
            }
        }

        // Check for exponent
        if matches!(buf.get(pos), Some(b'e') | Some(b'E')) {
            hint = NumberHint::Float;
            pos += 1;

            // Optional sign
            if matches!(buf.get(pos), Some(b'+') | Some(b'-')) {
                pos += 1;
            }

            // Exponent digits
            while let Some(&b) = buf.get(pos) {
                if b.is_ascii_digit() {
                    pos += 1;
                } else {
                    break;
                }
            }
        }

        self.pos = pos;

        // Check if we're at end of buffer - might need more data
        // Numbers end at whitespace, punctuation, or true EOF
        if pos == buf.len() {
            // At end of buffer - need more data to see terminator
            self.state = ScanState::InNumber { start, hint };
            return Ok(SpannedToken {
                token: Token::NeedMore { consumed: start },
                span: Span::new(start, pos - start),
            });
        }

        let end = pos;

        // Validate we actually parsed something
        if end == start || (end == start + 1 && buf.get(start) == Some(&b'-')) {
            return Err(ScanError {
                kind: ScanErrorKind::UnexpectedChar(
                    buf.get(pos).map(|&b| b as char).unwrap_or('?'),
                ),
                span: Span::new(start, 1),
            });
        }

        Ok(SpannedToken {
            token: Token::Number { start, end, hint },
            span: Span::new(start, end - start),
        })
    }

    /// Scan a literal keyword (true, false, null)
    fn scan_literal(
        &mut self,
        buf: &[u8],
        start: usize,
        expected: &'static [u8],
        token: Token,
    ) -> ScanResult {
        self.scan_literal_content(buf, start, expected, 0, token)
    }

    fn resume_literal(
        &mut self,
        buf: &[u8],
        start: usize,
        expected: &'static [u8],
        matched: usize,
    ) -> ScanResult {
        let token = match expected {
            b"true" => Token::True,
            b"false" => Token::False,
            b"null" => Token::Null,
            _ => unreachable!(),
        };
        self.scan_literal_content(buf, start, expected, matched, token)
    }

    fn scan_literal_content(
        &mut self,
        buf: &[u8],
        start: usize,
        expected: &'static [u8],
        mut matched: usize,
        token: Token,
    ) -> ScanResult {
        while matched < expected.len() {
            match buf.get(self.pos) {
                Some(&b) if b == expected[matched] => {
                    self.pos += 1;
                    matched += 1;
                }
                Some(&b) => {
                    return Err(ScanError {
                        kind: ScanErrorKind::UnexpectedChar(b as char),
                        span: Span::new(self.pos, 1),
                    });
                }
                None => {
                    // Need more data
                    self.state = ScanState::InLiteral {
                        start,
                        expected,
                        matched,
                    };
                    return Ok(SpannedToken {
                        token: Token::NeedMore { consumed: start },
                        span: Span::new(start, self.pos - start),
                    });
                }
            }
        }

        Ok(SpannedToken {
            token,
            span: Span::new(start, expected.len()),
        })
    }
}

impl Default for Scanner {
    fn default() -> Self {
        Self::new()
    }
}

/// Check if a 128-bit window contains a specific byte (SIMD-friendly)
#[inline]
const fn contains_byte(window: u128, byte: u8) -> bool {
    let pattern = u128::from_ne_bytes([byte; 16]);
    let xor = window ^ pattern;
    let has_zero = (xor.wrapping_sub(0x01010101010101010101010101010101))
        & !xor
        & 0x80808080808080808080808080808080;
    has_zero != 0
}

// =============================================================================
// String decoding utilities (second pass)
// =============================================================================

/// Decode a JSON string from a buffer, handling escape sequences.
///
/// This is the "second pass" - only called when the deserializer actually needs
/// the string content. For borrowed strings without escapes, use `decode_string_borrowed`.
///
/// # Arguments
/// * `buf` - The buffer containing the string
/// * `start` - Start index (after opening quote)
/// * `end` - End index (before closing quote)
///
/// # Returns
/// The decoded string, or an error if the string contains invalid escapes.
pub fn decode_string_owned(
    buf: &[u8],
    start: usize,
    end: usize,
) -> Result<alloc::string::String, ScanError> {
    use alloc::string::String;

    let slice = &buf[start..end];
    let mut result = String::with_capacity(end - start);
    let mut i = 0;

    while i < slice.len() {
        let byte = slice[i];
        if byte == b'\\' {
            i += 1;
            if i >= slice.len() {
                return Err(ScanError {
                    kind: ScanErrorKind::UnexpectedEof("in escape sequence"),
                    span: Span::new(start + i - 1, 1),
                });
            }

            match slice[i] {
                b'"' => result.push('"'),
                b'\\' => result.push('\\'),
                b'/' => result.push('/'),
                b'b' => result.push('\x08'),
                b'f' => result.push('\x0c'),
                b'n' => result.push('\n'),
                b'r' => result.push('\r'),
                b't' => result.push('\t'),
                b'u' => {
                    i += 1;
                    if i + 4 > slice.len() {
                        return Err(ScanError {
                            kind: ScanErrorKind::UnexpectedEof("in unicode escape"),
                            span: Span::new(start + i - 2, slice.len() - i + 2),
                        });
                    }

                    let hex = &slice[i..i + 4];
                    let hex_str = str::from_utf8(hex).map_err(|_| ScanError {
                        kind: ScanErrorKind::InvalidUtf8,
                        span: Span::new(start + i, 4),
                    })?;

                    let code_unit = u16::from_str_radix(hex_str, 16).map_err(|_| ScanError {
                        kind: ScanErrorKind::UnexpectedChar('?'),
                        span: Span::new(start + i, 4),
                    })?;

                    i += 4;

                    // Check for surrogate pairs
                    let code_point = if (0xD800..=0xDBFF).contains(&code_unit) {
                        // High surrogate - expect \uXXXX to follow
                        if i + 6 > slice.len() || slice[i] != b'\\' || slice[i + 1] != b'u' {
                            return Err(ScanError {
                                kind: ScanErrorKind::InvalidUtf8,
                                span: Span::new(start + i - 6, 6),
                            });
                        }

                        i += 2; // Skip \u
                        let low_hex = &slice[i..i + 4];
                        let low_hex_str = str::from_utf8(low_hex).map_err(|_| ScanError {
                            kind: ScanErrorKind::InvalidUtf8,
                            span: Span::new(start + i, 4),
                        })?;

                        let low_unit =
                            u16::from_str_radix(low_hex_str, 16).map_err(|_| ScanError {
                                kind: ScanErrorKind::UnexpectedChar('?'),
                                span: Span::new(start + i, 4),
                            })?;

                        i += 4;

                        if !(0xDC00..=0xDFFF).contains(&low_unit) {
                            return Err(ScanError {
                                kind: ScanErrorKind::InvalidUtf8,
                                span: Span::new(start + i - 4, 4),
                            });
                        }

                        // Combine surrogates
                        let high = code_unit as u32;
                        let low = low_unit as u32;
                        0x10000 + ((high & 0x3FF) << 10) + (low & 0x3FF)
                    } else if (0xDC00..=0xDFFF).contains(&code_unit) {
                        // Lone low surrogate
                        return Err(ScanError {
                            kind: ScanErrorKind::InvalidUtf8,
                            span: Span::new(start + i - 4, 4),
                        });
                    } else {
                        code_unit as u32
                    };

                    let c = char::from_u32(code_point).ok_or_else(|| ScanError {
                        kind: ScanErrorKind::InvalidUtf8,
                        span: Span::new(start + i - 4, 4),
                    })?;

                    result.push(c);
                    continue; // Don't increment i again
                }
                other => {
                    // Unknown escape - just push the character
                    result.push(other as char);
                }
            }
            i += 1;
        } else {
            // Regular UTF-8 byte
            // Fast path for ASCII
            if byte < 0x80 {
                result.push(byte as char);
                i += 1;
            } else {
                // Multi-byte UTF-8 sequence - consume only one character
                let remaining = &slice[i..];
                match str::from_utf8(remaining) {
                    Ok(s) => {
                        // Consume exactly one UTF-8 char, then continue scanning
                        let ch = s.chars().next().expect("non-empty remaining slice");
                        result.push(ch);
                        i += ch.len_utf8();
                    }
                    Err(e) => {
                        // Partial valid UTF-8 - extract one character if possible
                        let valid_len = e.valid_up_to();
                        if valid_len > 0 {
                            let valid = str::from_utf8(&remaining[..valid_len])
                                .expect("valid_up_to guarantees valid UTF-8");
                            let ch = valid.chars().next().expect("non-empty valid slice");
                            result.push(ch);
                            i += ch.len_utf8();
                        } else {
                            return Err(ScanError {
                                kind: ScanErrorKind::InvalidUtf8,
                                span: Span::new(start + i, 1),
                            });
                        }
                    }
                }
            }
        }
    }

    Ok(result)
}

/// Try to borrow a string directly from the buffer (zero-copy).
///
/// This only works for strings without escape sequences. Returns `None` if
/// the string contains escapes or invalid UTF-8.
///
/// # Arguments
/// * `buf` - The buffer containing the string
/// * `start` - Start index (after opening quote)
/// * `end` - End index (before closing quote)
pub fn decode_string_borrowed(buf: &[u8], start: usize, end: usize) -> Option<&str> {
    let slice = &buf[start..end];

    // Quick check for backslashes
    if slice.contains(&b'\\') {
        return None;
    }

    str::from_utf8(slice).ok()
}

/// Try to borrow a string directly from the buffer (zero-copy), without UTF-8 validation.
///
/// # Safety
/// The caller must ensure the buffer contains valid UTF-8.
///
/// # Arguments
/// * `buf` - The buffer containing valid UTF-8
/// * `start` - Start index (after opening quote)
/// * `end` - End index (before closing quote)
pub unsafe fn decode_string_borrowed_unchecked(
    buf: &[u8],
    start: usize,
    end: usize,
) -> Option<&str> {
    let slice = &buf[start..end];

    // Quick check for backslashes
    if slice.contains(&b'\\') {
        return None;
    }

    // SAFETY: Caller guarantees the buffer is valid UTF-8
    Some(unsafe { str::from_utf8_unchecked(slice) })
}

/// Decode a JSON string, returning either a borrowed or owned string.
///
/// Uses `Cow<str>` to avoid allocation when possible.
pub fn decode_string<'a>(
    buf: &'a [u8],
    start: usize,
    end: usize,
    has_escapes: bool,
) -> Result<alloc::borrow::Cow<'a, str>, ScanError> {
    use alloc::borrow::Cow;

    if has_escapes {
        decode_string_owned(buf, start, end).map(Cow::Owned)
    } else {
        decode_string_borrowed(buf, start, end)
            .map(Cow::Borrowed)
            .ok_or_else(|| ScanError {
                kind: ScanErrorKind::InvalidUtf8,
                span: Span::new(start, end - start),
            })
    }
}

/// Decode a JSON string without UTF-8 validation, returning either a borrowed or owned string.
///
/// # Safety
/// The caller must ensure the buffer contains valid UTF-8.
pub unsafe fn decode_string_unchecked<'a>(
    buf: &'a [u8],
    start: usize,
    end: usize,
    has_escapes: bool,
) -> Result<alloc::borrow::Cow<'a, str>, ScanError> {
    use alloc::borrow::Cow;

    if has_escapes {
        decode_string_owned(buf, start, end).map(Cow::Owned)
    } else {
        // SAFETY: Caller guarantees buffer is valid UTF-8
        unsafe { decode_string_borrowed_unchecked(buf, start, end) }
            .map(Cow::Borrowed)
            .ok_or_else(|| ScanError {
                kind: ScanErrorKind::InvalidUtf8,
                span: Span::new(start, end - start),
            })
    }
}

/// Parse a number from the buffer.
///
/// Returns the appropriate numeric type based on the hint and value.
#[derive(Debug, Clone, PartialEq)]
pub enum ParsedNumber {
    /// Unsigned 64-bit integer
    U64(u64),
    /// Signed 64-bit integer
    I64(i64),
    /// Unsigned 128-bit integer
    U128(u128),
    /// Signed 128-bit integer
    I128(i128),
    /// 64-bit floating point
    F64(f64),
}

/// Parse a number from the buffer slice.
#[cfg(feature = "lexical-parse")]
pub fn parse_number(
    buf: &[u8],
    start: usize,
    end: usize,
    hint: NumberHint,
) -> Result<ParsedNumber, ScanError> {
    use lexical_parse_float::FromLexical as _;
    use lexical_parse_integer::FromLexical as _;

    let slice = &buf[start..end];

    match hint {
        NumberHint::Float => f64::from_lexical(slice)
            .map(ParsedNumber::F64)
            .map_err(|_| ScanError {
                kind: ScanErrorKind::UnexpectedChar('?'),
                span: Span::new(start, end - start),
            }),
        NumberHint::Signed => {
            if let Ok(n) = i64::from_lexical(slice) {
                Ok(ParsedNumber::I64(n))
            } else if let Ok(n) = i128::from_lexical(slice) {
                Ok(ParsedNumber::I128(n))
            } else {
                Err(ScanError {
                    kind: ScanErrorKind::UnexpectedChar('?'),
                    span: Span::new(start, end - start),
                })
            }
        }
        NumberHint::Unsigned => {
            if let Ok(n) = u64::from_lexical(slice) {
                Ok(ParsedNumber::U64(n))
            } else if let Ok(n) = u128::from_lexical(slice) {
                Ok(ParsedNumber::U128(n))
            } else {
                Err(ScanError {
                    kind: ScanErrorKind::UnexpectedChar('?'),
                    span: Span::new(start, end - start),
                })
            }
        }
    }
}

/// Parse a number from the buffer slice (std fallback).
#[cfg(not(feature = "lexical-parse"))]
pub fn parse_number(
    buf: &[u8],
    start: usize,
    end: usize,
    hint: NumberHint,
) -> Result<ParsedNumber, ScanError> {
    let slice = &buf[start..end];
    let s = str::from_utf8(slice).map_err(|_| ScanError {
        kind: ScanErrorKind::InvalidUtf8,
        span: Span::new(start, end - start),
    })?;

    match hint {
        NumberHint::Float => s
            .parse::<f64>()
            .map(ParsedNumber::F64)
            .map_err(|_| ScanError {
                kind: ScanErrorKind::UnexpectedChar('?'),
                span: Span::new(start, end - start),
            }),
        NumberHint::Signed => {
            if let Ok(n) = s.parse::<i64>() {
                Ok(ParsedNumber::I64(n))
            } else if let Ok(n) = s.parse::<i128>() {
                Ok(ParsedNumber::I128(n))
            } else {
                Err(ScanError {
                    kind: ScanErrorKind::UnexpectedChar('?'),
                    span: Span::new(start, end - start),
                })
            }
        }
        NumberHint::Unsigned => {
            if let Ok(n) = s.parse::<u64>() {
                Ok(ParsedNumber::U64(n))
            } else if let Ok(n) = s.parse::<u128>() {
                Ok(ParsedNumber::U128(n))
            } else {
                Err(ScanError {
                    kind: ScanErrorKind::UnexpectedChar('?'),
                    span: Span::new(start, end - start),
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use facet_testhelpers::test;

    #[test]
    fn test_simple_tokens() {
        let input = b"{}[],:";
        let mut scanner = Scanner::new();

        assert!(matches!(
            scanner.next_token(input).unwrap().token,
            Token::ObjectStart
        ));
        assert!(matches!(
            scanner.next_token(input).unwrap().token,
            Token::ObjectEnd
        ));
        assert!(matches!(
            scanner.next_token(input).unwrap().token,
            Token::ArrayStart
        ));
        assert!(matches!(
            scanner.next_token(input).unwrap().token,
            Token::ArrayEnd
        ));
        assert!(matches!(
            scanner.next_token(input).unwrap().token,
            Token::Comma
        ));
        assert!(matches!(
            scanner.next_token(input).unwrap().token,
            Token::Colon
        ));
        assert!(matches!(
            scanner.next_token(input).unwrap().token,
            Token::Eof
        ));
    }

    #[test]
    fn test_string_no_escapes() {
        let input = b"\"hello world\"";
        let mut scanner = Scanner::new();

        let result = scanner.next_token(input).unwrap();
        assert!(matches!(
            result.token,
            Token::String {
                start: 1,
                end: 12,
                has_escapes: false
            }
        ));
    }

    #[test]
    fn test_string_with_escapes() {
        let input = br#""hello\nworld""#;
        let mut scanner = Scanner::new();

        let result = scanner.next_token(input).unwrap();
        assert!(matches!(
            result.token,
            Token::String {
                start: 1,
                end: 13,
                has_escapes: true
            }
        ));
    }

    #[test]
    fn test_numbers() {
        let mut scanner = Scanner::new();

        // Unsigned (with terminator so scanner knows number is complete)
        let result = scanner.next_token(b"42,").unwrap();
        assert!(matches!(
            result.token,
            Token::Number {
                hint: NumberHint::Unsigned,
                ..
            }
        ));

        // Signed
        scanner.set_pos(0);
        let result = scanner.next_token(b"-42]").unwrap();
        assert!(matches!(
            result.token,
            Token::Number {
                hint: NumberHint::Signed,
                ..
            }
        ));

        // Float
        scanner.set_pos(0);
        let result = scanner.next_token(b"3.14}").unwrap();
        assert!(matches!(
            result.token,
            Token::Number {
                hint: NumberHint::Float,
                ..
            }
        ));

        // Exponent
        scanner.set_pos(0);
        let result = scanner.next_token(b"1e10 ").unwrap();
        assert!(matches!(
            result.token,
            Token::Number {
                hint: NumberHint::Float,
                ..
            }
        ));

        // Number at end of buffer returns NeedMore (streaming behavior)
        scanner.set_pos(0);
        let result = scanner.next_token(b"42").unwrap();
        assert!(matches!(result.token, Token::NeedMore { .. }));
    }

    #[test]
    fn test_literals() {
        let mut scanner = Scanner::new();

        // Literals need terminators too (scanner can't know if "truex" is coming)
        let result = scanner.next_token(b"true,").unwrap();
        assert!(matches!(result.token, Token::True));

        scanner.set_pos(0);
        let result = scanner.next_token(b"false]").unwrap();
        assert!(matches!(result.token, Token::False));

        scanner.set_pos(0);
        let result = scanner.next_token(b"null}").unwrap();
        assert!(matches!(result.token, Token::Null));
    }

    #[test]
    fn test_whitespace_handling() {
        let input = b"  {\n\t\"key\"  :  42  }  ";
        let mut scanner = Scanner::new();

        assert!(matches!(
            scanner.next_token(input).unwrap().token,
            Token::ObjectStart
        ));
        assert!(matches!(
            scanner.next_token(input).unwrap().token,
            Token::String { .. }
        ));
        assert!(matches!(
            scanner.next_token(input).unwrap().token,
            Token::Colon
        ));
        assert!(matches!(
            scanner.next_token(input).unwrap().token,
            Token::Number { .. }
        ));
        assert!(matches!(
            scanner.next_token(input).unwrap().token,
            Token::ObjectEnd
        ));
        assert!(matches!(
            scanner.next_token(input).unwrap().token,
            Token::Eof
        ));
    }

    #[test]
    fn test_decode_string_no_escapes() {
        let input = b"hello world";
        let result = decode_string_borrowed(input, 0, input.len());
        assert_eq!(result, Some("hello world"));
    }

    #[test]
    fn test_decode_string_with_escapes() {
        let input = br#"hello\nworld"#;
        let result = decode_string_owned(input, 0, input.len()).unwrap();
        assert_eq!(result, "hello\nworld");
    }

    #[test]
    fn test_decode_string_unicode() {
        // \u0048 = 'H', \u0065 = 'e', \u006C = 'l', \u006C = 'l', \u006F = 'o'
        let input = br#"\u0048\u0065\u006C\u006C\u006F"#;
        let result = decode_string_owned(input, 0, input.len()).unwrap();
        assert_eq!(result, "Hello");
    }

    #[test]
    fn test_decode_string_surrogate_pair() {
        // U+1F600 (grinning face) = \uD83D\uDE00
        let input = br#"\uD83D\uDE00"#;
        let result = decode_string_owned(input, 0, input.len()).unwrap();
        assert_eq!(result, "😀");
    }

    #[test]
    fn test_decode_cow_borrowed() {
        let input = b"simple";
        let result = decode_string(input, 0, input.len(), false).unwrap();
        assert!(matches!(result, alloc::borrow::Cow::Borrowed(_)));
        assert_eq!(&*result, "simple");
    }

    #[test]
    fn test_decode_cow_owned() {
        let input = br#"has\tescape"#;
        let result = decode_string(input, 0, input.len(), true).unwrap();
        assert!(matches!(result, alloc::borrow::Cow::Owned(_)));
        assert_eq!(&*result, "has\tescape");
    }

    #[test]
    fn test_parse_numbers() {
        assert_eq!(
            parse_number(b"42", 0, 2, NumberHint::Unsigned).unwrap(),
            ParsedNumber::U64(42)
        );
        assert_eq!(
            parse_number(b"-42", 0, 3, NumberHint::Signed).unwrap(),
            ParsedNumber::I64(-42)
        );
        #[allow(clippy::approx_constant)]
        {
            assert_eq!(
                parse_number(b"3.14", 0, 4, NumberHint::Float).unwrap(),
                ParsedNumber::F64(3.14)
            );
        }
    }
}
