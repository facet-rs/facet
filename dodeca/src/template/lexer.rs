//! Lexer for the template language
//!
//! Tokenizes Jinja-like template syntax with precise span tracking.

use super::ast::Span;
use std::sync::Arc;

/// A token with its span
#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl Token {
    pub fn new(kind: TokenKind, offset: usize, len: usize) -> Self {
        Self {
            kind,
            span: Span::new(offset.into(), len),
        }
    }
}

/// Token types
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // Literals
    Text(String),   // Raw template text
    String(String), // "string" or 'string'
    Int(i64),       // 123
    Float(f64),     // 1.23
    Ident(String),  // variable_name

    // Keywords
    If,
    Elif,
    Else,
    Endif,
    For,
    In,
    Endfor,
    Block,
    Endblock,
    Extends,
    Include,
    Import,
    Macro,
    Endmacro,
    True,
    False,
    None,
    Not,
    And,
    Or,
    Is,
    As,
    Set,

    // Delimiters
    ExprOpen,     // {{
    ExprClose,    // }}
    TagOpen,      // {%
    TagClose,     // %}
    CommentOpen,  // {#
    CommentClose, // #}

    // Operators
    Dot,         // .
    Comma,       // ,
    Colon,       // :
    Pipe,        // |
    LParen,      // (
    RParen,      // )
    LBracket,    // [
    RBracket,    // ]
    LBrace,      // {
    RBrace,      // }
    Plus,        // +
    Minus,       // -
    Star,        // *
    Slash,       // /
    Percent,     // %
    DoubleSlash, // //
    DoubleStar,  // **
    DoubleColon, // ::
    Tilde,       // ~ (string concat)
    Eq,          // ==
    Ne,          // !=
    Lt,          // <
    Le,          // <=
    Gt,          // >
    Ge,          // >=
    Assign,      // =

    // Special
    Eof,
    Error(String),
}

impl TokenKind {
    /// Check if this is a keyword
    pub fn from_ident(s: &str) -> TokenKind {
        match s {
            "if" => TokenKind::If,
            "elif" => TokenKind::Elif,
            "else" => TokenKind::Else,
            "endif" => TokenKind::Endif,
            "for" => TokenKind::For,
            "in" => TokenKind::In,
            "endfor" => TokenKind::Endfor,
            "block" => TokenKind::Block,
            "endblock" => TokenKind::Endblock,
            "extends" => TokenKind::Extends,
            "include" => TokenKind::Include,
            "import" => TokenKind::Import,
            "macro" => TokenKind::Macro,
            "endmacro" => TokenKind::Endmacro,
            "true" | "True" => TokenKind::True,
            "false" | "False" => TokenKind::False,
            "none" | "None" => TokenKind::None,
            "not" => TokenKind::Not,
            "and" => TokenKind::And,
            "or" => TokenKind::Or,
            "is" => TokenKind::Is,
            "as" => TokenKind::As,
            "set" => TokenKind::Set,
            _ => TokenKind::Ident(s.to_string()),
        }
    }
}

/// Lexer state (owns the source string via Arc for cheap cloning)
pub struct Lexer {
    source: Arc<String>,
    /// Current byte position in source
    pos: usize,
    /// Are we inside a tag/expression (vs raw text)?
    in_code: bool,
    /// Pending tokens (for lookahead/pushback)
    pending: Vec<Token>,
}

impl Lexer {
    pub fn new(source: Arc<String>) -> Self {
        Self {
            source,
            pos: 0,
            in_code: false,
            pending: Vec::new(),
        }
    }

    /// Get the source string
    pub fn source(&self) -> &Arc<String> {
        &self.source
    }

    /// Peek at the next character without consuming
    fn peek(&self) -> Option<char> {
        self.source[self.pos..].chars().next()
    }

    /// Peek at the next n bytes as a string slice
    fn peek_n(&self, n: usize) -> Option<&str> {
        if self.pos + n <= self.source.len() {
            Some(&self.source[self.pos..self.pos + n])
        } else {
            None
        }
    }

    /// Advance by one character and return it
    fn advance(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += c.len_utf8();
        Some(c)
    }

    /// Skip whitespace (only when in code mode)
    fn skip_whitespace(&mut self) {
        while let Some(c) = self.peek() {
            if c.is_whitespace() {
                self.advance();
            } else {
                break;
            }
        }
    }

    /// Get the next token
    pub fn next_token(&mut self) -> Token {
        // Return pending tokens first
        if let Some(token) = self.pending.pop() {
            return token;
        }

        if self.in_code {
            self.lex_code()
        } else {
            self.lex_text()
        }
    }

    /// Lex raw template text until we hit a delimiter
    fn lex_text(&mut self) -> Token {
        let start = self.pos;
        let mut text = String::new();

        while let Some(c) = self.peek() {
            // Check for opening delimiters
            if c == '{' {
                if let Some("{{" | "{%" | "{#") = self.peek_n(2) {
                    break;
                }
            }
            text.push(self.advance().unwrap());
        }

        if text.is_empty() {
            // Must be at a delimiter or EOF
            self.lex_delimiter_or_eof()
        } else {
            Token::new(TokenKind::Text(text), start, self.pos - start)
        }
    }

    /// Lex an opening delimiter or EOF
    fn lex_delimiter_or_eof(&mut self) -> Token {
        let start = self.pos;

        match self.peek_n(2) {
            Some("{{") => {
                self.pos += 2;
                self.in_code = true;
                Token::new(TokenKind::ExprOpen, start, 2)
            }
            Some("{%") => {
                self.pos += 2;
                self.in_code = true;
                Token::new(TokenKind::TagOpen, start, 2)
            }
            Some("{#") => {
                self.pos += 2;
                // Skip comment content and continue lexing
                self.skip_comment();
                self.next_token()
            }
            _ => Token::new(TokenKind::Eof, start, 0),
        }
    }

    /// Skip a comment {# ... #} - consumes content without returning a token
    fn skip_comment(&mut self) {
        let mut depth = 1;

        while depth > 0 {
            match self.peek_n(2) {
                Some("#}") => {
                    self.pos += 2;
                    depth -= 1;
                }
                Some("{#") => {
                    self.pos += 2;
                    depth += 1;
                }
                Some(_) => {
                    self.advance();
                }
                None => {
                    // Unclosed comment - just stop
                    return;
                }
            }
        }
    }

    /// Lex code (inside {{ }} or {% %})
    fn lex_code(&mut self) -> Token {
        self.skip_whitespace();

        let start = self.pos;

        // Check for closing delimiters and two-character operators
        if let Some(next2) = self.peek_n(2) {
            let token = match next2 {
                "}}" => {
                    self.pos += 2;
                    self.in_code = false;
                    Some(Token::new(TokenKind::ExprClose, start, 2))
                }
                "%}" => {
                    self.pos += 2;
                    self.in_code = false;
                    Some(Token::new(TokenKind::TagClose, start, 2))
                }
                "//" => {
                    self.pos += 2;
                    Some(Token::new(TokenKind::DoubleSlash, start, 2))
                }
                "**" => {
                    self.pos += 2;
                    Some(Token::new(TokenKind::DoubleStar, start, 2))
                }
                "::" => {
                    self.pos += 2;
                    Some(Token::new(TokenKind::DoubleColon, start, 2))
                }
                "==" => {
                    self.pos += 2;
                    Some(Token::new(TokenKind::Eq, start, 2))
                }
                "!=" => {
                    self.pos += 2;
                    Some(Token::new(TokenKind::Ne, start, 2))
                }
                "<=" => {
                    self.pos += 2;
                    Some(Token::new(TokenKind::Le, start, 2))
                }
                ">=" => {
                    self.pos += 2;
                    Some(Token::new(TokenKind::Ge, start, 2))
                }
                _ => None,
            };
            if let Some(t) = token {
                return t;
            }
        }

        // Single character or longer tokens
        match self.peek() {
            None => Token::new(TokenKind::Eof, start, 0),
            Some(c) => match c {
                '.' => {
                    self.advance();
                    Token::new(TokenKind::Dot, start, 1)
                }
                ',' => {
                    self.advance();
                    Token::new(TokenKind::Comma, start, 1)
                }
                ':' => {
                    self.advance();
                    Token::new(TokenKind::Colon, start, 1)
                }
                '|' => {
                    self.advance();
                    Token::new(TokenKind::Pipe, start, 1)
                }
                '(' => {
                    self.advance();
                    Token::new(TokenKind::LParen, start, 1)
                }
                ')' => {
                    self.advance();
                    Token::new(TokenKind::RParen, start, 1)
                }
                '[' => {
                    self.advance();
                    Token::new(TokenKind::LBracket, start, 1)
                }
                ']' => {
                    self.advance();
                    Token::new(TokenKind::RBracket, start, 1)
                }
                '{' => {
                    self.advance();
                    Token::new(TokenKind::LBrace, start, 1)
                }
                '}' => {
                    self.advance();
                    Token::new(TokenKind::RBrace, start, 1)
                }
                '+' => {
                    self.advance();
                    Token::new(TokenKind::Plus, start, 1)
                }
                '-' => {
                    self.advance();
                    Token::new(TokenKind::Minus, start, 1)
                }
                '*' => {
                    self.advance();
                    Token::new(TokenKind::Star, start, 1)
                }
                '/' => {
                    self.advance();
                    Token::new(TokenKind::Slash, start, 1)
                }
                '%' => {
                    self.advance();
                    Token::new(TokenKind::Percent, start, 1)
                }
                '~' => {
                    self.advance();
                    Token::new(TokenKind::Tilde, start, 1)
                }
                '<' => {
                    self.advance();
                    Token::new(TokenKind::Lt, start, 1)
                }
                '>' => {
                    self.advance();
                    Token::new(TokenKind::Gt, start, 1)
                }
                '=' => {
                    self.advance();
                    Token::new(TokenKind::Assign, start, 1)
                }
                '"' | '\'' => self.lex_string(c),
                '0'..='9' => self.lex_number(),
                c if c.is_alphabetic() || c == '_' => self.lex_ident(),
                _ => {
                    self.advance();
                    Token::new(
                        TokenKind::Error(format!("Unexpected character: {c}")),
                        start,
                        1,
                    )
                }
            },
        }
    }

    /// Lex a string literal
    fn lex_string(&mut self, quote: char) -> Token {
        let start = self.pos;
        self.advance(); // consume opening quote

        let mut value = String::new();

        loop {
            match self.advance() {
                None => {
                    return Token::new(
                        TokenKind::Error("Unclosed string".to_string()),
                        start,
                        self.pos - start,
                    );
                }
                Some(c) if c == quote => break,
                Some('\\') => {
                    // Escape sequence
                    match self.advance() {
                        Some('n') => value.push('\n'),
                        Some('t') => value.push('\t'),
                        Some('r') => value.push('\r'),
                        Some('\\') => value.push('\\'),
                        Some(c) if c == quote => value.push(c),
                        Some(c) => {
                            value.push('\\');
                            value.push(c);
                        }
                        None => break,
                    }
                }
                Some(c) => value.push(c),
            }
        }

        Token::new(TokenKind::String(value), start, self.pos - start)
    }

    /// Lex a number (int or float)
    fn lex_number(&mut self) -> Token {
        let start = self.pos;
        let mut s = String::new();
        let mut is_float = false;

        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                s.push(self.advance().unwrap());
            } else if c == '.' && !is_float {
                // Could be float or field access
                // Look ahead to see if there's a digit after the dot
                let dot_pos = self.pos;
                self.advance(); // consume the .

                if let Some(next) = self.peek() {
                    if next.is_ascii_digit() {
                        is_float = true;
                        s.push('.');
                        continue;
                    }
                }

                // Not a float - emit int and push dot token for next call
                let int_val: i64 = s.parse().unwrap_or(0);
                self.pending.push(Token::new(TokenKind::Dot, dot_pos, 1));
                return Token::new(TokenKind::Int(int_val), start, dot_pos - start);
            } else if c == '_' {
                // Allow underscores in numbers (visual separator)
                self.advance();
            } else {
                break;
            }
        }

        if is_float {
            let val: f64 = s.parse().unwrap_or(0.0);
            Token::new(TokenKind::Float(val), start, self.pos - start)
        } else {
            let val: i64 = s.parse().unwrap_or(0);
            Token::new(TokenKind::Int(val), start, self.pos - start)
        }
    }

    /// Lex an identifier or keyword
    fn lex_ident(&mut self) -> Token {
        let start = self.pos;
        let mut s = String::new();

        while let Some(c) = self.peek() {
            if c.is_alphanumeric() || c == '_' {
                s.push(self.advance().unwrap());
            } else {
                break;
            }
        }

        let kind = TokenKind::from_ident(&s);
        Token::new(kind, start, self.pos - start)
    }
}

/// Iterator implementation for convenient use
impl Iterator for Lexer {
    type Item = Token;

    fn next(&mut self) -> Option<Self::Item> {
        let token = self.next_token();
        if matches!(token.kind, TokenKind::Eof) {
            None
        } else {
            Some(token)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lex(s: &str) -> Vec<TokenKind> {
        Lexer::new(Arc::new(s.to_string()))
            .map(|t| t.kind)
            .collect()
    }

    #[test]
    fn test_text_only() {
        assert_eq!(
            lex("hello world"),
            vec![TokenKind::Text("hello world".to_string())]
        );
    }

    #[test]
    fn test_expr() {
        assert_eq!(
            lex("{{ name }}"),
            vec![
                TokenKind::ExprOpen,
                TokenKind::Ident("name".to_string()),
                TokenKind::ExprClose,
            ]
        );
    }

    #[test]
    fn test_mixed() {
        assert_eq!(
            lex("Hello, {{ name }}!"),
            vec![
                TokenKind::Text("Hello, ".to_string()),
                TokenKind::ExprOpen,
                TokenKind::Ident("name".to_string()),
                TokenKind::ExprClose,
                TokenKind::Text("!".to_string()),
            ]
        );
    }

    #[test]
    fn test_tag() {
        assert_eq!(
            lex("{% if true %}yes{% endif %}"),
            vec![
                TokenKind::TagOpen,
                TokenKind::If,
                TokenKind::True,
                TokenKind::TagClose,
                TokenKind::Text("yes".to_string()),
                TokenKind::TagOpen,
                TokenKind::Endif,
                TokenKind::TagClose,
            ]
        );
    }

    #[test]
    fn test_field_access() {
        assert_eq!(
            lex("{{ user.name }}"),
            vec![
                TokenKind::ExprOpen,
                TokenKind::Ident("user".to_string()),
                TokenKind::Dot,
                TokenKind::Ident("name".to_string()),
                TokenKind::ExprClose,
            ]
        );
    }

    #[test]
    fn test_filter() {
        assert_eq!(
            lex("{{ name | upper }}"),
            vec![
                TokenKind::ExprOpen,
                TokenKind::Ident("name".to_string()),
                TokenKind::Pipe,
                TokenKind::Ident("upper".to_string()),
                TokenKind::ExprClose,
            ]
        );
    }

    #[test]
    fn test_string() {
        assert_eq!(
            lex("{{ \"hello\" }}"),
            vec![
                TokenKind::ExprOpen,
                TokenKind::String("hello".to_string()),
                TokenKind::ExprClose,
            ]
        );
    }

    #[test]
    fn test_number() {
        assert_eq!(
            lex("{{ 42 }}"),
            vec![
                TokenKind::ExprOpen,
                TokenKind::Int(42),
                TokenKind::ExprClose,
            ]
        );
    }
}
