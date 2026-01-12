//! Event-based parser for Styx.

use std::borrow::Cow;
use std::iter::Peekable;

use crate::Span;
use crate::callback::ParseCallback;
use crate::event::{Event, ScalarKind, Separator};
use crate::lexer::Lexer;
use crate::token::{Token, TokenKind};

/// Event-based parser for Styx documents.
pub struct Parser<'src> {
    lexer: Peekable<LexerIter<'src>>,
}

/// Wrapper to make Lexer into an Iterator.
struct LexerIter<'src> {
    lexer: Lexer<'src>,
    done: bool,
}

impl<'src> Iterator for LexerIter<'src> {
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

impl<'src> Parser<'src> {
    /// Create a new parser for the given source.
    pub fn new(source: &'src str) -> Self {
        let lexer = Lexer::new(source);
        Self {
            lexer: LexerIter { lexer, done: false }.peekable(),
        }
    }

    /// Parse and emit events to callback.
    pub fn parse<C: ParseCallback<'src>>(mut self, callback: &mut C) {
        if !callback.event(Event::DocumentStart) {
            return;
        }

        // Parse top-level entries (implicit object at document root)
        self.parse_entries(callback, None);

        callback.event(Event::DocumentEnd);
    }

    /// Convenience: parse and collect all events.
    pub fn parse_to_vec(self) -> Vec<Event<'src>> {
        let mut events = Vec::new();
        self.parse(&mut events);
        events
    }

    /// Peek at the next token.
    fn peek(&mut self) -> Option<&Token<'src>> {
        // Skip whitespace when peeking
        while let Some(token) = self.lexer.peek() {
            if token.kind == TokenKind::Whitespace {
                self.lexer.next();
            } else {
                break;
            }
        }
        self.lexer.peek()
    }

    /// Peek at the next token without skipping whitespace.
    fn peek_raw(&mut self) -> Option<&Token<'src>> {
        self.lexer.peek()
    }

    /// Consume the next token.
    fn advance(&mut self) -> Option<Token<'src>> {
        self.lexer.next()
    }

    /// Skip whitespace tokens.
    fn skip_whitespace(&mut self) {
        while let Some(token) = self.lexer.peek() {
            if token.kind == TokenKind::Whitespace {
                self.lexer.next();
            } else {
                break;
            }
        }
    }

    /// Skip whitespace and newlines.
    fn skip_whitespace_and_newlines(&mut self) {
        while let Some(token) = self.lexer.peek() {
            if token.kind == TokenKind::Whitespace || token.kind == TokenKind::Newline {
                self.lexer.next();
            } else {
                break;
            }
        }
    }

    /// Parse entries in an object or at document level.
    fn parse_entries<C: ParseCallback<'src>>(
        &mut self,
        callback: &mut C,
        closing: Option<TokenKind>,
    ) {
        self.skip_whitespace_and_newlines();

        loop {
            // Check for closing token or EOF
            if let Some(token) = self.peek() {
                if token.kind == TokenKind::Eof {
                    break;
                }
                if let Some(close) = closing {
                    if token.kind == close {
                        break;
                    }
                }
            } else {
                break;
            }

            // Handle doc comments
            if let Some(token) = self.peek() {
                if token.kind == TokenKind::DocComment {
                    let token = self.advance().unwrap();
                    if !callback.event(Event::DocComment {
                        span: token.span,
                        text: token.text,
                    }) {
                        return;
                    }
                    self.skip_whitespace_and_newlines();
                    continue;
                }

                // Handle line comments
                if token.kind == TokenKind::LineComment {
                    let token = self.advance().unwrap();
                    if !callback.event(Event::Comment {
                        span: token.span,
                        text: token.text,
                    }) {
                        return;
                    }
                    self.skip_whitespace_and_newlines();
                    continue;
                }
            }

            // Parse entry
            if !self.parse_entry(callback) {
                return;
            }

            // Skip entry separator (newlines or comma handled in parse_entry)
            self.skip_whitespace_and_newlines();
        }
    }

    /// Parse a single entry (key and optional value).
    fn parse_entry<C: ParseCallback<'src>>(&mut self, callback: &mut C) -> bool {
        if !callback.event(Event::EntryStart) {
            return false;
        }

        // Collect atoms for this entry
        let atoms = self.collect_entry_atoms();

        if atoms.is_empty() {
            // Empty entry - just end it
            return callback.event(Event::EntryEnd);
        }

        // First atom is the key
        let key_atom = &atoms[0];
        if !callback.event(Event::Key {
            span: key_atom.span,
            value: self.process_scalar_value(key_atom),
            kind: key_atom.kind,
        }) {
            return false;
        }

        if atoms.len() == 1 {
            // Just a key, implicit unit value
            if !callback.event(Event::Unit {
                span: key_atom.span,
            }) {
                return false;
            }
        } else if atoms.len() == 2 {
            // Key and value
            if !self.emit_atom_as_value(&atoms[1], callback) {
                return false;
            }
        } else {
            // Multiple atoms: nested key path
            // a b c → key=a, value={b: c}
            // Emit as implicit nested object
            let start_span = atoms[1].span;
            if !callback.event(Event::ObjectStart {
                span: start_span,
                separator: Separator::Newline,
            }) {
                return false;
            }

            // Recursively emit remaining atoms as entry
            if !callback.event(Event::EntryStart) {
                return false;
            }

            // Second atom becomes key
            if !callback.event(Event::Key {
                span: atoms[1].span,
                value: self.process_scalar_value(&atoms[1]),
                kind: atoms[1].kind,
            }) {
                return false;
            }

            // Rest become value(s)
            if atoms.len() == 3 {
                if !self.emit_atom_as_value(&atoms[2], callback) {
                    return false;
                }
            } else {
                // Even more nesting
                let inner_start = atoms[2].span;
                if !callback.event(Event::ObjectStart {
                    span: inner_start,
                    separator: Separator::Newline,
                }) {
                    return false;
                }

                // Continue recursively for remaining atoms
                if !self.emit_nested_atoms(&atoms[2..], callback) {
                    return false;
                }

                if !callback.event(Event::ObjectEnd { span: inner_start }) {
                    return false;
                }
            }

            if !callback.event(Event::EntryEnd) {
                return false;
            }

            if !callback.event(Event::ObjectEnd { span: start_span }) {
                return false;
            }
        }

        callback.event(Event::EntryEnd)
    }

    /// Emit nested atoms as key-value pairs.
    fn emit_nested_atoms<C: ParseCallback<'src>>(
        &self,
        atoms: &[Atom<'src>],
        callback: &mut C,
    ) -> bool {
        if atoms.is_empty() {
            return true;
        }

        if !callback.event(Event::EntryStart) {
            return false;
        }

        if !callback.event(Event::Key {
            span: atoms[0].span,
            value: self.process_scalar_value(&atoms[0]),
            kind: atoms[0].kind,
        }) {
            return false;
        }

        if atoms.len() == 1 {
            if !callback.event(Event::Unit {
                span: atoms[0].span,
            }) {
                return false;
            }
        } else if atoms.len() == 2 {
            if !self.emit_atom_as_value(&atoms[1], callback) {
                return false;
            }
        } else {
            let inner_start = atoms[1].span;
            if !callback.event(Event::ObjectStart {
                span: inner_start,
                separator: Separator::Newline,
            }) {
                return false;
            }

            if !self.emit_nested_atoms(&atoms[1..], callback) {
                return false;
            }

            if !callback.event(Event::ObjectEnd { span: inner_start }) {
                return false;
            }
        }

        callback.event(Event::EntryEnd)
    }

    /// Collect atoms until entry boundary (newline, comma, closing brace/paren, or EOF).
    fn collect_entry_atoms(&mut self) -> Vec<Atom<'src>> {
        let mut atoms = Vec::new();

        loop {
            self.skip_whitespace();

            let Some(token) = self.peek() else {
                break;
            };

            match token.kind {
                // Entry boundaries
                TokenKind::Newline | TokenKind::Comma | TokenKind::Eof => break,
                TokenKind::RBrace | TokenKind::RParen => break,

                // Comments end the entry
                TokenKind::LineComment | TokenKind::DocComment => break,

                // Nested structures
                TokenKind::LBrace => {
                    atoms.push(self.parse_object_atom());
                }
                TokenKind::LParen => {
                    atoms.push(self.parse_sequence_atom());
                }

                // Tags
                TokenKind::At => {
                    atoms.push(self.parse_tag_or_unit_atom());
                }

                // Scalars
                TokenKind::BareScalar
                | TokenKind::QuotedScalar
                | TokenKind::RawScalar
                | TokenKind::HeredocStart => {
                    atoms.push(self.parse_scalar_atom());
                }

                // Skip whitespace (handled above)
                TokenKind::Whitespace => {
                    self.advance();
                }

                // Unexpected tokens
                _ => {
                    // Skip and continue
                    self.advance();
                }
            }
        }

        atoms
    }

    /// Parse a scalar atom.
    fn parse_scalar_atom(&mut self) -> Atom<'src> {
        let token = self.advance().unwrap();
        match token.kind {
            TokenKind::BareScalar => Atom {
                span: token.span,
                kind: ScalarKind::Bare,
                content: AtomContent::Scalar(token.text),
            },
            TokenKind::QuotedScalar => Atom {
                span: token.span,
                kind: ScalarKind::Quoted,
                content: AtomContent::Scalar(token.text),
            },
            TokenKind::RawScalar => Atom {
                span: token.span,
                kind: ScalarKind::Raw,
                content: AtomContent::Scalar(token.text),
            },
            TokenKind::HeredocStart => {
                // Collect heredoc content
                let start_span = token.span;
                let mut content = String::new();
                let mut end_span = start_span;

                loop {
                    let Some(token) = self.advance() else {
                        break;
                    };
                    match token.kind {
                        TokenKind::HeredocContent => {
                            content.push_str(token.text);
                        }
                        TokenKind::HeredocEnd => {
                            end_span = token.span;
                            break;
                        }
                        _ => break,
                    }
                }

                Atom {
                    span: Span {
                        start: start_span.start,
                        end: end_span.end,
                    },
                    kind: ScalarKind::Heredoc,
                    content: AtomContent::Heredoc(content),
                }
            }
            _ => unreachable!(),
        }
    }

    /// Parse an object atom (for nested objects).
    fn parse_object_atom(&mut self) -> Atom<'src> {
        let open = self.advance().unwrap(); // consume '{'
        let start_span = open.span;

        // For now, just track the braces and collect everything as nested
        let mut depth = 1;
        let mut end_span = start_span;

        while depth > 0 {
            let Some(token) = self.advance() else {
                break;
            };
            match token.kind {
                TokenKind::LBrace => depth += 1,
                TokenKind::RBrace => {
                    depth -= 1;
                    if depth == 0 {
                        end_span = token.span;
                    }
                }
                TokenKind::Eof => break,
                _ => {}
            }
        }

        Atom {
            span: Span {
                start: start_span.start,
                end: end_span.end,
            },
            kind: ScalarKind::Bare, // placeholder
            content: AtomContent::Object,
        }
    }

    /// Parse a sequence atom.
    fn parse_sequence_atom(&mut self) -> Atom<'src> {
        let open = self.advance().unwrap(); // consume '('
        let start_span = open.span;

        let mut depth = 1;
        let mut end_span = start_span;

        while depth > 0 {
            let Some(token) = self.advance() else {
                break;
            };
            match token.kind {
                TokenKind::LParen => depth += 1,
                TokenKind::RParen => {
                    depth -= 1;
                    if depth == 0 {
                        end_span = token.span;
                    }
                }
                TokenKind::Eof => break,
                _ => {}
            }
        }

        Atom {
            span: Span {
                start: start_span.start,
                end: end_span.end,
            },
            kind: ScalarKind::Bare,
            content: AtomContent::Sequence,
        }
    }

    /// Parse a tag or unit atom.
    fn parse_tag_or_unit_atom(&mut self) -> Atom<'src> {
        let at = self.advance().unwrap(); // consume '@'
        let start_span = at.span;

        // Check if followed by a tag name
        if let Some(token) = self.peek_raw() {
            if token.kind == TokenKind::BareScalar && token.span.start == start_span.end {
                // Tag name immediately follows @
                let name_token = self.advance().unwrap();
                return Atom {
                    span: Span {
                        start: start_span.start,
                        end: name_token.span.end,
                    },
                    kind: ScalarKind::Bare,
                    content: AtomContent::Tag(name_token.text),
                };
            }
        }

        // Just @ (unit)
        Atom {
            span: start_span,
            kind: ScalarKind::Bare,
            content: AtomContent::Unit,
        }
    }

    /// Emit an atom as a value event.
    fn emit_atom_as_value<C: ParseCallback<'src>>(
        &self,
        atom: &Atom<'src>,
        callback: &mut C,
    ) -> bool {
        match &atom.content {
            AtomContent::Scalar(text) => callback.event(Event::Scalar {
                span: atom.span,
                value: self.process_scalar(text, atom.kind),
                kind: atom.kind,
            }),
            AtomContent::Heredoc(content) => callback.event(Event::Scalar {
                span: atom.span,
                value: Cow::Owned(content.clone()),
                kind: ScalarKind::Heredoc,
            }),
            AtomContent::Unit => callback.event(Event::Unit { span: atom.span }),
            AtomContent::Tag(name) => {
                // For now, emit as a scalar with the tag name
                // TODO: proper tag handling with payload
                if !callback.event(Event::TagStart {
                    span: atom.span,
                    name,
                }) {
                    return false;
                }
                callback.event(Event::TagEnd)
            }
            AtomContent::Object => {
                // Re-parse the object content
                // For now, emit as empty object
                if !callback.event(Event::ObjectStart {
                    span: atom.span,
                    separator: Separator::Newline,
                }) {
                    return false;
                }
                callback.event(Event::ObjectEnd { span: atom.span })
            }
            AtomContent::Sequence => {
                // Re-parse the sequence content
                // For now, emit as empty sequence
                if !callback.event(Event::SequenceStart { span: atom.span }) {
                    return false;
                }
                callback.event(Event::SequenceEnd { span: atom.span })
            }
        }
    }

    /// Process scalar value (escape handling for keys).
    fn process_scalar_value(&self, atom: &Atom<'src>) -> Cow<'src, str> {
        match &atom.content {
            AtomContent::Scalar(text) => self.process_scalar(text, atom.kind),
            AtomContent::Heredoc(content) => Cow::Owned(content.clone()),
            AtomContent::Unit => Cow::Borrowed("@"),
            AtomContent::Tag(name) => Cow::Borrowed(name),
            AtomContent::Object => Cow::Borrowed("{}"),
            AtomContent::Sequence => Cow::Borrowed("()"),
        }
    }

    /// Process a scalar, handling escapes for quoted strings.
    fn process_scalar(&self, text: &'src str, kind: ScalarKind) -> Cow<'src, str> {
        match kind {
            ScalarKind::Bare | ScalarKind::Raw | ScalarKind::Heredoc => Cow::Borrowed(text),
            ScalarKind::Quoted => self.unescape_quoted(text),
        }
    }

    /// Unescape a quoted string.
    fn unescape_quoted(&self, text: &'src str) -> Cow<'src, str> {
        // Remove surrounding quotes
        let inner = if text.starts_with('"') && text.ends_with('"') && text.len() >= 2 {
            &text[1..text.len() - 1]
        } else {
            text
        };

        // Check if any escapes present
        if !inner.contains('\\') {
            return Cow::Borrowed(inner);
        }

        // Process escapes
        let mut result = String::with_capacity(inner.len());
        let mut chars = inner.chars().peekable();

        while let Some(c) = chars.next() {
            if c == '\\' {
                match chars.next() {
                    Some('n') => result.push('\n'),
                    Some('r') => result.push('\r'),
                    Some('t') => result.push('\t'),
                    Some('\\') => result.push('\\'),
                    Some('"') => result.push('"'),
                    Some('0') => result.push('\0'),
                    Some('u') => {
                        // Unicode escape \u{XXXX}
                        if chars.next() == Some('{') {
                            let mut hex = String::new();
                            while let Some(&c) = chars.peek() {
                                if c == '}' {
                                    chars.next();
                                    break;
                                }
                                hex.push(chars.next().unwrap());
                            }
                            if let Ok(code) = u32::from_str_radix(&hex, 16) {
                                if let Some(ch) = char::from_u32(code) {
                                    result.push(ch);
                                }
                            }
                        }
                    }
                    Some(c) => {
                        // Unknown escape, keep as-is
                        result.push('\\');
                        result.push(c);
                    }
                    None => {
                        result.push('\\');
                    }
                }
            } else {
                result.push(c);
            }
        }

        Cow::Owned(result)
    }
}

/// An atom collected during entry parsing.
#[derive(Debug)]
struct Atom<'src> {
    span: Span,
    kind: ScalarKind,
    content: AtomContent<'src>,
}

/// Content of an atom.
#[derive(Debug)]
enum AtomContent<'src> {
    Scalar(&'src str),
    Heredoc(String),
    Unit,
    Tag(&'src str),
    Object,
    Sequence,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(source: &str) -> Vec<Event<'_>> {
        Parser::new(source).parse_to_vec()
    }

    #[test]
    fn test_empty_document() {
        let events = parse("");
        assert_eq!(events, vec![Event::DocumentStart, Event::DocumentEnd]);
    }

    #[test]
    fn test_simple_entry() {
        let events = parse("foo bar");
        assert!(events.contains(&Event::DocumentStart));
        assert!(events.contains(&Event::DocumentEnd));
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::Key { value, .. } if value == "foo"))
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::Scalar { value, .. } if value == "bar"))
        );
    }

    #[test]
    fn test_key_only() {
        let events = parse("foo");
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::Key { value, .. } if value == "foo"))
        );
        assert!(events.iter().any(|e| matches!(e, Event::Unit { .. })));
    }

    #[test]
    fn test_multiple_entries() {
        let events = parse("foo bar\nbaz qux");
        let keys: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                Event::Key { value, .. } => Some(value.as_ref()),
                _ => None,
            })
            .collect();
        assert_eq!(keys, vec!["foo", "baz"]);
    }

    #[test]
    fn test_quoted_string() {
        let events = parse(r#"name "hello world""#);
        assert!(events
            .iter()
            .any(|e| matches!(e, Event::Scalar { value, kind: ScalarKind::Quoted, .. } if value == "hello world")));
    }

    #[test]
    fn test_quoted_escape() {
        let events = parse(r#"msg "hello\nworld""#);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::Scalar { value, .. } if value == "hello\nworld"))
        );
    }

    #[test]
    fn test_nested_keys() {
        let events = parse("a b c");
        // Should produce: key=a, value=(implicit object with key=b, value=c)
        let keys: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                Event::Key { value, .. } => Some(value.as_ref()),
                _ => None,
            })
            .collect();
        assert_eq!(keys, vec!["a", "b"]);
    }

    #[test]
    fn test_unit_value() {
        let events = parse("flag @");
        assert!(events.iter().any(|e| matches!(e, Event::Unit { .. })));
    }

    #[test]
    fn test_tag() {
        let events = parse("type @user");
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::TagStart { name, .. } if *name == "user"))
        );
    }

    #[test]
    fn test_comments() {
        let events = parse("// comment\nfoo bar");
        assert!(events.iter().any(|e| matches!(e, Event::Comment { .. })));
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::Key { value, .. } if value == "foo"))
        );
    }

    #[test]
    fn test_doc_comments() {
        let events = parse("/// doc\nfoo bar");
        assert!(events.iter().any(|e| matches!(e, Event::DocComment { .. })));
    }
}
