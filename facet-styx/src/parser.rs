//! Styx parser implementing the FormatParser trait.

use std::borrow::Cow;

use facet_core::Facet;
use facet_format::{
    ContainerKind, FieldKey, FieldLocationHint, FormatParser, ParseEvent, SavePoint, ScalarValue,
};
use styx_parse::{Lexer, ScalarKind, Span, Token, TokenKind};

use crate::error::{StyxError, StyxErrorKind};
use crate::trace;

/// Streaming Styx parser implementing FormatParser.
#[derive(Clone)]
pub struct StyxParser<'de> {
    input: &'de str,
    lexer: Lexer<'de>,
    /// Stack of parsing contexts.
    stack: Vec<ContextState>,
    /// Peeked token (if any).
    peeked_token: Option<Token<'de>>,
    /// Peeked events queue (if any).
    peeked_events: Vec<ParseEvent<'de>>,
    /// Whether we've emitted the root struct start.
    root_started: bool,
    /// Whether parsing is complete.
    complete: bool,
    /// Current span for error reporting.
    current_span: Option<Span>,
    /// Pending key for the current entry.
    pending_key: Option<Cow<'de, str>>,
    /// Whether we're expecting a value after a key.
    expecting_value: bool,
    /// Expression mode: parse a single value, not an implicit root object.
    expr_mode: bool,
    /// Start offset of the value being peeked (for capture_raw).
    peek_start_offset: Option<usize>,
    /// Buffered doc comments for the next field key.
    pending_doc: Vec<Cow<'de, str>>,
    /// Saved parser state for save/restore.
    saved_state: Option<Box<StyxParser<'de>>>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum ContextState {
    /// Inside an object (braces or implicit root).
    Object { implicit: bool },
    /// Inside a sequence (parens).
    Sequence,
}

impl<'de> StyxParser<'de> {
    /// Create a new parser for the given source (document mode).
    pub fn new(source: &'de str) -> Self {
        Self {
            input: source,
            lexer: Lexer::new(source),
            stack: Vec::new(),
            peeked_token: None,
            peeked_events: Vec::new(),
            root_started: false,
            complete: false,
            current_span: None,
            pending_key: None,
            expecting_value: false,
            expr_mode: false,
            peek_start_offset: None,
            pending_doc: Vec::new(),
            saved_state: None,
        }
    }

    /// Create a new parser in expression mode.
    ///
    /// Expression mode parses a single value rather than an implicit root object.
    /// Use this for parsing embedded values like default values in schemas.
    pub fn new_expr(source: &'de str) -> Self {
        Self {
            input: source,
            lexer: Lexer::new(source),
            stack: Vec::new(),
            peeked_token: None,
            peeked_events: Vec::new(),
            root_started: false,
            complete: false,
            current_span: None,
            pending_key: None,
            expecting_value: true, // Start expecting a value immediately
            expr_mode: true,
            peek_start_offset: None,
            pending_doc: Vec::new(),
            saved_state: None,
        }
    }

    /// Peek at the next token without consuming it.
    fn peek_token(&mut self) -> Option<&Token<'de>> {
        if self.peeked_token.is_none() {
            loop {
                let token = self.lexer.next_token();
                // Skip whitespace and comments
                match token.kind {
                    TokenKind::Whitespace | TokenKind::LineComment => continue,
                    TokenKind::Eof => {
                        self.peeked_token = Some(token);
                        break;
                    }
                    _ => {
                        self.peeked_token = Some(token);
                        break;
                    }
                }
            }
        }
        self.peeked_token.as_ref()
    }

    /// Consume the next token.
    fn next_token(&mut self) -> Token<'de> {
        if let Some(token) = self.peeked_token.take() {
            self.current_span = Some(token.span);
            return token;
        }
        loop {
            let token = self.lexer.next_token();
            match token.kind {
                TokenKind::Whitespace | TokenKind::LineComment => continue,
                _ => {
                    self.current_span = Some(token.span);
                    return token;
                }
            }
        }
    }

    /// Skip newlines and return true if any were found.
    fn skip_newlines(&mut self) -> bool {
        let mut found = false;
        loop {
            if let Some(token) = self.peek_token()
                && token.kind == TokenKind::Newline
            {
                self.next_token();
                found = true;
                continue;
            }
            break;
        }
        found
    }

    /// Parse a scalar value into a ScalarValue.
    fn parse_scalar(&self, text: &'de str, kind: ScalarKind) -> ScalarValue<'de> {
        match kind {
            ScalarKind::Bare => {
                // Try to parse as number or bool
                if text == "true" {
                    ScalarValue::Bool(true)
                } else if text == "false" {
                    ScalarValue::Bool(false)
                } else if text == "null" {
                    ScalarValue::Null
                } else if let Ok(n) = text.parse::<i64>() {
                    ScalarValue::I64(n)
                } else if let Ok(n) = text.parse::<u64>() {
                    ScalarValue::U64(n)
                } else if let Ok(n) = text.parse::<f64>() {
                    ScalarValue::F64(n)
                } else {
                    // Bare identifier - treat as string
                    ScalarValue::Str(Cow::Borrowed(text))
                }
            }
            ScalarKind::Quoted => {
                // Quoted strings are definitely strings
                let inner = self.unescape_quoted(text);
                ScalarValue::Str(inner)
            }
            ScalarKind::Raw | ScalarKind::Heredoc => {
                // Raw and heredoc are strings
                ScalarValue::Str(Cow::Borrowed(text))
            }
        }
    }

    /// Unescape a quoted string.
    fn unescape_quoted(&self, text: &'de str) -> Cow<'de, str> {
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
                    Some('u') => {
                        if chars.next() == Some('{') {
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
                    }
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

    /// Get the scalar kind for a token.
    fn token_to_scalar_kind(&self, kind: TokenKind) -> ScalarKind {
        match kind {
            TokenKind::BareScalar => ScalarKind::Bare,
            TokenKind::QuotedScalar => ScalarKind::Quoted,
            TokenKind::RawScalar => ScalarKind::Raw,
            TokenKind::HeredocStart | TokenKind::HeredocContent | TokenKind::HeredocEnd => {
                ScalarKind::Heredoc
            }
            _ => ScalarKind::Bare,
        }
    }

    fn error(&self, kind: StyxErrorKind) -> StyxError {
        StyxError::new(kind, self.current_span)
    }

    /// Parse a tag and emit appropriate events.
    /// Called after consuming the @ token.
    /// Returns the first event to emit (others are queued in peeked_events).
    fn parse_tag(&mut self, at_span_end: u32) -> ParseEvent<'de> {
        // Check if followed by identifier (tag name)
        if let Some(next) = self.peek_token()
            && next.kind == TokenKind::BareScalar
            && next.span.start == at_span_end
        {
            let name_token = self.next_token();
            let tag_name = name_token.text;

            // Check for payload
            if let Some(next) = self.peek_token() {
                if next.kind == TokenKind::At && next.span.start == name_token.span.end {
                    // @foo@ - tag with explicit unit payload
                    self.next_token(); // consume the @
                    self.peeked_events
                        .push(ParseEvent::Scalar(ScalarValue::Unit));
                    return ParseEvent::VariantTag(Some(tag_name));
                } else if next.kind == TokenKind::LBrace && next.span.start == name_token.span.end {
                    // @foo{...} - tag with object payload
                    self.next_token(); // consume {
                    self.stack.push(ContextState::Object { implicit: false });
                    self.peeked_events
                        .push(ParseEvent::StructStart(ContainerKind::Object));
                    return ParseEvent::VariantTag(Some(tag_name));
                } else if next.kind == TokenKind::LParen && next.span.start == name_token.span.end {
                    // @foo(...) - tag with sequence payload
                    self.next_token(); // consume (
                    self.stack.push(ContextState::Sequence);
                    self.peeked_events
                        .push(ParseEvent::SequenceStart(ContainerKind::Array));
                    return ParseEvent::VariantTag(Some(tag_name));
                }
            }

            // @foo - named tag with implicit unit payload
            self.peeked_events
                .push(ParseEvent::Scalar(ScalarValue::Unit));
            return ParseEvent::VariantTag(Some(tag_name));
        }

        // Just @ alone - unit tag (no name) with unit payload
        self.peeked_events
            .push(ParseEvent::Scalar(ScalarValue::Unit));
        ParseEvent::VariantTag(None)
    }
}

impl<'de> FormatParser<'de> for StyxParser<'de> {
    type Error = StyxError;

    fn next_event(&mut self) -> Result<Option<ParseEvent<'de>>, Self::Error> {
        // Return queued event if any (FIFO - take from front)
        if !self.peeked_events.is_empty() {
            let event = self.peeked_events.remove(0);
            // Clear peek_start_offset when consuming peeked events
            if self.peeked_events.is_empty() {
                self.peek_start_offset = None;
            }
            trace!(?event, "next_event: returning queued event");
            return Ok(Some(event));
        }

        if self.complete {
            trace!("next_event: parsing complete");
            return Ok(None);
        }

        // Skip newlines between entries, but NOT when expecting a value.
        // A newline after a key means the key has unit value.
        if !self.expecting_value {
            self.skip_newlines();
        }

        // Handle root struct start (skip in expression mode)
        if !self.root_started && !self.expr_mode {
            self.root_started = true;
            self.stack.push(ContextState::Object { implicit: true });
            trace!("next_event: emitting root StructStart");
            return Ok(Some(ParseEvent::StructStart(ContainerKind::Object)));
        }
        self.root_started = true;

        // If we're expecting a value after a key
        if self.expecting_value {
            self.expecting_value = false;
            trace!("next_event: expecting value after key");

            let token = self.peek_token().cloned();
            if let Some(token) = token {
                match token.kind {
                    TokenKind::Newline | TokenKind::Eof | TokenKind::RBrace | TokenKind::Comma => {
                        // No value - emit unit
                        trace!("next_event: no value found, emitting Unit");
                        return Ok(Some(ParseEvent::Scalar(ScalarValue::Unit)));
                    }
                    TokenKind::LBrace => {
                        // Nested object
                        self.next_token();
                        self.stack.push(ContextState::Object { implicit: false });
                        trace!("next_event: nested object StructStart");
                        return Ok(Some(ParseEvent::StructStart(ContainerKind::Object)));
                    }
                    TokenKind::LParen => {
                        // Sequence
                        self.next_token();
                        self.stack.push(ContextState::Sequence);
                        trace!("next_event: SequenceStart");
                        return Ok(Some(ParseEvent::SequenceStart(ContainerKind::Array)));
                    }
                    TokenKind::At => {
                        // Tag - could be @, @foo, @foo@, @foo(...), @foo{...}
                        self.next_token();
                        let event = self.parse_tag(token.span.end);
                        trace!(?event, "next_event: parsed tag");
                        return Ok(Some(event));
                    }
                    TokenKind::BareScalar
                    | TokenKind::QuotedScalar
                    | TokenKind::RawScalar
                    | TokenKind::HeredocStart => {
                        let token = self.next_token();
                        let kind = self.token_to_scalar_kind(token.kind);

                        // Handle heredoc content
                        let text = if token.kind == TokenKind::HeredocStart {
                            // Collect heredoc content
                            let mut content = String::new();
                            loop {
                                let next = self.next_token();
                                match next.kind {
                                    TokenKind::HeredocContent => {
                                        content.push_str(next.text);
                                    }
                                    TokenKind::HeredocEnd => break,
                                    _ => break,
                                }
                            }
                            trace!(?content, "next_event: heredoc scalar");
                            return Ok(Some(ParseEvent::Scalar(ScalarValue::Str(Cow::Owned(
                                content,
                            )))));
                        } else {
                            token.text
                        };

                        let scalar = self.parse_scalar(text, kind);
                        trace!(?scalar, "next_event: scalar value");
                        return Ok(Some(ParseEvent::Scalar(scalar)));
                    }
                    _ => {}
                }
            }
        }

        // Check for end of current context
        let token = self.peek_token().cloned();
        if let Some(token) = token {
            match token.kind {
                TokenKind::Eof => {
                    // Pop remaining contexts
                    if let Some(ctx) = self.stack.pop() {
                        match ctx {
                            ContextState::Object { .. } => {
                                if self.stack.is_empty() {
                                    self.complete = true;
                                }
                                trace!("next_event: EOF StructEnd");
                                return Ok(Some(ParseEvent::StructEnd));
                            }
                            ContextState::Sequence => {
                                trace!("next_event: EOF SequenceEnd");
                                return Ok(Some(ParseEvent::SequenceEnd));
                            }
                        }
                    }
                    // In expression mode with empty stack, we're done
                    self.complete = true;
                    return Ok(None);
                }
                TokenKind::RBrace => {
                    self.next_token();
                    match self.stack.pop() {
                        Some(ContextState::Object { implicit: false }) => {
                            trace!("next_event: RBrace StructEnd");
                            return Ok(Some(ParseEvent::StructEnd));
                        }
                        _ => {
                            // Mismatched brace - error
                            return Err(self.error(StyxErrorKind::UnexpectedToken {
                                got: "}".to_string(),
                                expected: "key or value",
                            }));
                        }
                    }
                }
                TokenKind::RParen => {
                    self.next_token();
                    match self.stack.pop() {
                        Some(ContextState::Sequence) => {
                            trace!("next_event: RParen SequenceEnd");
                            return Ok(Some(ParseEvent::SequenceEnd));
                        }
                        _ => {
                            return Err(self.error(StyxErrorKind::UnexpectedToken {
                                got: ")".to_string(),
                                expected: "value",
                            }));
                        }
                    }
                }
                TokenKind::Comma => {
                    // Skip comma separators
                    self.next_token();
                    self.skip_newlines();
                    return self.next_event();
                }
                TokenKind::Newline => {
                    self.next_token();
                    return self.next_event();
                }
                TokenKind::DocComment => {
                    // Buffer doc comments to attach to the next field key
                    let token = self.next_token();
                    // Doc comment text is "/// comment" - strip the "/// " prefix
                    let text = token.text.strip_prefix("///").unwrap_or(token.text);
                    let text = text.strip_prefix(' ').unwrap_or(text);
                    self.pending_doc.push(Cow::Borrowed(text));
                    return self.next_event();
                }
                _ => {}
            }
        }

        // In object context, parse key-value
        if matches!(self.stack.last(), Some(ContextState::Object { .. })) {
            let token = self.peek_token().cloned();
            if let Some(token) = token {
                match token.kind {
                    TokenKind::BareScalar | TokenKind::QuotedScalar => {
                        let key_token = self.next_token();
                        let key = if key_token.kind == TokenKind::QuotedScalar {
                            self.unescape_quoted(key_token.text)
                        } else {
                            Cow::Borrowed(key_token.text)
                        };

                        self.pending_key = Some(key.clone());
                        self.expecting_value = true;

                        // Take any buffered doc comments
                        let doc = std::mem::take(&mut self.pending_doc);

                        trace!(?key, ?doc, "next_event: FieldKey");
                        return Ok(Some(ParseEvent::FieldKey(FieldKey::with_doc(
                            key,
                            FieldLocationHint::KeyValue,
                            doc,
                        ))));
                    }
                    TokenKind::At => {
                        // In object context, @ starts a key.
                        // The key is the full tagged value representation:
                        // - `@` alone = key "@"
                        // - `@foo` = key "@foo" (with implicit unit value for the entry)
                        // - `@foo{...}` = key "@foo{...}" (the whole thing is the key!)
                        //
                        // This is because Styx documents are implicitly objects, so
                        // `@object{fields (a b c)}` becomes `{ @object{fields (a b c)} @ }`
                        // where the entire tagged value is a key with unit value.
                        //
                        // For now, we only handle simple cases: `@` and `@name` as keys.
                        // Complex tagged values as keys would need the parser to serialize
                        // the tagged value back to a string representation.
                        let at_token = self.next_token();

                        // Check if followed immediately by identifier
                        if let Some(next) = self.peek_token()
                            && next.kind == TokenKind::BareScalar
                            && next.span.start == at_token.span.end
                        {
                            let name_token = self.next_token();
                            let tag_name = name_token.text.to_string();
                            let name_end = name_token.span.end;

                            // Check what follows the tag name
                            let after_info = self.peek_token().map(|t| (t.span.start, t.kind));
                            if let Some((after_start, after_kind)) = after_info
                                && after_start == name_end
                            {
                                match after_kind {
                                    TokenKind::LBrace | TokenKind::LParen | TokenKind::At => {
                                        // @foo{...} or @foo(...) or @foo@ as a key
                                        // This is complex - for now, error
                                        return Err(self.error(StyxErrorKind::UnexpectedToken {
                                                    expected: "simple key",
                                                    got: format!(
                                                        "complex tagged value @{}{} cannot be used as object key",
                                                        tag_name,
                                                        match after_kind {
                                                            TokenKind::LBrace => "{...}",
                                                            TokenKind::LParen => "(...)",
                                                            TokenKind::At => "@",
                                                            _ => "",
                                                        }
                                                    ),
                                                }));
                                    }
                                    _ => {}
                                }
                            }

                            // @name with space after = tagged key with tag name
                            let tag_name_str = name_token.text;
                            // Still store "@name" as pending_key for error reporting
                            self.pending_key = Some(Cow::Owned(format!("@{}", tag_name_str)));
                            self.expecting_value = true;
                            let doc = std::mem::take(&mut self.pending_doc);
                            trace!(tag = tag_name_str, ?doc, "next_event: FieldKey (tagged)");
                            return Ok(Some(ParseEvent::FieldKey(FieldKey::tagged_with_doc(
                                tag_name_str,
                                FieldLocationHint::KeyValue,
                                doc,
                            ))));
                        }

                        // @ alone or @ followed by space/newline = unit key (None)
                        self.pending_key = Some(Cow::Borrowed("@"));
                        self.expecting_value = true;
                        let doc = std::mem::take(&mut self.pending_doc);
                        trace!(?doc, "next_event: FieldKey (unit)");
                        return Ok(Some(ParseEvent::FieldKey(FieldKey::unit_with_doc(
                            FieldLocationHint::KeyValue,
                            doc,
                        ))));
                    }
                    _ => {}
                }
            }
        }

        // In sequence context, parse values
        if matches!(self.stack.last(), Some(ContextState::Sequence)) {
            let token = self.peek_token().cloned();
            if let Some(token) = token {
                match token.kind {
                    TokenKind::BareScalar
                    | TokenKind::QuotedScalar
                    | TokenKind::RawScalar
                    | TokenKind::HeredocStart => {
                        let token = self.next_token();
                        let kind = self.token_to_scalar_kind(token.kind);
                        let scalar = self.parse_scalar(token.text, kind);
                        return Ok(Some(ParseEvent::Scalar(scalar)));
                    }
                    TokenKind::LBrace => {
                        self.next_token();
                        self.stack.push(ContextState::Object { implicit: false });
                        return Ok(Some(ParseEvent::StructStart(ContainerKind::Object)));
                    }
                    TokenKind::LParen => {
                        self.next_token();
                        self.stack.push(ContextState::Sequence);
                        return Ok(Some(ParseEvent::SequenceStart(ContainerKind::Array)));
                    }
                    TokenKind::At => {
                        // Tag in sequence context
                        self.next_token();
                        let event = self.parse_tag(token.span.end);
                        return Ok(Some(event));
                    }
                    _ => {}
                }
            }
        }

        Ok(None)
    }

    fn peek_event(&mut self) -> Result<Option<ParseEvent<'de>>, Self::Error> {
        if self.peeked_events.is_empty() {
            // Record the lexer position before consuming any tokens
            self.peek_start_offset = Some(self.lexer.position() as usize);
            if let Some(event) = self.next_event()? {
                // Insert at front since next_event may have pushed follow-up events
                self.peeked_events.insert(0, event);
            }
        }
        Ok(self.peeked_events.first().cloned())
    }

    fn skip_value(&mut self) -> Result<(), Self::Error> {
        // Consume the next value, handling nested structures
        let mut depth = 0i32;
        loop {
            let event = self.next_event()?;
            trace!(?event, depth, "skip_value");
            match event {
                Some(ParseEvent::StructStart(_)) | Some(ParseEvent::SequenceStart(_)) => {
                    depth += 1;
                }
                Some(ParseEvent::StructEnd) | Some(ParseEvent::SequenceEnd) => {
                    if depth == 0 {
                        // Safety: unexpected End at depth 0 (malformed input or bug)
                        break;
                    }
                    depth -= 1;
                    if depth == 0 {
                        // Normal case: matched the opening container
                        break;
                    }
                }
                Some(ParseEvent::Scalar(_)) => {
                    if depth == 0 {
                        break;
                    }
                }
                Some(ParseEvent::VariantTag(_)) => {
                    // VariantTag followed by payload - continue to consume the payload
                }
                Some(ParseEvent::FieldKey(_)) | Some(ParseEvent::OrderedField) => {
                    // Continue
                }
                None => break,
            }
        }
        Ok(())
    }

    fn save(&mut self) -> SavePoint {
        // Clone the current parser state (without the saved_state field to avoid recursion)
        let mut clone = self.clone();
        clone.saved_state = None;
        self.saved_state = Some(Box::new(clone));
        SavePoint(0)
    }

    fn restore(&mut self, _save_point: SavePoint) {
        if let Some(saved) = self.saved_state.take() {
            *self = *saved;
        }
    }

    fn current_span(&self) -> Option<facet_reflect::Span> {
        self.current_span.map(|s| facet_reflect::Span {
            offset: s.start as usize,
            len: (s.end - s.start) as usize,
        })
    }

    fn raw_capture_shape(&self) -> Option<&'static facet_core::Shape> {
        Some(crate::RawStyx::SHAPE)
    }

    fn capture_raw(&mut self) -> Result<Option<&'de str>, Self::Error> {
        // Get the start offset - either from peek_event or current position
        let start_offset = self
            .peek_start_offset
            .take()
            .unwrap_or_else(|| self.lexer.position() as usize);

        // Skip the entire value (including nested structures)
        self.skip_value()?;

        let end_offset = self.lexer.position() as usize;

        // Extract the raw slice
        let raw_str = &self.input[start_offset..end_offset];

        // Trim surrounding whitespace/newlines
        let raw_str = raw_str.trim();

        Ok(Some(raw_str))
    }
}
