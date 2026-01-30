//! Tokenizer for the Styx configuration language.

use crate::{Span, Token, TokenKind};
use tracing::trace;

/// A tokenizer that produces tokens from Styx source text.
#[derive(Clone)]
pub struct Tokenizer<'src> {
    /// The source text being tokenized.
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
    /// Indentation of the closing delimiter (set when found).
    /// This is the number of spaces/tabs before the closing delimiter.
    closing_indent: Option<usize>,
}

impl<'src> Tokenizer<'src> {
    /// Create a new tokenizer for the given source text.
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

    /// Get the closing indent for the current heredoc (if any).
    /// This is set after parsing heredoc content, before returning HeredocEnd.
    /// Used by the lexer to apply dedent to heredoc content.
    #[inline]
    pub fn heredoc_closing_indent(&self) -> Option<usize> {
        self.heredoc_state.as_ref().and_then(|s| s.closing_indent)
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
            return self.tokenize_heredoc_content(&state.delimiter);
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
            '@' => self.tokenize_at_or_tag(),

            // Quoted scalar
            '"' => self.tokenize_quoted_scalar(),

            // Comment or doc comment
            '/' if self.starts_with("///") => self.tokenize_doc_comment(),
            '/' if self.starts_with("//") => self.tokenize_line_comment(),
            // Single / is a bare scalar (e.g., /usr/bin/foo)
            '/' => self.tokenize_bare_scalar(),

            // Heredoc - only if << is followed by uppercase letter
            // parser[impl scalar.heredoc.invalid]
            '<' if self.starts_with("<<")
                && matches!(self.peek_nth(2), Some(c) if c.is_ascii_uppercase()) =>
            {
                self.tokenize_heredoc_start()
            }
            // << not followed by uppercase is an error
            '<' if self.starts_with("<<") => {
                let start = self.pos;
                self.advance(); // <
                self.advance(); // <
                self.token(TokenKind::Error, start)
            }

            // Raw string
            'r' if matches!(self.peek_nth(1), Some('#' | '"')) => self.tokenize_raw_string(),

            // Whitespace
            ' ' | '\t' => self.tokenize_whitespace(),

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
            _ if is_bare_scalar_start(c) => self.tokenize_bare_scalar(),

            // Error: unrecognized character
            _ => {
                self.advance();
                self.token(TokenKind::Error, start)
            }
        }
    }

    /// Tokenize horizontal whitespace (spaces and tabs).
    fn tokenize_whitespace(&mut self) -> Token<'src> {
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

    /// Tokenize a bare (unquoted) scalar.
    fn tokenize_bare_scalar(&mut self) -> Token<'src> {
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

    /// Tokenize `@` (unit) or `@name` (tag).
    fn tokenize_at_or_tag(&mut self) -> Token<'src> {
        let start = self.pos;
        self.advance(); // consume `@`

        // Check if followed by tag name start: [A-Za-z_]
        match self.peek() {
            Some(c) if c.is_ascii_alphabetic() || c == '_' => {
                // Tag name: consume [A-Za-z0-9_-]*
                // But stop before `r#` or `r"` which starts a raw string payload
                self.advance();
                while let Some(c) = self.peek() {
                    // Check for raw string start: if current char is part of tag
                    // and next would be `r` followed by `#` or `"`, stop here
                    if c == 'r' && matches!(self.peek_nth(1), Some('#' | '"')) {
                        // Don't consume `r` - it's the start of a raw string
                        break;
                    }
                    if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                        self.advance();
                    } else {
                        break;
                    }
                }
                self.token(TokenKind::Tag, start)
            }
            _ => {
                // Standalone @ = unit
                self.token(TokenKind::At, start)
            }
        }
    }

    /// Tokenize a quoted scalar: `"..."`.
    fn tokenize_quoted_scalar(&mut self) -> Token<'src> {
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
    /// Tokenize a line comment: `// ...`.
    fn tokenize_line_comment(&mut self) -> Token<'src> {
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

    /// Tokenize a doc comment: `/// ...`.
    fn tokenize_doc_comment(&mut self) -> Token<'src> {
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

    /// Tokenize a heredoc start: `<<DELIM`.
    ///
    /// Per parser[scalar.heredoc.syntax]: delimiter MUST match `[A-Z][A-Z0-9_]*`
    /// and not exceed 16 characters.
    // parser[impl scalar.heredoc.syntax]
    fn tokenize_heredoc_start(&mut self) -> Token<'src> {
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
            closing_indent: None,
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

    /// Tokenize heredoc content until we find the closing delimiter.
    /// Per parser[scalar.heredoc.syntax]: The closing delimiter line MAY be indented;
    /// that indentation is stripped from content lines.
    fn tokenize_heredoc_content(&mut self, delimiter: &str) -> Token<'src> {
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
        let mut closing_indent = 0usize;
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
            if let Some(indent_len) = self.find_heredoc_delimiter(delimiter) {
                found_end = true;
                closing_indent = indent_len;
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
        // The next call would otherwise re-enter tokenize_heredoc_content forever.
        if self.is_eof() && !found_end {
            self.heredoc_state = None;
            return self.token(TokenKind::Error, start);
        }

        // Store the closing indent so the lexer can apply dedent
        if let Some(ref mut state) = self.heredoc_state {
            state.closing_indent = Some(closing_indent);
        }

        self.token(TokenKind::HeredocContent, start)
    }

    // parser[impl scalar.raw.syntax]
    /// Tokenize a raw string: `r#*"..."#*`.
    /// Returns the entire raw string including delimiters.
    fn tokenize_raw_string(&mut self) -> Token<'src> {
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

impl<'src> Iterator for Tokenizer<'src> {
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
    use facet_testhelpers::test;

    fn tokenize(source: &str) -> Vec<(TokenKind, &str)> {
        Tokenizer::new(source).map(|t| (t.kind, t.text)).collect()
    }

    #[test]
    fn test_structural_tokens() {
        assert_eq!(tokenize("{"), vec![(TokenKind::LBrace, "{")]);
        assert_eq!(tokenize("}"), vec![(TokenKind::RBrace, "}")]);
        assert_eq!(tokenize("("), vec![(TokenKind::LParen, "(")]);
        assert_eq!(tokenize(")"), vec![(TokenKind::RParen, ")")]);
        assert_eq!(tokenize(","), vec![(TokenKind::Comma, ",")]);
        assert_eq!(tokenize(">"), vec![(TokenKind::Gt, ">")]);
        assert_eq!(tokenize("@"), vec![(TokenKind::At, "@")]);
    }

    #[test]
    fn test_bare_scalar() {
        assert_eq!(tokenize("hello"), vec![(TokenKind::BareScalar, "hello")]);
        assert_eq!(tokenize("42"), vec![(TokenKind::BareScalar, "42")]);
        assert_eq!(tokenize("true"), vec![(TokenKind::BareScalar, "true")]);
        assert_eq!(
            tokenize("https://example.com/path"),
            vec![(TokenKind::BareScalar, "https://example.com/path")]
        );
    }

    #[test]
    fn test_quoted_scalar() {
        assert_eq!(
            tokenize(r#""hello world""#),
            vec![(TokenKind::QuotedScalar, r#""hello world""#)]
        );
        assert_eq!(
            tokenize(r#""with \"escapes\"""#),
            vec![(TokenKind::QuotedScalar, r#""with \"escapes\"""#)]
        );
    }

    #[test]
    fn test_raw_scalar() {
        // Raw scalars now include the full text with delimiters (for lossless CST)
        assert_eq!(
            tokenize(r#"r"hello""#),
            vec![(TokenKind::RawScalar, r#"r"hello""#)]
        );
        assert_eq!(
            tokenize(r##"r#"hello"#"##),
            vec![(TokenKind::RawScalar, r##"r#"hello"#"##)]
        );
    }

    #[test]
    fn test_comments() {
        assert_eq!(
            tokenize("// comment"),
            vec![(TokenKind::LineComment, "// comment")]
        );
        assert_eq!(
            tokenize("/// doc"),
            vec![(TokenKind::DocComment, "/// doc")]
        );
    }

    #[test]
    fn test_whitespace() {
        assert_eq!(tokenize("  \t"), vec![(TokenKind::Whitespace, "  \t")]);
        assert_eq!(tokenize("\n"), vec![(TokenKind::Newline, "\n")]);
        assert_eq!(tokenize("\r\n"), vec![(TokenKind::Newline, "\r\n")]);
    }

    #[test]
    fn test_mixed() {
        let tokens = tokenize("{host localhost}");
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
        let tokens = tokenize("<<EOF\nhello\nworld\nEOF");
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
        assert!(
            tokenize("<<A\nx\nA")
                .iter()
                .all(|t| t.0 != TokenKind::Error)
        );
        // Multiple uppercase letters
        assert!(
            tokenize("<<EOF\nx\nEOF")
                .iter()
                .all(|t| t.0 != TokenKind::Error)
        );
        // With digits after first char
        assert!(
            tokenize("<<MY123\nx\nMY123")
                .iter()
                .all(|t| t.0 != TokenKind::Error)
        );
        // With underscores
        assert!(
            tokenize("<<MY_DELIM\nx\nMY_DELIM")
                .iter()
                .all(|t| t.0 != TokenKind::Error)
        );
        // 16 chars (max allowed)
        assert!(
            tokenize("<<ABCDEFGHIJKLMNOP\nx\nABCDEFGHIJKLMNOP")
                .iter()
                .all(|t| t.0 != TokenKind::Error)
        );
    }

    // parser[verify scalar.heredoc.syntax]
    #[test]
    fn test_heredoc_must_start_uppercase() {
        // Starts with digit - error
        assert!(tokenize("<<123FOO").iter().any(|t| t.0 == TokenKind::Error));
        // Starts with underscore - error
        assert!(tokenize("<<_FOO").iter().any(|t| t.0 == TokenKind::Error));
        // Lowercase - error (tokenizer won't even recognize it as heredoc delimiter chars)
        let tokens = tokenize("<<foo");
        // This will be << followed by bare scalar "foo"
        assert!(!tokens.iter().any(|t| t.0 == TokenKind::HeredocStart));
    }

    // parser[verify scalar.heredoc.syntax]
    #[test]
    fn test_heredoc_max_16_chars() {
        // 17 chars - error
        assert!(
            tokenize("<<ABCDEFGHIJKLMNOPQ\nx\nABCDEFGHIJKLMNOPQ")
                .iter()
                .any(|t| t.0 == TokenKind::Error)
        );
    }

    #[test]
    fn test_slash_in_bare_scalar() {
        // Single slash followed by text should be a bare scalar
        let tokens = tokenize("/foo");
        assert_eq!(tokens, vec![(TokenKind::BareScalar, "/foo")]);

        // Path-like value
        let tokens = tokenize("/usr/bin/foo");
        assert_eq!(tokens, vec![(TokenKind::BareScalar, "/usr/bin/foo")]);

        // But // is still a comment
        let tokens = tokenize("// comment");
        assert_eq!(tokens, vec![(TokenKind::LineComment, "// comment")]);
    }

    #[test]
    fn test_attribute_syntax_tokens() {
        // Check how the tokenizer tokenizes attribute syntax
        let tokens = tokenize("server host>localhost");
        // Tokenizer produces separate tokens - attribute syntax is handled by the parser
        assert_eq!(
            tokens,
            vec![
                (TokenKind::BareScalar, "server"),
                (TokenKind::Whitespace, " "),
                (TokenKind::BareScalar, "host"),
                (TokenKind::Gt, ">"),
                (TokenKind::BareScalar, "localhost"),
            ]
        );
    }

    #[test]
    fn test_unterminated_heredoc() {
        // Heredoc without closing delimiter should be an error
        let tokens = tokenize("<<EOF\nhello world\n");
        eprintln!("tokens = {:?}", tokens);
        assert!(
            tokens.iter().any(|t| t.0 == TokenKind::Error),
            "Expected Error token for unterminated heredoc"
        );
    }

    #[test]
    fn test_unterminated_string() {
        // String without closing quote should be an error
        let tokens = tokenize("\"hello");
        eprintln!("tokens = {:?}", tokens);
        assert!(
            tokens.iter().any(|t| t.0 == TokenKind::Error),
            "Expected Error token for unterminated string"
        );
    }
}
