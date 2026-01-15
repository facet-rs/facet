//! CST parser for Styx using rowan's GreenNodeBuilder.
//!
//! This parser produces a lossless concrete syntax tree that preserves
//! all whitespace, comments, and exact source representation.

use rowan::GreenNode;
use styx_parse::{Lexer, Token, TokenKind};

use crate::syntax_kind::{SyntaxKind, SyntaxNode};

/// A parsed Styx document.
#[derive(Debug, Clone)]
pub struct Parse {
    green: GreenNode,
    errors: Vec<ParseError>,
}

impl Parse {
    /// Get the root syntax node.
    pub fn syntax(&self) -> SyntaxNode {
        SyntaxNode::new_root(self.green.clone())
    }

    /// Get parse errors.
    pub fn errors(&self) -> &[ParseError] {
        &self.errors
    }

    /// Check if parsing succeeded without errors.
    pub fn is_ok(&self) -> bool {
        self.errors.is_empty()
    }

    /// Convert to Result, returning errors if any.
    pub fn ok(self) -> Result<SyntaxNode, Vec<ParseError>> {
        if self.errors.is_empty() {
            Ok(self.syntax())
        } else {
            Err(self.errors)
        }
    }

    /// Get the green node (for testing/debugging).
    pub fn green(&self) -> &GreenNode {
        &self.green
    }
}

/// A parse error with location information.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    /// Byte offset where the error occurred.
    pub offset: u32,
    /// Error message.
    pub message: String,
}

impl ParseError {
    fn new(offset: u32, message: impl Into<String>) -> Self {
        Self {
            offset,
            message: message.into(),
        }
    }
}

/// Parse Styx source into a CST.
pub fn parse(source: &str) -> Parse {
    let parser = CstParser::new(source);
    parser.parse()
}

/// CST parser that builds a green tree using rowan.
struct CstParser<'src> {
    #[allow(dead_code)]
    source: &'src str,
    lexer: std::iter::Peekable<TokenIter<'src>>,
    builder: rowan::GreenNodeBuilder<'static>,
    errors: Vec<ParseError>,
}

/// Iterator adapter for the lexer that includes EOF.
struct TokenIter<'src> {
    lexer: Lexer<'src>,
    done: bool,
}

impl<'src> Iterator for TokenIter<'src> {
    type Item = Token<'src>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        let token = self.lexer.next_token();
        if token.kind == TokenKind::Eof {
            self.done = true;
        }
        Some(token)
    }
}

impl<'src> CstParser<'src> {
    fn new(source: &'src str) -> Self {
        let lexer = Lexer::new(source);
        Self {
            source,
            lexer: TokenIter { lexer, done: false }.peekable(),
            builder: rowan::GreenNodeBuilder::new(),
            errors: Vec::new(),
        }
    }

    fn parse(mut self) -> Parse {
        self.builder.start_node(SyntaxKind::DOCUMENT.into());
        self.parse_entries(None);
        self.builder.finish_node();

        Parse {
            green: self.builder.finish(),
            errors: self.errors,
        }
    }

    /// Peek at the current token kind.
    fn peek(&mut self) -> TokenKind {
        self.lexer.peek().map(|t| t.kind).unwrap_or(TokenKind::Eof)
    }

    /// Peek at the current token.
    fn peek_token(&mut self) -> Option<&Token<'src>> {
        self.lexer.peek()
    }

    /// Get the current token's start position, if any.
    fn current_pos(&mut self) -> u32 {
        self.lexer.peek().map(|t| t.span.start).unwrap_or(0)
    }

    /// Consume and add the current token to the tree.
    fn bump(&mut self) {
        if let Some(token) = self.lexer.next() {
            self.builder
                .token(SyntaxKind::from(token.kind).into(), token.text);
        }
    }

    /// Skip trivia (whitespace and line comments), adding them to the tree.
    fn skip_trivia(&mut self) {
        while matches!(
            self.peek(),
            TokenKind::Whitespace | TokenKind::Newline | TokenKind::LineComment
        ) {
            self.bump();
        }
    }

    /// Skip horizontal whitespace only.
    fn skip_whitespace(&mut self) {
        while self.peek() == TokenKind::Whitespace {
            self.bump();
        }
    }

    /// Check if we're at EOF.
    fn at_eof(&mut self) -> bool {
        self.peek() == TokenKind::Eof
    }

    /// Check if we're at a token that ends an entry.
    fn at_entry_end(&mut self, closing: Option<TokenKind>) -> bool {
        let kind = self.peek();
        kind == TokenKind::Eof
            || kind == TokenKind::Newline
            || kind == TokenKind::Comma
            || closing.is_some_and(|c| kind == c)
    }

    /// Check if the current position starts an attribute (bare_scalar followed by =).
    fn at_attribute(&mut self) -> bool {
        if self.peek() != TokenKind::BareScalar {
            return false;
        }
        // Check if there's an = sign after this bare scalar (possibly with whitespace)
        let token = match self.peek_token() {
            Some(t) => t,
            None => return false,
        };
        let after_scalar = token.span.end as usize;

        // Look for = in the source after the scalar, skipping whitespace
        let rest = &self.source[after_scalar..];
        for ch in rest.chars() {
            match ch {
                ' ' | '\t' => continue,
                '=' => return true,
                _ => return false,
            }
        }
        false
    }

    /// Parse entries (at document level or inside an object).
    fn parse_entries(&mut self, closing: Option<TokenKind>) {
        loop {
            self.skip_trivia();

            // Check for doc comments - they attach to the next entry
            while self.peek() == TokenKind::DocComment {
                self.bump();
                // Skip whitespace/newlines after doc comment
                while matches!(self.peek(), TokenKind::Whitespace | TokenKind::Newline) {
                    self.bump();
                }
            }

            // Check for closing or EOF
            if self.at_eof() {
                break;
            }
            if closing.is_some_and(|close| self.peek() == close) {
                break;
            }

            // Parse an entry
            self.parse_entry(closing);

            // Handle separator
            self.skip_whitespace();
            if matches!(self.peek(), TokenKind::Comma | TokenKind::Newline) {
                self.bump();
            }
        }
    }

    /// Parse a single entry.
    ///
    /// An entry can be:
    /// - A sequence of attributes: `key1=value1 key2=value2`
    /// - A key with zero or more values: `key` or `key value1 value2`
    /// - A key followed by attributes and then more values: `div id=main { ... }`
    fn parse_entry(&mut self, closing: Option<TokenKind>) {
        self.builder.start_node(SyntaxKind::ENTRY.into());

        // Check if this starts with attributes (entry is just attributes)
        if self.at_attribute() {
            self.parse_attributes(closing);
        } else {
            // Parse first atom as the key
            if !self.at_entry_end(closing) {
                self.builder.start_node(SyntaxKind::KEY.into());
                self.parse_atom();
                self.builder.finish_node();
            }

            // Skip horizontal whitespace
            self.skip_whitespace();

            // Parse remaining atoms/attributes as values
            while !self.at_entry_end(closing) {
                // Check if we have attributes next
                if self.at_attribute() {
                    self.builder.start_node(SyntaxKind::VALUE.into());
                    self.parse_attributes(closing);
                    self.builder.finish_node();
                } else {
                    self.builder.start_node(SyntaxKind::VALUE.into());
                    self.parse_atom();
                    self.builder.finish_node();
                }
                self.skip_whitespace();
            }
        }

        self.builder.finish_node();
    }

    /// Parse a sequence of attributes: `key1=value1 key2=value2 ...`
    fn parse_attributes(&mut self, closing: Option<TokenKind>) {
        self.builder.start_node(SyntaxKind::ATTRIBUTES.into());

        while self.at_attribute() {
            self.parse_attribute();
            self.skip_whitespace();

            // Stop if we hit entry end
            if self.at_entry_end(closing) {
                break;
            }
        }

        self.builder.finish_node();
    }

    /// Parse a single attribute: `key=value`
    fn parse_attribute(&mut self) {
        self.builder.start_node(SyntaxKind::ATTRIBUTE.into());

        // Key (bare scalar)
        self.bump();

        // Skip whitespace before =
        self.skip_whitespace();

        // = sign
        if self.peek() == TokenKind::Eq {
            self.bump();
        }

        // Skip whitespace after =
        self.skip_whitespace();

        // Value
        self.parse_atom();

        self.builder.finish_node();
    }

    /// Parse a single atom (scalar, object, sequence, tag, or unit).
    fn parse_atom(&mut self) {
        let kind = self.peek();
        match kind {
            TokenKind::LBrace => self.parse_object(),
            TokenKind::LParen => self.parse_sequence(),
            TokenKind::At => self.parse_tag_or_unit(),
            TokenKind::BareScalar | TokenKind::QuotedScalar | TokenKind::RawScalar => {
                self.builder.start_node(SyntaxKind::SCALAR.into());
                self.bump();
                self.builder.finish_node();
            }
            TokenKind::HeredocStart => self.parse_heredoc(),
            _ => {
                // Error: unexpected token
                let pos = self.current_pos();
                self.errors.push(ParseError::new(
                    pos,
                    format!("unexpected token: {:?}", kind),
                ));
                // Consume the error token
                self.bump();
            }
        }
    }

    /// Parse an object `{ ... }`.
    fn parse_object(&mut self) {
        self.builder.start_node(SyntaxKind::OBJECT.into());

        // Consume `{`
        self.bump();

        // Parse entries until `}`
        self.parse_entries(Some(TokenKind::RBrace));

        // Consume `}` or error
        self.skip_trivia();
        if self.peek() == TokenKind::RBrace {
            self.bump();
        } else {
            let pos = self.current_pos();
            self.errors
                .push(ParseError::new(pos, "unclosed object, expected `}`"));
        }

        self.builder.finish_node();
    }

    /// Parse a sequence `( ... )`.
    fn parse_sequence(&mut self) {
        self.builder.start_node(SyntaxKind::SEQUENCE.into());

        // Consume `(`
        self.bump();

        // Parse elements until `)`
        loop {
            self.skip_trivia();

            if self.at_eof() {
                let pos = self.current_pos();
                self.errors
                    .push(ParseError::new(pos, "unclosed sequence, expected `)`"));
                break;
            }
            if self.peek() == TokenKind::RParen {
                break;
            }

            // In sequences, each element is wrapped in an ENTRY with just a KEY
            self.builder.start_node(SyntaxKind::ENTRY.into());
            self.builder.start_node(SyntaxKind::KEY.into());
            self.parse_atom();
            self.builder.finish_node();
            self.builder.finish_node();

            // Skip whitespace between elements
            self.skip_whitespace();
        }

        // Consume `)` if present
        if self.peek() == TokenKind::RParen {
            self.bump();
        }

        self.builder.finish_node();
    }

    /// Parse `@` (unit) or `@name` (tag).
    fn parse_tag_or_unit(&mut self) {
        // Consume @
        let at_token = self.lexer.next();
        let at_end = at_token.as_ref().map(|t| t.span.end).unwrap_or(0);

        // Check what follows the @
        let (is_unit, next_start) = match self.lexer.peek() {
            None => (true, 0),
            Some(t) => {
                // It's a unit if followed by whitespace, newline, or structural token
                // or if the bare scalar doesn't start immediately after @
                let is_unit = t.kind != TokenKind::BareScalar || t.span.start != at_end;
                (is_unit, t.span.start)
            }
        };
        let _ = next_start; // Silence unused warning

        if is_unit {
            // Just @
            self.builder.start_node(SyntaxKind::UNIT.into());
            if let Some(token) = at_token {
                self.builder.token(SyntaxKind::AT.into(), token.text);
            }
            self.builder.finish_node();
        } else {
            // @name with optional payload
            self.builder.start_node(SyntaxKind::TAG.into());

            // Add @
            if let Some(token) = at_token {
                self.builder.token(SyntaxKind::AT.into(), token.text);
            }

            // Add tag name
            self.builder.start_node(SyntaxKind::TAG_NAME.into());
            self.bump(); // The bare scalar
            self.builder.finish_node();

            // Check for payload (skip whitespace first)
            self.skip_whitespace();

            // If there's a payload (another atom), parse it
            if !matches!(
                self.peek(),
                TokenKind::Eof
                    | TokenKind::Newline
                    | TokenKind::Comma
                    | TokenKind::RBrace
                    | TokenKind::RParen
            ) {
                self.builder.start_node(SyntaxKind::TAG_PAYLOAD.into());
                self.parse_atom();
                self.builder.finish_node();
            }

            self.builder.finish_node();
        }
    }

    /// Parse a heredoc `<<DELIM...DELIM`.
    fn parse_heredoc(&mut self) {
        self.builder.start_node(SyntaxKind::HEREDOC.into());

        // Consume heredoc start
        self.bump();

        // Consume content if present
        if self.peek() == TokenKind::HeredocContent {
            self.bump();
        }

        // Consume end marker
        if self.peek() == TokenKind::HeredocEnd {
            self.bump();
        } else {
            let pos = self.current_pos();
            self.errors
                .push(ParseError::new(pos, "unterminated heredoc"));
        }

        self.builder.finish_node();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_ok(source: &str) -> SyntaxNode {
        let parse = parse(source);
        assert!(parse.is_ok(), "parse errors: {:?}", parse.errors());
        parse.syntax()
    }

    #[allow(dead_code)]
    fn debug_tree(node: &SyntaxNode) -> String {
        format!("{:#?}", node)
    }

    #[test]
    fn test_empty_document() {
        let node = parse_ok("");
        assert_eq!(node.kind(), SyntaxKind::DOCUMENT);
    }

    #[test]
    fn test_simple_entry() {
        let node = parse_ok("host localhost");
        assert_eq!(node.kind(), SyntaxKind::DOCUMENT);

        // Check we have an entry
        let entry = node.children().next().unwrap();
        assert_eq!(entry.kind(), SyntaxKind::ENTRY);
    }

    #[test]
    fn test_object() {
        let node = parse_ok("{ host localhost }");
        let entry = node.children().next().unwrap();
        assert_eq!(entry.kind(), SyntaxKind::ENTRY);

        // The key should contain an object
        let key = entry.children().next().unwrap();
        assert_eq!(key.kind(), SyntaxKind::KEY);

        let obj = key.children().next().unwrap();
        assert_eq!(obj.kind(), SyntaxKind::OBJECT);
    }

    #[test]
    fn test_sequence() {
        let node = parse_ok("items (a b c)");
        let entry = node.children().next().unwrap();
        let value = entry.children().nth(1).unwrap();
        assert_eq!(value.kind(), SyntaxKind::VALUE);

        let seq = value.children().next().unwrap();
        assert_eq!(seq.kind(), SyntaxKind::SEQUENCE);
    }

    #[test]
    fn test_roundtrip() {
        let sources = [
            "host localhost",
            "{ a b, c d }",
            "items (1 2 3)",
            "name \"hello world\"",
            "@unit",
            "@tag payload",
            "// comment\nkey value",
        ];

        for source in sources {
            let parse = parse(source);
            let reconstructed = parse.syntax().to_string();
            assert_eq!(source, reconstructed, "roundtrip failed for: {}", source);
        }
    }

    #[test]
    fn test_preserves_whitespace() {
        let source = "  host   localhost  ";
        let parse = parse(source);
        assert_eq!(source, parse.syntax().to_string());
    }

    #[test]
    fn test_preserves_comments() {
        let source = "// header comment\nhost localhost // trailing";
        let parse = parse(source);
        assert_eq!(source, parse.syntax().to_string());
    }

    #[test]
    fn test_unit() {
        let node = parse_ok("empty @");
        let entry = node.children().next().unwrap();
        let value = entry.children().nth(1).unwrap();
        let unit = value.children().next().unwrap();
        assert_eq!(unit.kind(), SyntaxKind::UNIT);
    }

    #[test]
    fn test_tag_with_payload() {
        let node = parse_ok("@Some value");
        let entry = node.children().next().unwrap();
        let key = entry.children().next().unwrap();
        let tag = key.children().next().unwrap();
        assert_eq!(tag.kind(), SyntaxKind::TAG);
    }

    #[test]
    fn test_heredoc() {
        let source = "content <<EOF\nhello\nworld\nEOF";
        let parse = parse(source);
        assert!(parse.is_ok(), "errors: {:?}", parse.errors());
        assert_eq!(source, parse.syntax().to_string());
    }

    #[test]
    fn test_attributes() {
        let source = "id=main class=\"container\"";
        let parse = parse(source);
        assert!(parse.is_ok(), "errors: {:?}", parse.errors());
        assert_eq!(source, parse.syntax().to_string());

        let entry = parse.syntax().children().next().unwrap();
        let attrs = entry.children().next().unwrap();
        assert_eq!(attrs.kind(), SyntaxKind::ATTRIBUTES);
    }

    #[test]
    fn test_multiple_values() {
        let source = "key value1 value2 value3";
        let parse = parse(source);
        assert!(parse.is_ok(), "errors: {:?}", parse.errors());

        let entry = parse.syntax().children().next().unwrap();
        // Should have KEY + 3 VALUEs
        let children: Vec<_> = entry.children().collect();
        assert_eq!(children.len(), 4);
        assert_eq!(children[0].kind(), SyntaxKind::KEY);
        assert_eq!(children[1].kind(), SyntaxKind::VALUE);
        assert_eq!(children[2].kind(), SyntaxKind::VALUE);
        assert_eq!(children[3].kind(), SyntaxKind::VALUE);
    }

    #[test]
    fn test_showcase_file() {
        let source = include_str!("../../../examples/showcase.styx");
        let parse = parse(source);

        // Should parse without errors
        assert!(parse.is_ok(), "parse errors: {:?}", parse.errors());

        // Should roundtrip perfectly
        assert_eq!(source, parse.syntax().to_string(), "roundtrip failed");
    }
}
