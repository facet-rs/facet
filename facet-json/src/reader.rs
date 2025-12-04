//! High-level streaming JSON reader.
//!
//! `JsonReader` wraps `ScanBuffer` and `Scanner` to provide a convenient
//! streaming token iterator that handles buffer management automatically.
//!
//! # Design: No Compaction
//!
//! This reader uses a simple grow-or-reset strategy:
//! - When scanner returns a complete token: materialize it, continue
//! - When scanner returns Eof (end of buffer): reset buffer, refill, continue
//! - When scanner returns NeedMore (mid-token): grow buffer if full, refill, continue
//!
//! We never compact (shift data left) because:
//! - Reset handles the "all data processed" case
//! - Grow handles the "mid-token" case
//! - Scanner indices remain stable (no adjustment needed)

use alloc::string::String;

use crate::scan_buffer::ScanBuffer;
use crate::scanner::{
    ParsedNumber, ScanError, ScanErrorKind, Scanner, SpannedToken, Token as ScanToken,
    decode_string_owned, parse_number,
};
use facet_reflect::Span;

/// Error from JSON reader operations
#[derive(Debug)]
pub enum ReaderError {
    /// IO error during refill
    Io(std::io::Error),
    /// Scanner error
    Scan(ScanError),
}

impl From<std::io::Error> for ReaderError {
    fn from(err: std::io::Error) -> Self {
        ReaderError::Io(err)
    }
}

impl From<ScanError> for ReaderError {
    fn from(err: ScanError) -> Self {
        ReaderError::Scan(err)
    }
}

impl core::fmt::Display for ReaderError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ReaderError::Io(e) => write!(f, "IO error: {e}"),
            ReaderError::Scan(e) => write!(f, "scan error: {:?}", e.kind),
        }
    }
}

impl std::error::Error for ReaderError {}

/// A materialized JSON token with its value decoded.
#[derive(Debug, Clone, PartialEq)]
pub enum JsonToken {
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
    /// String value (decoded, with escapes processed)
    String(String),
    /// Unsigned integer
    U64(u64),
    /// Signed integer
    I64(i64),
    /// Unsigned 128-bit integer
    U128(u128),
    /// Signed 128-bit integer
    I128(i128),
    /// Floating point number
    F64(f64),
    /// End of input
    Eof,
}

/// Spanned JSON token with location information
#[derive(Debug, Clone)]
pub struct SpannedJsonToken {
    /// The token
    pub token: JsonToken,
    /// Source span (byte offset from start of input)
    pub span: Span,
}

/// Streaming JSON reader that handles buffer management automatically.
#[cfg(feature = "std")]
pub struct JsonReader<R> {
    reader: R,
    buffer: ScanBuffer,
    scanner: Scanner,
    /// Total bytes processed (for span calculation across refills)
    bytes_processed: usize,
}

#[cfg(feature = "std")]
impl<R: std::io::Read> JsonReader<R> {
    /// Create a new streaming JSON reader.
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            buffer: ScanBuffer::new(),
            scanner: Scanner::new(),
            bytes_processed: 0,
        }
    }

    /// Create a new streaming JSON reader with custom buffer capacity.
    pub fn with_capacity(reader: R, capacity: usize) -> Self {
        Self {
            reader,
            buffer: ScanBuffer::with_capacity(capacity),
            scanner: Scanner::new(),
            bytes_processed: 0,
        }
    }

    /// Read the next token from the stream.
    pub fn next_token(&mut self) -> Option<Result<SpannedJsonToken, ReaderError>> {
        loop {
            // Ensure we have data to scan
            if self.buffer.filled() == 0 && !self.buffer.is_eof() {
                match self.buffer.refill(&mut self.reader) {
                    Ok(0) => {
                        return Some(Ok(SpannedJsonToken {
                            token: JsonToken::Eof,
                            span: Span::new(self.bytes_processed, 0),
                        }));
                    }
                    Ok(_) => {}
                    Err(e) => return Some(Err(ReaderError::Io(e))),
                }
            }

            let result = self.scanner.next_token(self.buffer.data());

            match result {
                Ok(spanned) => {
                    match &spanned.token {
                        ScanToken::NeedMore { .. } => {
                            // Mid-token, need more data
                            // Grow buffer if it's full, then refill
                            if self.buffer.filled() == self.buffer.capacity() {
                                self.buffer.grow();
                            }

                            match self.buffer.refill(&mut self.reader) {
                                Ok(0) if self.buffer.is_eof() => {
                                    // True EOF while mid-token = error
                                    return Some(Err(ReaderError::Scan(ScanError {
                                        kind: ScanErrorKind::UnexpectedEof("incomplete token"),
                                        span: Span::new(self.bytes_processed, 0),
                                    })));
                                }
                                Ok(_) => continue,
                                Err(e) => return Some(Err(ReaderError::Io(e))),
                            }
                        }
                        ScanToken::Eof => {
                            // Scanner reached end of buffer
                            if !self.buffer.is_eof() {
                                // Not true EOF - all tokens processed, reset and refill
                                self.bytes_processed += self.scanner.pos();
                                self.buffer.reset();
                                self.scanner.set_pos(0);

                                match self.buffer.refill(&mut self.reader) {
                                    Ok(0) => {
                                        return Some(Ok(SpannedJsonToken {
                                            token: JsonToken::Eof,
                                            span: Span::new(self.bytes_processed, 0),
                                        }));
                                    }
                                    Ok(_) => continue,
                                    Err(e) => return Some(Err(ReaderError::Io(e))),
                                }
                            }
                            // True EOF
                            return Some(Ok(SpannedJsonToken {
                                token: JsonToken::Eof,
                                span: Span::new(self.bytes_processed + spanned.span.offset, 0),
                            }));
                        }
                        _ => {
                            // Complete token - materialize it
                            return Some(self.materialize_token(&spanned));
                        }
                    }
                }
                Err(e) => {
                    return Some(Err(ReaderError::Scan(ScanError {
                        kind: e.kind,
                        span: Span::new(self.bytes_processed + e.span.offset, e.span.len),
                    })));
                }
            }
        }
    }

    fn materialize_token(&self, spanned: &SpannedToken) -> Result<SpannedJsonToken, ReaderError> {
        let buf = self.buffer.data();
        let span = Span::new(self.bytes_processed + spanned.span.offset, spanned.span.len);

        let token = match &spanned.token {
            ScanToken::ObjectStart => JsonToken::ObjectStart,
            ScanToken::ObjectEnd => JsonToken::ObjectEnd,
            ScanToken::ArrayStart => JsonToken::ArrayStart,
            ScanToken::ArrayEnd => JsonToken::ArrayEnd,
            ScanToken::Colon => JsonToken::Colon,
            ScanToken::Comma => JsonToken::Comma,
            ScanToken::Null => JsonToken::Null,
            ScanToken::True => JsonToken::True,
            ScanToken::False => JsonToken::False,
            ScanToken::String { start, end, .. } => {
                let s = decode_string_owned(buf, *start, *end).map_err(ReaderError::Scan)?;
                JsonToken::String(s)
            }
            ScanToken::Number { start, end, hint } => {
                let parsed = parse_number(buf, *start, *end, *hint).map_err(ReaderError::Scan)?;
                match parsed {
                    ParsedNumber::U64(n) => JsonToken::U64(n),
                    ParsedNumber::I64(n) => JsonToken::I64(n),
                    ParsedNumber::U128(n) => JsonToken::U128(n),
                    ParsedNumber::I128(n) => JsonToken::I128(n),
                    ParsedNumber::F64(n) => JsonToken::F64(n),
                }
            }
            ScanToken::Eof | ScanToken::NeedMore { .. } => unreachable!(),
        };

        Ok(SpannedJsonToken { token, span })
    }
}

/// Async streaming JSON reader for tokio.
#[cfg(feature = "tokio")]
pub struct AsyncJsonReader<R> {
    reader: R,
    buffer: ScanBuffer,
    scanner: Scanner,
    bytes_processed: usize,
}

#[cfg(feature = "tokio")]
impl<R: tokio::io::AsyncRead + Unpin> AsyncJsonReader<R> {
    /// Create a new async streaming JSON reader.
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            buffer: ScanBuffer::new(),
            scanner: Scanner::new(),
            bytes_processed: 0,
        }
    }

    /// Create with custom buffer capacity.
    pub fn with_capacity(reader: R, capacity: usize) -> Self {
        Self {
            reader,
            buffer: ScanBuffer::with_capacity(capacity),
            scanner: Scanner::new(),
            bytes_processed: 0,
        }
    }

    /// Read the next token asynchronously.
    pub async fn next_token(&mut self) -> Option<Result<SpannedJsonToken, ReaderError>> {
        loop {
            if self.buffer.filled() == 0 && !self.buffer.is_eof() {
                match self.buffer.refill_tokio(&mut self.reader).await {
                    Ok(0) => {
                        return Some(Ok(SpannedJsonToken {
                            token: JsonToken::Eof,
                            span: Span::new(self.bytes_processed, 0),
                        }));
                    }
                    Ok(_) => {}
                    Err(e) => return Some(Err(ReaderError::Io(e))),
                }
            }

            let result = self.scanner.next_token(self.buffer.data());

            match result {
                Ok(spanned) => match &spanned.token {
                    ScanToken::NeedMore { .. } => {
                        if self.buffer.filled() == self.buffer.capacity() {
                            self.buffer.grow();
                        }
                        match self.buffer.refill_tokio(&mut self.reader).await {
                            Ok(0) if self.buffer.is_eof() => {
                                return Some(Err(ReaderError::Scan(ScanError {
                                    kind: ScanErrorKind::UnexpectedEof("incomplete token"),
                                    span: Span::new(self.bytes_processed, 0),
                                })));
                            }
                            Ok(_) => continue,
                            Err(e) => return Some(Err(ReaderError::Io(e))),
                        }
                    }
                    ScanToken::Eof => {
                        if !self.buffer.is_eof() {
                            self.bytes_processed += self.scanner.pos();
                            self.buffer.reset();
                            self.scanner.set_pos(0);
                            match self.buffer.refill_tokio(&mut self.reader).await {
                                Ok(0) => {
                                    return Some(Ok(SpannedJsonToken {
                                        token: JsonToken::Eof,
                                        span: Span::new(self.bytes_processed, 0),
                                    }));
                                }
                                Ok(_) => continue,
                                Err(e) => return Some(Err(ReaderError::Io(e))),
                            }
                        }
                        return Some(Ok(SpannedJsonToken {
                            token: JsonToken::Eof,
                            span: Span::new(self.bytes_processed + spanned.span.offset, 0),
                        }));
                    }
                    _ => {
                        return Some(self.materialize_token(&spanned));
                    }
                },
                Err(e) => {
                    return Some(Err(ReaderError::Scan(ScanError {
                        kind: e.kind,
                        span: Span::new(self.bytes_processed + e.span.offset, e.span.len),
                    })));
                }
            }
        }
    }

    fn materialize_token(&self, spanned: &SpannedToken) -> Result<SpannedJsonToken, ReaderError> {
        let buf = self.buffer.data();
        let span = Span::new(self.bytes_processed + spanned.span.offset, spanned.span.len);

        let token = match &spanned.token {
            ScanToken::ObjectStart => JsonToken::ObjectStart,
            ScanToken::ObjectEnd => JsonToken::ObjectEnd,
            ScanToken::ArrayStart => JsonToken::ArrayStart,
            ScanToken::ArrayEnd => JsonToken::ArrayEnd,
            ScanToken::Colon => JsonToken::Colon,
            ScanToken::Comma => JsonToken::Comma,
            ScanToken::Null => JsonToken::Null,
            ScanToken::True => JsonToken::True,
            ScanToken::False => JsonToken::False,
            ScanToken::String { start, end, .. } => {
                let s = decode_string_owned(buf, *start, *end).map_err(ReaderError::Scan)?;
                JsonToken::String(s)
            }
            ScanToken::Number { start, end, hint } => {
                let parsed = parse_number(buf, *start, *end, *hint).map_err(ReaderError::Scan)?;
                match parsed {
                    ParsedNumber::U64(n) => JsonToken::U64(n),
                    ParsedNumber::I64(n) => JsonToken::I64(n),
                    ParsedNumber::U128(n) => JsonToken::U128(n),
                    ParsedNumber::I128(n) => JsonToken::I128(n),
                    ParsedNumber::F64(n) => JsonToken::F64(n),
                }
            }
            ScanToken::Eof | ScanToken::NeedMore { .. } => unreachable!(),
        };

        Ok(SpannedJsonToken { token, span })
    }
}

/// Async streaming JSON reader for futures-io (smol, async-std).
#[cfg(feature = "futures-io")]
pub struct FuturesJsonReader<R> {
    reader: R,
    buffer: ScanBuffer,
    scanner: Scanner,
    bytes_processed: usize,
}

#[cfg(feature = "futures-io")]
impl<R: futures_io::AsyncRead + Unpin> FuturesJsonReader<R> {
    /// Create a new async streaming JSON reader.
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            buffer: ScanBuffer::new(),
            scanner: Scanner::new(),
            bytes_processed: 0,
        }
    }

    /// Create with custom buffer capacity.
    pub fn with_capacity(reader: R, capacity: usize) -> Self {
        Self {
            reader,
            buffer: ScanBuffer::with_capacity(capacity),
            scanner: Scanner::new(),
            bytes_processed: 0,
        }
    }

    /// Read the next token asynchronously.
    pub async fn next_token(&mut self) -> Option<Result<SpannedJsonToken, ReaderError>> {
        loop {
            if self.buffer.filled() == 0 && !self.buffer.is_eof() {
                match self.buffer.refill_futures(&mut self.reader).await {
                    Ok(0) => {
                        return Some(Ok(SpannedJsonToken {
                            token: JsonToken::Eof,
                            span: Span::new(self.bytes_processed, 0),
                        }));
                    }
                    Ok(_) => {}
                    Err(e) => return Some(Err(ReaderError::Io(e))),
                }
            }

            let result = self.scanner.next_token(self.buffer.data());

            match result {
                Ok(spanned) => match &spanned.token {
                    ScanToken::NeedMore { .. } => {
                        if self.buffer.filled() == self.buffer.capacity() {
                            self.buffer.grow();
                        }
                        match self.buffer.refill_futures(&mut self.reader).await {
                            Ok(0) if self.buffer.is_eof() => {
                                return Some(Err(ReaderError::Scan(ScanError {
                                    kind: ScanErrorKind::UnexpectedEof("incomplete token"),
                                    span: Span::new(self.bytes_processed, 0),
                                })));
                            }
                            Ok(_) => continue,
                            Err(e) => return Some(Err(ReaderError::Io(e))),
                        }
                    }
                    ScanToken::Eof => {
                        if !self.buffer.is_eof() {
                            self.bytes_processed += self.scanner.pos();
                            self.buffer.reset();
                            self.scanner.set_pos(0);
                            match self.buffer.refill_futures(&mut self.reader).await {
                                Ok(0) => {
                                    return Some(Ok(SpannedJsonToken {
                                        token: JsonToken::Eof,
                                        span: Span::new(self.bytes_processed, 0),
                                    }));
                                }
                                Ok(_) => continue,
                                Err(e) => return Some(Err(ReaderError::Io(e))),
                            }
                        }
                        return Some(Ok(SpannedJsonToken {
                            token: JsonToken::Eof,
                            span: Span::new(self.bytes_processed + spanned.span.offset, 0),
                        }));
                    }
                    _ => {
                        return Some(self.materialize_token(&spanned));
                    }
                },
                Err(e) => {
                    return Some(Err(ReaderError::Scan(ScanError {
                        kind: e.kind,
                        span: Span::new(self.bytes_processed + e.span.offset, e.span.len),
                    })));
                }
            }
        }
    }

    fn materialize_token(&self, spanned: &SpannedToken) -> Result<SpannedJsonToken, ReaderError> {
        let buf = self.buffer.data();
        let span = Span::new(self.bytes_processed + spanned.span.offset, spanned.span.len);

        let token = match &spanned.token {
            ScanToken::ObjectStart => JsonToken::ObjectStart,
            ScanToken::ObjectEnd => JsonToken::ObjectEnd,
            ScanToken::ArrayStart => JsonToken::ArrayStart,
            ScanToken::ArrayEnd => JsonToken::ArrayEnd,
            ScanToken::Colon => JsonToken::Colon,
            ScanToken::Comma => JsonToken::Comma,
            ScanToken::Null => JsonToken::Null,
            ScanToken::True => JsonToken::True,
            ScanToken::False => JsonToken::False,
            ScanToken::String { start, end, .. } => {
                let s = decode_string_owned(buf, *start, *end).map_err(ReaderError::Scan)?;
                JsonToken::String(s)
            }
            ScanToken::Number { start, end, hint } => {
                let parsed = parse_number(buf, *start, *end, *hint).map_err(ReaderError::Scan)?;
                match parsed {
                    ParsedNumber::U64(n) => JsonToken::U64(n),
                    ParsedNumber::I64(n) => JsonToken::I64(n),
                    ParsedNumber::U128(n) => JsonToken::U128(n),
                    ParsedNumber::I128(n) => JsonToken::I128(n),
                    ParsedNumber::F64(n) => JsonToken::F64(n),
                }
            }
            ScanToken::Eof | ScanToken::NeedMore { .. } => unreachable!(),
        };

        Ok(SpannedJsonToken { token, span })
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;
    use std::io::{Cursor, Read};

    /// A reader wrapper that simulates short reads by returning at most N bytes per read.
    struct ShortReadAdapter<R> {
        inner: R,
        max_bytes_per_read: usize,
    }

    impl<R> ShortReadAdapter<R> {
        fn new(inner: R, max_bytes_per_read: usize) -> Self {
            Self {
                inner,
                max_bytes_per_read,
            }
        }
    }

    impl<R: Read> Read for ShortReadAdapter<R> {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let len = buf.len().min(self.max_bytes_per_read);
            self.inner.read(&mut buf[..len])
        }
    }

    fn collect_tokens<R: Read>(reader: &mut JsonReader<R>) -> Vec<JsonToken> {
        let mut tokens = Vec::new();
        loop {
            let result = reader.next_token().unwrap().unwrap();
            let is_eof = matches!(result.token, JsonToken::Eof);
            tokens.push(result.token);
            if is_eof {
                break;
            }
        }
        tokens
    }

    #[test]
    fn test_simple() {
        let json = r#"{"name": "test", "value": 42}"#;
        let mut reader = JsonReader::new(Cursor::new(json));
        let tokens = collect_tokens(&mut reader);

        assert_eq!(
            tokens,
            vec![
                JsonToken::ObjectStart,
                JsonToken::String("name".to_string()),
                JsonToken::Colon,
                JsonToken::String("test".to_string()),
                JsonToken::Comma,
                JsonToken::String("value".to_string()),
                JsonToken::Colon,
                JsonToken::U64(42),
                JsonToken::ObjectEnd,
                JsonToken::Eof,
            ]
        );
    }

    #[test]
    fn test_small_buffer() {
        // Use a tiny 4-byte buffer - forces buffer growth for strings
        let json = r#"{"hello": "world"}"#;
        let mut reader = JsonReader::with_capacity(Cursor::new(json), 4);
        let tokens = collect_tokens(&mut reader);

        assert_eq!(
            tokens,
            vec![
                JsonToken::ObjectStart,
                JsonToken::String("hello".to_string()),
                JsonToken::Colon,
                JsonToken::String("world".to_string()),
                JsonToken::ObjectEnd,
                JsonToken::Eof,
            ]
        );
    }

    #[test]
    fn test_short_reads() {
        // Simulate network-like conditions: only 1-2 bytes per read
        let json = r#"{"hello": "world"}"#;
        let adapter = ShortReadAdapter::new(Cursor::new(json), 2);
        let mut reader = JsonReader::with_capacity(adapter, 4);
        let tokens = collect_tokens(&mut reader);

        assert_eq!(
            tokens,
            vec![
                JsonToken::ObjectStart,
                JsonToken::String("hello".to_string()),
                JsonToken::Colon,
                JsonToken::String("world".to_string()),
                JsonToken::ObjectEnd,
                JsonToken::Eof,
            ]
        );
    }

    #[test]
    fn test_single_byte_reads() {
        // Extreme case: 1 byte at a time
        let json = r#"[1, 2, 3]"#;
        let adapter = ShortReadAdapter::new(Cursor::new(json), 1);
        let mut reader = JsonReader::with_capacity(adapter, 2);
        let tokens = collect_tokens(&mut reader);

        assert_eq!(
            tokens,
            vec![
                JsonToken::ArrayStart,
                JsonToken::U64(1),
                JsonToken::Comma,
                JsonToken::U64(2),
                JsonToken::Comma,
                JsonToken::U64(3),
                JsonToken::ArrayEnd,
                JsonToken::Eof,
            ]
        );
    }

    #[test]
    fn test_numbers() {
        let json = r#"[1, -5, 3.14, 1e10]"#;
        let mut reader = JsonReader::new(Cursor::new(json));
        let tokens = collect_tokens(&mut reader);

        assert!(matches!(tokens[1], JsonToken::U64(1)));
        assert!(matches!(tokens[3], JsonToken::I64(-5)));
        assert!(matches!(tokens[5], JsonToken::F64(_)));
        assert!(matches!(tokens[7], JsonToken::F64(_)));
    }

    #[test]
    fn test_escapes() {
        let json = r#"{"msg": "hello\nworld"}"#;
        let mut reader = JsonReader::new(Cursor::new(json));
        let tokens = collect_tokens(&mut reader);

        assert_eq!(tokens[3], JsonToken::String("hello\nworld".to_string()));
    }

    #[test]
    fn test_escapes_with_short_reads() {
        // Escapes spanning read boundaries
        let json = r#"{"msg": "a\nb\tc"}"#;
        let adapter = ShortReadAdapter::new(Cursor::new(json), 3);
        let mut reader = JsonReader::with_capacity(adapter, 4);
        let tokens = collect_tokens(&mut reader);

        assert_eq!(tokens[3], JsonToken::String("a\nb\tc".to_string()));
    }
}

#[cfg(all(test, feature = "tokio"))]
mod tokio_tests {
    use super::*;
    use std::io::Cursor;

    #[tokio::test]
    async fn test_async_simple() {
        let json = r#"{"name": "test", "value": 42}"#;
        let cursor = Cursor::new(json.as_bytes().to_vec());
        let mut reader = AsyncJsonReader::new(cursor);

        let mut tokens = Vec::new();
        loop {
            let result = reader.next_token().await.unwrap().unwrap();
            let is_eof = matches!(result.token, JsonToken::Eof);
            tokens.push(result.token);
            if is_eof {
                break;
            }
        }

        assert_eq!(
            tokens,
            vec![
                JsonToken::ObjectStart,
                JsonToken::String("name".to_string()),
                JsonToken::Colon,
                JsonToken::String("test".to_string()),
                JsonToken::Comma,
                JsonToken::String("value".to_string()),
                JsonToken::Colon,
                JsonToken::U64(42),
                JsonToken::ObjectEnd,
                JsonToken::Eof,
            ]
        );
    }

    #[tokio::test]
    async fn test_async_small_buffer() {
        let json = r#"{"hello": "world"}"#;
        let cursor = Cursor::new(json.as_bytes().to_vec());
        let mut reader = AsyncJsonReader::with_capacity(cursor, 4);

        let mut tokens = Vec::new();
        loop {
            let result = reader.next_token().await.unwrap().unwrap();
            let is_eof = matches!(result.token, JsonToken::Eof);
            tokens.push(result.token);
            if is_eof {
                break;
            }
        }

        assert_eq!(
            tokens,
            vec![
                JsonToken::ObjectStart,
                JsonToken::String("hello".to_string()),
                JsonToken::Colon,
                JsonToken::String("world".to_string()),
                JsonToken::ObjectEnd,
                JsonToken::Eof,
            ]
        );
    }
}
