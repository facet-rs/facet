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
    // parser[impl entry.key-equality] parser[impl entry.path.sibling] parser[impl entry.path.reopen]
    fn parse_entries<C: ParseCallback<'src>>(
        &mut self,
        callback: &mut C,
        closing: Option<TokenKind>,
    ) {
        trace!("Parsing entries, closing token: {:?}", closing);
        let mut path_state = PathState::default();
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

            // Parse entry with path state tracking
            if !self.parse_entry_with_path_check(callback, &mut path_state) {
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

    /// Parse a single entry with path state tracking.
    // parser[impl entry.key-equality] parser[impl entry.structure] parser[impl entry.path]
    // parser[impl entry.path.sibling] parser[impl entry.path.reopen]
    fn parse_entry_with_path_check<C: ParseCallback<'src>>(
        &mut self,
        callback: &mut C,
        path_state: &mut PathState,
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
        // Heredoc scalars, objects, and sequences are not allowed as keys
        match &key_atom.content {
            AtomContent::Heredoc(_) => {
                if !callback.event(Event::Error {
                    span: key_atom.span,
                    kind: ParseErrorKind::InvalidKey,
                }) {
                    return false;
                }
            }
            AtomContent::Object { .. } | AtomContent::Sequence { .. } => {
                if !callback.event(Event::Error {
                    span: key_atom.span,
                    kind: ParseErrorKind::InvalidKey,
                }) {
                    return false;
                }
            }
            _ => {}
        }

        // parser[impl entry.path]
        // Check if this is a dotted path (bare scalar containing '.')
        if let AtomContent::Scalar(text) = &key_atom.content
            && key_atom.kind == ScalarKind::Bare
            && text.contains('.')
        {
            return self.emit_dotted_path_entry(text, key_atom.span, &atoms, callback, path_state);
        }

        // Non-dotted key: treat as single-segment path
        let key_text = match &key_atom.content {
            AtomContent::Scalar(text) => {
                let processed = self.process_scalar(text, key_atom.kind);
                processed.into_owned()
            }
            AtomContent::Unit => "@".to_string(),
            AtomContent::Tag { name, .. } => format!("@{}", name),
            _ => key_atom.span.start.to_string(), // Fallback for invalid keys
        };

        // Determine value kind
        let value_kind = if atoms.len() >= 2 {
            match &atoms[1].content {
                AtomContent::Object { .. } | AtomContent::Attributes { .. } => {
                    PathValueKind::Object
                }
                _ => PathValueKind::Terminal,
            }
        } else {
            // Implicit unit value
            PathValueKind::Terminal
        };

        // Check path state
        let path = vec![key_text];
        if let Err(err) = path_state.check_and_update(&path, key_atom.span, value_kind)
            && !self.emit_path_error(err, key_atom.span, callback)
        {
            return false;
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
            // parser[impl entry.toomany]
            // 3+ atoms is an error - emit the second atom as value, then error on the third
            if !self.emit_atom_as_value(&atoms[1], callback) {
                return false;
            }

            // Emit error for the third atom (and beyond)
            // Common case: `key @tag {}` where user meant `@tag{}`
            let third_atom = &atoms[2];
            if !callback.event(Event::Error {
                span: third_atom.span,
                kind: ParseErrorKind::TooManyAtoms,
            }) {
                return false;
            }
        }

        callback.event(Event::EntryEnd)
    }

    /// Emit an error for a path validation failure.
    fn emit_path_error<C: ParseCallback<'src>>(
        &self,
        err: PathError,
        span: Span,
        callback: &mut C,
    ) -> bool {
        let kind = match err {
            PathError::Duplicate { original } => ParseErrorKind::DuplicateKey { original },
            PathError::Reopened { closed_path } => ParseErrorKind::ReopenedPath { closed_path },
            PathError::NestIntoTerminal { terminal_path } => {
                ParseErrorKind::NestIntoTerminal { terminal_path }
            }
        };
        callback.event(Event::Error { span, kind })
    }

    /// Emit a dotted path entry.
    /// `a.b.c value` expands to `a { b { c value } }`
    // parser[impl entry.path] parser[impl entry.path.sibling] parser[impl entry.path.reopen]
    fn emit_dotted_path_entry<C: ParseCallback<'src>>(
        &self,
        path_text: &'src str,
        path_span: Span,
        atoms: &[Atom<'src>],
        callback: &mut C,
        path_state: &mut PathState,
    ) -> bool {
        // Split the path on '.'
        let segments: Vec<&str> = path_text.split('.').collect();

        if segments.is_empty() || segments.iter().any(|s| s.is_empty()) {
            // Invalid path (empty segment like "a..b" or ".a" or "a.")
            if !callback.event(Event::Error {
                span: path_span,
                kind: ParseErrorKind::InvalidKey,
            }) {
                return false;
            }
            return callback.event(Event::EntryEnd);
        }

        // Build full path as Vec<String>
        let path: Vec<String> = segments.iter().map(|s| s.to_string()).collect();

        // Determine value kind based on the value atom
        let value_kind = if atoms.len() >= 2 {
            match &atoms[1].content {
                AtomContent::Object { .. } | AtomContent::Attributes { .. } => {
                    PathValueKind::Object
                }
                _ => PathValueKind::Terminal,
            }
        } else {
            // Implicit unit value
            PathValueKind::Terminal
        };

        // Check path state for duplicates, reopening, and nesting errors
        if let Err(err) = path_state.check_and_update(&path, path_span, value_kind)
            && !self.emit_path_error(err, path_span, callback)
        {
            return false;
        }

        // Calculate spans for each segment
        // This is approximate - we use the path span and divide it up
        let mut current_offset = path_span.start;

        // Emit nested structure: for each segment except the last, emit Key + ObjectStart
        let depth = segments.len();
        for (i, segment) in segments.iter().enumerate() {
            let segment_len = segment.len() as u32;
            let segment_span = Span::new(current_offset, current_offset + segment_len);

            if i > 0 {
                // Start a new entry for nested segments
                if !callback.event(Event::EntryStart) {
                    return false;
                }
            }

            // Emit this segment as a key
            if !callback.event(Event::Key {
                span: segment_span,
                tag: None,
                payload: Some(Cow::Borrowed(segment)),
                kind: ScalarKind::Bare,
            }) {
                return false;
            }

            if i < depth - 1 {
                // Not the last segment - emit ObjectStart (value is nested object)
                if !callback.event(Event::ObjectStart {
                    span: segment_span,
                    separator: Separator::Newline,
                }) {
                    return false;
                }
            }

            // Move past this segment and the dot
            current_offset += segment_len + 1; // +1 for the dot
        }

        // Emit the actual value
        if atoms.len() == 1 {
            // Just the path, implicit unit value
            if !callback.event(Event::Unit { span: path_span }) {
                return false;
            }
        } else if atoms.len() == 2 {
            // Path and value
            if !self.emit_atom_as_value(&atoms[1], callback) {
                return false;
            }
        } else {
            // parser[impl entry.toomany]
            // 3+ atoms is an error
            if !self.emit_atom_as_value(&atoms[1], callback) {
                return false;
            }
            let third_atom = &atoms[2];
            if !callback.event(Event::Error {
                span: third_atom.span,
                kind: ParseErrorKind::TooManyAtoms,
            }) {
                return false;
            }
        }

        // Close all the nested structures (in reverse order)
        for i in (0..depth).rev() {
            if i < depth - 1 {
                // Close the nested object
                if !callback.event(Event::ObjectEnd {
                    span: path_span, // Use path span for all closes
                }) {
                    return false;
                }
            }
            // Close the entry
            if !callback.event(Event::EntryEnd) {
                return false;
            }
        }

        true
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

                // Error tokens - emit parse error
                TokenKind::Error => {
                    let token = self.advance().unwrap();
                    // Record the error but continue parsing
                    // The error will be emitted later when processing atoms
                    atoms.push(Atom {
                        span: token.span,
                        kind: ScalarKind::Bare,
                        content: AtomContent::Error,
                    });
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

        // Check if > immediately follows (no whitespace)
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

        if eq_kind != TokenKind::Gt || eq_start != start_span.end {
            // No > or whitespace gap - return as regular scalar
            return Atom {
                span: start_span,
                kind: ScalarKind::Bare,
                content: AtomContent::Scalar(first_key),
            };
        }

        // Consume the > and record its span
        let gt_token = self.advance().unwrap();
        let gt_span = gt_token.span;

        // Track trailing > errors
        let mut trailing_gt_spans = Vec::new();

        // Value must immediately follow > (no whitespace)
        let val_info = self.peek_raw().map(|t| (t.span.start, t.kind));

        let Some((val_start, val_kind)) = val_info else {
            // Error: missing value after > (EOF)
            trailing_gt_spans.push(gt_span);
            return Atom {
                span: Span::new(start_span.start, gt_span.end),
                kind: ScalarKind::Bare,
                content: AtomContent::Attributes {
                    entries: vec![],
                    trailing_gt_spans,
                },
            };
        };

        if val_start != eq_end {
            // Error: whitespace after >
            trailing_gt_spans.push(gt_span);
            return Atom {
                span: Span::new(start_span.start, gt_span.end),
                kind: ScalarKind::Bare,
                content: AtomContent::Attributes {
                    entries: vec![],
                    trailing_gt_spans,
                },
            };
        }

        // Check if what follows is a valid attribute value
        if !matches!(
            val_kind,
            TokenKind::BareScalar
                | TokenKind::QuotedScalar
                | TokenKind::RawScalar
                | TokenKind::LParen
                | TokenKind::LBrace
                | TokenKind::At
                | TokenKind::HeredocStart
        ) {
            // Error: invalid token after > (e.g., newline, comma, etc.)
            trailing_gt_spans.push(gt_span);
            return Atom {
                span: Span::new(start_span.start, gt_span.end),
                kind: ScalarKind::Bare,
                content: AtomContent::Attributes {
                    entries: vec![],
                    trailing_gt_spans,
                },
            };
        }

        // Parse the first value
        let first_value = self.parse_attribute_value();
        let Some(first_value) = first_value else {
            // Invalid value type - this shouldn't happen given the check above
            trailing_gt_spans.push(gt_span);
            return Atom {
                span: Span::new(start_span.start, gt_span.end),
                kind: ScalarKind::Bare,
                content: AtomContent::Attributes {
                    entries: vec![],
                    trailing_gt_spans,
                },
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

            // Check for > immediately after key
            let eq_info = self.peek_raw().map(|t| (t.kind, t.span, t.span.end));
            let Some((eq_kind, loop_gt_span, loop_eq_end)) = eq_info else {
                // No more tokens - we consumed a bare scalar that's not an attribute
                // This is lost, but we stop here
                break;
            };

            if eq_kind != TokenKind::Gt || loop_gt_span.start != key_span.end {
                // Not an attribute - the consumed scalar is lost
                break;
            }

            // Consume >
            self.advance();

            // Check for value
            let val_info = self.peek_raw().map(|t| (t.span.start, t.kind));
            let Some((val_start, val_kind)) = val_info else {
                // Error: trailing > at end of input
                trailing_gt_spans.push(loop_gt_span);
                break;
            };

            if val_start != loop_eq_end {
                // Error: whitespace after >
                trailing_gt_spans.push(loop_gt_span);
                break;
            }

            // Check if valid attribute value follows
            if !matches!(
                val_kind,
                TokenKind::BareScalar
                    | TokenKind::QuotedScalar
                    | TokenKind::RawScalar
                    | TokenKind::LParen
                    | TokenKind::LBrace
                    | TokenKind::At
                    | TokenKind::HeredocStart
            ) {
                // Error: invalid token after >
                trailing_gt_spans.push(loop_gt_span);
                break;
            }

            let Some(value) = self.parse_attribute_value() else {
                // Shouldn't happen given the check above
                trailing_gt_spans.push(loop_gt_span);
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
            .or_else(|| trailing_gt_spans.last().map(|s| s.end))
            .unwrap_or(start_span.end);

        Atom {
            span: Span {
                start: start_span.start,
                end: end_span,
            },
            kind: ScalarKind::Bare,
            content: AtomContent::Attributes {
                entries: attrs,
                trailing_gt_spans,
            },
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
                // parser[impl scalar.heredoc.syntax]
                let start_span = token.span;
                let mut content = String::new();
                let mut end_span = start_span;
                let mut is_error = false;
                let mut end_token_text = "";

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
                            end_token_text = token.text;
                            break;
                        }
                        TokenKind::Error => {
                            // Unterminated heredoc
                            end_span = token.span;
                            is_error = true;
                            break;
                        }
                        _ => break,
                    }
                }

                // If the closing delimiter was indented, strip that indentation from content lines
                // Per parser[scalar.heredoc.syntax]: The closing delimiter line MAY be indented;
                // that indentation is stripped from content lines.
                let indent_len = end_token_text
                    .chars()
                    .take_while(|c| *c == ' ' || *c == '\t')
                    .count();
                if indent_len > 0 && !content.is_empty() {
                    content = Self::dedent_heredoc_content(&content, indent_len);
                }

                if is_error {
                    Atom {
                        span: Span {
                            start: start_span.start,
                            end: end_span.end,
                        },
                        kind: ScalarKind::Heredoc,
                        content: AtomContent::Error,
                    }
                } else {
                    Atom {
                        span: Span {
                            start: start_span.start,
                            end: end_span.end,
                        },
                        kind: ScalarKind::Heredoc,
                        content: AtomContent::Heredoc(content),
                    }
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
        let mut pending_doc_comment: Option<(Span, &'src str)> = None;
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
                if let Some((span, _)) = pending_doc_comment {
                    dangling_doc_comment_spans.push(span);
                }
                break;
            };

            // Capture span before matching (needed for error reporting)
            let token_span = token.span;

            match token.kind {
                TokenKind::RBrace => {
                    // Check for dangling doc comment before closing
                    if let Some((span, _)) = pending_doc_comment {
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
                    // Track doc comment for the next entry
                    let doc_token = self.advance().unwrap();
                    pending_doc_comment = Some((doc_token.span, doc_token.text));
                }

                TokenKind::Eof => {
                    // Unclosed object
                    unclosed = true;
                    if let Some((span, _)) = pending_doc_comment {
                        dangling_doc_comment_spans.push(span);
                    }
                    break;
                }

                _ => {
                    // Capture and clear pending doc comment for this entry
                    let doc_comment = pending_doc_comment.take();

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

                        let (value, too_many_atoms_span) = if entry_atoms.len() == 1 {
                            // Just a key, implicit unit value
                            (
                                Atom {
                                    span: key.span,
                                    kind: ScalarKind::Bare,
                                    content: AtomContent::Unit,
                                },
                                None,
                            )
                        } else if entry_atoms.len() == 2 {
                            // Key and value
                            (entry_atoms[1].clone(), None)
                        } else {
                            // parser[impl entry.toomany]
                            // 3+ atoms is an error - use second as value, record third for error
                            (entry_atoms[1].clone(), Some(entry_atoms[2].span))
                        };
                        entries.push(ObjectEntry {
                            key,
                            value,
                            doc_comment,
                            too_many_atoms_span,
                        });
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
                // No separators seen = inline format (like comma-separated)
                separator: separator_mode.unwrap_or(Separator::Comma),
                duplicate_key_spans,
                mixed_separator_spans,
                dangling_doc_comment_spans,
                unclosed,
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
            // But the bare scalar may contain @ which is not valid in tag names.
            // We need to split at the first @ if present.
            let name_token = self.advance().unwrap();
            let full_text = name_token.text;

            // Find where the tag name ends (at first @ or end of token)
            let tag_name_len = full_text.find('@').unwrap_or(full_text.len());
            let name = &full_text[..tag_name_len];
            let name_span = Span {
                start: name_token.span.start,
                end: name_token.span.start + tag_name_len as u32,
            };
            let name_end = name_span.end;

            // If there's leftover after the tag name (starting with @), we need to handle it
            // For now, if the tag name is empty (token started with @), this is @@ which is
            // unit followed by unit - but that should have been lexed differently.
            // If tag name is non-empty and there's @ after, that @ is the unit payload.
            let has_trailing_at = tag_name_len < full_text.len();

            // parser[impl tag.syntax]
            // Validate tag name: must match @[A-Za-z_][A-Za-z0-9_.-]*
            let invalid_tag_name = name.is_empty() || !Self::is_valid_tag_name(name);

            // Check for payload
            let payload = if has_trailing_at {
                // The @ after the tag name is the payload (unit)
                // Any text after that @ is also part of this token but we ignore it
                // since it would be invalid anyway (e.g., @foo@bar is @foo with unit @, then bar is separate)
                let at_pos = name_token.span.start + tag_name_len as u32;
                Some(Atom {
                    span: Span {
                        start: at_pos,
                        end: at_pos + 1,
                    },
                    kind: ScalarKind::Bare,
                    content: AtomContent::Unit,
                })
            } else {
                // Check for payload (must immediately follow tag name, no whitespace)
                self.parse_tag_payload(name_end)
            };
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
    /// Must match pattern: [A-Za-z_][A-Za-z0-9_-]*
    /// Note: dots are NOT allowed in tag names (they are path separators in keys).
    // parser[impl tag.syntax]
    fn is_valid_tag_name(name: &str) -> bool {
        let mut chars = name.chars();

        // First char: letter or underscore
        match chars.next() {
            Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
            _ => return false,
        }

        // Rest: alphanumeric, underscore, or hyphen (no dots!)
        chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
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
            AtomContent::Scalar(text) => {
                // parser[impl scalar.quoted.escapes]
                // Validate escape sequences for quoted scalars
                if atom.kind == ScalarKind::Quoted {
                    for (offset, seq) in Self::validate_quoted_escapes(text) {
                        let error_start = atom.span.start + offset as u32;
                        let error_span = Span::new(error_start, error_start + seq.len() as u32);
                        if !callback.event(Event::Error {
                            span: error_span,
                            kind: ParseErrorKind::InvalidEscape(seq),
                        }) {
                            return false;
                        }
                    }
                }
                callback.event(Event::Scalar {
                    span: atom.span,
                    value: self.process_scalar(text, atom.kind),
                    kind: atom.kind,
                })
            }
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
                    // Emit doc comment before entry if present
                    if let Some((span, text)) = &entry.doc_comment
                        && !callback.event(Event::DocComment { span: *span, text })
                    {
                        return false;
                    }
                    if !callback.event(Event::EntryStart) {
                        return false;
                    }
                    if !self.emit_atom_as_key(&entry.key, callback) {
                        return false;
                    }
                    if !self.emit_atom_as_value(&entry.value, callback) {
                        return false;
                    }
                    // parser[impl entry.toomany]
                    // Emit error for too many atoms
                    if let Some(span) = entry.too_many_atoms_span
                        && !callback.event(Event::Error {
                            span,
                            kind: ParseErrorKind::TooManyAtoms,
                        })
                    {
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
            AtomContent::Attributes {
                entries,
                trailing_gt_spans,
            } => {
                // Emit errors for trailing > without value
                for gt_span in trailing_gt_spans {
                    if !callback.event(Event::Error {
                        span: *gt_span,
                        kind: ParseErrorKind::ExpectedValue,
                    }) {
                        return false;
                    }
                }

                // Emit as comma-separated object
                if !callback.event(Event::ObjectStart {
                    span: atom.span,
                    separator: Separator::Comma,
                }) {
                    return false;
                }

                for attr in entries {
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
            AtomContent::Error => {
                // Error atom - emit as unexpected token error
                callback.event(Event::Error {
                    span: atom.span,
                    kind: ParseErrorKind::UnexpectedToken,
                })
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
            AtomContent::Scalar(text) => {
                // parser[impl scalar.quoted.escapes]
                // Validate escape sequences for quoted scalars
                if atom.kind == ScalarKind::Quoted {
                    for (offset, seq) in Self::validate_quoted_escapes(text) {
                        let error_start = atom.span.start + offset as u32;
                        let error_span = Span::new(error_start, error_start + seq.len() as u32);
                        if !callback.event(Event::Error {
                            span: error_span,
                            kind: ParseErrorKind::InvalidEscape(seq),
                        }) {
                            return false;
                        }
                    }
                }
                callback.event(Event::Key {
                    span: atom.span,
                    tag: None,
                    payload: Some(self.process_scalar(text, atom.kind)),
                    kind: atom.kind,
                })
            }
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
                            // parser[impl scalar.quoted.escapes]
                            // Validate escape sequences for quoted scalars
                            if inner.kind == ScalarKind::Quoted {
                                for (offset, seq) in Self::validate_quoted_escapes(text) {
                                    let error_start = inner.span.start + offset as u32;
                                    let error_span =
                                        Span::new(error_start, error_start + seq.len() as u32);
                                    if !callback.event(Event::Error {
                                        span: error_span,
                                        kind: ParseErrorKind::InvalidEscape(seq),
                                    }) {
                                        return false;
                                    }
                                }
                            }
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
                        | AtomContent::Attributes { .. }
                        | AtomContent::Error => {
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
            | AtomContent::Attributes { .. }
            | AtomContent::Error => {
                // Objects, sequences, error tokens not allowed as keys
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

    /// Validate escape sequences in a quoted string and return invalid escapes.
    /// Returns a list of (byte_offset_within_string, invalid_sequence) pairs.
    /// parser[impl scalar.quoted.escapes]
    fn validate_quoted_escapes(text: &str) -> Vec<(usize, String)> {
        let mut errors = Vec::new();

        // Remove surrounding quotes for validation
        let inner = if text.starts_with('"') && text.ends_with('"') && text.len() >= 2 {
            &text[1..text.len() - 1]
        } else {
            text
        };

        let mut chars = inner.char_indices().peekable();

        while let Some((i, c)) = chars.next() {
            if c == '\\' {
                let escape_start = i;
                match chars.next() {
                    Some((_, 'n' | 'r' | 't' | '\\' | '"')) => {
                        // Valid escape
                    }
                    Some((_, 'u')) => {
                        // Unicode escape - validate format
                        match chars.peek() {
                            Some((_, '{')) => {
                                // \u{X...} form - consume until }
                                chars.next(); // consume '{'
                                let mut valid = true;
                                let mut found_close = false;
                                for (_, c) in chars.by_ref() {
                                    if c == '}' {
                                        found_close = true;
                                        break;
                                    }
                                    if !c.is_ascii_hexdigit() {
                                        valid = false;
                                    }
                                }
                                if !found_close || !valid {
                                    // Extract the sequence for error reporting
                                    let end = chars.peek().map(|(i, _)| *i).unwrap_or(inner.len());
                                    let seq = &inner[escape_start..end.min(escape_start + 12)];
                                    errors.push((escape_start + 1, format!("\\{}", &seq[1..])));
                                }
                            }
                            Some((_, c)) if c.is_ascii_hexdigit() => {
                                // \uXXXX form - need exactly 4 hex digits
                                let mut count = 1;
                                while count < 4 {
                                    match chars.peek() {
                                        Some((_, c)) if c.is_ascii_hexdigit() => {
                                            chars.next();
                                            count += 1;
                                        }
                                        _ => break,
                                    }
                                }
                                if count != 4 {
                                    let end = chars.peek().map(|(i, _)| *i).unwrap_or(inner.len());
                                    let seq = &inner[escape_start..end];
                                    errors.push((escape_start + 1, seq.to_string()));
                                }
                            }
                            _ => {
                                // Invalid \u with no hex digits
                                errors.push((escape_start + 1, "\\u".to_string()));
                            }
                        }
                    }
                    Some((_, c)) => {
                        // Invalid escape sequence
                        errors.push((escape_start + 1, format!("\\{}", c)));
                    }
                    None => {
                        // Trailing backslash
                        errors.push((escape_start + 1, "\\".to_string()));
                    }
                }
            }
        }

        errors
    }

    /// Dedent heredoc content by stripping `indent_len` characters from the start of each line.
    /// Per parser[scalar.heredoc.syntax]: when the closing delimiter is indented,
    /// that indentation is stripped from content lines.
    fn dedent_heredoc_content(content: &str, indent_len: usize) -> String {
        content
            .lines()
            .map(|line| {
                // Strip up to indent_len whitespace characters from the start of each line
                let mut chars = line.chars();
                let mut stripped = 0;
                while stripped < indent_len {
                    match chars.clone().next() {
                        Some(' ') | Some('\t') => {
                            chars.next();
                            stripped += 1;
                        }
                        _ => break,
                    }
                }
                chars.as_str()
            })
            .collect::<Vec<_>>()
            .join("\n")
            + if content.ends_with('\n') { "\n" } else { "" }
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
    Attributes {
        entries: Vec<AttributeEntry<'src>>,
        /// Spans of trailing `>` without values (for error reporting).
        trailing_gt_spans: Vec<Span>,
    },
    /// A lexer error token.
    Error,
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
    /// Doc comment preceding this entry, if any.
    doc_comment: Option<(Span, &'src str)>,
    /// Span of unexpected third atom (for TooManyAtoms error).
    // parser[impl entry.toomany]
    too_many_atoms_span: Option<Span>,
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
            AtomContent::Attributes { .. } => KeyValue::Scalar("{}".into()),
            AtomContent::Error => KeyValue::Scalar("<error>".into()),
        }
    }
}

/// Whether a path leads to an object (can have children) or a terminal value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PathValueKind {
    /// Path leads to an object (explicit `{}` or implicit from dotted path).
    Object,
    /// Path leads to a terminal value (scalar, sequence, tag, unit).
    Terminal,
}

/// Tracks dotted path state for sibling detection and reopen errors.
// parser[impl entry.path.sibling] parser[impl entry.path.reopen]
#[derive(Default)]
struct PathState {
    /// The current open path segments.
    current_path: Vec<String>,
    /// Paths that have been closed (sibling appeared at same level).
    closed_paths: std::collections::HashSet<Vec<String>>,
    /// Full paths that have been assigned, with their value kind and span.
    assigned_paths: HashMap<Vec<String>, (Span, PathValueKind)>,
}

/// Error returned when path validation fails.
#[derive(Debug)]
enum PathError {
    /// Exact duplicate path.
    Duplicate { original: Span },
    /// Trying to reopen a closed path.
    Reopened { closed_path: Vec<String> },
    /// Trying to nest into a terminal value.
    NestIntoTerminal { terminal_path: Vec<String> },
}

impl PathState {
    /// Check a path and update state. Returns error if path is invalid.
    fn check_and_update(
        &mut self,
        path: &[String],
        span: Span,
        value_kind: PathValueKind,
    ) -> Result<(), PathError> {
        // 1. Check for duplicate (exact same path)
        if let Some(&(original, _)) = self.assigned_paths.get(path) {
            return Err(PathError::Duplicate { original });
        }

        // 2. Check if any proper prefix is closed or has a terminal value
        for i in 1..path.len() {
            let prefix = &path[..i];
            if self.closed_paths.contains(prefix) {
                return Err(PathError::Reopened {
                    closed_path: prefix.to_vec(),
                });
            }
            if let Some(&(_, PathValueKind::Terminal)) = self.assigned_paths.get(prefix) {
                return Err(PathError::NestIntoTerminal {
                    terminal_path: prefix.to_vec(),
                });
            }
        }

        // 3. Find common prefix length with current path
        let common_len = self
            .current_path
            .iter()
            .zip(path.iter())
            .take_while(|(a, b)| a == b)
            .count();

        // 4. Close paths beyond the common prefix
        // Everything in current_path[common_len..] gets closed
        for i in common_len..self.current_path.len() {
            let closed: Vec<String> = self.current_path[..=i].to_vec();
            self.closed_paths.insert(closed);
        }

        // 5. Record intermediate path segments as objects (if not already assigned)
        for i in 1..path.len() {
            let prefix = path[..i].to_vec();
            self.assigned_paths
                .entry(prefix)
                .or_insert((span, PathValueKind::Object));
        }

        // 6. Update assigned paths and current path
        self.assigned_paths
            .insert(path.to_vec(), (span, value_kind));
        self.current_path = path.to_vec();

        Ok(())
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
    fn test_too_many_atoms() {
        // parser[verify entry.toomany]
        // 3+ atoms should produce an error
        let events = parse("a b c");
        // Should produce: key=a, value=b, error on c
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::Key { payload: Some(value), .. } if value == "a"))
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::Scalar { value, .. } if value == "b"))
        );
        assert!(events.iter().any(|e| matches!(
            e,
            Event::Error {
                kind: ParseErrorKind::TooManyAtoms,
                ..
            }
        )));
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
        let events = parse("server host>localhost");
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
        let events = parse("config name>app tags>(a b) opts>{x 1}");
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
            "Missing SequenceStart for tags>(a b)"
        );
    }

    // parser[verify attr.atom]
    #[test]
    fn test_multiple_attributes() {
        // When attributes are at root level without a preceding key,
        // the first attribute key becomes the entry key, and the rest form the value
        let events = parse("server host>localhost port>8080");
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
    fn test_too_many_atoms_with_attributes() {
        // parser[verify entry.toomany]
        // Old key-path syntax is now an error
        let events = parse("spec selector matchLabels app>web tier>frontend");
        // Should produce error for too many atoms
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Error {
                    kind: ParseErrorKind::TooManyAtoms,
                    ..
                }
            )),
            "Should have TooManyAtoms error"
        );
    }

    // parser[verify attr.syntax]
    #[test]
    fn test_attribute_no_spaces() {
        // Spaces around > means it's NOT attribute syntax
        let events = parse("x > y");
        // This should be: key=x, then ">" and "y" as values (nested)
        // Since > is its own token when preceded by whitespace
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
        // "x" should be the first key, and ">" should NOT be treated as attribute syntax
        assert!(keys.contains(&"x"), "Missing key 'x'");
        // There should not be ">" as a key (it would be a value)
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
        // @Some.Type is now invalid since dots are not allowed in tag names
        assert!(
            parse("@Some.Type")
                .iter()
                .any(|e| matches!(e, Event::Error { .. })),
            "@Some.Type should be invalid (dots not allowed)"
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

    // parser[verify scalar.quoted.escapes]
    #[test]
    fn test_invalid_escape_null() {
        // \0 is no longer a valid escape - must use \u{0} instead
        let events = parse(r#"x "\0""#);
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Error {
                    kind: ParseErrorKind::InvalidEscape(seq),
                    ..
                } if seq == "\\0"
            )),
            "\\0 should be rejected as invalid escape"
        );
    }

    // parser[verify scalar.quoted.escapes]
    #[test]
    fn test_invalid_escape_unknown() {
        // \q, \?, \a etc. are not valid escapes
        let events = parse(r#"x "\q""#);
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Error {
                    kind: ParseErrorKind::InvalidEscape(seq),
                    ..
                } if seq == "\\q"
            )),
            "\\q should be rejected as invalid escape"
        );
    }

    // parser[verify scalar.quoted.escapes]
    #[test]
    fn test_invalid_escape_multiple() {
        // Multiple invalid escapes should all be reported
        let events = parse(r#"x "\0\q\?""#);
        let invalid_escapes: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                Event::Error {
                    kind: ParseErrorKind::InvalidEscape(seq),
                    ..
                } => Some(seq.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            invalid_escapes.len(),
            3,
            "Should report 3 invalid escapes, got: {:?}",
            invalid_escapes
        );
    }

    // parser[verify scalar.quoted.escapes]
    #[test]
    fn test_valid_escapes_still_work() {
        // Make sure valid escapes still work
        let events = parse(r#"x "a\nb\tc\\d\"e""#);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::Scalar { value, .. } if value == "a\nb\tc\\d\"e")),
            "Valid escapes should still work"
        );
        // No errors should be reported
        assert!(
            !events.iter().any(|e| matches!(
                e,
                Event::Error {
                    kind: ParseErrorKind::InvalidEscape(_),
                    ..
                }
            )),
            "Valid escapes should not produce errors"
        );
    }

    // parser[verify scalar.quoted.escapes]
    #[test]
    fn test_invalid_escape_in_key() {
        // Invalid escapes in keys should also be reported
        let events = parse(r#""\0" value"#);
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Error {
                    kind: ParseErrorKind::InvalidEscape(seq),
                    ..
                } if seq == "\\0"
            )),
            "\\0 in key should be rejected as invalid escape"
        );
    }

    // parser[verify entry.structure]
    #[test]
    fn test_simple_key_value_with_attributes() {
        // Simple key-value where value is an attributes object
        let events = parse("server host>localhost port>8080");
        // Should have keys: server, host, port
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
        // No errors should be reported
        assert!(
            !events.iter().any(|e| matches!(
                e,
                Event::Error {
                    kind: ParseErrorKind::TooManyAtoms,
                    ..
                }
            )),
            "Simple key-value with attributes should not produce TooManyAtoms"
        );
    }

    // parser[verify entry.path]
    #[test]
    fn test_dotted_path_simple() {
        // a.b value should expand to a { b value }
        let events = parse("a.b value");
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
        assert_eq!(keys, vec!["a", "b"], "Should have keys 'a' and 'b'");
        // Should have ObjectStart for the nested object
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::ObjectStart { .. })),
            "Should have ObjectStart for nested structure"
        );
        // Should have the value
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::Scalar { value, .. } if value == "value")),
            "Should have scalar value 'value'"
        );
        // No errors
        assert!(
            !events.iter().any(|e| matches!(e, Event::Error { .. })),
            "Simple dotted path should not have errors"
        );
    }

    // parser[verify entry.path]
    #[test]
    fn test_dotted_path_three_segments() {
        // a.b.c deep should expand to a { b { c deep } }
        let events = parse("a.b.c deep");
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
        assert_eq!(keys, vec!["a", "b", "c"], "Should have keys 'a', 'b', 'c'");
        // Should have two ObjectStart events for nested objects
        let obj_starts: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, Event::ObjectStart { .. }))
            .collect();
        assert_eq!(
            obj_starts.len(),
            2,
            "Should have 2 ObjectStart for nested structure"
        );
        // No errors
        assert!(
            !events.iter().any(|e| matches!(e, Event::Error { .. })),
            "Three-segment dotted path should not have errors"
        );
    }

    // parser[verify entry.path]
    #[test]
    fn test_dotted_path_with_implicit_unit() {
        // a.b without value should have implicit unit
        let events = parse("a.b");
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
        assert_eq!(keys, vec!["a", "b"], "Should have keys 'a' and 'b'");
        // Should have Unit for implicit value
        assert!(
            events.iter().any(|e| matches!(e, Event::Unit { .. })),
            "Should have implicit unit value"
        );
    }

    // parser[verify entry.path]
    #[test]
    fn test_dotted_path_empty_segment() {
        // a..b value - empty segment is invalid
        let events = parse("a..b value");
        assert!(
            events.iter().any(|e| matches!(e, Event::Error { .. })),
            "Empty segment in dotted path should produce error"
        );
    }

    // parser[verify entry.path]
    #[test]
    fn test_dotted_path_trailing_dot() {
        // a.b. value - trailing dot is invalid
        let events = parse("a.b. value");
        assert!(
            events.iter().any(|e| matches!(e, Event::Error { .. })),
            "Trailing dot in dotted path should produce error"
        );
    }

    // parser[verify entry.path]
    #[test]
    fn test_dotted_path_leading_dot() {
        // .a.b value - leading dot is invalid
        let events = parse(".a.b value");
        assert!(
            events.iter().any(|e| matches!(e, Event::Error { .. })),
            "Leading dot in dotted path should produce error"
        );
    }

    // parser[verify entry.path]
    #[test]
    fn test_dotted_path_with_object_value() {
        // a.b { c d } should expand to a { b { c d } }
        let events = parse("a.b { c d }");
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
        assert!(keys.contains(&"a"), "Should have 'a'");
        assert!(keys.contains(&"b"), "Should have 'b'");
        assert!(keys.contains(&"c"), "Should have 'c'");
        // No errors
        assert!(
            !events.iter().any(|e| matches!(e, Event::Error { .. })),
            "Dotted path with object value should not have errors"
        );
    }

    // parser[verify entry.path]
    #[test]
    fn test_dotted_path_with_attributes_value() {
        // selector.matchLabels app>web - dotted path with attributes as value
        let events = parse("selector.matchLabels app>web");
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
        assert!(keys.contains(&"selector"), "Should have 'selector'");
        assert!(keys.contains(&"matchLabels"), "Should have 'matchLabels'");
        assert!(keys.contains(&"app"), "Should have 'app' from attribute");
        // No errors
        assert!(
            !events.iter().any(|e| matches!(e, Event::Error { .. })),
            "Dotted path with attributes value should not have errors"
        );
    }

    // parser[verify entry.path]
    #[test]
    fn test_dot_in_value_is_literal() {
        // key example.com - dot in value position is literal, not path separator
        let events = parse("key example.com");
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
        assert_eq!(keys, vec!["key"], "Should have only one key 'key'");
        // Value should be the full domain
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::Scalar { value, .. } if value == "example.com")),
            "Value should be 'example.com' as a single scalar"
        );
        // No errors
        assert!(
            !events.iter().any(|e| matches!(e, Event::Error { .. })),
            "Dot in value should not cause errors"
        );
    }

    // parser[verify entry.path.sibling]
    #[test]
    fn test_sibling_dotted_paths() {
        // Sibling paths under common prefix should be allowed
        let events = parse("foo.bar.x value1\nfoo.bar.y value2\nfoo.baz value3");
        // Should have no errors
        assert!(
            !events.iter().any(|e| matches!(e, Event::Error { .. })),
            "Sibling dotted paths should not cause errors"
        );
        // Should have all keys
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
        assert!(keys.contains(&"foo"), "Should have 'foo'");
        assert!(keys.contains(&"bar"), "Should have 'bar'");
        assert!(keys.contains(&"baz"), "Should have 'baz'");
        assert!(keys.contains(&"x"), "Should have 'x'");
        assert!(keys.contains(&"y"), "Should have 'y'");
    }

    // parser[verify entry.path.reopen]
    #[test]
    fn test_reopen_closed_path_error() {
        // Can't reopen a path after moving to a sibling
        let events = parse("foo.bar {}\nfoo.baz {}\nfoo.bar.x value");
        // Should have a reopen error
        let errors: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, Event::Error { .. }))
            .collect();
        assert_eq!(
            errors.len(),
            1,
            "Should have exactly one error for reopening closed path"
        );
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Error {
                    kind: ParseErrorKind::ReopenedPath { .. },
                    ..
                }
            )),
            "Error should be ReopenedPath"
        );
    }

    // parser[verify entry.path.reopen]
    #[test]
    fn test_reopen_nested_closed_path_error() {
        // Can't reopen a nested path after moving to a higher-level sibling
        let events = parse("a.b.c {}\na.b.d {}\na.x {}\na.b.e {}");
        // Should have a reopen error for a.b
        let errors: Vec<_> = events
            .iter()
            .filter(|e| {
                matches!(
                    e,
                    Event::Error {
                        kind: ParseErrorKind::ReopenedPath { .. },
                        ..
                    }
                )
            })
            .collect();
        assert_eq!(errors.len(), 1, "Should have exactly one reopen error");
    }

    // parser[verify entry.path.reopen]
    #[test]
    fn test_nest_into_scalar_error() {
        // Can't nest into a path that has a scalar value
        let events = parse("a.b value\na.b.c deep");
        // Should have a nest-into-terminal error
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Error {
                    kind: ParseErrorKind::NestIntoTerminal { .. },
                    ..
                }
            )),
            "Should have NestIntoTerminal error"
        );
    }

    // parser[verify entry.path.sibling]
    #[test]
    fn test_different_top_level_paths_ok() {
        // Different top-level paths don't conflict
        let events = parse("server.host localhost\ndatabase.port 5432");
        assert!(
            !events.iter().any(|e| matches!(e, Event::Error { .. })),
            "Different top-level paths should not conflict"
        );
    }
}
