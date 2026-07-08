//! Low-level JSON scanner that finds token boundaries without materializing strings.
//!
//! The scanner's job is to identify where tokens are in a buffer, not to interpret them.
//! String content is returned as indices + a `has_escapes` flag. The deserializer
//! decides whether to decode escapes based on the target type.
//!
//! This design enables:
//! - Zero-copy borrowed strings (when no escapes)
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
#[non_exhaustive]
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
/// The scanner operates on a complete byte buffer and tracks position.
#[derive(Clone)]
pub struct Scanner {
    /// Current position in the buffer
    pos: usize,
    /// Whether to allow JSONC-style comments (`//` and `/* */`)
    allow_comments: bool,
}

impl Scanner {
    /// Create a new scanner starting at position 0
    pub const fn new() -> Self {
        Self {
            pos: 0,
            allow_comments: false,
        }
    }

    /// Create a new scanner that accepts JSONC-style comments (`//` and `/* */`).
    pub const fn new_with_comments() -> Self {
        Self {
            pos: 0,
            allow_comments: true,
        }
    }

    /// Create a scanner starting at a specific position
    #[allow(dead_code)]
    pub const fn at_position(pos: usize) -> Self {
        Self {
            pos,
            allow_comments: false,
        }
    }

    /// Current position in the buffer
    pub const fn pos(&self) -> usize {
        self.pos
    }

    /// Whether JSONC-style comments are accepted.
    pub const fn allows_comments(&self) -> bool {
        self.allow_comments
    }

    /// Set position (used after buffer operations)
    #[allow(dead_code)]
    pub const fn set_pos(&mut self, pos: usize) {
        self.pos = pos;
    }

    /// Return the next non-whitespace/comment byte without changing scanner
    /// state.
    pub fn peek_significant_byte(&self, buf: &[u8]) -> Result<Option<(usize, u8)>, ScanError> {
        self.peek_significant_byte_from(buf, self.pos)
    }

    /// Return the next non-whitespace/comment byte at or after `pos` without
    /// changing scanner state.
    pub fn peek_significant_byte_from(
        &self,
        buf: &[u8],
        pos: usize,
    ) -> Result<Option<(usize, u8)>, ScanError> {
        let mut scanner = self.clone();
        scanner.pos = pos;
        scanner.skip_whitespace_if_needed(buf)?;
        Ok(buf
            .get(scanner.pos)
            .copied()
            .map(|byte| (scanner.pos, byte)))
    }

    /// Consume one expected punctuation byte after whitespace/comments.
    pub fn consume_punctuation(
        &mut self,
        buf: &[u8],
        expected: u8,
    ) -> Result<Option<Span>, ScanError> {
        self.skip_whitespace_if_needed(buf)?;

        let start = self.pos;
        if buf.get(self.pos) == Some(&expected) {
            self.pos += 1;
            Ok(Some(Span::new(start, 1)))
        } else {
            Ok(None)
        }
    }

    pub fn try_consume_exact_string(
        &mut self,
        buf: &[u8],
        expected: &[u8],
    ) -> Result<Option<Span>, ScanError> {
        if !can_match_unescaped_string(expected) {
            return Ok(None);
        }

        let original = self.pos;
        self.skip_whitespace_if_needed(buf)?;

        let start = self.pos;
        if buf.get(self.pos) != Some(&b'"') {
            self.pos = original;
            return Ok(None);
        }
        self.pos += 1;

        if !bytes_match_at(buf, self.pos, expected) {
            self.pos = original;
            return Ok(None);
        }
        self.pos += expected.len();

        if buf.get(self.pos) != Some(&b'"') {
            self.pos = original;
            return Ok(None);
        }
        self.pos += 1;

        Ok(Some(Span::new(start, self.pos - start)))
    }

    #[inline]
    pub fn try_consume_exact_field_name_colon(
        &mut self,
        buf: &[u8],
        expected: &[u8],
        require_comma: bool,
    ) -> Result<Option<Span>, ScanError> {
        if !can_match_unescaped_string(expected) {
            return Ok(None);
        }

        let original = self.pos;
        self.skip_whitespace_if_needed(buf)?;

        if require_comma {
            if buf.get(self.pos) != Some(&b',') {
                self.pos = original;
                return Ok(None);
            }
            self.pos += 1;
            self.skip_whitespace_if_needed(buf)?;
        }

        let start = self.pos;
        if buf.get(self.pos) != Some(&b'"') {
            self.pos = original;
            return Ok(None);
        }
        self.pos += 1;

        if !bytes_match_at(buf, self.pos, expected) {
            self.pos = original;
            return Ok(None);
        }
        self.pos += expected.len();

        if buf.get(self.pos) != Some(&b'"') {
            self.pos = original;
            return Ok(None);
        }
        self.pos += 1;
        let span = Span::new(start, self.pos - start);

        self.skip_whitespace_if_needed(buf)?;
        if buf.get(self.pos) != Some(&b':') {
            self.pos = original;
            return Ok(None);
        }
        self.pos += 1;

        Ok(Some(span))
    }

    #[inline]
    pub fn try_consume_one_byte_field_name_colon(
        &mut self,
        buf: &[u8],
        expected: u8,
        require_comma: bool,
    ) -> Result<Option<Span>, ScanError> {
        if matches!(expected, b'"' | b'\\' | 0x00..=0x1f) {
            return Ok(None);
        }

        let original = self.pos;
        self.skip_whitespace_if_needed(buf)?;

        if require_comma {
            if buf.get(self.pos) != Some(&b',') {
                self.pos = original;
                return Ok(None);
            }
            self.pos += 1;
            self.skip_whitespace_if_needed(buf)?;
        }

        let start = self.pos;
        if !matches!(
            (buf.get(self.pos), buf.get(self.pos + 1), buf.get(self.pos + 2)),
            (Some(b'"'), Some(byte), Some(b'"')) if *byte == expected
        ) {
            self.pos = original;
            return Ok(None);
        }
        self.pos += 3;
        let span = Span::new(start, 3);

        self.skip_whitespace_if_needed(buf)?;
        if buf.get(self.pos) != Some(&b':') {
            self.pos = original;
            return Ok(None);
        }
        self.pos += 1;

        Ok(Some(span))
    }

    #[cfg(all(
        facet_json_jit_active,
        any(
            all(target_os = "macos", target_arch = "aarch64"),
            all(target_os = "linux", target_arch = "x86_64")
        )
    ))]
    #[inline]
    pub fn try_consume_one_byte_field_name_colon_f64(
        &mut self,
        buf: &[u8],
        expected: u8,
        require_comma: bool,
    ) -> Result<Option<(Span, f64)>, ScanError> {
        if matches!(expected, b'"' | b'\\' | 0x00..=0x1f) {
            return Ok(None);
        }

        let original = self.pos;
        self.skip_whitespace_if_needed(buf)?;

        if require_comma {
            if buf.get(self.pos) != Some(&b',') {
                self.pos = original;
                return Ok(None);
            }
            self.pos += 1;
            self.skip_whitespace_if_needed(buf)?;
        }

        if !matches!(
            (buf.get(self.pos), buf.get(self.pos + 1), buf.get(self.pos + 2)),
            (Some(b'"'), Some(byte), Some(b'"')) if *byte == expected
        ) {
            self.pos = original;
            return Ok(None);
        }
        self.pos += 3;

        self.skip_whitespace_if_needed(buf)?;
        if buf.get(self.pos) != Some(&b':') {
            self.pos = original;
            return Ok(None);
        }
        self.pos += 1;

        self.try_consume_f64_number_with_rollback(buf, original)
    }

    #[cfg(all(
        facet_json_jit_active,
        any(
            all(target_os = "macos", target_arch = "aarch64"),
            all(target_os = "linux", target_arch = "x86_64")
        )
    ))]
    #[inline]
    pub fn try_consume_array_object_start(
        &mut self,
        buf: &[u8],
        require_comma: bool,
    ) -> Result<Option<Span>, ScanError> {
        let original = self.pos;
        self.skip_whitespace_if_needed(buf)?;

        if require_comma {
            if buf.get(self.pos) != Some(&b',') {
                self.pos = original;
                return Ok(None);
            }
            self.pos += 1;
            self.skip_whitespace_if_needed(buf)?;
        }

        let start = self.pos;
        if buf.get(self.pos) != Some(&b'{') {
            self.pos = original;
            return Ok(None);
        }
        self.pos += 1;

        Ok(Some(Span::new(start, 1)))
    }

    #[inline]
    pub fn try_consume_i32_number(&mut self, buf: &[u8]) -> Result<Option<(Span, i32)>, ScanError> {
        let original = self.pos;
        self.skip_whitespace_if_needed(buf)?;

        let start = self.pos;
        let Some(&first) = buf.get(self.pos) else {
            self.pos = original;
            return Ok(None);
        };
        let negative = first == b'-';
        if negative {
            self.pos += 1;
        } else if !first.is_ascii_digit() {
            self.pos = original;
            return Ok(None);
        }

        let digits_start = self.pos;
        let mut value = 0i64;
        while let Some(&byte) = buf.get(self.pos) {
            if !byte.is_ascii_digit() {
                break;
            }
            value = match value
                .checked_mul(10)
                .and_then(|value| value.checked_add((byte - b'0') as i64))
            {
                Some(value) => value,
                None => {
                    self.pos = original;
                    return Ok(None);
                }
            };
            self.pos += 1;
        }

        if self.pos == digits_start {
            self.pos = original;
            return Ok(None);
        }

        if matches!(buf.get(self.pos), Some(b'.' | b'e' | b'E')) {
            self.pos = original;
            return Ok(None);
        }

        let value = if negative { -value } else { value };
        let Ok(value) = i32::try_from(value) else {
            self.pos = original;
            return Ok(None);
        };

        Ok(Some((Span::new(start, self.pos - start), value)))
    }

    #[cfg(all(
        facet_json_jit_active,
        any(
            all(target_os = "macos", target_arch = "aarch64"),
            all(target_os = "linux", target_arch = "x86_64")
        )
    ))]
    #[inline]
    pub fn try_consume_bool(&mut self, buf: &[u8]) -> Result<Option<(Span, bool)>, ScanError> {
        let original = self.pos;
        self.skip_whitespace_if_needed(buf)?;

        let start = self.pos;
        if bytes_match_at(buf, self.pos, b"true") {
            self.pos += 4;
            return Ok(Some((Span::new(start, 4), true)));
        }
        if bytes_match_at(buf, self.pos, b"false") {
            self.pos += 5;
            return Ok(Some((Span::new(start, 5), false)));
        }

        self.pos = original;
        Ok(None)
    }

    #[cfg(all(
        facet_json_jit_active,
        any(
            all(target_os = "macos", target_arch = "aarch64"),
            all(target_os = "linux", target_arch = "x86_64")
        )
    ))]
    #[inline]
    pub fn try_consume_null(&mut self, buf: &[u8]) -> Result<Option<Span>, ScanError> {
        let original = self.pos;
        self.skip_whitespace_if_needed(buf)?;

        let start = self.pos;
        if bytes_match_at(buf, self.pos, b"null") {
            self.pos += 4;
            return Ok(Some(Span::new(start, 4)));
        }

        self.pos = original;
        Ok(None)
    }

    #[cfg(all(
        facet_json_jit_active,
        any(
            all(target_os = "macos", target_arch = "aarch64"),
            all(target_os = "linux", target_arch = "x86_64")
        )
    ))]
    #[inline]
    pub fn try_consume_unsigned_integer<T>(
        &mut self,
        buf: &[u8],
    ) -> Result<Option<(Span, T)>, ScanError>
    where
        T: TryFrom<u128>,
    {
        let original = self.pos;
        self.skip_whitespace_if_needed(buf)?;

        let start = self.pos;
        let Some(&first) = buf.get(self.pos) else {
            self.pos = original;
            return Ok(None);
        };
        if !first.is_ascii_digit() {
            self.pos = original;
            return Ok(None);
        }

        let mut value = 0u128;
        while let Some(&byte) = buf.get(self.pos) {
            if !byte.is_ascii_digit() {
                break;
            }
            value = match value
                .checked_mul(10)
                .and_then(|value| value.checked_add((byte - b'0') as u128))
            {
                Some(value) => value,
                None => {
                    self.pos = original;
                    return Ok(None);
                }
            };
            self.pos += 1;
        }

        if matches!(buf.get(self.pos), Some(b'.' | b'e' | b'E')) {
            self.pos = original;
            return Ok(None);
        }

        let Ok(value) = T::try_from(value) else {
            self.pos = original;
            return Ok(None);
        };

        Ok(Some((Span::new(start, self.pos - start), value)))
    }

    #[cfg(all(
        facet_json_jit_active,
        any(
            all(target_os = "macos", target_arch = "aarch64"),
            all(target_os = "linux", target_arch = "x86_64")
        )
    ))]
    #[inline]
    pub fn try_consume_signed_integer<T>(
        &mut self,
        buf: &[u8],
    ) -> Result<Option<(Span, T)>, ScanError>
    where
        T: TryFrom<i128>,
    {
        let original = self.pos;
        self.skip_whitespace_if_needed(buf)?;

        let start = self.pos;
        let Some(&first) = buf.get(self.pos) else {
            self.pos = original;
            return Ok(None);
        };
        let negative = first == b'-';
        if negative {
            self.pos += 1;
        } else if !first.is_ascii_digit() {
            self.pos = original;
            return Ok(None);
        }

        let digits_start = self.pos;
        let mut value = 0i128;
        while let Some(&byte) = buf.get(self.pos) {
            if !byte.is_ascii_digit() {
                break;
            }
            value = if negative {
                match value
                    .checked_mul(10)
                    .and_then(|value| value.checked_sub((byte - b'0') as i128))
                {
                    Some(value) => value,
                    None => {
                        self.pos = original;
                        return Ok(None);
                    }
                }
            } else {
                match value
                    .checked_mul(10)
                    .and_then(|value| value.checked_add((byte - b'0') as i128))
                {
                    Some(value) => value,
                    None => {
                        self.pos = original;
                        return Ok(None);
                    }
                }
            };
            self.pos += 1;
        }

        if self.pos == digits_start || matches!(buf.get(self.pos), Some(b'.' | b'e' | b'E')) {
            self.pos = original;
            return Ok(None);
        }

        let Ok(value) = T::try_from(value) else {
            self.pos = original;
            return Ok(None);
        };

        Ok(Some((Span::new(start, self.pos - start), value)))
    }

    #[cfg(all(
        facet_json_jit_active,
        any(
            all(target_os = "macos", target_arch = "aarch64"),
            all(target_os = "linux", target_arch = "x86_64")
        )
    ))]
    #[inline]
    pub fn try_consume_f64_number(&mut self, buf: &[u8]) -> Result<Option<(Span, f64)>, ScanError> {
        let original = self.pos;
        self.try_consume_f64_number_with_rollback(buf, original)
    }

    #[cfg(all(
        facet_json_jit_active,
        any(
            all(target_os = "macos", target_arch = "aarch64"),
            all(target_os = "linux", target_arch = "x86_64")
        )
    ))]
    #[inline(always)]
    fn try_consume_f64_number_with_rollback(
        &mut self,
        buf: &[u8],
        original: usize,
    ) -> Result<Option<(Span, f64)>, ScanError> {
        self.skip_whitespace_if_needed(buf)?;

        let start = self.pos;
        if !matches!(buf.get(self.pos), Some(b'-' | b'0'..=b'9')) {
            self.pos = original;
            return Ok(None);
        }

        let (end, hint) = match self.scan_number_bounds(buf, start) {
            Ok(bounds) => bounds,
            Err(_) => {
                self.pos = original;
                return Ok(None);
            }
        };

        let value = if hint == NumberHint::Float {
            match parse_f64(buf, start, end) {
                Ok(value) => value,
                Err(_) => {
                    self.pos = original;
                    return Ok(None);
                }
            }
        } else {
            match parse_number(buf, start, end, hint) {
                Ok(ParsedNumber::F64(value)) => value,
                Ok(ParsedNumber::I64(value)) => value as f64,
                Ok(ParsedNumber::U64(value)) => value as f64,
                Ok(ParsedNumber::I128(value)) => value as f64,
                Ok(ParsedNumber::U128(value)) => value as f64,
                Err(_) => {
                    self.pos = original;
                    return Ok(None);
                }
            }
        };

        Ok(Some((Span::new(start, end - start), value)))
    }

    /// Scan the next token from the buffer.
    pub fn next_token(&mut self, buf: &[u8]) -> ScanResult {
        self.skip_whitespace_if_needed(buf)?;

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

    fn skip_whitespace(&mut self, buf: &[u8]) -> Result<(), ScanError> {
        let mut pos = self.pos;
        loop {
            // Skip standard JSON whitespace.
            while let Some(&b) = buf.get(pos) {
                match b {
                    b' ' | b'\t' | b'\n' | b'\r' => pos += 1,
                    _ => break,
                }
            }

            if !self.allow_comments {
                break;
            }

            // Check for a comment introducer.
            match (buf.get(pos), buf.get(pos + 1)) {
                (Some(&b'/'), Some(&b'/')) => {
                    // Line comment: skip everything up to and including the newline.
                    pos += 2;
                    while let Some(&b) = buf.get(pos) {
                        pos += 1;
                        if b == b'\n' {
                            break;
                        }
                    }
                    // Loop back to consume any whitespace/comments that follow.
                }
                (Some(&b'/'), Some(&b'*')) => {
                    // Block comment: skip until the closing `*/`.
                    let comment_start = pos;
                    pos += 2;
                    loop {
                        match buf.get(pos) {
                            Some(&b'*') if buf.get(pos + 1) == Some(&b'/') => {
                                pos += 2;
                                break;
                            }
                            Some(_) => pos += 1,
                            None => {
                                self.pos = pos;
                                return Err(ScanError {
                                    kind: ScanErrorKind::UnexpectedEof("in block comment"),
                                    span: Span::new(comment_start, pos - comment_start),
                                });
                            }
                        }
                    }
                    // Loop back to consume any whitespace/comments that follow.
                }
                _ => break,
            }
        }
        self.pos = pos;
        Ok(())
    }

    #[inline]
    fn skip_whitespace_if_needed(&mut self, buf: &[u8]) -> Result<(), ScanError> {
        match buf.get(self.pos) {
            Some(b' ' | b'\t' | b'\n' | b'\r') => self.skip_whitespace(buf),
            Some(b'/') if self.allow_comments => self.skip_whitespace(buf),
            _ => Ok(()),
        }
    }

    /// Scan a string, finding its boundaries and noting if it has escapes.
    fn scan_string(&mut self, buf: &[u8], start: usize) -> ScanResult {
        // Skip opening quote
        self.pos += 1;
        let content_start = self.pos;

        self.scan_string_content(buf, start, content_start, false, false)
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
                        return Err(ScanError {
                            kind: ScanErrorKind::UnexpectedEof("in unicode escape"),
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
                            return Err(ScanError {
                                kind: ScanErrorKind::UnexpectedEof("in unicode escape"),
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
        Err(ScanError {
            kind: ScanErrorKind::UnexpectedEof("in string"),
            span: Span::new(start, self.pos - start),
        })
    }

    /// Scan a number, finding its boundaries and determining its type hint.
    fn scan_number(&mut self, buf: &[u8], start: usize) -> ScanResult {
        let (end, hint) = self.scan_number_bounds(buf, start)?;

        Ok(SpannedToken {
            token: Token::Number { start, end, hint },
            span: Span::new(start, end - start),
        })
    }

    #[inline]
    fn scan_number_bounds(
        &mut self,
        buf: &[u8],
        start: usize,
    ) -> Result<(usize, NumberHint), ScanError> {
        let mut hint = NumberHint::Unsigned;

        if buf.get(self.pos) == Some(&b'-') {
            hint = NumberHint::Signed;
            self.pos += 1;
        }

        self.scan_number_content_bounds(buf, start, hint)
    }

    #[inline]
    fn scan_number_content_bounds(
        &mut self,
        buf: &[u8],
        start: usize,
        mut hint: NumberHint,
    ) -> Result<(usize, NumberHint), ScanError> {
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

        Ok((end, hint))
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
                    return Err(ScanError {
                        kind: ScanErrorKind::UnexpectedEof("in literal"),
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

fn can_match_unescaped_string(expected: &[u8]) -> bool {
    !expected
        .iter()
        .any(|byte| matches!(*byte, b'"' | b'\\' | 0x00..=0x1f))
}

#[inline]
fn bytes_match_at(buf: &[u8], pos: usize, expected: &[u8]) -> bool {
    match expected {
        [] => true,
        [a] => buf.get(pos) == Some(a),
        [a, b] => {
            matches!((buf.get(pos), buf.get(pos + 1)), (Some(x), Some(y)) if x == a && y == b)
        }
        [a, b, c] => {
            matches!((buf.get(pos), buf.get(pos + 1), buf.get(pos + 2)), (Some(x), Some(y), Some(z)) if x == a && y == b && z == c)
        }
        [a, b, c, d] => {
            matches!((buf.get(pos), buf.get(pos + 1), buf.get(pos + 2), buf.get(pos + 3)), (Some(w), Some(x), Some(y), Some(z)) if w == a && x == b && y == c && z == d)
        }
        _ => {
            let Some(end) = pos.checked_add(expected.len()) else {
                return false;
            };
            buf.get(pos..end) == Some(expected)
        }
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

/// Decode a JSON string with escape sequences without UTF-8 validation.
///
/// # Safety
/// The caller must ensure the buffer contains valid UTF-8.
pub unsafe fn decode_string_owned_unchecked(
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
                    // SAFETY: Caller guarantees valid UTF-8, hex digits are ASCII
                    let hex_str = unsafe { str::from_utf8_unchecked(hex) };

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
                        // SAFETY: Caller guarantees valid UTF-8, hex digits are ASCII
                        let low_hex_str = unsafe { str::from_utf8_unchecked(low_hex) };

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
                // Multi-byte UTF-8 sequence
                // SAFETY: Caller guarantees valid UTF-8
                let remaining = &slice[i..];
                let s = unsafe { str::from_utf8_unchecked(remaining) };
                let ch = s.chars().next().expect("non-empty remaining slice");
                result.push(ch);
                i += ch.len_utf8();
            }
        }
    }

    Ok(result)
}

/// Decode a JSON string, returning either a borrowed or owned string.
///
/// Uses `Cow<str>` to avoid allocation when possible.
#[allow(dead_code)]
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
#[allow(dead_code)]
pub unsafe fn decode_string_unchecked<'a>(
    buf: &'a [u8],
    start: usize,
    end: usize,
    has_escapes: bool,
) -> Result<alloc::borrow::Cow<'a, str>, ScanError> {
    use alloc::borrow::Cow;

    if has_escapes {
        // SAFETY: Caller guarantees buffer is valid UTF-8
        unsafe { decode_string_owned_unchecked(buf, start, end) }.map(Cow::Owned)
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
    use lexical_parse_integer::FromLexical as _;

    let slice = &buf[start..end];

    match hint {
        NumberHint::Float => parse_f64(buf, start, end).map(ParsedNumber::F64),
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

#[cfg(feature = "lexical-parse")]
#[inline]
fn parse_f64(buf: &[u8], start: usize, end: usize) -> Result<f64, ScanError> {
    use lexical_parse_float::FromLexical as _;

    f64::from_lexical(&buf[start..end]).map_err(|_| ScanError {
        kind: ScanErrorKind::UnexpectedChar('?'),
        span: Span::new(start, end - start),
    })
}

/// Parse a number from the buffer slice, skipping UTF-8 validation.
///
/// # Safety
/// The caller must ensure that `buf[start..end]` contains valid UTF-8.
/// For lexical-parse, this is a no-op since it works on bytes directly.
#[cfg(feature = "lexical-parse")]
pub unsafe fn parse_number_unchecked(
    buf: &[u8],
    start: usize,
    end: usize,
    hint: NumberHint,
) -> Result<ParsedNumber, ScanError> {
    // lexical-parse works on bytes, no UTF-8 validation needed
    parse_number(buf, start, end, hint)
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

    parse_number_inner(s, start, end, hint)
}

/// Parse a number from the buffer slice, skipping UTF-8 validation.
///
/// # Safety
/// The caller must ensure that `buf[start..end]` contains valid UTF-8.
/// This is guaranteed when the input came from `&str` (TRUSTED_UTF8=true).
#[cfg(not(feature = "lexical-parse"))]
pub unsafe fn parse_number_unchecked(
    buf: &[u8],
    start: usize,
    end: usize,
    hint: NumberHint,
) -> Result<ParsedNumber, ScanError> {
    let slice = &buf[start..end];
    // SAFETY: Caller guarantees the buffer is valid UTF-8
    let s = unsafe { str::from_utf8_unchecked(slice) };

    parse_number_inner(s, start, end, hint)
}

#[cfg(not(feature = "lexical-parse"))]
fn parse_number_inner(
    s: &str,
    start: usize,
    end: usize,
    hint: NumberHint,
) -> Result<ParsedNumber, ScanError> {
    match hint {
        NumberHint::Float => parse_f64_inner(s, start, end).map(ParsedNumber::F64),
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

#[cfg(all(
    not(feature = "lexical-parse"),
    facet_json_jit_active,
    any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    )
))]
#[inline]
fn parse_f64(buf: &[u8], start: usize, end: usize) -> Result<f64, ScanError> {
    let slice = &buf[start..end];
    let s = str::from_utf8(slice).map_err(|_| ScanError {
        kind: ScanErrorKind::InvalidUtf8,
        span: Span::new(start, end - start),
    })?;

    parse_f64_inner(s, start, end)
}

#[cfg(not(feature = "lexical-parse"))]
#[inline]
fn parse_f64_inner(s: &str, start: usize, end: usize) -> Result<f64, ScanError> {
    s.parse::<f64>().map_err(|_| ScanError {
        kind: ScanErrorKind::UnexpectedChar('?'),
        span: Span::new(start, end - start),
    })
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

        // Number at end of buffer is complete in the complete-buffer parser.
        scanner.set_pos(0);
        let result = scanner.next_token(b"42").unwrap();
        assert!(matches!(
            result.token,
            Token::Number {
                hint: NumberHint::Unsigned,
                ..
            }
        ));
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

    #[cfg(all(
        facet_json_jit_active,
        any(
            all(target_os = "macos", target_arch = "aarch64"),
            all(target_os = "linux", target_arch = "x86_64")
        )
    ))]
    #[test]
    fn try_consume_f64_number_direct_path() {
        let mut scanner = Scanner::new();
        let (span, value) = scanner
            .try_consume_f64_number(b"  -0.03125,")
            .unwrap()
            .unwrap();
        assert_eq!(span, Span::new(2, 8));
        assert_eq!(value, -0.03125);

        let mut scanner = Scanner::new();
        let (span, value) = scanner.try_consume_f64_number(b"9000}").unwrap().unwrap();
        assert_eq!(span, Span::new(0, 4));
        assert_eq!(value, 9000.0);
    }

    #[cfg(all(
        facet_json_jit_active,
        any(
            all(target_os = "macos", target_arch = "aarch64"),
            all(target_os = "linux", target_arch = "x86_64")
        )
    ))]
    #[test]
    fn try_consume_f64_number_rolls_back_on_invalid_float() {
        let mut scanner = Scanner::new();
        assert!(scanner.try_consume_f64_number(b"1e,").unwrap().is_none());
        assert_eq!(scanner.pos(), 0);
    }

    #[cfg(all(
        facet_json_jit_active,
        any(
            all(target_os = "macos", target_arch = "aarch64"),
            all(target_os = "linux", target_arch = "x86_64")
        )
    ))]
    #[test]
    fn try_consume_one_byte_field_name_colon_f64_rolls_back_together() {
        let mut scanner = Scanner::new();
        let (span, value) = scanner
            .try_consume_one_byte_field_name_colon_f64(br#" "x": -1.25,"#, b'x', false)
            .unwrap()
            .unwrap();
        assert_eq!(span, Span::new(6, 5));
        assert_eq!(value, -1.25);

        let mut scanner = Scanner::new();
        assert!(
            scanner
                .try_consume_one_byte_field_name_colon_f64(br#" "x": "1.25""#, b'x', false)
                .unwrap()
                .is_none()
        );
        assert_eq!(scanner.pos(), 0);
    }
}
