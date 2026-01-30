//! Pull-based event parser for Styx.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet, VecDeque};

use styx_tokenizer::Span;

use crate::events::{ParseErrorKind, ScalarKind};
use crate::{Event, Lexeme, Lexer};

/// Wraps lexer with a single pending slot for stashing boundary lexemes.
#[derive(Clone)]
struct LexemeSource<'src> {
    lexer: Lexer<'src>,
    /// Single pending lexeme slot. When collect_entry_atoms hits a boundary
    /// (comma, closing brace, etc.), it stashes the lexeme here instead of
    /// discarding it. Limited to exactly one slot - if we ever need more,
    /// that's a bug in our logic.
    pending: Option<Lexeme<'src>>,
}

impl<'src> LexemeSource<'src> {
    fn new(source: &'src str) -> Self {
        Self {
            lexer: Lexer::new(source),
            pending: None,
        }
    }

    fn next(&mut self) -> Lexeme<'src> {
        self.pending
            .take()
            .unwrap_or_else(|| self.lexer.next_lexeme())
    }

    fn stash(&mut self, lexeme: Lexeme<'src>) {
        assert!(self.pending.is_none(), "double stash - this is a bug");
        self.pending = Some(lexeme);
    }
}

/// Pull-based event parser for Styx.
#[derive(Clone)]
pub struct Parser<'src> {
    input: &'src str,
    source: LexemeSource<'src>,
    state: ParserState,
    event_queue: VecDeque<Event<'src>>,
}

/// Parser state machine states.
#[derive(Clone)]
enum ParserState {
    /// Haven't emitted DocumentStart yet.
    BeforeDocument,

    /// Expression mode: parse a single value without document wrapper.
    BeforeExpression,

    /// At implicit document root.
    DocumentRoot {
        seen_keys: HashMap<KeyValue, Span>,
        pending_doc_comment: Option<Span>,
        path_state: PathState,
    },

    /// Inside explicit object { ... }.
    InObject {
        start_span: Span,
        seen_keys: HashMap<KeyValue, Span>,
        pending_doc_comment: Option<Span>,
        /// Parent state to restore when we pop.
        parent: Box<ParserState>,
    },

    /// Document ended.
    AfterDocument,

    /// Expression mode ended.
    AfterExpression,
}

impl<'src> Parser<'src> {
    /// Create a new parser for the given source.
    pub fn new(source: &'src str) -> Self {
        Self {
            input: source,
            source: LexemeSource::new(source),
            state: ParserState::BeforeDocument,
            event_queue: VecDeque::new(),
        }
    }

    /// Create a new parser in expression mode.
    ///
    /// Expression mode parses a single value rather than a document with implicit root object.
    /// Use this for parsing embedded values like default values in schemas.
    pub fn new_expr(source: &'src str) -> Self {
        Self {
            input: source,
            source: LexemeSource::new(source),
            state: ParserState::BeforeExpression,
            event_queue: VecDeque::new(),
        }
    }

    /// Get the next event from the parser.
    pub fn next_event(&mut self) -> Option<Event<'src>> {
        // Drain queue first
        if let Some(event) = self.event_queue.pop_front() {
            return Some(event);
        }

        // Advance state machine
        self.advance()
    }

    /// Parse all events into a vector.
    pub fn parse_to_vec(mut self) -> Vec<Event<'src>> {
        let mut events = Vec::new();
        while let Some(event) = self.next_event() {
            events.push(event);
        }
        events
    }

    /// Advance the state machine.
    fn advance(&mut self) -> Option<Event<'src>> {
        match &self.state {
            ParserState::BeforeDocument => {
                self.state = ParserState::DocumentRoot {
                    seen_keys: HashMap::new(),
                    pending_doc_comment: None,
                    path_state: PathState::default(),
                };
                Some(Event::DocumentStart)
            }
            ParserState::BeforeExpression => self.advance_expression(),
            ParserState::AfterExpression => None,
            ParserState::AfterDocument => self.check_trailing_content(),
            ParserState::DocumentRoot { .. } => self.advance_document_root(),
            ParserState::InObject { .. } => self.advance_in_object(),
        }
    }

    /// Advance when in expression mode - parse a single value.
    fn advance_expression(&mut self) -> Option<Event<'src>> {
        loop {
            let lexeme = self.source.next();
            match lexeme {
                // Skip whitespace/newlines/comments
                Lexeme::Newline { .. } | Lexeme::Comment { .. } => continue,
                Lexeme::Eof => {
                    self.state = ParserState::AfterExpression;
                    return None;
                }
                _ => {
                    // Parse a single atom as the value
                    let atom = self.parse_atom(lexeme);
                    self.emit_atom_as_value(&atom);
                    self.state = ParserState::AfterExpression;
                    return self.event_queue.pop_front();
                }
            }
        }
    }

    /// Check for trailing content after explicit root object.
    /// Returns an error event if there's non-whitespace content, otherwise None.
    fn check_trailing_content(&mut self) -> Option<Event<'src>> {
        loop {
            let lexeme = self.source.next();
            match lexeme {
                // Skip whitespace, newlines, and comments - these are allowed after document
                Lexeme::Newline { .. } | Lexeme::Comment { .. } => continue,
                Lexeme::Eof => return None,
                // Any other content is an error
                _ => {
                    let span = lexeme.span();
                    // Consume remaining tokens to find the full extent of trailing content
                    let mut end = span.end;
                    loop {
                        match self.source.next() {
                            Lexeme::Eof => break,
                            lex => end = lex.span().end,
                        }
                    }
                    return Some(Event::Error {
                        span: Span::new(span.start, end),
                        kind: ParseErrorKind::TrailingContent,
                    });
                }
            }
        }
    }

    /// Advance when in DocumentRoot state.
    fn advance_document_root(&mut self) -> Option<Event<'src>> {
        loop {
            let lexeme = self.source.next();
            match lexeme {
                Lexeme::Eof => {
                    if let ParserState::DocumentRoot {
                        pending_doc_comment,
                        ..
                    } = &mut self.state
                        && let Some(span) = pending_doc_comment.take()
                    {
                        self.event_queue.push_back(Event::Error {
                            span,
                            kind: ParseErrorKind::DanglingDocComment,
                        });
                    }
                    self.event_queue.push_back(Event::DocumentEnd);
                    self.state = ParserState::AfterDocument;
                    return self.event_queue.pop_front();
                }
                Lexeme::Newline { .. } | Lexeme::Comma { .. } => continue,
                Lexeme::Comment { span, text } => {
                    return Some(Event::Comment { span, text });
                }
                Lexeme::DocComment { span, text } => {
                    if let ParserState::DocumentRoot {
                        pending_doc_comment,
                        ..
                    } = &mut self.state
                    {
                        *pending_doc_comment = Some(span);
                    }
                    // Strip `/// ` or `///` prefix
                    let line = text
                        .strip_prefix("/// ")
                        .or_else(|| text.strip_prefix("///"))
                        .unwrap_or(text);
                    return Some(Event::DocComment {
                        span,
                        lines: vec![line],
                    });
                }
                Lexeme::ObjectStart { span } => {
                    // Explicit root object - after it closes, document is done
                    self.state = ParserState::InObject {
                        start_span: span,
                        seen_keys: HashMap::new(),
                        pending_doc_comment: None,
                        parent: Box::new(ParserState::AfterDocument),
                    };
                    return Some(Event::ObjectStart { span });
                }
                _ => {
                    if let ParserState::DocumentRoot {
                        pending_doc_comment,
                        ..
                    } = &mut self.state
                    {
                        *pending_doc_comment = None;
                    }
                    let atoms = self.collect_entry_atoms(lexeme);
                    if !atoms.is_empty() {
                        self.emit_entry_at_root(&atoms);
                    }
                    return self.event_queue.pop_front();
                }
            }
        }
    }

    /// Advance when in InObject state.
    fn advance_in_object(&mut self) -> Option<Event<'src>> {
        let start = if let ParserState::InObject { start_span, .. } = &self.state {
            *start_span
        } else {
            return None;
        };

        loop {
            let lexeme = self.source.next();
            match lexeme {
                Lexeme::Eof => {
                    if let ParserState::InObject {
                        pending_doc_comment,
                        ..
                    } = &mut self.state
                        && let Some(span) = pending_doc_comment.take()
                    {
                        self.event_queue.push_back(Event::Error {
                            span,
                            kind: ParseErrorKind::DanglingDocComment,
                        });
                    }
                    self.event_queue.push_back(Event::Error {
                        span: start,
                        kind: ParseErrorKind::UnclosedObject,
                    });
                    self.event_queue.push_back(Event::ObjectEnd { span: start });
                    self.pop_state();
                    return self.event_queue.pop_front();
                }
                Lexeme::ObjectEnd { span } => {
                    if let ParserState::InObject {
                        pending_doc_comment,
                        ..
                    } = &mut self.state
                        && let Some(doc_span) = pending_doc_comment.take()
                    {
                        self.event_queue.push_back(Event::Error {
                            span: doc_span,
                            kind: ParseErrorKind::DanglingDocComment,
                        });
                    }
                    self.pop_state();
                    return Some(Event::ObjectEnd { span });
                }
                Lexeme::Newline { .. } | Lexeme::Comma { .. } => continue,
                Lexeme::Comment { span, text } => {
                    return Some(Event::Comment { span, text });
                }
                Lexeme::DocComment { span, text } => {
                    if let ParserState::InObject {
                        pending_doc_comment,
                        ..
                    } = &mut self.state
                    {
                        *pending_doc_comment = Some(span);
                    }
                    // Strip `/// ` or `///` prefix
                    let line = text
                        .strip_prefix("/// ")
                        .or_else(|| text.strip_prefix("///"))
                        .unwrap_or(text);
                    return Some(Event::DocComment {
                        span,
                        lines: vec![line],
                    });
                }
                _ => {
                    if let ParserState::InObject {
                        pending_doc_comment,
                        ..
                    } = &mut self.state
                    {
                        *pending_doc_comment = None;
                    }
                    let atoms = self.collect_entry_atoms(lexeme);
                    if !atoms.is_empty() {
                        self.emit_entry_in_object(&atoms);
                    }
                    return self.event_queue.pop_front();
                }
            }
        }
    }

    /// Pop the current state and restore parent.
    fn pop_state(&mut self) {
        let parent = match &mut self.state {
            ParserState::InObject { parent, .. } => {
                std::mem::replace(parent.as_mut(), ParserState::AfterDocument)
            }
            _ => ParserState::AfterDocument,
        };
        self.state = parent;
    }

    /// Emit entry at document root (with path state).
    fn emit_entry_at_root(&mut self, atoms: &[Atom<'src>]) {
        if atoms.is_empty() {
            return;
        }

        let key_atom = &atoms[0];

        // Check for invalid key types
        if let AtomContent::Scalar {
            kind: ScalarKind::Heredoc,
            ..
        } = &key_atom.content
        {
            // For heredocs, point at just the opening marker (<<TAG), not the whole content
            let error_span = self.heredoc_start_span(key_atom.span);
            self.event_queue.push_back(Event::Error {
                span: error_span,
                kind: ParseErrorKind::InvalidKey,
            });
        }

        // Check for dotted path
        if let AtomContent::Scalar {
            value,
            kind: ScalarKind::Bare,
        } = &key_atom.content
            && value.contains('.')
        {
            self.emit_dotted_path_entry(value.clone(), key_atom.span, atoms, true);
            return;
        }

        // Simple key - use path state for duplicate detection at root level
        // (path_state handles both simple and dotted paths uniformly)
        let key_value = KeyValue::from_atom(key_atom);

        if let ParserState::DocumentRoot { path_state, .. } = &mut self.state {
            // Check path state - this handles duplicates for us
            let key_text = key_value.to_key_string();
            let path = vec![key_text];
            let value_kind = if atoms.len() >= 2 {
                match &atoms[1].content {
                    AtomContent::Object { .. } | AtomContent::Attributes(_) => {
                        PathValueKind::Object
                    }
                    _ => PathValueKind::Terminal,
                }
            } else {
                PathValueKind::Terminal
            };

            if let Err(err) = path_state.check_and_update(&path, key_atom.span, value_kind) {
                self.emit_path_error(err, key_atom.span);
            }
        }

        self.emit_simple_entry(atoms);
    }

    /// Emit entry inside an object (no path state).
    fn emit_entry_in_object(&mut self, atoms: &[Atom<'src>]) {
        if atoms.is_empty() {
            return;
        }

        let key_atom = &atoms[0];

        // Check for invalid key types
        if let AtomContent::Scalar {
            kind: ScalarKind::Heredoc,
            ..
        } = &key_atom.content
        {
            self.event_queue.push_back(Event::Error {
                span: key_atom.span,
                kind: ParseErrorKind::InvalidKey,
            });
        }

        // Check for dotted path (still allowed in nested objects)
        if let AtomContent::Scalar {
            value,
            kind: ScalarKind::Bare,
        } = &key_atom.content
            && value.contains('.')
        {
            self.emit_dotted_path_entry(value.clone(), key_atom.span, atoms, false);
            return;
        }

        // Simple key - check for duplicates
        let key_value = KeyValue::from_atom(key_atom);

        if let ParserState::InObject { seen_keys, .. } = &mut self.state {
            if let Some(&original_span) = seen_keys.get(&key_value) {
                self.event_queue.push_back(Event::Error {
                    span: key_atom.span,
                    kind: ParseErrorKind::DuplicateKey {
                        original: original_span,
                    },
                });
            } else {
                seen_keys.insert(key_value, key_atom.span);
            }
        }

        self.emit_simple_entry(atoms);
    }

    /// Emit a simple (non-dotted) entry.
    fn emit_simple_entry(&mut self, atoms: &[Atom<'src>]) {
        let key_atom = &atoms[0];

        self.event_queue.push_back(Event::EntryStart);
        self.emit_atom_as_key(key_atom);

        if atoms.len() == 1 {
            self.event_queue.push_back(Event::Unit {
                span: key_atom.span,
            });
        } else if atoms.len() >= 2 {
            self.emit_atom_as_value(&atoms[1]);
        }

        if atoms.len() > 2 {
            self.event_queue.push_back(Event::Error {
                span: atoms[2].span,
                kind: ParseErrorKind::TooManyAtoms,
            });
        }

        self.event_queue.push_back(Event::EntryEnd);
    }

    /// Collect atoms for an entry.
    fn collect_entry_atoms(&mut self, first: Lexeme<'src>) -> Vec<Atom<'src>> {
        let mut atoms = Vec::new();
        let first_atom = self.parse_atom(first);
        let first_atom_end = first_atom.span.end;
        let first_is_bare = matches!(
            &first_atom.content,
            AtomContent::Scalar {
                kind: ScalarKind::Bare,
                ..
            }
        );
        atoms.push(first_atom);

        loop {
            let lexeme = self.source.next();
            match lexeme {
                Lexeme::Eof
                | Lexeme::Newline { .. }
                | Lexeme::Comma { .. }
                | Lexeme::ObjectEnd { .. }
                | Lexeme::SeqEnd { .. } => {
                    self.source.stash(lexeme);
                    break;
                }
                Lexeme::Comment { span, text } => {
                    self.event_queue.push_back(Event::Comment { span, text });
                    break;
                }
                Lexeme::DocComment { span, text } => {
                    // Strip `/// ` or `///` prefix
                    let line = text
                        .strip_prefix("/// ")
                        .or_else(|| text.strip_prefix("///"))
                        .unwrap_or(text);
                    self.event_queue.push_back(Event::DocComment {
                        span,
                        lines: vec![line],
                    });
                    break;
                }
                Lexeme::ObjectStart { span } | Lexeme::SeqStart { span } => {
                    // Check for MissingWhitespaceBeforeBlock: bare scalar immediately
                    // followed by { or ( with no whitespace
                    if atoms.len() == 1 && first_is_bare && first_atom_end == span.start {
                        self.event_queue.push_back(Event::Error {
                            span,
                            kind: ParseErrorKind::MissingWhitespaceBeforeBlock,
                        });
                    }
                    let atom = self.parse_atom(lexeme);
                    atoms.push(atom);
                }
                _ => {
                    let atom = self.parse_atom(lexeme);
                    atoms.push(atom);
                }
            }
        }

        atoms
    }

    /// Parse a single atom.
    fn parse_atom(&mut self, lexeme: Lexeme<'src>) -> Atom<'src> {
        match lexeme {
            Lexeme::Scalar { span, value, kind } => Atom {
                span,
                content: AtomContent::Scalar { value, kind },
            },
            Lexeme::Unit { span } => {
                // Check if this is an invalid tag like @.foo or @1digit
                // The lexer produces Unit + Scalar when the tag name is invalid
                let next = self.source.next();
                if let Lexeme::Scalar {
                    span: scalar_span,
                    value,
                    kind: ScalarKind::Bare,
                } = &next
                {
                    // Adjacent spans = invalid tag (e.g., @.foo where @ is at 2 and .foo starts at 3)
                    if scalar_span.start == span.end {
                        return Atom {
                            span: Span::new(span.start, scalar_span.end),
                            content: AtomContent::Tag {
                                name: "", // empty name signals invalid
                                payload: Some(Box::new(Atom {
                                    span: *scalar_span,
                                    content: AtomContent::Scalar {
                                        value: value.clone(),
                                        kind: ScalarKind::Bare,
                                    },
                                })),
                                invalid_name: true,
                                error_span: Some(*scalar_span), // Error points at the name, not @
                            },
                        };
                    }
                }
                // Not an invalid tag, stash and return unit
                self.source.stash(next);
                Atom {
                    span,
                    content: AtomContent::Unit,
                }
            }
            Lexeme::Tag {
                span,
                name,
                has_payload,
            } => {
                // Check if this tag is followed by an adjacent scalar starting with '.'
                // This happens with @Some.Type where lexer produces Tag("Some") + Scalar(".Type")
                if !has_payload {
                    let next = self.source.next();
                    if let Lexeme::Scalar {
                        span: scalar_span,
                        value,
                        kind: ScalarKind::Bare,
                    } = &next
                        && scalar_span.start == span.end
                        && value.starts_with('.')
                    {
                        // Combined invalid tag name like @Some.Type
                        let combined_name_span = Span::new(span.start + 1, scalar_span.end);
                        return Atom {
                            span: Span::new(span.start, scalar_span.end),
                            content: AtomContent::Tag {
                                name,
                                payload: None,
                                invalid_name: true,
                                error_span: Some(combined_name_span),
                            },
                        };
                    }
                    self.source.stash(next);
                }

                let invalid_name = !is_valid_tag_name(name);
                let payload = if has_payload {
                    let next = self.source.next();
                    Some(Box::new(self.parse_atom(next)))
                } else {
                    None
                };
                let end = payload.as_ref().map(|p| p.span.end).unwrap_or(span.end);
                // For invalid tags, error span includes the @ (it's part of the tag)
                let error_span = if invalid_name { Some(span) } else { None };
                Atom {
                    span: Span::new(span.start, end),
                    content: AtomContent::Tag {
                        name,
                        payload,
                        invalid_name,
                        error_span,
                    },
                }
            }
            Lexeme::ObjectStart { span } => self.parse_object_atom(span),
            Lexeme::SeqStart { span } => self.parse_sequence_atom(span),
            Lexeme::AttrKey { key_span, key, .. } => self.parse_attributes(key_span, key),
            Lexeme::Error { span, message } => {
                // Check if this is an invalid escape error from a quoted string
                if message.contains("escape") {
                    // Extract the raw text to find escape positions
                    let raw_text = &self.input[span.start as usize..span.end as usize];
                    // Strip quotes if present
                    let inner = if raw_text.starts_with('"') && raw_text.ends_with('"') {
                        &raw_text[1..raw_text.len() - 1]
                    } else {
                        raw_text
                    };
                    Atom {
                        span,
                        content: AtomContent::InvalidEscapeScalar {
                            raw_inner: Cow::Borrowed(inner),
                        },
                    }
                } else {
                    Atom {
                        span,
                        content: AtomContent::Error { message },
                    }
                }
            }
            Lexeme::ObjectEnd { span }
            | Lexeme::SeqEnd { span }
            | Lexeme::Comma { span }
            | Lexeme::Newline { span } => Atom {
                span,
                content: AtomContent::Error {
                    message: "unexpected token",
                },
            },
            Lexeme::Comment { span, .. } | Lexeme::DocComment { span, .. } => Atom {
                span,
                content: AtomContent::Error {
                    message: "unexpected token",
                },
            },
            Lexeme::Eof => Atom {
                span: Span::new(self.input.len() as u32, self.input.len() as u32),
                content: AtomContent::Error {
                    message: "unexpected end of input",
                },
            },
        }
    }

    /// Parse an object atom.
    fn parse_object_atom(&mut self, start_span: Span) -> Atom<'src> {
        let mut entries: Vec<ObjectEntry<'src>> = Vec::new();
        let mut seen_keys: HashMap<KeyValue, Span> = HashMap::new();
        let mut duplicate_key_spans: Vec<(Span, Span)> = Vec::new();
        let mut dangling_doc_comment_spans: Vec<Span> = Vec::new();
        let mut pending_doc_comments: Vec<(Span, &'src str)> = Vec::new();
        let mut unclosed = false;
        let mut end_span = start_span;

        loop {
            let lexeme = self.source.next();
            match lexeme {
                Lexeme::Eof => {
                    unclosed = true;
                    for (span, _) in &pending_doc_comments {
                        dangling_doc_comment_spans.push(*span);
                    }
                    break;
                }
                Lexeme::ObjectEnd { span } => {
                    for (s, _) in &pending_doc_comments {
                        dangling_doc_comment_spans.push(*s);
                    }
                    end_span = span;
                    break;
                }
                Lexeme::Newline { .. } | Lexeme::Comma { .. } => continue,
                Lexeme::Comment { .. } => continue,
                Lexeme::DocComment { span, text } => {
                    pending_doc_comments.push((span, text));
                }
                _ => {
                    let doc_comment = if pending_doc_comments.is_empty() {
                        None
                    } else {
                        // Collect all doc comments, stripping the `/// ` prefix from each
                        let first_span = pending_doc_comments.first().unwrap().0;
                        let last_span = pending_doc_comments.last().unwrap().0;
                        let combined_span = Span::new(first_span.start, last_span.end);
                        let lines: Vec<&'src str> = pending_doc_comments
                            .iter()
                            .map(|(_, text)| {
                                // Strip `/// ` or `///` prefix
                                text.strip_prefix("/// ")
                                    .or_else(|| text.strip_prefix("///"))
                                    .unwrap_or(*text)
                            })
                            .collect();
                        pending_doc_comments.clear();
                        Some((combined_span, lines))
                    };
                    let entry_atoms = self.collect_entry_atoms(lexeme);

                    if !entry_atoms.is_empty() {
                        let key = entry_atoms[0].clone();
                        let key_value = KeyValue::from_atom(&key);

                        if let Some(&original_span) = seen_keys.get(&key_value) {
                            duplicate_key_spans.push((original_span, key.span));
                        } else {
                            seen_keys.insert(key_value, key.span);
                        }

                        let (value, too_many_atoms_span) = if entry_atoms.len() == 1 {
                            (
                                Atom {
                                    span: key.span,
                                    content: AtomContent::Unit,
                                },
                                None,
                            )
                        } else if entry_atoms.len() == 2 {
                            (entry_atoms[1].clone(), None)
                        } else {
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
            span: Span::new(start_span.start, end_span.end),
            content: AtomContent::Object {
                entries,
                duplicate_key_spans,
                dangling_doc_comment_spans,
                unclosed,
            },
        }
    }

    /// Parse a sequence atom.
    fn parse_sequence_atom(&mut self, start_span: Span) -> Atom<'src> {
        let mut elements: Vec<Atom<'src>> = Vec::new();
        let mut unclosed = false;
        let mut comma_spans: Vec<Span> = Vec::new();
        let mut end_span = start_span;

        loop {
            let lexeme = self.source.next();
            match lexeme {
                Lexeme::Eof => {
                    unclosed = true;
                    break;
                }
                Lexeme::SeqEnd { span } => {
                    end_span = span;
                    break;
                }
                Lexeme::Newline { .. } => continue,
                Lexeme::Comma { span } => {
                    comma_spans.push(span);
                    continue;
                }
                Lexeme::Comment { .. } | Lexeme::DocComment { .. } => continue,
                _ => {
                    let elem = self.parse_atom(lexeme);
                    elements.push(elem);
                }
            }
        }

        Atom {
            span: Span::new(start_span.start, end_span.end),
            content: AtomContent::Sequence {
                elements,
                unclosed,
                comma_spans,
            },
        }
    }

    /// Parse attributes.
    fn parse_attributes(&mut self, first_span: Span, first_key: &'src str) -> Atom<'src> {
        let mut attrs = Vec::new();
        let first_value = self.parse_attribute_value();
        attrs.push(AttributeEntry {
            key: first_key,
            key_span: first_span,
            value: first_value,
        });

        loop {
            let lexeme = self.source.next();
            match lexeme {
                Lexeme::AttrKey { key_span, key, .. } => {
                    let value = self.parse_attribute_value();
                    attrs.push(AttributeEntry {
                        key,
                        key_span,
                        value,
                    });
                }
                other => {
                    self.source.stash(other);
                    break;
                }
            }
        }

        let end = attrs
            .last()
            .map(|a| a.value.span.end)
            .unwrap_or(first_span.end);
        Atom {
            span: Span::new(first_span.start, end),
            content: AtomContent::Attributes(attrs),
        }
    }

    /// Parse an attribute value.
    fn parse_attribute_value(&mut self) -> Atom<'src> {
        let lexeme = self.source.next();
        self.parse_atom(lexeme)
    }

    /// Emit dotted path entry.
    fn emit_dotted_path_entry(
        &mut self,
        path_text: Cow<'src, str>,
        path_span: Span,
        atoms: &[Atom<'src>],
        check_path_state: bool,
    ) {
        let segments: Vec<&str> = path_text.split('.').collect();

        if segments.is_empty() || segments.iter().any(|s| s.is_empty()) {
            self.event_queue.push_back(Event::Error {
                span: path_span,
                kind: ParseErrorKind::InvalidKey,
            });
            self.event_queue.push_back(Event::EntryStart);
            self.event_queue.push_back(Event::EntryEnd);
            return;
        }

        // Check path state at root
        if check_path_state
            && let ParserState::DocumentRoot {
                seen_keys,
                path_state,
                ..
            } = &mut self.state
        {
            let first_key_value = KeyValue::Scalar(segments[0].to_string());
            seen_keys.entry(first_key_value).or_insert(path_span);

            let path: Vec<String> = segments.iter().map(|s| s.to_string()).collect();
            let value_kind = if atoms.len() >= 2 {
                match &atoms[1].content {
                    AtomContent::Object { .. } | AtomContent::Attributes(_) => {
                        PathValueKind::Object
                    }
                    _ => PathValueKind::Terminal,
                }
            } else {
                PathValueKind::Terminal
            };

            if let Err(err) = path_state.check_and_update(&path, path_span, value_kind) {
                self.emit_path_error(err, path_span);
            }
        }

        // Emit nested structure
        let depth = segments.len();
        let mut current_offset = path_span.start;

        for (i, segment) in segments.iter().enumerate() {
            let segment_len = segment.len() as u32;
            let segment_span = Span::new(current_offset, current_offset + segment_len);

            self.event_queue.push_back(Event::EntryStart);
            self.event_queue.push_back(Event::Key {
                span: segment_span,
                tag: None,
                payload: Some(Cow::Owned(segment.to_string())),
                kind: ScalarKind::Bare,
            });

            if i < depth - 1 {
                self.event_queue
                    .push_back(Event::ObjectStart { span: segment_span });
            }

            current_offset += segment_len + 1;
        }

        // Emit value
        if atoms.len() == 1 {
            self.event_queue.push_back(Event::Unit { span: path_span });
        } else if atoms.len() >= 2 {
            self.emit_atom_as_value(&atoms[1]);
        }

        if atoms.len() > 2 {
            self.event_queue.push_back(Event::Error {
                span: atoms[2].span,
                kind: ParseErrorKind::TooManyAtoms,
            });
        }

        // Close nested structures
        for i in (0..depth).rev() {
            if i < depth - 1 {
                self.event_queue
                    .push_back(Event::ObjectEnd { span: path_span });
            }
            self.event_queue.push_back(Event::EntryEnd);
        }
    }

    /// Emit path error.
    fn emit_path_error(&mut self, err: PathError, span: Span) {
        let kind = match err {
            PathError::Duplicate { original } => ParseErrorKind::DuplicateKey { original },
            PathError::Reopened { closed_path } => ParseErrorKind::ReopenedPath { closed_path },
            PathError::NestIntoTerminal { terminal_path } => {
                ParseErrorKind::NestIntoTerminal { terminal_path }
            }
        };
        self.event_queue.push_back(Event::Error { span, kind });
    }

    /// Get the span of just the heredoc opening marker (<<TAG\n).
    fn heredoc_start_span(&self, heredoc_span: Span) -> Span {
        let text = &self.input[heredoc_span.start as usize..heredoc_span.end as usize];
        // Find the first newline - that's the end of the opening marker
        let end_offset = text.find('\n').map(|i| i + 1).unwrap_or(text.len());
        Span::new(heredoc_span.start, heredoc_span.start + end_offset as u32)
    }

    /// Emit atom as key.
    fn emit_atom_as_key(&mut self, atom: &Atom<'src>) {
        match &atom.content {
            AtomContent::Scalar { value, kind } => {
                // The lexer already processed escape sequences.
                self.event_queue.push_back(Event::Key {
                    span: atom.span,
                    tag: None,
                    payload: Some(value.clone()),
                    kind: *kind,
                });
            }
            AtomContent::Unit => {
                self.event_queue.push_back(Event::Key {
                    span: atom.span,
                    tag: None,
                    payload: None,
                    kind: ScalarKind::Bare,
                });
            }
            AtomContent::Tag {
                name,
                payload,
                invalid_name,
                error_span,
            } => {
                if *invalid_name {
                    self.event_queue.push_back(Event::Error {
                        span: error_span.unwrap_or(atom.span),
                        kind: ParseErrorKind::InvalidTagName,
                    });
                }
                match payload {
                    None => {
                        self.event_queue.push_back(Event::Key {
                            span: atom.span,
                            tag: Some(name),
                            payload: None,
                            kind: ScalarKind::Bare,
                        });
                    }
                    Some(inner) => match &inner.content {
                        AtomContent::Scalar { value, kind } => {
                            if *kind == ScalarKind::Quoted {
                                self.emit_escape_errors(value, inner.span);
                            }
                            self.event_queue.push_back(Event::Key {
                                span: atom.span,
                                tag: Some(name),
                                payload: Some(value.clone()),
                                kind: *kind,
                            });
                        }
                        AtomContent::Unit => {
                            self.event_queue.push_back(Event::Key {
                                span: atom.span,
                                tag: Some(name),
                                payload: None,
                                kind: ScalarKind::Bare,
                            });
                        }
                        _ => {
                            self.event_queue.push_back(Event::Error {
                                span: inner.span,
                                kind: ParseErrorKind::InvalidKey,
                            });
                        }
                    },
                }
            }
            AtomContent::InvalidEscapeScalar { raw_inner } => {
                // Emit the escape errors at their specific positions
                let inner_start = atom.span.start + 1;
                for (offset, seq) in validate_escapes(raw_inner) {
                    let error_start = inner_start + offset as u32;
                    let error_span = Span::new(error_start, error_start + seq.len() as u32);
                    self.event_queue.push_back(Event::Error {
                        span: error_span,
                        kind: ParseErrorKind::InvalidEscape(seq),
                    });
                }
                // Still emit a key event (with the partially-processed value)
                self.event_queue.push_back(Event::Key {
                    span: atom.span,
                    tag: None,
                    payload: Some(Cow::Owned(unescape_quoted(raw_inner).into_owned())),
                    kind: ScalarKind::Quoted,
                });
            }
            AtomContent::Error { message } => {
                let kind = if message.contains("invalid tag name") {
                    ParseErrorKind::InvalidTagName
                } else {
                    ParseErrorKind::InvalidKey
                };
                self.event_queue.push_back(Event::Error {
                    span: atom.span,
                    kind,
                });
            }
            _ => {
                self.event_queue.push_back(Event::Error {
                    span: atom.span,
                    kind: ParseErrorKind::InvalidKey,
                });
            }
        }
    }

    /// Emit atom as value.
    fn emit_atom_as_value(&mut self, atom: &Atom<'src>) {
        match &atom.content {
            AtomContent::Scalar { value, kind } => {
                // The lexer already processed escape sequences.
                self.event_queue.push_back(Event::Scalar {
                    span: atom.span,
                    value: value.clone(),
                    kind: *kind,
                });
            }
            AtomContent::Unit => {
                self.event_queue.push_back(Event::Unit { span: atom.span });
            }
            AtomContent::Tag {
                name,
                payload,
                invalid_name,
                error_span,
            } => {
                if *invalid_name {
                    self.event_queue.push_back(Event::Error {
                        span: error_span.unwrap_or(atom.span),
                        kind: ParseErrorKind::InvalidTagName,
                    });
                }
                self.event_queue.push_back(Event::TagStart {
                    span: atom.span,
                    name,
                });
                if let Some(inner) = payload {
                    self.emit_atom_as_value(inner);
                }
                self.event_queue.push_back(Event::TagEnd);
            }
            AtomContent::Object {
                entries,
                duplicate_key_spans,
                dangling_doc_comment_spans,
                unclosed,
            } => {
                self.event_queue
                    .push_back(Event::ObjectStart { span: atom.span });

                if *unclosed {
                    self.event_queue.push_back(Event::Error {
                        span: atom.span,
                        kind: ParseErrorKind::UnclosedObject,
                    });
                }

                for (original, dup) in duplicate_key_spans {
                    self.event_queue.push_back(Event::Error {
                        span: *dup,
                        kind: ParseErrorKind::DuplicateKey {
                            original: *original,
                        },
                    });
                }

                for span in dangling_doc_comment_spans {
                    self.event_queue.push_back(Event::Error {
                        span: *span,
                        kind: ParseErrorKind::DanglingDocComment,
                    });
                }

                for entry in entries {
                    if let Some((span, lines)) = &entry.doc_comment {
                        self.event_queue.push_back(Event::DocComment {
                            span: *span,
                            lines: lines.clone(),
                        });
                    }
                    self.event_queue.push_back(Event::EntryStart);
                    self.emit_atom_as_key(&entry.key);
                    self.emit_atom_as_value(&entry.value);
                    if let Some(span) = entry.too_many_atoms_span {
                        self.event_queue.push_back(Event::Error {
                            span,
                            kind: ParseErrorKind::TooManyAtoms,
                        });
                    }
                    self.event_queue.push_back(Event::EntryEnd);
                }

                self.event_queue
                    .push_back(Event::ObjectEnd { span: atom.span });
            }
            AtomContent::Sequence {
                elements,
                unclosed,
                comma_spans,
            } => {
                self.event_queue
                    .push_back(Event::SequenceStart { span: atom.span });

                if *unclosed {
                    self.event_queue.push_back(Event::Error {
                        span: atom.span,
                        kind: ParseErrorKind::UnclosedSequence,
                    });
                }

                for span in comma_spans {
                    self.event_queue.push_back(Event::Error {
                        span: *span,
                        kind: ParseErrorKind::CommaInSequence,
                    });
                }

                for elem in elements {
                    self.emit_atom_as_value(elem);
                }

                self.event_queue
                    .push_back(Event::SequenceEnd { span: atom.span });
            }
            AtomContent::Attributes(attrs) => {
                self.event_queue
                    .push_back(Event::ObjectStart { span: atom.span });

                for attr in attrs {
                    self.event_queue.push_back(Event::EntryStart);
                    self.event_queue.push_back(Event::Key {
                        span: attr.key_span,
                        tag: None,
                        payload: Some(Cow::Borrowed(attr.key)),
                        kind: ScalarKind::Bare,
                    });
                    self.emit_atom_as_value(&attr.value);
                    self.event_queue.push_back(Event::EntryEnd);
                }

                self.event_queue
                    .push_back(Event::ObjectEnd { span: atom.span });
            }
            AtomContent::InvalidEscapeScalar { raw_inner } => {
                // Emit the escape errors at their specific positions
                // The span includes quotes, so offset by 1 for the opening quote
                let inner_start = atom.span.start + 1;
                for (offset, seq) in validate_escapes(raw_inner) {
                    let error_start = inner_start + offset as u32;
                    let error_span = Span::new(error_start, error_start + seq.len() as u32);
                    self.event_queue.push_back(Event::Error {
                        span: error_span,
                        kind: ParseErrorKind::InvalidEscape(seq),
                    });
                }
                // Also emit the scalar value (with invalid escapes replaced/kept)
                self.event_queue.push_back(Event::Scalar {
                    span: atom.span,
                    value: Cow::Owned(unescape_quoted(raw_inner).into_owned()),
                    kind: ScalarKind::Quoted,
                });
            }
            AtomContent::Error { message } => {
                let kind = if message.contains("invalid tag name") {
                    ParseErrorKind::InvalidTagName
                } else if message.contains("expected a value") {
                    ParseErrorKind::ExpectedValue
                } else {
                    ParseErrorKind::UnexpectedToken
                };
                self.event_queue.push_back(Event::Error {
                    span: atom.span,
                    kind,
                });
            }
        }
    }

    /// Emit escape errors.
    fn emit_escape_errors(&mut self, text: &str, span: Span) {
        for (offset, seq) in validate_escapes(text) {
            let error_start = span.start + offset as u32;
            let error_span = Span::new(error_start, error_start + seq.len() as u32);
            self.event_queue.push_back(Event::Error {
                span: error_span,
                kind: ParseErrorKind::InvalidEscape(seq),
            });
        }
    }
}

// ============================================================================
// Atom types
// ============================================================================

#[derive(Debug, Clone)]
struct Atom<'src> {
    span: Span,
    content: AtomContent<'src>,
}

#[derive(Debug, Clone)]
enum AtomContent<'src> {
    Scalar {
        value: Cow<'src, str>,
        kind: ScalarKind,
    },
    Unit,
    Tag {
        name: &'src str,
        payload: Option<Box<Atom<'src>>>,
        invalid_name: bool,
        /// For invalid tags, the span to use for the error (excludes @).
        /// If None, uses atom.span.
        error_span: Option<Span>,
    },
    Object {
        entries: Vec<ObjectEntry<'src>>,
        duplicate_key_spans: Vec<(Span, Span)>,
        dangling_doc_comment_spans: Vec<Span>,
        unclosed: bool,
    },
    Sequence {
        elements: Vec<Atom<'src>>,
        unclosed: bool,
        comma_spans: Vec<Span>,
    },
    Attributes(Vec<AttributeEntry<'src>>),
    /// A quoted scalar with invalid escape sequences.
    /// We store the raw inner text (without quotes) to scan for escape errors.
    InvalidEscapeScalar {
        raw_inner: Cow<'src, str>,
    },
    /// An error from the lexer.
    Error {
        message: &'src str,
    },
}

#[derive(Debug, Clone)]
struct ObjectEntry<'src> {
    key: Atom<'src>,
    value: Atom<'src>,
    doc_comment: Option<(Span, Vec<&'src str>)>,
    too_many_atoms_span: Option<Span>,
}

#[derive(Debug, Clone)]
struct AttributeEntry<'src> {
    key: &'src str,
    key_span: Span,
    value: Atom<'src>,
}

// ============================================================================
// Key comparison
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum KeyValue {
    Scalar(String),
    Unit,
    Tagged {
        name: String,
        payload: Option<Box<KeyValue>>,
    },
}

impl KeyValue {
    fn from_atom(atom: &Atom<'_>) -> Self {
        match &atom.content {
            AtomContent::Scalar { value, .. } => KeyValue::Scalar(value.to_string()),
            AtomContent::Unit => KeyValue::Unit,
            AtomContent::Tag { name, payload, .. } => KeyValue::Tagged {
                name: (*name).to_string(),
                payload: payload.as_ref().map(|p| Box::new(KeyValue::from_atom(p))),
            },
            AtomContent::Object { .. } => KeyValue::Scalar("{}".into()),
            AtomContent::Sequence { .. } => KeyValue::Scalar("()".into()),
            AtomContent::Attributes(_) => KeyValue::Scalar("{}".into()),
            AtomContent::InvalidEscapeScalar { raw_inner } => {
                // This is raw text that failed escape processing - just use it as-is
                KeyValue::Scalar(raw_inner.to_string())
            }
            AtomContent::Error { .. } => KeyValue::Scalar("<error>".into()),
        }
    }

    fn to_key_string(&self) -> String {
        match self {
            KeyValue::Scalar(s) => s.clone(),
            KeyValue::Unit => "@".to_string(),
            KeyValue::Tagged { name, .. } => format!("@{}", name),
        }
    }
}

// ============================================================================
// Path tracking
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PathValueKind {
    Object,
    Terminal,
}

#[derive(Debug, Clone)]
enum PathError {
    Duplicate { original: Span },
    Reopened { closed_path: Vec<String> },
    NestIntoTerminal { terminal_path: Vec<String> },
}

#[derive(Default, Clone)]
struct PathState {
    current_path: Vec<String>,
    closed_paths: HashSet<Vec<String>>,
    assigned_paths: HashMap<Vec<String>, (Span, PathValueKind)>,
}

impl PathState {
    fn check_and_update(
        &mut self,
        path: &[String],
        span: Span,
        value_kind: PathValueKind,
    ) -> Result<(), PathError> {
        // Check for duplicate
        if let Some(&(original, _)) = self.assigned_paths.get(path) {
            return Err(PathError::Duplicate { original });
        }

        // Check prefixes
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

        // Close paths beyond common prefix
        let common_len = self
            .current_path
            .iter()
            .zip(path.iter())
            .take_while(|(a, b)| a == b)
            .count();

        for i in common_len..self.current_path.len() {
            let closed: Vec<String> = self.current_path[..=i].to_vec();
            self.closed_paths.insert(closed);
        }

        // Record intermediate segments as objects
        for i in 1..path.len() {
            let prefix = path[..i].to_vec();
            self.assigned_paths
                .entry(prefix)
                .or_insert((span, PathValueKind::Object));
        }

        self.assigned_paths
            .insert(path.to_vec(), (span, value_kind));
        self.current_path = path.to_vec();

        Ok(())
    }
}

// ============================================================================
// Helpers
// ============================================================================

fn is_valid_tag_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn unescape_quoted(text: &str) -> Cow<'_, str> {
    if !text.contains('\\') {
        return Cow::Borrowed(text);
    }

    let mut result = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => result.push('\n'),
                Some('r') => result.push('\r'),
                Some('t') => result.push('\t'),
                Some('\\') => result.push('\\'),
                Some('"') => result.push('"'),
                Some('u') => match chars.peek() {
                    Some('{') => {
                        chars.next();
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
                    Some(&c) if c.is_ascii_hexdigit() => {
                        let mut hex = String::with_capacity(4);
                        for _ in 0..4 {
                            if let Some(&c) = chars.peek() {
                                if c.is_ascii_hexdigit() {
                                    hex.push(chars.next().unwrap());
                                } else {
                                    break;
                                }
                            }
                        }
                        if hex.len() == 4
                            && let Ok(code) = u32::from_str_radix(&hex, 16)
                            && let Some(ch) = char::from_u32(code)
                        {
                            result.push(ch);
                        }
                    }
                    _ => {}
                },
                Some(c) => {
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

fn validate_escapes(text: &str) -> Vec<(usize, String)> {
    let mut errors = Vec::new();
    let mut chars = text.char_indices().peekable();

    while let Some((i, c)) = chars.next() {
        if c == '\\' {
            let escape_start = i;
            match chars.next() {
                Some((_, 'n' | 'r' | 't' | '\\' | '"')) => {}
                Some((_, 'u')) => match chars.peek() {
                    Some((_, '{')) => {
                        chars.next();
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
                            let end = chars.peek().map(|(i, _)| *i).unwrap_or(text.len());
                            let seq = &text[escape_start..end.min(escape_start + 12)];
                            errors.push((escape_start, seq.to_string()));
                        }
                    }
                    Some((_, c)) if c.is_ascii_hexdigit() => {
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
                            let end = chars.peek().map(|(i, _)| *i).unwrap_or(text.len());
                            let seq = &text[escape_start..end];
                            errors.push((escape_start, seq.to_string()));
                        }
                    }
                    _ => {
                        errors.push((escape_start, "\\u".to_string()));
                    }
                },
                Some((_, c)) => {
                    errors.push((escape_start, format!("\\{}", c)));
                }
                None => {
                    errors.push((escape_start, "\\".to_string()));
                }
            }
        }
    }

    errors
}

#[cfg(test)]
mod tests;
