use alloc::string::String;
use core::str;

use crate::JsonErrorKind;

/// Position in the input (byte index)
pub type Pos = usize;

/// Error encountered during tokenization
#[derive(Debug, Clone, PartialEq)]
pub struct TokenizeError {
    pub kind: JsonErrorKind,
    pub pos: Pos,
}

/// Result alias for tokenizer operations
pub type TokenizeResult<T> = Result<T, TokenizeError>;

/// JSON tokens with their starting position in the input
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    LBrace(Pos),   // '{'
    RBrace(Pos),   // '}'
    LBracket(Pos), // '['
    RBracket(Pos), // ']'
    Colon(Pos),    // ':'
    Comma(Pos),    // ','
    String(String, Pos),
    Number(f64, Pos),
    True(Pos),
    False(Pos),
    Null(Pos),
    EOF(Pos),
}

/// Simple JSON tokenizer producing tokens from byte input.
pub struct Tokenizer<'input> {
    input: &'input [u8],
    pos: Pos,
}

impl<'input> Tokenizer<'input> {
    /// Create a new tokenizer for the given input slice.
    pub fn new(input: &'input [u8]) -> Self {
        Tokenizer { input, pos: 0 }
    }

    /// Current position in the input (byte index)
    pub fn position(&self) -> Pos {
        self.pos
    }

    /// Return the next token or a TokenizeError with kind and position
    pub fn next_token(&mut self) -> TokenizeResult<Token> {
        self.skip_whitespace();
        let start = self.pos;
        let c = match self.input.get(self.pos).copied() {
            Some(c) => c,
            None => return Ok(Token::EOF(self.pos)),
        };
        match c {
            b'{' => {
                self.pos += 1;
                Ok(Token::LBrace(start))
            }
            b'}' => {
                self.pos += 1;
                Ok(Token::RBrace(start))
            }
            b'[' => {
                self.pos += 1;
                Ok(Token::LBracket(start))
            }
            b']' => {
                self.pos += 1;
                Ok(Token::RBracket(start))
            }
            b':' => {
                self.pos += 1;
                Ok(Token::Colon(start))
            }
            b',' => {
                self.pos += 1;
                Ok(Token::Comma(start))
            }
            b'"' => self.parse_string(start),
            b'-' | b'0'..=b'9' => self.parse_number(start),
            b't' => self.parse_literal(b"true", || Token::True(start)),
            b'f' => self.parse_literal(b"false", || Token::False(start)),
            b'n' => self.parse_literal(b"null", || Token::Null(start)),
            _ => Err(TokenizeError {
                kind: JsonErrorKind::UnexpectedCharacter(c as char),
                pos: start,
            }),
        }
    }

    fn skip_whitespace(&mut self) {
        while let Some(&b) = self.input.get(self.pos) {
            match b {
                b' ' | b'\t' | b'\n' | b'\r' => self.pos += 1,
                _ => break,
            }
        }
    }

    fn parse_string(&mut self, start: Pos) -> TokenizeResult<Token> {
        // skip opening quote
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
        Ok(Token::String(s, start))
    }

    fn parse_number(&mut self, start: Pos) -> TokenizeResult<Token> {
        let mut end = self.pos;
        // optional leading '-'
        if self.input[end] == b'-' {
            end += 1;
        }
        // integer part
        while end < self.input.len() && (b'0'..=b'9').contains(&self.input[end]) {
            end += 1;
        }
        // fractional part
        if end < self.input.len() && self.input[end] == b'.' {
            end += 1;
            while end < self.input.len() && (b'0'..=b'9').contains(&self.input[end]) {
                end += 1;
            }
        }
        // optional exponent
        if end < self.input.len() && (self.input[end] == b'e' || self.input[end] == b'E') {
            end += 1;
            if end < self.input.len() && (self.input[end] == b'+' || self.input[end] == b'-') {
                end += 1;
            }
            while end < self.input.len() && (b'0'..=b'9').contains(&self.input[end]) {
                end += 1;
            }
        }
        let slice = &self.input[self.pos..end];
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
        Ok(Token::Number(num, start))
    }

    fn parse_literal<F>(&mut self, pat: &[u8], ctor: F) -> TokenizeResult<Token>
    where
        F: FnOnce() -> Token,
    {
        let start = self.pos;
        let end = start + pat.len();
        if end <= self.input.len() && &self.input[start..end] == pat {
            self.pos = end;
            Ok(ctor())
        } else {
            let got = self.input.get(start).copied().unwrap_or(b'?') as char;
            Err(TokenizeError {
                kind: JsonErrorKind::UnexpectedCharacter(got),
                pos: start,
            })
        }
    }
}
