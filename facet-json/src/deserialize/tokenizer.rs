use alloc::string::String;
use alloc::vec::Vec;
use core::str;

use crate::JsonErrorKind;

/// Position in the input (byte index)
pub type Pos = usize;

/// A span in the input, with a start position and length
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: Pos,
    pub len: usize,
}

impl Span {
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
    /// End position (start + length)
    pub fn end(&self) -> Pos {
        self.start + self.len
    }
}

/// A value of type `T` annotated with its `Span`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spanned<T> {
    pub node: T,
    pub span: Span,
}

/// Error encountered during tokenization
#[derive(Debug, Clone, PartialEq)]
pub struct TokenizeError {
    pub kind: JsonErrorKind,
    pub pos: Pos,
}

/// Tokenization result, yielding a spanned token
pub type TokenizeResult = Result<Spanned<Token>, TokenizeError>;

/// JSON tokens (without positions)
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    LBrace,   // '{'
    RBrace,   // '}'
    LBracket, // '['
    RBracket, // ']'
    Colon,    // ':'
    Comma,    // ','
    String(String),
    Number(f64),
    True,
    False,
    Null,
    EOF,
}

/// Simple JSON tokenizer producing spanned tokens from byte input.
pub struct Tokenizer<'input> {
    input: &'input [u8],
    pos: Pos,
    /// Current JSON document path (e.g. "$.users[0].name")
    pub path: String,
    input: &'input [u8],
    pos: Pos,
}

impl<'input> Clone for Tokenizer<'input> {
    fn clone(&self) -> Self {
        Tokenizer {
            input: self.input,
            pos: self.pos,
            path: self.path.clone(),
        }
    }
}
    fn clone(&self) -> Self {
        Tokenizer {
            input: self.input,
            pos: self.pos,
        }
    }
}

impl<'input> Tokenizer<'input> {
    /// Create a new tokenizer for the given input slice.
    pub fn new(input: &'input [u8], path: String) -> Self {
        Tokenizer { input, pos: 0, path }
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
                return Err(TokenizeError {
                    kind: JsonErrorKind::UnexpectedCharacter(c as char),
                    pos: start,
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
        while let Some(&b) = self.input.get(self.pos) {
            match b {
                b'"' => {
                    self.pos += 1;
                    break;
                }
                b'\\' => {
                    self.pos += 1;
                    if let Some(&esc) = self.input.get(self.pos) {
                        buf.push(esc);
                        self.pos += 1;
                    } else {
                        return Err(TokenizeError {
                            kind: JsonErrorKind::UnexpectedEof("in string escape"),
                            pos: self.pos,
                        });
                    }
                }
                _ => {
                    buf.push(b);
                    self.pos += 1;
                }
            }
        }
        let s = match str::from_utf8(&buf) {
            Ok(st) => st.to_string(),
            Err(e) => {
                return Err(TokenizeError {
                    kind: JsonErrorKind::InvalidUtf8(e.to_string()),
                    pos: start,
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
        while end < self.input.len() && (b'0'..=b'9').contains(&self.input[end]) {
            end += 1;
        }
        if end < self.input.len() && self.input[end] == b'.' {
            end += 1;
            while end < self.input.len() && (b'0'..=b'9').contains(&self.input[end]) {
                end += 1;
            }
        }
        if end < self.input.len() && (self.input[end] == b'e' || self.input[end] == b'E') {
            end += 1;
            if end < self.input.len() && (self.input[end] == b'+' || self.input[end] == b'-') {
                end += 1;
            }
            while end < self.input.len() && (b'0'..=b'9').contains(&self.input[end]) {
                end += 1;
            }
        }
        let slice = &self.input[start..end];
        let text = match str::from_utf8(slice) {
            Ok(t) => t,
            Err(e) => {
                return Err(TokenizeError {
                    kind: JsonErrorKind::InvalidUtf8(e.to_string()),
                    pos: start,
                });
            }
        };
        let num = match text.parse::<f64>() {
            Ok(n) => n,
            Err(_) => {
                return Err(TokenizeError {
                    kind: JsonErrorKind::NumberOutOfRange(0.0),
                    pos: start,
                });
            }
        };
        self.pos = end;
        let len = end - start;
        let span = Span::new(start, len);
        Ok(Spanned {
            node: Token::Number(num),
            span,
        })
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
            let got = self.input.get(start).copied().unwrap_or(b'?') as char;
            Err(TokenizeError {
                kind: JsonErrorKind::UnexpectedCharacter(got),
                pos: start,
            })
        }
    }
}
