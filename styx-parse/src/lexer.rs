//! Lexer for Styx - produces lexemes from tokens.
//!
//! The Lexer sits between the Tokenizer and Parser:
//! - Tokenizer → Token (raw: At, BareScalar, LBrace, etc.)
//! - Lexer → Lexeme (atoms: Scalar, Tag, Unit, structural markers)
//! - Parser → Events (structure: entries, objects, sequences)

use std::borrow::Cow;

use styx_tokenizer::{Span, Token, TokenKind, Tokenizer};

use crate::events::ScalarKind;

/// A lexeme produced by the Lexer from raw tokens.
#[derive(Debug, Clone, PartialEq)]
pub enum Lexeme<'src> {
    /// A scalar value (bare, quoted, raw, or heredoc)
    Scalar {
        span: Span,
        value: Cow<'src, str>,
        kind: ScalarKind,
    },

    /// Unit value: standalone `@`
    Unit { span: Span },

    /// A tag: `@name`
    /// The payload (if any) comes as the next lexeme
    Tag {
        span: Span,
        name: &'src str,
        /// True if an immediate payload follows (no whitespace): `@tag{}`, `@tag()`, `@tag"x"`, `@tag@`
        has_payload: bool,
    },

    /// Start of object `{`
    ObjectStart { span: Span },

    /// End of object `}`
    ObjectEnd { span: Span },

    /// Start of sequence `(`
    SeqStart { span: Span },

    /// End of sequence `)`
    SeqEnd { span: Span },

    /// An attribute key `key>` - value follows as next lexeme(s)
    AttrKey {
        /// Span of the full `key>` including the `>`
        span: Span,
        /// Span of just the key (excluding `>`)
        key_span: Span,
        /// The key text
        key: &'src str,
    },

    /// Comma separator
    Comma { span: Span },

    /// Newline (significant for separator detection)
    Newline { span: Span },

    /// Line comment `// ...`
    Comment { span: Span, text: &'src str },

    /// Doc comment `/// ...`
    DocComment { span: Span, text: &'src str },

    /// End of input
    Eof,

    /// Tokenizer error
    Error { span: Span, message: &'static str },
}

impl Lexeme<'_> {
    /// Get the span of this lexeme.
    /// Returns a zero-length span at position 0 for Eof.
    pub fn span(&self) -> Span {
        match self {
            Lexeme::Scalar { span, .. }
            | Lexeme::Unit { span }
            | Lexeme::Tag { span, .. }
            | Lexeme::ObjectStart { span }
            | Lexeme::ObjectEnd { span }
            | Lexeme::SeqStart { span }
            | Lexeme::SeqEnd { span }
            | Lexeme::AttrKey { span, .. }
            | Lexeme::Comma { span }
            | Lexeme::Newline { span }
            | Lexeme::Comment { span, .. }
            | Lexeme::DocComment { span, .. }
            | Lexeme::Error { span, .. } => *span,
            Lexeme::Eof => Span::new(0, 0),
        }
    }
}

/// Lexer that produces lexemes from tokens.
#[derive(Clone)]
pub struct Lexer<'src> {
    tokenizer: Tokenizer<'src>,
    /// Peeked token (if any)
    peeked: Option<Token<'src>>,
}

impl<'src> Lexer<'src> {
    /// Create a new lexer for the given source.
    pub fn new(source: &'src str) -> Self {
        Self {
            tokenizer: Tokenizer::new(source),
            peeked: None,
        }
    }

    /// Peek at the next token without consuming it.
    fn peek_token(&mut self) -> &Token<'src> {
        if self.peeked.is_none() {
            self.peeked = Some(self.tokenizer.next_token());
        }
        self.peeked.as_ref().unwrap()
    }

    /// Consume and return the next token.
    fn next_token(&mut self) -> Token<'src> {
        self.peeked
            .take()
            .unwrap_or_else(|| self.tokenizer.next_token())
    }

    /// Get the next lexeme.
    pub fn next_lexeme(&mut self) -> Lexeme<'src> {
        // Skip whitespace (but not newlines - those are significant)
        loop {
            let tok = self.peek_token();
            if tok.kind == TokenKind::Whitespace {
                self.next_token();
            } else {
                break;
            }
        }

        let tok = self.next_token();

        match tok.kind {
            TokenKind::Eof => Lexeme::Eof,

            TokenKind::LBrace => Lexeme::ObjectStart { span: tok.span },
            TokenKind::RBrace => Lexeme::ObjectEnd { span: tok.span },
            TokenKind::LParen => Lexeme::SeqStart { span: tok.span },
            TokenKind::RParen => Lexeme::SeqEnd { span: tok.span },
            TokenKind::Comma => Lexeme::Comma { span: tok.span },
            TokenKind::Gt => {
                // Standalone `>` (with whitespace before it) - not valid in Styx
                // Attribute syntax requires no space: `key>value`
                Lexeme::Error {
                    span: tok.span,
                    message: "unexpected `>` (attribute syntax requires no spaces: key>value)",
                }
            }
            TokenKind::Newline => Lexeme::Newline { span: tok.span },

            TokenKind::LineComment => Lexeme::Comment {
                span: tok.span,
                text: tok.text,
            },
            TokenKind::DocComment => Lexeme::DocComment {
                span: tok.span,
                text: tok.text,
            },

            TokenKind::At => {
                // Check if followed immediately by a bare scalar (invalid tag like @123)
                let next = self.peek_token();
                if next.span.start == tok.span.end && next.kind == TokenKind::BareScalar {
                    // Consume the adjacent token to include it in the error span
                    let bad_tok = self.next_token();
                    return Lexeme::Error {
                        span: Span::new(tok.span.start, bad_tok.span.end),
                        message: "invalid tag name",
                    };
                }
                // Standalone @ = unit
                Lexeme::Unit { span: tok.span }
            }

            TokenKind::Tag => {
                // Tag token includes the @ and name, e.g. "@foo"
                // Extract the name (skip the @)
                let name = &tok.text[1..];

                // Check if payload follows immediately (no whitespace)
                // Payload can be: { ( " r#" @ or Tag
                let payload_tok = self.peek_token();
                let is_adjacent = payload_tok.span.start == tok.span.end;
                let is_valid_payload = matches!(
                    payload_tok.kind,
                    TokenKind::LBrace
                        | TokenKind::LParen
                        | TokenKind::QuotedScalar
                        | TokenKind::RawScalar
                        | TokenKind::At
                        | TokenKind::Tag
                );

                // If a bare scalar is adjacent (no whitespace), it's an invalid tag name
                // e.g., @org/package where /package is adjacent
                // But structural tokens like ) } , or newlines are fine - they end the tag
                if is_adjacent && !is_valid_payload && payload_tok.kind == TokenKind::BareScalar {
                    // Consume the adjacent token to include it in the error span
                    let bad_tok = self.next_token();
                    return Lexeme::Error {
                        span: Span::new(tok.span.start, bad_tok.span.end),
                        message: "invalid tag name",
                    };
                }

                Lexeme::Tag {
                    span: tok.span,
                    name,
                    has_payload: is_adjacent && is_valid_payload,
                }
            }

            TokenKind::BareScalar => {
                // Check if followed by `>` (attribute syntax)
                let next = self.peek_token();
                let is_attr = next.kind == TokenKind::Gt && next.span.start == tok.span.end;
                let gt_end = next.span.end;
                if is_attr {
                    // Attribute: key>
                    self.next_token(); // consume `>`

                    // Check that value follows immediately (no whitespace after `>`)
                    let value_tok = self.peek_token();
                    let gt_span = Span::new(gt_end - 1, gt_end);
                    if value_tok.kind == TokenKind::Newline || value_tok.kind == TokenKind::Eof {
                        return Lexeme::Error {
                            span: gt_span,
                            message: "expected a value",
                        };
                    }
                    if value_tok.kind == TokenKind::Whitespace {
                        return Lexeme::Error {
                            span: gt_span,
                            message: "whitespace after `>` in attribute (use key>value with no spaces)",
                        };
                    }

                    return Lexeme::AttrKey {
                        span: Span::new(tok.span.start, gt_end),
                        key_span: tok.span,
                        key: tok.text,
                    };
                }

                Lexeme::Scalar {
                    span: tok.span,
                    value: Cow::Borrowed(tok.text),
                    kind: ScalarKind::Bare,
                }
            }

            TokenKind::QuotedScalar => {
                // Process escape sequences
                let inner = &tok.text[1..tok.text.len() - 1]; // strip quotes
                match process_escapes(inner) {
                    Ok(value) => Lexeme::Scalar {
                        span: tok.span,
                        value,
                        kind: ScalarKind::Quoted,
                    },
                    Err(msg) => Lexeme::Error {
                        span: tok.span,
                        message: msg,
                    },
                }
            }

            TokenKind::RawScalar => {
                // r#"..."# - extract content between quotes
                let text = tok.text;
                // Count leading #s after 'r'
                let hash_count = text[1..].chars().take_while(|&c| c == '#').count();
                // Content is between r##" and "##
                let start = 1 + hash_count + 1; // r + hashes + quote
                let end = text.len() - hash_count - 1; // quote + hashes
                let content = &text[start..end];

                Lexeme::Scalar {
                    span: tok.span,
                    value: Cow::Borrowed(content),
                    kind: ScalarKind::Raw,
                }
            }

            TokenKind::HeredocStart => {
                // Collect heredoc content
                let start_span = tok.span;
                let mut content = String::new();
                let end_span;
                let mut closing_indent = 0usize;

                loop {
                    // Check for closing indent before consuming content token
                    // (it's set after HeredocContent is produced, before HeredocEnd)
                    if let Some(indent) = self.tokenizer.heredoc_closing_indent() {
                        closing_indent = indent;
                    }

                    let next = self.next_token();
                    match next.kind {
                        TokenKind::HeredocContent => {
                            content.push_str(next.text);
                        }
                        TokenKind::HeredocEnd => {
                            end_span = next.span;
                            break;
                        }
                        TokenKind::Eof => {
                            return Lexeme::Error {
                                span: start_span,
                                message: "unterminated heredoc",
                            };
                        }
                        _ => {
                            return Lexeme::Error {
                                span: next.span,
                                message: "unexpected token in heredoc",
                            };
                        }
                    }
                }

                // Apply dedent if closing delimiter was indented
                if closing_indent > 0 {
                    content = dedent_heredoc(&content, closing_indent);
                }

                Lexeme::Scalar {
                    span: Span::new(start_span.start, end_span.end),
                    value: Cow::Owned(content),
                    kind: ScalarKind::Heredoc,
                }
            }

            TokenKind::HeredocContent | TokenKind::HeredocEnd => {
                // Should not see these outside heredoc context
                Lexeme::Error {
                    span: tok.span,
                    message: "unexpected heredoc token",
                }
            }

            TokenKind::Whitespace => {
                // Should have been skipped above
                unreachable!("whitespace should be skipped")
            }

            TokenKind::Error => Lexeme::Error {
                span: tok.span,
                message: "tokenizer error",
            },
        }
    }
}

impl<'src> Iterator for Lexer<'src> {
    type Item = Lexeme<'src>;

    fn next(&mut self) -> Option<Self::Item> {
        let lexeme = self.next_lexeme();
        if matches!(lexeme, Lexeme::Eof) {
            None
        } else {
            Some(lexeme)
        }
    }
}

/// Strip up to `indent_len` whitespace characters from the start of each line.
fn dedent_heredoc(content: &str, indent_len: usize) -> String {
    let mut result = String::with_capacity(content.len());
    for (i, line) in content.split('\n').enumerate() {
        if i > 0 {
            result.push('\n');
        }
        // Strip up to indent_len whitespace chars from start of line
        let mut stripped = 0;
        let mut char_indices = line.char_indices().peekable();
        while stripped < indent_len {
            if let Some(&(_, ch)) = char_indices.peek() {
                if ch == ' ' || ch == '\t' {
                    char_indices.next();
                    stripped += 1;
                } else {
                    break;
                }
            } else {
                break;
            }
        }
        // Append the rest of the line
        if let Some(&(idx, _)) = char_indices.peek() {
            result.push_str(&line[idx..]);
        }
    }
    result
}

/// Process escape sequences in a quoted string.
fn process_escapes(s: &str) -> Result<Cow<'_, str>, &'static str> {
    // Fast path: no escapes
    if !s.contains('\\') {
        return Ok(Cow::Borrowed(s));
    }

    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c != '\\' {
            result.push(c);
            continue;
        }

        match chars.next() {
            Some('\\') => result.push('\\'),
            Some('"') => result.push('"'),
            Some('n') => result.push('\n'),
            Some('r') => result.push('\r'),
            Some('t') => result.push('\t'),
            Some('u') => {
                // Unicode escape: \uXXXX or \u{X...}
                match chars.peek() {
                    Some('{') => {
                        chars.next(); // consume '{'
                        let mut hex = String::new();
                        loop {
                            match chars.next() {
                                Some('}') => break,
                                Some(c) if c.is_ascii_hexdigit() => hex.push(c),
                                _ => return Err("invalid unicode escape"),
                            }
                        }
                        let code =
                            u32::from_str_radix(&hex, 16).map_err(|_| "invalid unicode escape")?;
                        let ch = char::from_u32(code).ok_or("invalid unicode code point")?;
                        result.push(ch);
                    }
                    Some(_) => {
                        // \uXXXX - exactly 4 hex digits
                        let mut hex = String::with_capacity(4);
                        for _ in 0..4 {
                            match chars.next() {
                                Some(c) if c.is_ascii_hexdigit() => hex.push(c),
                                _ => return Err("invalid unicode escape"),
                            }
                        }
                        let code =
                            u32::from_str_radix(&hex, 16).map_err(|_| "invalid unicode escape")?;
                        let ch = char::from_u32(code).ok_or("invalid unicode code point")?;
                        result.push(ch);
                    }
                    None => return Err("invalid unicode escape"),
                }
            }
            Some(_) => return Err("invalid escape sequence"),
            None => return Err("trailing backslash"),
        }
    }

    Ok(Cow::Owned(result))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_escapes_double_backslash() {
        // Input: path\\to\\file (two backslash pairs)
        // Expected: path\to\file (two literal backslashes)
        let result = process_escapes(r"path\\to\\file").unwrap();
        assert_eq!(result, r"path\to\file");
    }

    fn lex(source: &str) -> Vec<Lexeme<'_>> {
        Lexer::new(source).collect()
    }

    #[test]
    fn test_unit() {
        let lexemes = lex("@");
        assert!(matches!(&lexemes[0], Lexeme::Unit { .. }));
    }

    #[test]
    fn test_tag_no_payload() {
        let lexemes = lex("@foo");
        assert!(matches!(
            &lexemes[0],
            Lexeme::Tag {
                name: "foo",
                has_payload: false,
                ..
            }
        ));
    }

    #[test]
    fn test_tag_with_object_payload() {
        let lexemes = lex("@tag{}");
        assert!(matches!(
            &lexemes[0],
            Lexeme::Tag {
                name: "tag",
                has_payload: true,
                ..
            }
        ));
        assert!(matches!(&lexemes[1], Lexeme::ObjectStart { .. }));
        assert!(matches!(&lexemes[2], Lexeme::ObjectEnd { .. }));
    }

    #[test]
    fn test_tag_with_space_before_object() {
        // @tag {} - space means NOT a payload
        let lexemes = lex("@tag {}");
        assert!(matches!(
            &lexemes[0],
            Lexeme::Tag {
                name: "tag",
                has_payload: false,
                ..
            }
        ));
    }

    #[test]
    fn test_bare_scalar() {
        let lexemes = lex("hello");
        assert!(matches!(
            &lexemes[0],
            Lexeme::Scalar {
                kind: ScalarKind::Bare,
                ..
            }
        ));
    }

    #[test]
    fn test_quoted_scalar() {
        let lexemes = lex(r#""hello\nworld""#);
        match &lexemes[0] {
            Lexeme::Scalar {
                value,
                kind: ScalarKind::Quoted,
                ..
            } => {
                assert_eq!(value.as_ref(), "hello\nworld");
            }
            other => panic!("expected quoted scalar, got {:?}", other),
        }
    }

    #[test]
    fn test_raw_scalar() {
        let lexemes = lex(r##"r#"hello"#"##);
        match &lexemes[0] {
            Lexeme::Scalar {
                value,
                kind: ScalarKind::Raw,
                ..
            } => {
                assert_eq!(value.as_ref(), "hello");
            }
            other => panic!("expected raw scalar, got {:?}", other),
        }
    }

    #[test]
    fn test_tag_with_quoted_payload() {
        let lexemes = lex(r#"@env"staging""#);
        assert!(matches!(
            &lexemes[0],
            Lexeme::Tag {
                name: "env",
                has_payload: true,
                ..
            }
        ));
        match &lexemes[1] {
            Lexeme::Scalar {
                value,
                kind: ScalarKind::Quoted,
                ..
            } => {
                assert_eq!(value.as_ref(), "staging");
            }
            other => panic!("expected quoted scalar, got {:?}", other),
        }
    }

    #[test]
    fn test_tag_with_sequence_payload() {
        let lexemes = lex("@rgb(255 128 0)");
        assert!(matches!(
            &lexemes[0],
            Lexeme::Tag {
                name: "rgb",
                has_payload: true,
                ..
            }
        ));
        assert!(matches!(&lexemes[1], Lexeme::SeqStart { .. }));
    }

    #[test]
    fn test_tag_with_unit_payload() {
        // @tag@ - tag with explicit unit payload
        let lexemes = lex("@tag@");
        assert!(matches!(
            &lexemes[0],
            Lexeme::Tag {
                name: "tag",
                has_payload: true,
                ..
            }
        ));
        assert!(matches!(&lexemes[1], Lexeme::Unit { .. }));
    }

    #[test]
    fn test_tag_with_raw_payload() {
        // @tagr#"x"# - tag "tag" with raw string payload
        let lexemes = lex(r##"@tagr#"x"#"##);
        assert!(matches!(
            &lexemes[0],
            Lexeme::Tag {
                name: "tag",
                has_payload: true,
                ..
            }
        ));
        match &lexemes[1] {
            Lexeme::Scalar {
                value,
                kind: ScalarKind::Raw,
                ..
            } => {
                assert_eq!(value.as_ref(), "x");
            }
            other => panic!("expected raw scalar, got {:?}", other),
        }
    }

    #[test]
    fn test_tag_with_space_before_sequence() {
        let lexemes = lex("@tag (a b)");
        assert!(matches!(
            &lexemes[0],
            Lexeme::Tag {
                name: "tag",
                has_payload: false,
                ..
            }
        ));
    }

    #[test]
    fn test_tag_with_space_before_quoted() {
        let lexemes = lex(r#"@tag "value""#);
        assert!(matches!(
            &lexemes[0],
            Lexeme::Tag {
                name: "tag",
                has_payload: false,
                ..
            }
        ));
    }

    // Note: @tag@ (explicit unit payload) requires tokenizer changes
    // The tokenizer currently produces `At` + `BareScalar("tag@")` because
    // `@` is allowed in bare scalars after the first char.
    // This will be addressed when we update the tokenizer.

    #[test]
    fn test_at_followed_by_digit() {
        // @123 is an invalid tag name - the error span includes both @ and 123
        let lexemes = lex("@123");
        assert!(matches!(
            &lexemes[0],
            Lexeme::Error {
                message: "invalid tag name",
                ..
            }
        ));
    }

    #[test]
    fn test_structural() {
        let lexemes = lex("{x 1}");
        assert!(matches!(&lexemes[0], Lexeme::ObjectStart { .. }));
        assert!(matches!(&lexemes[1], Lexeme::Scalar { .. }));
        assert!(matches!(&lexemes[2], Lexeme::Scalar { .. }));
        assert!(matches!(&lexemes[3], Lexeme::ObjectEnd { .. }));
    }

    #[test]
    fn test_sequence() {
        let lexemes = lex("(a b)");
        assert!(matches!(&lexemes[0], Lexeme::SeqStart { .. }));
        assert!(matches!(&lexemes[1], Lexeme::Scalar { .. }));
        assert!(matches!(&lexemes[2], Lexeme::Scalar { .. }));
        assert!(matches!(&lexemes[3], Lexeme::SeqEnd { .. }));
    }

    #[test]
    fn test_newlines_preserved() {
        let lexemes = lex("a\nb");
        assert!(matches!(&lexemes[0], Lexeme::Scalar { .. }));
        assert!(matches!(&lexemes[1], Lexeme::Newline { .. }));
        assert!(matches!(&lexemes[2], Lexeme::Scalar { .. }));
    }

    #[test]
    fn test_unicode_escape_braces() {
        let lexemes = lex(r#""\u{1F600}""#);
        match &lexemes[0] {
            Lexeme::Scalar { value, .. } => {
                assert_eq!(value.as_ref(), "😀");
            }
            other => panic!("expected scalar, got {:?}", other),
        }
    }

    #[test]
    fn test_unicode_escape_4digit() {
        let lexemes = lex(r#""\u0041""#);
        match &lexemes[0] {
            Lexeme::Scalar { value, .. } => {
                assert_eq!(value.as_ref(), "A");
            }
            other => panic!("expected scalar, got {:?}", other),
        }
    }

    #[test]
    fn test_dotted_value_is_scalar() {
        // Dots in bare scalars are just part of the value
        // Parser handles dot-splitting for keys
        let lexemes = lex("a.b.c");
        match &lexemes[0] {
            Lexeme::Scalar {
                value,
                kind: ScalarKind::Bare,
                ..
            } => {
                assert_eq!(value.as_ref(), "a.b.c");
            }
            other => panic!("expected scalar, got {:?}", other),
        }
    }

    #[test]
    fn test_attr_key() {
        let lexemes = lex("name>value");
        assert!(matches!(&lexemes[0], Lexeme::AttrKey { key: "name", .. }));
        assert!(matches!(&lexemes[1], Lexeme::Scalar { .. }));
    }

    #[test]
    fn test_attr_key_with_object() {
        let lexemes = lex("opts>{x 1}");
        assert!(matches!(&lexemes[0], Lexeme::AttrKey { key: "opts", .. }));
        assert!(matches!(&lexemes[1], Lexeme::ObjectStart { .. }));
    }

    #[test]
    fn test_attr_key_with_sequence() {
        let lexemes = lex("tags>(a b)");
        assert!(matches!(&lexemes[0], Lexeme::AttrKey { key: "tags", .. }));
        assert!(matches!(&lexemes[1], Lexeme::SeqStart { .. }));
    }

    #[test]
    fn test_standalone_gt_error() {
        // `x > y` with spaces - the `>` is not attribute syntax
        let lexemes = lex("x > y");
        assert!(matches!(&lexemes[0], Lexeme::Scalar { .. }));
        assert!(matches!(&lexemes[1], Lexeme::Error { .. }));
    }

    #[test]
    fn test_attr_whitespace_after_gt_error() {
        // `name> value` with space after `>` is an error
        let lexemes = lex("name> value");
        assert!(matches!(
            &lexemes[0],
            Lexeme::Error {
                message: "whitespace after `>` in attribute (use key>value with no spaces)",
                ..
            }
        ));
    }
}
