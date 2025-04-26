use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;
use core::str;

/// Position in the input (byte index)
pub type Pos = usize;

/// A span in the input, with a start position and length
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    /// Starting position of the span in bytes
    start: Pos,
    /// Length of the span in bytes
    len: usize,
}

impl Span {
    /// Creates a new span with the given start position and length
    pub fn new(start: Pos, len: usize) -> Self {
        Span { start, len }
    }
    /// Start position of the span
    pub fn start(&self) -> Pos {
        self.start
    }
    /// Length of the span
    pub fn len(&self) -> usize {
        self.len
    }
    /// Returns `true` if this span has zero length
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    /// End position (start + length)
    pub fn end(&self) -> Pos {
        self.start + self.len
    }
}

/// A value of type `T` annotated with its `Span`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spanned<T> {
    /// The actual data/value being wrapped
    pub node: T,
    /// The span information indicating the position and length in the source
    pub span: Span,
}

/// Error encountered during tokenization
#[derive(Debug, Clone, PartialEq)]
pub struct TokenError {
    /// The specific type of error that occurred during tokenization
    pub kind: TokenErrorKind,
    /// The location in the source where the error occurred
    pub span: Span,
}

/// Types of errors that can occur during tokenization
#[derive(Debug, Clone, PartialEq)]
pub enum TokenErrorKind {
    /// Unexpected character encountered
    UnexpectedCharacter(char),
    /// End of file reached unexpectedly
    UnexpectedEof(&'static str),
    /// Invalid UTF-8 sequence
    InvalidUtf8(String),
    /// Number is out of range
    NumberOutOfRange(f64),
}

impl Display for TokenErrorKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            TokenErrorKind::UnexpectedCharacter(c) => write!(f, "unexpected character: '{}'", c),
            TokenErrorKind::UnexpectedEof(context) => write!(f, "unexpected EOF {}", context),
            TokenErrorKind::InvalidUtf8(detail) => write!(f, "invalid UTF-8: {}", detail),
            TokenErrorKind::NumberOutOfRange(n) => write!(f, "number out of range: {}", n),
        }
    }
}

/// Tokenization result, yielding a spanned token
pub type TokenizeResult = Result<Spanned<Token>, TokenError>;

/// JSON tokens (without positions)
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    /// Left brace character: '{'
    LBrace,
    /// Right brace character: '}'
    RBrace,
    /// Left bracket character: '['
    LBracket,
    /// Right bracket character: ']'
    RBracket,
    /// Colon character: ':'
    Colon,
    /// Comma character: ','
    Comma,
    /// A JSON string value — todo: should be a Cow
    String(String),
    /// A 64-bit floating point number value — used if the value contains a decimal point
    F64(f64),
    /// A signed 64-bit integer number value — used if the value does not contain a decimal point but contains a sign
    I64(i64),
    /// An unsigned 64-bit integer number value — used if the value does not contain a decimal point and does not contain a sign
    U64(u64),
    /// The JSON boolean value 'true'
    True,
    /// The JSON boolean value 'false'
    False,
    /// The JSON null value
    Null,
    /// End of file marker
    EOF,
}

use core::fmt::{self, Display, Formatter};

impl Display for Token {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Token::LBrace => write!(f, "{{"),
            Token::RBrace => write!(f, "}}"),
            Token::LBracket => write!(f, "["),
            Token::RBracket => write!(f, "]"),
            Token::Colon => write!(f, ":"),
            Token::Comma => write!(f, ","),
            Token::String(s) => write!(f, "\"{}\"", s),
            Token::F64(n) => write!(f, "{}", n),
            Token::I64(n) => write!(f, "{}", n),
            Token::U64(n) => write!(f, "{}", n),
            Token::True => write!(f, "true"),
            Token::False => write!(f, "false"),
            Token::Null => write!(f, "null"),
            Token::EOF => write!(f, "EOF"),
        }
    }
}

/// Simple JSON tokenizer producing spanned tokens from byte input.
pub struct Tokenizer<'input> {
    input: &'input [u8],
    pos: Pos,
}

impl<'input> Tokenizer<'input> {
    /// Create a new tokenizer for the given input slice.
    pub fn new(input: &'input [u8]) -> Self {
        Tokenizer { input, pos: 0 }
    }

    /// Current cursor position in the input
    pub fn position(&self) -> Pos {
        self.pos
    }

    /// Return the next spanned token or a TokenizeError
    pub fn next_token(&mut self) -> TokenizeResult {
        self.skip_whitespace();
        let start = self.pos;
        let c = match self.input.get(self.pos).copied() {
            Some(c) => c,
            None => {
                // EOF at this position
                let span = Span::new(self.pos, 0);
                return Ok(Spanned {
                    node: Token::EOF,
                    span,
                });
            }
        };
        let sp = match c {
            b'{' => {
                self.pos += 1;
                Spanned {
                    node: Token::LBrace,
                    span: Span::new(start, 1),
                }
            }
            b'}' => {
                self.pos += 1;
                Spanned {
                    node: Token::RBrace,
                    span: Span::new(start, 1),
                }
            }
            b'[' => {
                self.pos += 1;
                Spanned {
                    node: Token::LBracket,
                    span: Span::new(start, 1),
                }
            }
            b']' => {
                self.pos += 1;
                Spanned {
                    node: Token::RBracket,
                    span: Span::new(start, 1),
                }
            }
            b':' => {
                self.pos += 1;
                Spanned {
                    node: Token::Colon,
                    span: Span::new(start, 1),
                }
            }
            b',' => {
                self.pos += 1;
                Spanned {
                    node: Token::Comma,
                    span: Span::new(start, 1),
                }
            }
            b'"' => return self.parse_string(start),
            b'-' | b'0'..=b'9' => return self.parse_number(start),
            b't' => return self.parse_literal(start, b"true", || Token::True),
            b'f' => return self.parse_literal(start, b"false", || Token::False),
            b'n' => return self.parse_literal(start, b"null", || Token::Null),
            _ => {
                return Err(TokenError {
                    kind: TokenErrorKind::UnexpectedCharacter(c as char),
                    span: Span::new(start, 1),
                });
            }
        };
        Ok(sp)
    }

    /// Skip whitespace characters
    fn skip_whitespace(&mut self) {
        while let Some(&b) = self.input.get(self.pos) {
            match b {
                b' ' | b'\t' | b'\n' | b'\r' => self.pos += 1,
                _ => break,
            }
        }
    }

    fn parse_string(&mut self, start: Pos) -> TokenizeResult {
        // Skip opening quote
        self.pos += 1;
        let mut buf = Vec::new();
        let content_start = self.pos;

        while let Some(&b) = self.input.get(self.pos) {
            match b {
                b'"' => {
                    self.pos += 1;
                    break;
                }
                b'\\' => {
                    self.pos += 1;
                    if let Some(&esc) = self.input.get(self.pos) {
                        match esc {
                            b'"' | b'\\' | b'/' => buf.push(esc),
                            b'b' => buf.push(b'\x08'), // backspace
                            b'f' => buf.push(b'\x0C'), // form feed
                            b'n' => buf.push(b'\n'),   // line feed
                            b'r' => buf.push(b'\r'),   // carriage return
                            b't' => buf.push(b'\t'),   // tab
                            _ => buf.push(esc), // other escapes (should handle \uXXXX properly)
                        }
                        self.pos += 1;
                    } else {
                        return Err(TokenError {
                            kind: TokenErrorKind::UnexpectedEof("in string escape"),
                            span: Span::new(self.pos, 0),
                        });
                    }
                }
                _ => {
                    buf.push(b);
                    self.pos += 1;
                }
            }
        }

        // Check if we reached the end without finding a closing quote
        if self.pos > self.input.len()
            || (self.pos == self.input.len() && self.input[self.pos - 1] != b'"')
        {
            return Err(TokenError {
                kind: TokenErrorKind::UnexpectedEof("in string literal"),
                span: Span::new(start, self.pos - start),
            });
        }

        let s = match str::from_utf8(&buf) {
            Ok(st) => st.to_string(),
            Err(e) => {
                return Err(TokenError {
                    kind: TokenErrorKind::InvalidUtf8(e.to_string()),
                    span: Span::new(content_start, buf.len()),
                });
            }
        };

        let len = self.pos - start;
        let span = Span::new(start, len);
        Ok(Spanned {
            node: Token::String(s),
            span,
        })
    }

    fn parse_number(&mut self, start: Pos) -> TokenizeResult {
        let mut end = self.pos;
        if self.input[end] == b'-' {
            end += 1;
        }
        while end < self.input.len() && self.input[end].is_ascii_digit() {
            end += 1;
        }
        if end < self.input.len() && self.input[end] == b'.' {
            end += 1;
            while end < self.input.len() && self.input[end].is_ascii_digit() {
                end += 1;
            }
        }
        if end < self.input.len() && (self.input[end] == b'e' || self.input[end] == b'E') {
            end += 1;
            if end < self.input.len() && (self.input[end] == b'+' || self.input[end] == b'-') {
                end += 1;
            }
            while end < self.input.len() && self.input[end].is_ascii_digit() {
                end += 1;
            }
        }
        let slice = &self.input[start..end];
        let span = Span::new(start, end - start);

        let text = match str::from_utf8(slice) {
            Ok(t) => t,
            Err(e) => {
                return Err(TokenError {
                    kind: TokenErrorKind::InvalidUtf8(e.to_string()),
                    span,
                });
            }
        };

        let token = if text.contains('.') || text.contains('e') || text.contains('E') {
            // If the number contains a decimal point or exponent, parse as f64
            match text.parse::<f64>() {
                Ok(n) => Token::F64(n),
                Err(_) => {
                    return Err(TokenError {
                        kind: TokenErrorKind::NumberOutOfRange(0.0),
                        span,
                    });
                }
            }
        } else if text.starts_with('-') {
            // If the number starts with a negative sign, parse as i64
            match text.parse::<i64>() {
                Ok(n) => Token::I64(n),
                Err(_) => {
                    // If i64 parsing fails, try to parse as f64 for error reporting
                    let num = text.parse::<f64>().unwrap_or(0.0);
                    return Err(TokenError {
                        kind: TokenErrorKind::NumberOutOfRange(num),
                        span,
                    });
                }
            }
        } else {
            // Otherwise, parse as u64
            match text.parse::<u64>() {
                Ok(n) => Token::U64(n),
                Err(_) => {
                    // If u64 parsing fails, try to parse as f64 for error reporting
                    let num = text.parse::<f64>().unwrap_or(0.0);
                    return Err(TokenError {
                        kind: TokenErrorKind::NumberOutOfRange(num),
                        span,
                    });
                }
            }
        };

        self.pos = end;
        Ok(Spanned { node: token, span })
    }

    fn parse_literal<F>(&mut self, start: Pos, pat: &[u8], ctor: F) -> TokenizeResult
    where
        F: FnOnce() -> Token,
    {
        let end = start + pat.len();
        if end <= self.input.len() && &self.input[start..end] == pat {
            self.pos = end;
            let span = Span::new(start, pat.len());
            Ok(Spanned { node: ctor(), span })
        } else {
            // Determine how much of the pattern matched before mismatch
            let actual_len = self.input.len().saturating_sub(start).min(pat.len());
            let span = Span::new(start, actual_len.max(1)); // Ensure span covers at least one character

            let got = self.input.get(start).copied().unwrap_or(b'?') as char;
            Err(TokenError {
                kind: TokenErrorKind::UnexpectedCharacter(got),
                span,
            })
        }
    }
}
