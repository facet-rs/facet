//! Event-based parser for Styx.

use std::borrow::Cow;
use std::collections::HashMap;
use std::iter::Peekable;

use crate::Span;
use crate::callback::ParseCallback;
use crate::event::{Event, ParseErrorKind, ScalarKind, Separator};
use crate::lexer::Lexer;
use crate::token::{Token, TokenKind};
#[allow(unused_imports)]
use crate::trace;

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
    // parser[impl document.root]
    pub fn parse<C: ParseCallback<'src>>(mut self, callback: &mut C) {
        if !callback.event(Event::DocumentStart) {
            return;
        }

        // Skip leading whitespace/newlines and emit any leading comments
        self.skip_whitespace_and_newlines();

        // Emit leading comments before checking for explicit root
        while let Some(token) = self.peek() {
            match token.kind {
                TokenKind::LineComment => {
                    let token = self.advance().unwrap();
                    if !callback.event(Event::Comment {
                        span: token.span,
                        text: token.text,
                    }) {
                        return;
                    }
                    self.skip_whitespace_and_newlines();
                }
                TokenKind::DocComment => {
                    let token = self.advance().unwrap();
                    if !callback.event(Event::DocComment {
                        span: token.span,
                        text: token.text,
                    }) {
                        return;
                    }
                    self.skip_whitespace_and_newlines();
                }
                _ => break,
            }
        }

        // parser[impl document.root]
        // If the document starts with `{`, parse as a single explicit block object
        if matches!(self.peek(), Some(t) if t.kind == TokenKind::LBrace) {
            let obj = self.parse_object_atom();
            self.emit_atom_as_value(&obj, callback);
        } else {
            // Parse top-level entries (implicit object at document root)
            self.parse_entries(callback, None);
        }

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
    // parser[impl entry.key-equality]
    fn parse_entries<C: ParseCallback<'src>>(
        &mut self,
        callback: &mut C,
        closing: Option<TokenKind>,
    ) {
        trace!("Parsing entries, closing token: {:?}", closing);
        let mut seen_keys: HashMap<KeyValue, Span> = HashMap::new();
        // Track last doc comment span for dangling detection
        // parser[impl comment.doc]
        let mut pending_doc_comment: Option<Span> = None;

        self.skip_whitespace_and_newlines();

        while let Some(token) = self.peek() {
            // Check for closing token or EOF
            if token.kind == TokenKind::Eof {
                break;
            }
            if let Some(close) = closing
                && token.kind == close
            {
                break;
            }

            // Handle doc comments
            if token.kind == TokenKind::DocComment {
                let token = self.advance().unwrap();
                pending_doc_comment = Some(token.span);
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

            // We're about to parse an entry, so any pending doc comment is attached
            pending_doc_comment = None;

            // Parse entry with duplicate key detection
            if !self.parse_entry_with_dup_check(callback, &mut seen_keys) {
                return;
            }

            // Skip entry separator (newlines or comma handled in parse_entry)
            self.skip_whitespace_and_newlines();
        }

        // parser[impl comment.doc]
        // If we exited with a pending doc comment, it's dangling (not followed by entry)
        if let Some(span) = pending_doc_comment {
            callback.event(Event::Error {
                span,
                kind: ParseErrorKind::DanglingDocComment,
            });
        }
    }

    /// Parse a single entry with duplicate key detection.
    // parser[impl entry.key-equality] parser[impl entry.structure]
    fn parse_entry_with_dup_check<C: ParseCallback<'src>>(
        &mut self,
        callback: &mut C,
        seen_keys: &mut HashMap<KeyValue, Span>,
    ) -> bool {
        if !callback.event(Event::EntryStart) {
            return false;
        }

        // Collect atoms for this entry
        let atoms = self.collect_entry_atoms();

        if atoms.is_empty() {
            // Empty entry - just end it
            return callback.event(Event::EntryEnd);
        }

        // First atom is the key - check for duplicates and invalid key types
        let key_atom = &atoms[0];

        // parser[impl entry.keys]
        // Heredoc scalars are not allowed as keys
        if key_atom.kind == ScalarKind::Heredoc
            && !callback.event(Event::Error {
                span: key_atom.span,
                kind: ParseErrorKind::InvalidKey,
            })
        {
            return false;
        }

        let key_value = KeyValue::from_atom(key_atom, self);
        if let Some(&original_span) = seen_keys.get(&key_value) {
            // Emit duplicate key error with reference to original
            if !callback.event(Event::Error {
                span: key_atom.span,
                kind: ParseErrorKind::DuplicateKey {
                    original: original_span,
                },
            }) {
                return false;
            }
        } else {
            seen_keys.insert(key_value, key_atom.span);
        }

        if !self.emit_atom_as_key(key_atom, callback) {
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
            // parser[impl entry.keypath]
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
            if !self.emit_atom_as_key(&atoms[1], callback) {
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

        if !self.emit_atom_as_key(&atoms[0], callback) {
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

                // Bare scalars - check for attribute syntax (key=value)
                // parser[impl attr.syntax] parser[impl entry.keypath.attributes]
                TokenKind::BareScalar => {
                    if self.is_attribute_start() {
                        atoms.push(self.parse_attributes());
                    } else {
                        atoms.push(self.parse_scalar_atom());
                    }
                }

                // Other scalars (quoted, raw, heredoc) - cannot be attribute keys
                TokenKind::QuotedScalar | TokenKind::RawScalar | TokenKind::HeredocStart => {
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

    /// Check if current position starts an attribute (bare_scalar immediately followed by =).
    // parser[impl attr.syntax]
    fn is_attribute_start(&mut self) -> bool {
        // We always try to parse as attribute; parse_attributes handles the fallback
        // if = doesn't immediately follow the bare scalar.
        true
    }

    /// Parse one or more attributes (key=value pairs).
    /// If the first token is not followed by =, returns a regular scalar atom.
    // parser[impl attr.syntax] parser[impl attr.values] parser[impl attr.atom]
    fn parse_attributes(&mut self) -> Atom<'src> {
        // First, consume the bare scalar (potential key)
        let first_token = self.advance().unwrap();
        let start_span = first_token.span;
        let first_key = first_token.text;

        // Check if = immediately follows (no whitespace)
        // Extract the info we need before borrowing self again
        let eq_info = self.peek_raw().map(|t| (t.kind, t.span.start, t.span.end));

        let Some((eq_kind, eq_start, eq_end)) = eq_info else {
            // No more tokens - return as regular scalar
            return Atom {
                span: start_span,
                kind: ScalarKind::Bare,
                content: AtomContent::Scalar(first_key),
            };
        };

        if eq_kind != TokenKind::Eq || eq_start != start_span.end {
            // No = or whitespace gap - return as regular scalar
            return Atom {
                span: start_span,
                kind: ScalarKind::Bare,
                content: AtomContent::Scalar(first_key),
            };
        }

        // Consume the =
        self.advance();

        // Value must immediately follow = (no whitespace)
        let val_start = self.peek_raw().map(|t| t.span.start);

        let Some(val_start) = val_start else {
            // Error: missing value after = - return key as scalar for now
            // TODO: emit error
            return Atom {
                span: start_span,
                kind: ScalarKind::Bare,
                content: AtomContent::Scalar(first_key),
            };
        };

        if val_start != eq_end {
            // Error: whitespace after = - return key as scalar for now
            // TODO: emit error
            return Atom {
                span: start_span,
                kind: ScalarKind::Bare,
                content: AtomContent::Scalar(first_key),
            };
        }

        // Parse the first value
        let first_value = self.parse_attribute_value();
        let Some(first_value) = first_value else {
            // Invalid value type - return key as scalar
            return Atom {
                span: start_span,
                kind: ScalarKind::Bare,
                content: AtomContent::Scalar(first_key),
            };
        };

        let mut attrs = vec![AttributeEntry {
            key: first_key,
            key_span: start_span,
            value: first_value,
        }];

        // Continue parsing more attributes (key=value pairs separated by whitespace)
        loop {
            self.skip_whitespace();

            // Extract token info before consuming
            let token_info = self.peek().map(|t| (t.kind, t.span, t.text));
            let Some((token_kind, key_span, key_text)) = token_info else {
                break;
            };

            // Must be a bare scalar
            if token_kind != TokenKind::BareScalar {
                break;
            }

            // Consume the scalar
            self.advance();

            // Check for = immediately after key
            let eq_info = self.peek_raw().map(|t| (t.kind, t.span.start, t.span.end));
            let Some((eq_kind, eq_start, eq_end)) = eq_info else {
                // No more tokens - we consumed a bare scalar that's not an attribute
                // This is lost, but we stop here
                break;
            };

            if eq_kind != TokenKind::Eq || eq_start != key_span.end {
                // Not an attribute - the consumed scalar is lost
                break;
            }

            // Consume =
            self.advance();

            // Check for value
            let val_start = self.peek_raw().map(|t| t.span.start);
            let Some(val_start) = val_start else {
                break;
            };

            if val_start != eq_end {
                // Whitespace after =
                break;
            }

            let Some(value) = self.parse_attribute_value() else {
                break;
            };

            attrs.push(AttributeEntry {
                key: key_text,
                key_span,
                value,
            });
        }

        let end_span = attrs
            .last()
            .map(|a| a.value.span.end)
            .unwrap_or(start_span.end);

        Atom {
            span: Span {
                start: start_span.start,
                end: end_span,
            },
            kind: ScalarKind::Bare,
            content: AtomContent::Attributes(attrs),
        }
    }

    /// Parse an attribute value (bare/quoted/raw scalar, sequence, or object).
    // parser[impl attr.values]
    fn parse_attribute_value(&mut self) -> Option<Atom<'src>> {
        let token = self.peek()?;

        match token.kind {
            TokenKind::BareScalar | TokenKind::QuotedScalar | TokenKind::RawScalar => {
                Some(self.parse_scalar_atom())
            }
            TokenKind::LParen => Some(self.parse_sequence_atom()),
            TokenKind::LBrace => Some(self.parse_object_atom()),
            TokenKind::At => Some(self.parse_tag_or_unit_atom()),
            // Heredocs are not typically used as attribute values, but support them
            TokenKind::HeredocStart => Some(self.parse_scalar_atom()),
            _ => None,
        }
    }

    /// Parse a scalar atom.
    fn parse_scalar_atom(&mut self) -> Atom<'src> {
        let token = self.advance().unwrap();
        trace!("Parsing scalar: {:?}", token.kind);
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
    // parser[impl object.syntax]
    fn parse_object_atom(&mut self) -> Atom<'src> {
        trace!("Parsing object");
        let open = self.advance().unwrap(); // consume '{'
        let start_span = open.span;

        let mut entries: Vec<ObjectEntry<'src>> = Vec::new();
        let mut separator_mode: Option<Separator> = None;
        let mut end_span = start_span;
        // parser[impl entry.key-equality]
        // Maps key value to its first occurrence span
        let mut seen_keys: HashMap<KeyValue, Span> = HashMap::new();
        // Pairs of (original_span, duplicate_span) for duplicate keys
        let mut duplicate_key_spans: Vec<(Span, Span)> = Vec::new();
        // parser[impl object.separators]
        let mut mixed_separator_spans: Vec<Span> = Vec::new();
        // parser[impl comment.doc]
        let mut pending_doc_comment: Option<Span> = None;
        let mut dangling_doc_comment_spans: Vec<Span> = Vec::new();
        // Track whether the object was properly closed
        let mut unclosed = false;

        loop {
            // Only skip horizontal whitespace initially
            self.skip_whitespace();

            let Some(token) = self.peek() else {
                // Unclosed object - EOF
                unclosed = true;
                // Check for dangling doc comment
                if let Some(span) = pending_doc_comment {
                    dangling_doc_comment_spans.push(span);
                }
                break;
            };

            // Capture span before matching (needed for error reporting)
            let token_span = token.span;

            match token.kind {
                TokenKind::RBrace => {
                    // Check for dangling doc comment before closing
                    if let Some(span) = pending_doc_comment {
                        dangling_doc_comment_spans.push(span);
                    }
                    let close = self.advance().unwrap();
                    end_span = close.span;
                    break;
                }

                TokenKind::Newline => {
                    // parser[impl object.separators]
                    if separator_mode == Some(Separator::Comma) {
                        // Error: mixed separators - record span and continue parsing
                        mixed_separator_spans.push(token_span);
                    }
                    separator_mode = Some(Separator::Newline);
                    self.advance();
                    // Consume consecutive newlines
                    while matches!(self.peek(), Some(t) if t.kind == TokenKind::Newline) {
                        self.advance();
                    }
                }

                TokenKind::Comma => {
                    // parser[impl object.separators]
                    if separator_mode == Some(Separator::Newline) {
                        // Error: mixed separators - record span and continue parsing
                        mixed_separator_spans.push(token_span);
                    }
                    separator_mode = Some(Separator::Comma);
                    self.advance();
                }

                TokenKind::LineComment => {
                    // Skip line comments
                    self.advance();
                }

                TokenKind::DocComment => {
                    // Track doc comment for dangling detection
                    pending_doc_comment = Some(token_span);
                    self.advance();
                }

                TokenKind::Eof => {
                    // Unclosed object
                    unclosed = true;
                    if let Some(span) = pending_doc_comment {
                        dangling_doc_comment_spans.push(span);
                    }
                    break;
                }

                _ => {
                    // About to parse entry, clear pending doc comment
                    pending_doc_comment = None;

                    // Parse entry atoms
                    let entry_atoms = self.collect_entry_atoms();
                    if !entry_atoms.is_empty() {
                        let key = entry_atoms[0].clone();

                        // parser[impl entry.key-equality]
                        // Check for duplicate key
                        let key_value = KeyValue::from_atom(&key, self);
                        if let Some(&original_span) = seen_keys.get(&key_value) {
                            duplicate_key_spans.push((original_span, key.span));
                        } else {
                            seen_keys.insert(key_value, key.span);
                        }

                        let value = if entry_atoms.len() == 1 {
                            // Just a key, implicit unit value
                            Atom {
                                span: key.span,
                                kind: ScalarKind::Bare,
                                content: AtomContent::Unit,
                            }
                        } else if entry_atoms.len() == 2 {
                            // Key and value
                            entry_atoms[1].clone()
                        } else {
                            // Multiple atoms: nested key path (a b c → a: {b: c})
                            self.build_nested_object(&entry_atoms[1..])
                        };
                        entries.push(ObjectEntry { key, value });
                    }
                }
            }
        }

        Atom {
            span: Span {
                start: start_span.start,
                end: end_span.end,
            },
            kind: ScalarKind::Bare,
            content: AtomContent::Object {
                entries,
                separator: separator_mode.unwrap_or(Separator::Newline),
                duplicate_key_spans,
                mixed_separator_spans,
                dangling_doc_comment_spans,
                unclosed,
            },
        }
    }

    /// Build a nested object from a slice of atoms (for key paths like `a b c`).
    fn build_nested_object(&self, atoms: &[Atom<'src>]) -> Atom<'src> {
        if atoms.is_empty() {
            // Shouldn't happen, but return unit as fallback
            return Atom {
                span: Span { start: 0, end: 0 },
                kind: ScalarKind::Bare,
                content: AtomContent::Unit,
            };
        }

        if atoms.len() == 1 {
            return atoms[0].clone();
        }

        // Build nested: first atom is key, rest becomes nested object value
        let key = atoms[0].clone();
        let value = self.build_nested_object(&atoms[1..]);
        let span = Span {
            start: key.span.start,
            end: value.span.end,
        };

        Atom {
            span,
            kind: ScalarKind::Bare,
            content: AtomContent::Object {
                entries: vec![ObjectEntry { key, value }],
                separator: Separator::Newline,
                duplicate_key_spans: Vec::new(), // Nested objects from keypaths can't have duplicates
                mixed_separator_spans: Vec::new(), // Nested objects from keypaths can't have mixed separators
                dangling_doc_comment_spans: Vec::new(), // Nested objects from keypaths can't have doc comments
                unclosed: false, // Nested objects from keypaths are always complete
            },
        }
    }

    /// Parse a sequence atom.
    // parser[impl sequence.syntax] parser[impl sequence.elements]
    fn parse_sequence_atom(&mut self) -> Atom<'src> {
        trace!("Parsing sequence");
        let open = self.advance().unwrap(); // consume '('
        let start_span = open.span;

        let mut elements: Vec<Atom<'src>> = Vec::new();
        let mut end_span = start_span;
        let mut unclosed = false;

        loop {
            // Sequences allow whitespace and newlines between elements
            self.skip_whitespace_and_newlines();

            let Some(token) = self.peek() else {
                // Unclosed sequence - EOF
                unclosed = true;
                break;
            };

            match token.kind {
                TokenKind::RParen => {
                    let close = self.advance().unwrap();
                    end_span = close.span;
                    break;
                }

                TokenKind::Comma => {
                    // Commas are NOT allowed in sequences per spec
                    // TODO: emit error event
                    self.advance(); // skip it and continue
                }

                TokenKind::LineComment | TokenKind::DocComment => {
                    // Skip comments inside sequences
                    self.advance();
                }

                TokenKind::Eof => {
                    // Unclosed sequence
                    unclosed = true;
                    break;
                }

                _ => {
                    // Parse a single element
                    if let Some(elem) = self.parse_single_atom() {
                        elements.push(elem);
                    }
                }
            }
        }

        Atom {
            span: Span {
                start: start_span.start,
                end: end_span.end,
            },
            kind: ScalarKind::Bare,
            content: AtomContent::Sequence { elements, unclosed },
        }
    }

    /// Parse a single atom (for sequence elements).
    fn parse_single_atom(&mut self) -> Option<Atom<'src>> {
        let token = self.peek()?;

        match token.kind {
            TokenKind::BareScalar
            | TokenKind::QuotedScalar
            | TokenKind::RawScalar
            | TokenKind::HeredocStart => Some(self.parse_scalar_atom()),
            TokenKind::LBrace => Some(self.parse_object_atom()),
            TokenKind::LParen => Some(self.parse_sequence_atom()),
            TokenKind::At => Some(self.parse_tag_or_unit_atom()),
            _ => None,
        }
    }

    /// Parse a tag or unit atom.
    // parser[impl tag.payload] parser[impl value.unit]
    fn parse_tag_or_unit_atom(&mut self) -> Atom<'src> {
        trace!("Parsing tag or unit");
        let at = self.advance().unwrap(); // consume '@'
        let start_span = at.span;

        // Check if followed by a tag name (must be immediately adjacent, no whitespace)
        if let Some(token) = self.peek_raw()
            && token.kind == TokenKind::BareScalar
            && token.span.start == start_span.end
        {
            // Tag name immediately follows @
            let name_token = self.advance().unwrap();
            let name = name_token.text;
            let name_span = name_token.span;
            let name_end = name_token.span.end;

            // parser[impl tag.syntax]
            // Validate tag name: must match @[A-Za-z_][A-Za-z0-9_.-]*
            let invalid_tag_name = !Self::is_valid_tag_name(name);

            // Check for payload (must immediately follow tag name, no whitespace)
            let payload = self.parse_tag_payload(name_end);
            let end_span = payload.as_ref().map(|p| p.span.end).unwrap_or(name_end);

            return Atom {
                span: Span {
                    start: start_span.start,
                    end: end_span,
                },
                kind: ScalarKind::Bare,
                content: AtomContent::Tag {
                    name,
                    payload: payload.map(Box::new),
                    invalid_name_span: if invalid_tag_name {
                        Some(name_span)
                    } else {
                        None
                    },
                },
            };
        }

        // Just @ (unit)
        Atom {
            span: start_span,
            kind: ScalarKind::Bare,
            content: AtomContent::Unit,
        }
    }

    /// Check if a tag name is valid per parser[tag.syntax].
    /// Must match pattern: [A-Za-z_][A-Za-z0-9_.-]*
    // parser[impl tag.syntax]
    fn is_valid_tag_name(name: &str) -> bool {
        let mut chars = name.chars();

        // First char: letter or underscore
        match chars.next() {
            Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
            _ => return false,
        }

        // Rest: alphanumeric, underscore, dot, or hyphen
        chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-')
    }

    /// Parse a tag payload if present (must immediately follow tag name).
    // parser[impl tag.payload]
    fn parse_tag_payload(&mut self, after_name: u32) -> Option<Atom<'src>> {
        let Some(token) = self.peek_raw() else {
            return None; // implicit unit
        };

        // Payload must immediately follow tag name (no whitespace)
        if token.span.start != after_name {
            return None; // implicit unit
        }

        match token.kind {
            // @tag{...} - tagged object
            TokenKind::LBrace => Some(self.parse_object_atom()),
            // @tag(...) - tagged sequence
            TokenKind::LParen => Some(self.parse_sequence_atom()),
            // @tag"..." or @tagr#"..."# or @tag<<HEREDOC - tagged scalar
            TokenKind::QuotedScalar | TokenKind::RawScalar | TokenKind::HeredocStart => {
                Some(self.parse_scalar_atom())
            }
            // @tag@ - explicit tagged unit
            TokenKind::At => {
                let at = self.advance().unwrap();
                Some(Atom {
                    span: at.span,
                    kind: ScalarKind::Bare,
                    content: AtomContent::Unit,
                })
            }
            // Anything else - implicit unit (no payload)
            _ => None,
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
            // parser[impl tag.payload]
            AtomContent::Tag {
                name,
                payload,
                invalid_name_span,
            } => {
                // parser[impl tag.syntax]
                // Emit error for invalid tag name
                if let Some(span) = invalid_name_span
                    && !callback.event(Event::Error {
                        span: *span,
                        kind: ParseErrorKind::InvalidTagName,
                    })
                {
                    return false;
                }

                if !callback.event(Event::TagStart {
                    span: atom.span,
                    name,
                }) {
                    return false;
                }
                // Emit payload if present
                if let Some(payload) = payload
                    && !self.emit_atom_as_value(payload, callback)
                {
                    return false;
                }
                // If no payload, it's an implicit unit (TagEnd implies it)
                callback.event(Event::TagEnd)
            }
            // parser[impl object.syntax]
            AtomContent::Object {
                entries,
                separator,
                duplicate_key_spans,
                mixed_separator_spans,
                dangling_doc_comment_spans,
                unclosed,
            } => {
                if !callback.event(Event::ObjectStart {
                    span: atom.span,
                    separator: *separator,
                }) {
                    return false;
                }

                // Emit error for unclosed object
                if *unclosed
                    && !callback.event(Event::Error {
                        span: atom.span,
                        kind: ParseErrorKind::UnclosedObject,
                    })
                {
                    return false;
                }

                // parser[impl entry.key-equality]
                // Emit errors for duplicate keys
                for (original_span, dup_span) in duplicate_key_spans {
                    if !callback.event(Event::Error {
                        span: *dup_span,
                        kind: ParseErrorKind::DuplicateKey {
                            original: *original_span,
                        },
                    }) {
                        return false;
                    }
                }

                // parser[impl object.separators]
                // Emit errors for mixed separators
                for mix_span in mixed_separator_spans {
                    if !callback.event(Event::Error {
                        span: *mix_span,
                        kind: ParseErrorKind::MixedSeparators,
                    }) {
                        return false;
                    }
                }

                // parser[impl comment.doc]
                // Emit errors for dangling doc comments
                for doc_span in dangling_doc_comment_spans {
                    if !callback.event(Event::Error {
                        span: *doc_span,
                        kind: ParseErrorKind::DanglingDocComment,
                    }) {
                        return false;
                    }
                }

                for entry in entries {
                    if !callback.event(Event::EntryStart) {
                        return false;
                    }
                    if !self.emit_atom_as_key(&entry.key, callback) {
                        return false;
                    }
                    if !self.emit_atom_as_value(&entry.value, callback) {
                        return false;
                    }
                    if !callback.event(Event::EntryEnd) {
                        return false;
                    }
                }

                callback.event(Event::ObjectEnd { span: atom.span })
            }
            // parser[impl sequence.syntax] parser[impl sequence.elements]
            AtomContent::Sequence { elements, unclosed } => {
                if !callback.event(Event::SequenceStart { span: atom.span }) {
                    return false;
                }

                // Emit error for unclosed sequence
                if *unclosed
                    && !callback.event(Event::Error {
                        span: atom.span,
                        kind: ParseErrorKind::UnclosedSequence,
                    })
                {
                    return false;
                }

                for elem in elements {
                    if !self.emit_atom_as_value(elem, callback) {
                        return false;
                    }
                }

                callback.event(Event::SequenceEnd { span: atom.span })
            }
            // parser[impl attr.atom]
            AtomContent::Attributes(attrs) => {
                // Emit as comma-separated object
                if !callback.event(Event::ObjectStart {
                    span: atom.span,
                    separator: Separator::Comma,
                }) {
                    return false;
                }

                for attr in attrs {
                    if !callback.event(Event::EntryStart) {
                        return false;
                    }
                    // Attribute keys are always bare scalars
                    if !callback.event(Event::Key {
                        span: attr.key_span,
                        tag: None,
                        payload: Some(Cow::Borrowed(attr.key)),
                        kind: ScalarKind::Bare,
                    }) {
                        return false;
                    }
                    if !self.emit_atom_as_value(&attr.value, callback) {
                        return false;
                    }
                    if !callback.event(Event::EntryEnd) {
                        return false;
                    }
                }

                callback.event(Event::ObjectEnd { span: atom.span })
            }
        }
    }

    /// Emit an atom as a key event.
    ///
    /// Keys can be scalars or unit, optionally tagged.
    /// Objects, sequences, and heredocs are not allowed as keys.
    // parser[impl entry.keys]
    fn emit_atom_as_key<C: ParseCallback<'src>>(
        &self,
        atom: &Atom<'src>,
        callback: &mut C,
    ) -> bool {
        match &atom.content {
            AtomContent::Scalar(text) => callback.event(Event::Key {
                span: atom.span,
                tag: None,
                payload: Some(self.process_scalar(text, atom.kind)),
                kind: atom.kind,
            }),
            AtomContent::Heredoc(_) => {
                // Heredocs are not allowed as keys
                callback.event(Event::Error {
                    span: atom.span,
                    kind: ParseErrorKind::InvalidKey,
                })
            }
            AtomContent::Unit => callback.event(Event::Key {
                span: atom.span,
                tag: None,
                payload: None,
                kind: ScalarKind::Bare,
            }),
            AtomContent::Tag {
                name,
                payload,
                invalid_name_span,
            } => {
                // Emit error for invalid tag name
                if let Some(span) = invalid_name_span
                    && !callback.event(Event::Error {
                        span: *span,
                        kind: ParseErrorKind::InvalidTagName,
                    })
                {
                    return false;
                }

                match payload {
                    None => {
                        // Tagged unit key: @tag
                        callback.event(Event::Key {
                            span: atom.span,
                            tag: Some(name),
                            payload: None,
                            kind: ScalarKind::Bare,
                        })
                    }
                    Some(inner) => match &inner.content {
                        AtomContent::Scalar(text) => {
                            // Tagged scalar key: @tag"value"
                            callback.event(Event::Key {
                                span: atom.span,
                                tag: Some(name),
                                payload: Some(self.process_scalar(text, inner.kind)),
                                kind: inner.kind,
                            })
                        }
                        AtomContent::Unit => {
                            // Tagged unit key: @tag@
                            callback.event(Event::Key {
                                span: atom.span,
                                tag: Some(name),
                                payload: None,
                                kind: ScalarKind::Bare,
                            })
                        }
                        AtomContent::Heredoc(_)
                        | AtomContent::Object { .. }
                        | AtomContent::Sequence { .. }
                        | AtomContent::Tag { .. }
                        | AtomContent::Attributes(_) => {
                            // Invalid key payload
                            callback.event(Event::Error {
                                span: inner.span,
                                kind: ParseErrorKind::InvalidKey,
                            })
                        }
                    },
                }
            }
            AtomContent::Object { .. }
            | AtomContent::Sequence { .. }
            | AtomContent::Attributes(_) => {
                // Objects, sequences not allowed as keys
                callback.event(Event::Error {
                    span: atom.span,
                    kind: ParseErrorKind::InvalidKey,
                })
            }
        }
    }

    /// Process a scalar, handling escapes for quoted strings and stripping delimiters for raw strings.
    fn process_scalar(&self, text: &'src str, kind: ScalarKind) -> Cow<'src, str> {
        match kind {
            ScalarKind::Bare | ScalarKind::Heredoc => Cow::Borrowed(text),
            ScalarKind::Raw => Cow::Borrowed(Self::strip_raw_delimiters(text)),
            ScalarKind::Quoted => self.unescape_quoted(text),
        }
    }

    /// Strip the r#*"..."#* delimiters from a raw string, returning just the content.
    fn strip_raw_delimiters(text: &str) -> &str {
        // Raw string format: r#*"content"#*
        // Skip the 'r'
        let after_r = text.strip_prefix('r').unwrap_or(text);

        // Count and skip opening #s
        let hash_count = after_r.chars().take_while(|&c| c == '#').count();
        let after_hashes = &after_r[hash_count..];

        // Skip opening "
        let after_quote = after_hashes.strip_prefix('"').unwrap_or(after_hashes);

        // Remove closing "# sequence
        let closing_len = 1 + hash_count; // " + #s
        if after_quote.len() >= closing_len {
            &after_quote[..after_quote.len() - closing_len]
        } else {
            after_quote
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
                    // parser[impl scalar.quoted.escapes]
                    Some('u') => {
                        // Unicode escape: \u{X...} or \uXXXX
                        match chars.peek() {
                            Some('{') => {
                                // \u{X...} form - variable length
                                chars.next(); // consume '{'
                                let mut hex = String::new();
                                while let Some(&c) = chars.peek() {
                                    if c == '}' {
                                        chars.next();
                                        break;
                                    }
                                    hex.push(chars.next().unwrap());
                                }
                                if let Ok(code) = u32::from_str_radix(&hex, 16)
                                    && let Some(ch) = char::from_u32(code)
                                {
                                    result.push(ch);
                                }
                            }
                            Some(c) if c.is_ascii_hexdigit() => {
                                // \uXXXX form - exactly 4 hex digits
                                let mut hex = String::with_capacity(4);
                                for _ in 0..4 {
                                    if let Some(&c) = chars.peek() {
                                        if c.is_ascii_hexdigit() {
                                            hex.push(chars.next().unwrap());
                                        } else {
                                            break;
                                        }
                                    } else {
                                        break;
                                    }
                                }
                                if hex.len() == 4 {
                                    if let Ok(code) = u32::from_str_radix(&hex, 16)
                                        && let Some(ch) = char::from_u32(code)
                                    {
                                        result.push(ch);
                                    }
                                } else {
                                    // Invalid escape - not enough digits, keep as-is
                                    result.push_str("\\u");
                                    result.push_str(&hex);
                                }
                            }
                            _ => {
                                // Invalid \u - keep as-is
                                result.push_str("\\u");
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
#[derive(Debug, Clone)]
struct Atom<'src> {
    span: Span,
    kind: ScalarKind,
    content: AtomContent<'src>,
}

/// Content of an atom.
// parser[impl object.syntax] parser[impl sequence.syntax]
#[derive(Debug, Clone)]
enum AtomContent<'src> {
    /// A scalar value (bare, quoted, or raw).
    Scalar(&'src str),
    /// Heredoc content (owned because it may be processed).
    Heredoc(String),
    /// Unit value `@`.
    Unit,
    /// A tag with optional payload.
    // parser[impl tag.payload]
    Tag {
        name: &'src str,
        payload: Option<Box<Atom<'src>>>,
        /// Span of invalid tag name (for error reporting).
        // parser[impl tag.syntax]
        invalid_name_span: Option<Span>,
    },
    /// An object with parsed entries.
    // parser[impl object.syntax]
    Object {
        entries: Vec<ObjectEntry<'src>>,
        separator: Separator,
        /// Pairs of (original_span, duplicate_span) for duplicate keys.
        duplicate_key_spans: Vec<(Span, Span)>,
        /// Spans of mixed separators (for error reporting).
        // parser[impl object.separators]
        mixed_separator_spans: Vec<Span>,
        /// Spans of dangling doc comments (for error reporting).
        // parser[impl comment.doc]
        dangling_doc_comment_spans: Vec<Span>,
        /// Whether the object was not properly closed (missing `}`).
        unclosed: bool,
    },
    /// A sequence with parsed elements.
    // parser[impl sequence.syntax] parser[impl sequence.elements]
    Sequence {
        elements: Vec<Atom<'src>>,
        /// Whether the sequence was not properly closed (missing `)`).
        unclosed: bool,
    },
    /// Attributes (key=value pairs that become an object).
    // parser[impl attr.syntax] parser[impl attr.atom]
    Attributes(Vec<AttributeEntry<'src>>),
}

/// An attribute entry (key=value).
#[derive(Debug, Clone)]
struct AttributeEntry<'src> {
    key: &'src str,
    key_span: Span,
    value: Atom<'src>,
}

/// An entry in an object (key-value pair).
#[derive(Debug, Clone)]
struct ObjectEntry<'src> {
    key: Atom<'src>,
    value: Atom<'src>,
}

/// A parsed key for equality comparison (duplicate key detection).
// parser[impl entry.key-equality]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum KeyValue {
    /// Scalar key (after escape processing).
    Scalar(String),
    /// Unit key (@).
    Unit,
    /// Tagged key.
    Tagged {
        name: String,
        payload: Option<Box<KeyValue>>,
    },
}

impl KeyValue {
    /// Create a KeyValue from an Atom for duplicate key comparison.
    // parser[impl entry.key-equality]
    fn from_atom<'a>(atom: &Atom<'a>, parser: &Parser<'a>) -> Self {
        match &atom.content {
            AtomContent::Scalar(text) => {
                // Process escapes for quoted strings
                let processed = parser.process_scalar(text, atom.kind);
                KeyValue::Scalar(processed.into_owned())
            }
            AtomContent::Heredoc(content) => KeyValue::Scalar(content.clone()),
            AtomContent::Unit => KeyValue::Unit,
            AtomContent::Tag { name, payload, .. } => KeyValue::Tagged {
                name: (*name).to_string(),
                payload: payload
                    .as_ref()
                    .map(|p| Box::new(KeyValue::from_atom(p, parser))),
            },
            // Objects/Sequences as keys are unusual, treat as their text repr
            AtomContent::Object { .. } => KeyValue::Scalar("{}".into()),
            AtomContent::Sequence { .. } => KeyValue::Scalar("()".into()),
            AtomContent::Attributes(_) => KeyValue::Scalar("{}".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use facet_testhelpers::test;

    fn parse(source: &str) -> Vec<Event<'_>> {
        tracing::debug!(source, "parsing");
        let events = Parser::new(source).parse_to_vec();
        tracing::debug!(?events, "parsed");
        events
    }

    /// Parse and log events for debugging
    #[allow(dead_code)]
    fn parse_debug(source: &str) -> Vec<Event<'_>> {
        tracing::info!(source, "parsing (debug mode)");
        let events = Parser::new(source).parse_to_vec();
        tracing::info!(?events, "parsed events");
        events
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
                .any(|e| matches!(e, Event::Key { payload: Some(value), .. } if value == "foo"))
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
                .any(|e| matches!(e, Event::Key { payload: Some(value), .. } if value == "foo"))
        );
        assert!(events.iter().any(|e| matches!(e, Event::Unit { .. })));
    }

    #[test]
    fn test_multiple_entries() {
        let events = parse("foo bar\nbaz qux");
        let keys: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                Event::Key {
                    payload: Some(value),
                    ..
                } => Some(value.as_ref()),
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
                Event::Key {
                    payload: Some(value),
                    ..
                } => Some(value.as_ref()),
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
    fn test_unit_key() {
        // @ followed by whitespace then value should emit Key with payload: None (unit key)
        let events = parse("@ server.schema.styx");
        trace!(?events, "parsed events for unit key test");
        // Should have: DocumentStart, EntryStart, Key (unit), Scalar (value), EntryEnd, DocumentEnd
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Key {
                    payload: None,
                    tag: None,
                    ..
                }
            )),
            "should have Key event with payload: None (unit key), got: {:?}",
            events
        );
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
                .any(|e| matches!(e, Event::Key { payload: Some(value), .. } if value == "foo"))
        );
    }

    #[test]
    fn test_doc_comments() {
        let events = parse("/// doc\nfoo bar");
        assert!(events.iter().any(|e| matches!(e, Event::DocComment { .. })));
    }

    // parser[verify comment.doc]
    #[test]
    fn test_doc_comment_followed_by_entry_ok() {
        let events = parse("/// documentation\nkey value");
        // Doc comment followed by entry is valid
        assert!(events.iter().any(|e| matches!(e, Event::DocComment { .. })));
        assert!(!events.iter().any(|e| matches!(
            e,
            Event::Error {
                kind: ParseErrorKind::DanglingDocComment,
                ..
            }
        )));
    }

    // parser[verify comment.doc]
    #[test]
    fn test_doc_comment_at_eof_error() {
        let events = parse("foo bar\n/// dangling");
        assert!(events.iter().any(|e| matches!(
            e,
            Event::Error {
                kind: ParseErrorKind::DanglingDocComment,
                ..
            }
        )));
    }

    // parser[verify comment.doc]
    #[test]
    fn test_doc_comment_before_closing_brace_error() {
        let events = parse("{foo bar\n/// dangling\n}");
        assert!(events.iter().any(|e| matches!(
            e,
            Event::Error {
                kind: ParseErrorKind::DanglingDocComment,
                ..
            }
        )));
    }

    // parser[verify comment.doc]
    #[test]
    fn test_multiple_doc_comments_before_entry_ok() {
        let events = parse("/// line 1\n/// line 2\nkey value");
        // Multiple consecutive doc comments before entry is fine
        let doc_count = events
            .iter()
            .filter(|e| matches!(e, Event::DocComment { .. }))
            .count();
        assert_eq!(doc_count, 2);
        assert!(!events.iter().any(|e| matches!(
            e,
            Event::Error {
                kind: ParseErrorKind::DanglingDocComment,
                ..
            }
        )));
    }

    // parser[verify object.syntax]
    #[test]
    fn test_nested_object() {
        let events = parse("outer {inner {x 1}}");
        // Should have nested ObjectStart/ObjectEnd events
        let obj_starts = events
            .iter()
            .filter(|e| matches!(e, Event::ObjectStart { .. }))
            .count();
        assert_eq!(
            obj_starts, 2,
            "Expected 2 ObjectStart events for nested objects"
        );
    }

    // parser[verify object.syntax]
    #[test]
    fn test_object_with_entries() {
        let events = parse("config {host localhost, port 8080}");
        // Check we have keys for host and port
        let keys: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                Event::Key {
                    payload: Some(value),
                    ..
                } => Some(value.as_ref()),
                _ => None,
            })
            .collect();
        assert!(keys.contains(&"config"), "Missing key 'config'");
        assert!(keys.contains(&"host"), "Missing key 'host'");
        assert!(keys.contains(&"port"), "Missing key 'port'");
    }

    // parser[verify sequence.syntax] parser[verify sequence.elements]
    #[test]
    fn test_sequence_elements() {
        let events = parse("items (a b c)");
        let scalars: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                Event::Scalar { value, .. } => Some(value.as_ref()),
                _ => None,
            })
            .collect();
        assert!(scalars.contains(&"a"), "Missing element 'a'");
        assert!(scalars.contains(&"b"), "Missing element 'b'");
        assert!(scalars.contains(&"c"), "Missing element 'c'");
    }

    // parser[verify sequence.syntax]
    #[test]
    fn test_nested_sequences() {
        let events = parse("matrix ((1 2) (3 4))");
        let seq_starts = events
            .iter()
            .filter(|e| matches!(e, Event::SequenceStart { .. }))
            .count();
        assert_eq!(
            seq_starts, 3,
            "Expected 3 SequenceStart events (outer + 2 inner)"
        );
    }

    // parser[verify tag.payload]
    #[test]
    fn test_tagged_object() {
        let events = parse("result @err{message oops}");
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::TagStart { name, .. } if *name == "err")),
            "Missing TagStart for @err"
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::ObjectStart { .. })),
            "Missing ObjectStart for tagged object"
        );
    }

    // parser[verify tag.payload]
    #[test]
    fn test_tagged_sequence() {
        let events = parse("color @rgb(255 128 0)");
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::TagStart { name, .. } if *name == "rgb")),
            "Missing TagStart for @rgb"
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::SequenceStart { .. })),
            "Missing SequenceStart for tagged sequence"
        );
    }

    // parser[verify tag.payload]
    #[test]
    fn test_tagged_scalar() {
        let events = parse(r#"name @nickname"Bob""#);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::TagStart { name, .. } if *name == "nickname")),
            "Missing TagStart for @nickname"
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::Scalar { value, .. } if value == "Bob")),
            "Missing Scalar for tagged string"
        );
    }

    // parser[verify tag.payload]
    #[test]
    fn test_tagged_explicit_unit() {
        let events = parse("nothing @empty@");
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::TagStart { name, .. } if *name == "empty")),
            "Missing TagStart for @empty"
        );
        // The explicit @ after tag creates a Unit payload
        let unit_count = events
            .iter()
            .filter(|e| matches!(e, Event::Unit { .. }))
            .count();
        assert!(
            unit_count >= 1,
            "Expected at least one Unit event for @empty@"
        );
    }

    // parser[verify tag.payload]
    #[test]
    fn test_tag_whitespace_gap() {
        // Whitespace between tag and potential payload = no payload (implicit unit)
        // Use a simpler case: key with tag value that has whitespace before object
        let events = parse("x @tag\ny {a b}");
        // @tag should be its own value (implicit unit), y {a b} is a separate entry
        let tag_events: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, Event::TagStart { .. } | Event::TagEnd))
            .collect();
        // There should be TagStart and TagEnd
        assert_eq!(tag_events.len(), 2, "Expected TagStart and TagEnd");
        // And the tag should NOT have the object as payload (object should be in a different entry)
        let keys: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                Event::Key {
                    payload: Some(value),
                    ..
                } => Some(value.as_ref()),
                _ => None,
            })
            .collect();
        assert!(keys.contains(&"x"), "Missing key 'x'");
        assert!(keys.contains(&"y"), "Missing key 'y'");
    }

    // parser[verify object.syntax]
    #[test]
    fn test_object_in_sequence() {
        let events = parse("servers ({host a} {host b})");
        // Sequence containing objects
        let obj_starts = events
            .iter()
            .filter(|e| matches!(e, Event::ObjectStart { .. }))
            .count();
        assert_eq!(
            obj_starts, 2,
            "Expected 2 ObjectStart events for objects in sequence"
        );
    }

    // parser[verify attr.syntax]
    #[test]
    fn test_simple_attribute() {
        let events = parse("server host=localhost");
        // key=server, value is object with {host: localhost}
        let keys: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                Event::Key {
                    payload: Some(value),
                    ..
                } => Some(value.as_ref()),
                _ => None,
            })
            .collect();
        assert!(keys.contains(&"server"), "Missing key 'server'");
        assert!(keys.contains(&"host"), "Missing key 'host' from attribute");
    }

    // parser[verify attr.values]
    #[test]
    fn test_attribute_values() {
        let events = parse("config name=app tags=(a b) opts={x 1}");
        let keys: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                Event::Key {
                    payload: Some(value),
                    ..
                } => Some(value.as_ref()),
                _ => None,
            })
            .collect();
        assert!(keys.contains(&"config"), "Missing key 'config'");
        assert!(keys.contains(&"name"), "Missing key 'name'");
        assert!(keys.contains(&"tags"), "Missing key 'tags'");
        assert!(keys.contains(&"opts"), "Missing key 'opts'");
        // Check sequence is present
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::SequenceStart { .. })),
            "Missing SequenceStart for tags=(a b)"
        );
    }

    // parser[verify attr.atom]
    #[test]
    fn test_multiple_attributes() {
        // When attributes are at root level without a preceding key,
        // the first attribute key becomes the entry key, and the rest form the value
        let events = parse("server host=localhost port=8080");
        // key=server, value is object with {host: localhost, port: 8080}
        let keys: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                Event::Key {
                    payload: Some(value),
                    ..
                } => Some(value.as_ref()),
                _ => None,
            })
            .collect();
        assert!(keys.contains(&"server"), "Missing key 'server'");
        assert!(keys.contains(&"host"), "Missing key 'host'");
        assert!(keys.contains(&"port"), "Missing key 'port'");
    }

    // parser[verify entry.keypath.attributes]
    #[test]
    fn test_keypath_with_attributes() {
        let events = parse("spec selector matchLabels app=web tier=frontend");
        // Nested: spec.selector.matchLabels = {app: web, tier: frontend}
        let keys: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                Event::Key {
                    payload: Some(value),
                    ..
                } => Some(value.as_ref()),
                _ => None,
            })
            .collect();
        assert!(keys.contains(&"spec"), "Missing key 'spec'");
        assert!(keys.contains(&"selector"), "Missing key 'selector'");
        assert!(keys.contains(&"matchLabels"), "Missing key 'matchLabels'");
        assert!(keys.contains(&"app"), "Missing key 'app'");
        assert!(keys.contains(&"tier"), "Missing key 'tier'");
    }

    // parser[verify attr.syntax]
    #[test]
    fn test_attribute_no_spaces() {
        // Spaces around = means it's NOT attribute syntax
        let events = parse("x = y");
        // This should be: key=x, then "=" and "y" as values (nested)
        // Since = is a valid bare scalar when preceded by whitespace
        let keys: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                Event::Key {
                    payload: Some(value),
                    ..
                } => Some(value.as_ref()),
                _ => None,
            })
            .collect();
        // "x" should be the first key, and "=" should NOT be treated as attribute syntax
        assert!(keys.contains(&"x"), "Missing key 'x'");
        // There should not be "=" as a key (it would be a value)
    }

    // parser[verify document.root]
    #[test]
    fn test_explicit_root_after_comment() {
        // Regular comment before explicit root object
        let events = parse("// comment\n{a 1}");
        // Should have ObjectStart (explicit root), not be treated as implicit root
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::ObjectStart { .. })),
            "Should have ObjectStart for explicit root after comment"
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::Key { payload: Some(value), .. } if value == "a")),
            "Should have key 'a'"
        );
    }

    // parser[verify document.root]
    #[test]
    fn test_explicit_root_after_doc_comment() {
        // Doc comment before explicit root object
        let events = parse("/// doc comment\n{a 1}");
        // Should have ObjectStart (explicit root) AND the doc comment
        assert!(
            events.iter().any(|e| matches!(e, Event::DocComment { .. })),
            "Should preserve doc comment"
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::ObjectStart { .. })),
            "Should have ObjectStart for explicit root after doc comment"
        );
    }

    // parser[verify entry.key-equality]
    #[test]
    fn test_duplicate_bare_key() {
        let events = parse("{a 1, a 2}");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Error {
                    kind: ParseErrorKind::DuplicateKey { .. },
                    ..
                }
            )),
            "Expected DuplicateKey error"
        );
    }

    // parser[verify entry.key-equality]
    #[test]
    fn test_duplicate_quoted_key() {
        let events = parse(r#"{"key" 1, "key" 2}"#);
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Error {
                    kind: ParseErrorKind::DuplicateKey { .. },
                    ..
                }
            )),
            "Expected DuplicateKey error for quoted keys"
        );
    }

    // parser[verify entry.key-equality]
    #[test]
    fn test_duplicate_key_escape_normalized() {
        // "ab" and "a\u{62}" should be considered duplicates after escape processing
        let events = parse(r#"{"ab" 1, "a\u{62}" 2}"#);
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Error {
                    kind: ParseErrorKind::DuplicateKey { .. },
                    ..
                }
            )),
            "Expected DuplicateKey error for escape-normalized keys"
        );
    }

    // parser[verify entry.key-equality]
    #[test]
    fn test_duplicate_unit_key() {
        let events = parse("{@ 1, @ 2}");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Error {
                    kind: ParseErrorKind::DuplicateKey { .. },
                    ..
                }
            )),
            "Expected DuplicateKey error for unit keys"
        );
    }

    // parser[verify entry.key-equality]
    #[test]
    fn test_duplicate_tagged_key() {
        let events = parse("{@foo 1, @foo 2}");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Error {
                    kind: ParseErrorKind::DuplicateKey { .. },
                    ..
                }
            )),
            "Expected DuplicateKey error for tagged keys"
        );
    }

    // parser[verify entry.key-equality]
    #[test]
    fn test_different_keys_ok() {
        let events = parse("{a 1, b 2, c 3}");
        assert!(
            !events.iter().any(|e| matches!(e, Event::Error { .. })),
            "Should not have any errors for different keys"
        );
    }

    // parser[verify entry.key-equality]
    #[test]
    fn test_duplicate_key_at_root() {
        // Test duplicate keys at the document root level (implicit root object)
        let events = parse("a 1\na 2");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Error {
                    kind: ParseErrorKind::DuplicateKey { .. },
                    ..
                }
            )),
            "Expected DuplicateKey error at document root level"
        );
    }

    // parser[verify object.separators]
    #[test]
    fn test_mixed_separators_comma_then_newline() {
        // Start with comma, then use newline - should error
        let events = parse("{a 1, b 2\nc 3}");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Error {
                    kind: ParseErrorKind::MixedSeparators,
                    ..
                }
            )),
            "Expected MixedSeparators error when comma mode followed by newline"
        );
    }

    // parser[verify object.separators]
    #[test]
    fn test_mixed_separators_newline_then_comma() {
        // Start with newline, then use comma - should error
        let events = parse("{a 1\nb 2, c 3}");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Error {
                    kind: ParseErrorKind::MixedSeparators,
                    ..
                }
            )),
            "Expected MixedSeparators error when newline mode followed by comma"
        );
    }

    // parser[verify object.separators]
    #[test]
    fn test_consistent_comma_separators() {
        // All commas - should be fine
        let events = parse("{a 1, b 2, c 3}");
        assert!(
            !events.iter().any(|e| matches!(
                e,
                Event::Error {
                    kind: ParseErrorKind::MixedSeparators,
                    ..
                }
            )),
            "Should not have MixedSeparators error for consistent comma separators"
        );
    }

    // parser[verify object.separators]
    #[test]
    fn test_consistent_newline_separators() {
        // All newlines - should be fine
        let events = parse("{a 1\nb 2\nc 3}");
        assert!(
            !events.iter().any(|e| matches!(
                e,
                Event::Error {
                    kind: ParseErrorKind::MixedSeparators,
                    ..
                }
            )),
            "Should not have MixedSeparators error for consistent newline separators"
        );
    }

    // parser[verify tag.syntax]
    #[test]
    fn test_valid_tag_names() {
        // Valid tag names should not produce errors
        assert!(
            !parse("@foo")
                .iter()
                .any(|e| matches!(e, Event::Error { .. })),
            "@foo should be valid"
        );
        assert!(
            !parse("@_private")
                .iter()
                .any(|e| matches!(e, Event::Error { .. })),
            "@_private should be valid"
        );
        assert!(
            !parse("@Some.Type")
                .iter()
                .any(|e| matches!(e, Event::Error { .. })),
            "@Some.Type should be valid"
        );
        assert!(
            !parse("@my-tag")
                .iter()
                .any(|e| matches!(e, Event::Error { .. })),
            "@my-tag should be valid"
        );
        assert!(
            !parse("@Type123")
                .iter()
                .any(|e| matches!(e, Event::Error { .. })),
            "@Type123 should be valid"
        );
    }

    // parser[verify tag.syntax]
    #[test]
    fn test_invalid_tag_name_starts_with_digit() {
        let events = parse("x @123");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Error {
                    kind: ParseErrorKind::InvalidTagName,
                    ..
                }
            )),
            "Tag starting with digit should be invalid"
        );
    }

    // parser[verify tag.syntax]
    #[test]
    fn test_invalid_tag_name_starts_with_hyphen() {
        let events = parse("x @-foo");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Error {
                    kind: ParseErrorKind::InvalidTagName,
                    ..
                }
            )),
            "Tag starting with hyphen should be invalid"
        );
    }

    // parser[verify tag.syntax]
    #[test]
    fn test_invalid_tag_name_starts_with_dot() {
        let events = parse("x @.foo");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Error {
                    kind: ParseErrorKind::InvalidTagName,
                    ..
                }
            )),
            "Tag starting with dot should be invalid"
        );
    }

    // parser[verify scalar.quoted.escapes]
    #[test]
    fn test_unicode_escape_braces() {
        let events = parse(r#"x "\u{1F600}""#);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::Scalar { value, .. } if value == "😀")),
            "\\u{{1F600}} should produce 😀"
        );
    }

    // parser[verify scalar.quoted.escapes]
    #[test]
    fn test_unicode_escape_4digit() {
        let events = parse(r#"x "\u0041""#);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::Scalar { value, .. } if value == "A")),
            "\\u0041 should produce A"
        );
    }

    // parser[verify scalar.quoted.escapes]
    #[test]
    fn test_unicode_escape_4digit_accented() {
        let events = parse(r#"x "\u00E9""#);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::Scalar { value, .. } if value == "é")),
            "\\u00E9 should produce é"
        );
    }

    // parser[verify scalar.quoted.escapes]
    #[test]
    fn test_unicode_escape_mixed() {
        // Mix of \uXXXX and \u{X} forms
        let events = parse(r#"x "\u0048\u{65}\u006C\u{6C}\u006F""#);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::Scalar { value, .. } if value == "Hello")),
            "Mixed unicode escapes should produce Hello"
        );
    }

    // parser[verify entry.keys]
    #[test]
    fn test_heredoc_key_rejected() {
        let events = parse("<<EOF\nkey\nEOF value");
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Error {
                    kind: ParseErrorKind::InvalidKey,
                    ..
                }
            )),
            "Heredoc as key should be rejected"
        );
    }
}
