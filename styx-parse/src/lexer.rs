//! Lexer for the Styx configuration language.

#[allow(unused_imports)]
use crate::trace;
use crate::{Span, Token, TokenKind};

/// A lexer that produces tokens from Styx source text.
pub struct Lexer<'src> {
    /// The source text being lexed.
    source: &'src str,
    /// The remaining source text (suffix of `source`).
    remaining: &'src str,
    /// Current byte position in `source`.
    pos: u32,

    /// State for heredoc parsing.
    heredoc_state: Option<HeredocState>,
}

/// State for tracking heredoc parsing.
#[derive(Debug, Clone)]
struct HeredocState {
    /// The delimiter to match (e.g., "EOF" for `<<EOF`)
    delimiter: String,
}

impl<'src> Lexer<'src> {
    /// Create a new lexer for the given source text.
    pub fn new(source: &'src str) -> Self {
        Self {
            source,
            remaining: source,
            pos: 0,
            heredoc_state: None,
        }
    }

    /// Get the current byte position.
    #[inline]
    pub fn position(&self) -> u32 {
        self.pos
    }

    /// Check if we're at the end of input.
    #[inline]
    pub fn is_eof(&self) -> bool {
        self.remaining.is_empty()
    }

    /// Peek at the next character without consuming it.
    #[inline]
    fn peek(&self) -> Option<char> {
        self.remaining.chars().next()
    }

    /// Peek at the nth character (0-indexed) without consuming.
    #[inline]
    fn peek_nth(&self, n: usize) -> Option<char> {
        self.remaining.chars().nth(n)
    }

    /// Advance by one character and return it.
    #[inline]
    fn advance(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += c.len_utf8() as u32;
        self.remaining = &self.remaining[c.len_utf8()..];
        Some(c)
    }

    /// Advance by n bytes.
    #[inline]
    fn advance_by(&mut self, n: usize) {
        self.pos += n as u32;
        self.remaining = &self.remaining[n..];
    }

    /// Check if the remaining text starts with the given prefix.
    #[inline]
    fn starts_with(&self, prefix: &str) -> bool {
        self.remaining.starts_with(prefix)
    }

    /// Create a token from the given start position to current position.
    fn token(&self, kind: TokenKind, start: u32) -> Token<'src> {
        let span = Span::new(start, self.pos);
        let text = &self.source[start as usize..self.pos as usize];
        trace!("Token {:?} at {:?}: {:?}", kind, span, text);
        Token::new(kind, span, text)
    }

    /// Get the next token.
    pub fn next_token(&mut self) -> Token<'src> {
        // Handle heredoc content if we're inside one
        if let Some(ref state) = self.heredoc_state.clone() {
            return self.lex_heredoc_content(&state.delimiter);
        }

        // Check for EOF
        if self.is_eof() {
            return self.token(TokenKind::Eof, self.pos);
        }

        let start = self.pos;
        let c = self.peek().unwrap();

        match c {
            // Structural tokens
            '{' => {
                self.advance();
                self.token(TokenKind::LBrace, start)
            }
            '}' => {
                self.advance();
                self.token(TokenKind::RBrace, start)
            }
            '(' => {
                self.advance();
                self.token(TokenKind::LParen, start)
            }
            ')' => {
                self.advance();
                self.token(TokenKind::RParen, start)
            }
            ',' => {
                self.advance();
                self.token(TokenKind::Comma, start)
            }
            '>' => {
                self.advance();
                self.token(TokenKind::Gt, start)
            }
            '@' => {
                self.advance();
                self.token(TokenKind::At, start)
            }

            // Quoted scalar
            '"' => self.lex_quoted_scalar(),

            // Comment or doc comment
            '/' if self.starts_with("///") => self.lex_doc_comment(),
            '/' if self.starts_with("//") => self.lex_line_comment(),
            // Single / is a bare scalar (e.g., /usr/bin/foo)
            '/' => self.lex_bare_scalar(),

            // Heredoc - only if << is followed by uppercase letter
            // parser[impl scalar.heredoc.invalid]
            '<' if self.starts_with("<<")
                && matches!(self.peek_nth(2), Some(c) if c.is_ascii_uppercase()) =>
            {
                self.lex_heredoc_start()
            }
            // << not followed by uppercase is an error
            '<' if self.starts_with("<<") => {
                let start = self.pos;
                self.advance(); // <
                self.advance(); // <
                self.token(TokenKind::Error, start)
            }

            // Raw string
            'r' if matches!(self.peek_nth(1), Some('#' | '"')) => self.lex_raw_string(),

            // Whitespace
            ' ' | '\t' => self.lex_whitespace(),

            // Newline
            '\n' => {
                self.advance();
                self.token(TokenKind::Newline, start)
            }
            '\r' if self.peek_nth(1) == Some('\n') => {
                self.advance();
                self.advance();
                self.token(TokenKind::Newline, start)
            }

            // Bare scalar (default for anything else that's not a special char)
            _ if is_bare_scalar_start(c) => self.lex_bare_scalar(),

            // Error: unrecognized character
            _ => {
                self.advance();
                self.token(TokenKind::Error, start)
            }
        }
    }

    /// Lex horizontal whitespace (spaces and tabs).
    fn lex_whitespace(&mut self) -> Token<'src> {
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c == ' ' || c == '\t' {
                self.advance();
            } else {
                break;
            }
        }
        self.token(TokenKind::Whitespace, start)
    }

    /// Lex a bare (unquoted) scalar.
    fn lex_bare_scalar(&mut self) -> Token<'src> {
        let start = self.pos;
        while let Some(c) = self.peek() {
            if is_bare_scalar_char(c) {
                self.advance();
            } else {
                break;
            }
        }
        self.token(TokenKind::BareScalar, start)
    }

    /// Lex a quoted scalar: `"..."`.
    fn lex_quoted_scalar(&mut self) -> Token<'src> {
        let start = self.pos;

        // Consume opening quote
        self.advance();

        loop {
            match self.peek() {
                None => {
                    // Unterminated string - return error
                    return self.token(TokenKind::Error, start);
                }
                Some('"') => {
                    self.advance();
                    break;
                }
                Some('\\') => {
                    // Escape sequence - consume backslash and next char
                    self.advance();
                    if self.peek().is_some() {
                        self.advance();
                    }
                }
                Some(_) => {
                    self.advance();
                }
            }
        }

        self.token(TokenKind::QuotedScalar, start)
    }

    // parser[impl comment.line]
    /// Lex a line comment: `// ...`.
    fn lex_line_comment(&mut self) -> Token<'src> {
        let start = self.pos;

        // Consume `//`
        self.advance();
        self.advance();

        // Consume until end of line
        while let Some(c) = self.peek() {
            if c == '\n' || c == '\r' {
                break;
            }
            self.advance();
        }

        self.token(TokenKind::LineComment, start)
    }

    /// Lex a doc comment: `/// ...`.
    fn lex_doc_comment(&mut self) -> Token<'src> {
        let start = self.pos;

        // Consume `///`
        self.advance();
        self.advance();
        self.advance();

        // Consume until end of line
        while let Some(c) = self.peek() {
            if c == '\n' || c == '\r' {
                break;
            }
            self.advance();
        }

        self.token(TokenKind::DocComment, start)
    }

    /// Lex a heredoc start: `<<DELIM`.
    ///
    /// Per parser[scalar.heredoc.syntax]: delimiter MUST match `[A-Z][A-Z0-9_]*`
    /// and not exceed 16 characters.
    // parser[impl scalar.heredoc.syntax]
    fn lex_heredoc_start(&mut self) -> Token<'src> {
        let start = self.pos;

        // Consume `<<`
        self.advance();
        self.advance();

        let delim_start = self.pos as usize;

        // First char MUST be uppercase letter
        match self.peek() {
            Some(c) if c.is_ascii_uppercase() => {
                self.advance();
            }
            _ => {
                // Invalid delimiter - first char not uppercase letter
                // Consume any remaining delimiter-like chars for error recovery
                while let Some(c) = self.peek() {
                    if c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_' {
                        self.advance();
                    } else {
                        break;
                    }
                }
                return self.token(TokenKind::Error, start);
            }
        }

        // Rest: uppercase, digit, or underscore
        while let Some(c) = self.peek() {
            if c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_' {
                self.advance();
            } else {
                break;
            }
        }

        let delimiter = &self.source[delim_start..self.pos as usize];

        // Check length <= 16
        if delimiter.len() > 16 {
            return self.token(TokenKind::Error, start);
        }

        // Consume optional language hint: ,lang where lang matches [a-z][a-z0-9_.-]*
        // parser[impl scalar.heredoc.lang]
        // The language hint is metadata and does not affect the scalar content.
        if self.peek() == Some(',') {
            self.advance(); // consume ','
            // First char must be lowercase letter
            if let Some(c) = self.peek()
                && c.is_ascii_lowercase()
            {
                self.advance();
                // Rest: lowercase, digit, underscore, dot, hyphen
                while let Some(c) = self.peek() {
                    if c.is_ascii_lowercase()
                        || c.is_ascii_digit()
                        || c == '_'
                        || c == '.'
                        || c == '-'
                    {
                        self.advance();
                    } else {
                        break;
                    }
                }
            }
        }

        // Consume newline after delimiter (and optional lang hint)
        if self.peek() == Some('\r') {
            self.advance();
        }
        if self.peek() == Some('\n') {
            self.advance();
        }

        // Set state for heredoc content
        self.heredoc_state = Some(HeredocState {
            delimiter: delimiter.to_string(),
        });

        self.token(TokenKind::HeredocStart, start)
    }

    /// Check if the remaining input starts with the heredoc delimiter (possibly indented).
    /// Returns Some(indent_len) if found, where indent_len is the number of leading spaces/tabs.
    /// The delimiter must be followed by newline or EOF to be valid.
    fn find_heredoc_delimiter(&self, delimiter: &str) -> Option<usize> {
        // Count leading whitespace
        let indent_len = self
            .remaining
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .count();

        // Check if delimiter follows the whitespace
        let after_indent = &self.remaining[indent_len..];
        if let Some(after_delim) = after_indent.strip_prefix(delimiter)
            && (after_delim.is_empty()
                || after_delim.starts_with('\n')
                || after_delim.starts_with("\r\n"))
        {
            return Some(indent_len);
        }
        None
    }

    /// Lex heredoc content until we find the closing delimiter.
    /// Per parser[scalar.heredoc.syntax]: The closing delimiter line MAY be indented;
    /// that indentation is stripped from content lines.
    fn lex_heredoc_content(&mut self, delimiter: &str) -> Token<'src> {
        let start = self.pos;

        // Check if we're at the delimiter (possibly indented) - end of heredoc
        if let Some(indent_len) = self.find_heredoc_delimiter(delimiter) {
            // This is the end delimiter - consume indent + delimiter
            self.advance_by(indent_len + delimiter.len());
            self.heredoc_state = None;
            return self.token(TokenKind::HeredocEnd, start);
        }

        // Consume content until we find the delimiter at start of a line (possibly indented)
        let mut found_end = false;
        while !self.is_eof() {
            // Consume the current line
            while let Some(c) = self.peek() {
                if c == '\n' {
                    self.advance();
                    break;
                } else if c == '\r' && self.peek_nth(1) == Some('\n') {
                    self.advance();
                    self.advance();
                    break;
                }
                self.advance();
            }

            // Check if next line starts with delimiter (possibly indented)
            if self.find_heredoc_delimiter(delimiter).is_some() {
                found_end = true;
                break;
            }

            if self.is_eof() {
                break;
            }
        }

        if start == self.pos
            && found_end
            && let Some(indent_len) = self.find_heredoc_delimiter(delimiter)
        {
            // No content, return the end delimiter
            self.advance_by(indent_len + delimiter.len());
            self.heredoc_state = None;
            return self.token(TokenKind::HeredocEnd, start);
        }

        // CRITICAL: If we hit EOF without finding the closing delimiter,
        // we must clear the heredoc state to avoid an infinite loop.
        // The next call would otherwise re-enter lex_heredoc_content forever.
        if self.is_eof() && !found_end {
            self.heredoc_state = None;
            return self.token(TokenKind::Error, start);
        }

        self.token(TokenKind::HeredocContent, start)
    }

    // parser[impl scalar.raw.syntax]
    /// Lex a raw string: `r#*"..."#*`.
    /// Returns the entire raw string including delimiters.
    fn lex_raw_string(&mut self) -> Token<'src> {
        let start = self.pos;

        // Consume `r`
        self.advance();

        // Count and consume `#` marks
        let mut hash_count: u8 = 0;
        while self.peek() == Some('#') {
            hash_count = hash_count.saturating_add(1);
            self.advance();
        }

        // Consume opening `"`
        if self.peek() == Some('"') {
            self.advance();
        } else {
            // Invalid raw string - no opening quote
            return self.token(TokenKind::Error, start);
        }

        // Consume content until we find the closing `"#*`
        loop {
            match self.peek() {
                None => {
                    // Unterminated raw string - return error
                    return self.token(TokenKind::Error, start);
                }
                Some('"') => {
                    // Check for closing sequence
                    let mut matched_hashes = 0u8;
                    let mut lookahead = 1;
                    while matched_hashes < hash_count {
                        if self.peek_nth(lookahead) == Some('#') {
                            matched_hashes += 1;
                            lookahead += 1;
                        } else {
                            break;
                        }
                    }

                    if matched_hashes == hash_count {
                        // Found the closing delimiter - consume it
                        self.advance(); // consume `"`
                        for _ in 0..hash_count {
                            self.advance(); // consume `#`s
                        }
                        // Return token with full text including delimiters
                        return self.token(TokenKind::RawScalar, start);
                    } else {
                        // Not a closing delimiter, consume the `"` as content
                        self.advance();
                    }
                }
                Some(_) => {
                    self.advance();
                }
            }
        }
    }
}

impl<'src> Iterator for Lexer<'src> {
    type Item = Token<'src>;

    fn next(&mut self) -> Option<Self::Item> {
        let token = self.next_token();
        if token.kind == TokenKind::Eof {
            None
        } else {
            Some(token)
        }
    }
}

// parser[impl scalar.bare.chars]
/// Check if a character can start a bare scalar.
fn is_bare_scalar_start(c: char) -> bool {
    // Cannot be special chars, whitespace, or `/` (to avoid confusion with comments)
    // `=` and `@` are allowed after first char but not at start
    !matches!(c, '{' | '}' | '(' | ')' | ',' | '"' | '=' | '@' | '>' | '/') && !c.is_whitespace()
}

// parser[impl scalar.bare.chars]
/// Check if a character can continue a bare scalar.
fn is_bare_scalar_char(c: char) -> bool {
    // Cannot be special chars or whitespace
    // `/`, `@`, and `=` are allowed after the first char
    // `>` is never allowed (attribute separator)
    !matches!(c, '{' | '}' | '(' | ')' | ',' | '"' | '>') && !c.is_whitespace()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lex(source: &str) -> Vec<(TokenKind, &str)> {
        Lexer::new(source).map(|t| (t.kind, t.text)).collect()
    }

    #[test]
    fn test_structural_tokens() {
        assert_eq!(lex("{"), vec![(TokenKind::LBrace, "{")]);
        assert_eq!(lex("}"), vec![(TokenKind::RBrace, "}")]);
        assert_eq!(lex("("), vec![(TokenKind::LParen, "(")]);
        assert_eq!(lex(")"), vec![(TokenKind::RParen, ")")]);
        assert_eq!(lex(","), vec![(TokenKind::Comma, ",")]);
        assert_eq!(lex(">"), vec![(TokenKind::Gt, ">")]);
        assert_eq!(lex("@"), vec![(TokenKind::At, "@")]);
    }

    #[test]
    fn test_bare_scalar() {
        assert_eq!(lex("hello"), vec![(TokenKind::BareScalar, "hello")]);
        assert_eq!(lex("42"), vec![(TokenKind::BareScalar, "42")]);
        assert_eq!(lex("true"), vec![(TokenKind::BareScalar, "true")]);
        assert_eq!(
            lex("https://example.com/path"),
            vec![(TokenKind::BareScalar, "https://example.com/path")]
        );
    }

    #[test]
    fn test_quoted_scalar() {
        assert_eq!(
            lex(r#""hello world""#),
            vec![(TokenKind::QuotedScalar, r#""hello world""#)]
        );
        assert_eq!(
            lex(r#""with \"escapes\"""#),
            vec![(TokenKind::QuotedScalar, r#""with \"escapes\"""#)]
        );
    }

    #[test]
    fn test_raw_scalar() {
        // Raw scalars now include the full text with delimiters (for lossless CST)
        assert_eq!(
            lex(r#"r"hello""#),
            vec![(TokenKind::RawScalar, r#"r"hello""#)]
        );
        assert_eq!(
            lex(r##"r#"hello"#"##),
            vec![(TokenKind::RawScalar, r##"r#"hello"#"##)]
        );
    }

    #[test]
    fn test_comments() {
        assert_eq!(
            lex("// comment"),
            vec![(TokenKind::LineComment, "// comment")]
        );
        assert_eq!(lex("/// doc"), vec![(TokenKind::DocComment, "/// doc")]);
    }

    #[test]
    fn test_whitespace() {
        assert_eq!(lex("  \t"), vec![(TokenKind::Whitespace, "  \t")]);
        assert_eq!(lex("\n"), vec![(TokenKind::Newline, "\n")]);
        assert_eq!(lex("\r\n"), vec![(TokenKind::Newline, "\r\n")]);
    }

    #[test]
    fn test_mixed() {
        let tokens = lex("{host localhost}");
        assert_eq!(
            tokens,
            vec![
                (TokenKind::LBrace, "{"),
                (TokenKind::BareScalar, "host"),
                (TokenKind::Whitespace, " "),
                (TokenKind::BareScalar, "localhost"),
                (TokenKind::RBrace, "}"),
            ]
        );
    }

    #[test]
    fn test_heredoc() {
        let tokens = lex("<<EOF\nhello\nworld\nEOF");
        assert_eq!(
            tokens,
            vec![
                (TokenKind::HeredocStart, "<<EOF\n"),
                (TokenKind::HeredocContent, "hello\nworld\n"),
                (TokenKind::HeredocEnd, "EOF"),
            ]
        );
    }

    // parser[verify scalar.heredoc.syntax]
    #[test]
    fn test_heredoc_valid_delimiters() {
        // Single uppercase letter
        assert!(lex("<<A\nx\nA").iter().all(|t| t.0 != TokenKind::Error));
        // Multiple uppercase letters
        assert!(lex("<<EOF\nx\nEOF").iter().all(|t| t.0 != TokenKind::Error));
        // With digits after first char
        assert!(
            lex("<<MY123\nx\nMY123")
                .iter()
                .all(|t| t.0 != TokenKind::Error)
        );
        // With underscores
        assert!(
            lex("<<MY_DELIM\nx\nMY_DELIM")
                .iter()
                .all(|t| t.0 != TokenKind::Error)
        );
        // 16 chars (max allowed)
        assert!(
            lex("<<ABCDEFGHIJKLMNOP\nx\nABCDEFGHIJKLMNOP")
                .iter()
                .all(|t| t.0 != TokenKind::Error)
        );
    }

    // parser[verify scalar.heredoc.syntax]
    #[test]
    fn test_heredoc_must_start_uppercase() {
        // Starts with digit - error
        assert!(lex("<<123FOO").iter().any(|t| t.0 == TokenKind::Error));
        // Starts with underscore - error
        assert!(lex("<<_FOO").iter().any(|t| t.0 == TokenKind::Error));
        // Lowercase - error (lexer won't even recognize it as heredoc delimiter chars)
        let tokens = lex("<<foo");
        // This will be << followed by bare scalar "foo"
        assert!(!tokens.iter().any(|t| t.0 == TokenKind::HeredocStart));
    }

    // parser[verify scalar.heredoc.syntax]
    #[test]
    fn test_heredoc_max_16_chars() {
        // 17 chars - error
        assert!(
            lex("<<ABCDEFGHIJKLMNOPQ\nx\nABCDEFGHIJKLMNOPQ")
                .iter()
                .any(|t| t.0 == TokenKind::Error)
        );
    }

    #[test]
    fn test_slash_in_bare_scalar() {
        // Single slash followed by text should be a bare scalar
        let tokens = lex("/foo");
        assert_eq!(tokens, vec![(TokenKind::BareScalar, "/foo")]);

        // Path-like value
        let tokens = lex("/usr/bin/foo");
        assert_eq!(tokens, vec![(TokenKind::BareScalar, "/usr/bin/foo")]);

        // But // is still a comment
        let tokens = lex("// comment");
        assert_eq!(tokens, vec![(TokenKind::LineComment, "// comment")]);
    }

    #[test]
    fn test_unterminated_heredoc() {
        // Heredoc without closing delimiter should be an error
        let tokens = lex("<<EOF\nhello world\n");
        eprintln!("tokens = {:?}", tokens);
        assert!(
            tokens.iter().any(|t| t.0 == TokenKind::Error),
            "Expected Error token for unterminated heredoc"
        );
    }

    #[test]
    fn test_unterminated_string() {
        // String without closing quote should be an error
        let tokens = lex("\"hello");
        eprintln!("tokens = {:?}", tokens);
        assert!(
            tokens.iter().any(|t| t.0 == TokenKind::Error),
            "Expected Error token for unterminated string"
        );
    }
}
