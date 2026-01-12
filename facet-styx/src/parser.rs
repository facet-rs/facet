//! Styx parser implementing the FormatParser trait.

use std::borrow::Cow;

use facet_format::{
    ContainerKind, FieldEvidence, FieldKey, FieldLocationHint, FormatParser, ParseEvent,
    ProbeStream, ScalarValue, ValueTypeHint,
};
use styx_parse::{Lexer, ScalarKind, Span, Token, TokenKind};

use crate::error::{StyxError, StyxErrorKind};

/// Streaming Styx parser implementing FormatParser.
pub struct StyxParser<'de> {
    lexer: Lexer<'de>,
    /// Stack of parsing contexts.
    stack: Vec<ContextState>,
    /// Peeked token (if any).
    peeked_token: Option<Token<'de>>,
    /// Peeked event (if any).
    peeked_event: Option<ParseEvent<'de>>,
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
}

#[derive(Debug, Clone)]
enum ContextState {
    /// Inside an object (braces or implicit root).
    Object { implicit: bool },
    /// Inside a sequence (parens).
    Sequence,
}

impl<'de> StyxParser<'de> {
    /// Create a new parser for the given source.
    pub fn new(source: &'de str) -> Self {
        Self {
            lexer: Lexer::new(source),
            stack: Vec::new(),
            peeked_token: None,
            peeked_event: None,
            root_started: false,
            complete: false,
            current_span: None,
            pending_key: None,
            expecting_value: false,
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
            if let Some(token) = self.peek_token() {
                if token.kind == TokenKind::Newline {
                    self.next_token();
                    found = true;
                    continue;
                }
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
                    // Treat as stringly-typed (like XML)
                    ScalarValue::StringlyTyped(Cow::Borrowed(text))
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
                    Some('0') => result.push('\0'),
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
                            if let Ok(code) = u32::from_str_radix(&hex, 16) {
                                if let Some(ch) = char::from_u32(code) {
                                    result.push(ch);
                                }
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
}

impl<'de> FormatParser<'de> for StyxParser<'de> {
    type Error = StyxError;
    type Probe<'a>
        = StyxProbe<'a, 'de>
    where
        Self: 'a;

    fn next_event(&mut self) -> Result<Option<ParseEvent<'de>>, Self::Error> {
        // Return peeked event if any
        if let Some(event) = self.peeked_event.take() {
            return Ok(Some(event));
        }

        if self.complete {
            return Ok(None);
        }

        // Skip newlines between entries
        self.skip_newlines();

        // Handle root struct start
        if !self.root_started {
            self.root_started = true;
            self.stack.push(ContextState::Object { implicit: true });
            return Ok(Some(ParseEvent::StructStart(ContainerKind::Object)));
        }

        // If we're expecting a value after a key
        if self.expecting_value {
            self.expecting_value = false;

            let token = self.peek_token().cloned();
            if let Some(token) = token {
                match token.kind {
                    TokenKind::Newline | TokenKind::Eof | TokenKind::RBrace | TokenKind::Comma => {
                        // No value - emit null (unit)
                        return Ok(Some(ParseEvent::Scalar(ScalarValue::Null)));
                    }
                    TokenKind::LBrace => {
                        // Nested object
                        self.next_token();
                        self.stack.push(ContextState::Object { implicit: false });
                        return Ok(Some(ParseEvent::StructStart(ContainerKind::Object)));
                    }
                    TokenKind::LParen => {
                        // Sequence
                        self.next_token();
                        self.stack.push(ContextState::Sequence);
                        return Ok(Some(ParseEvent::SequenceStart(ContainerKind::Array)));
                    }
                    TokenKind::At => {
                        // Could be unit @ or a tag
                        self.next_token();
                        // Check if followed by identifier
                        if let Some(next) = self.peek_token() {
                            if next.kind == TokenKind::BareScalar
                                && next.span.start == token.span.end
                            {
                                // Tag @name - emit as variant
                                let name_token = self.next_token();
                                return Ok(Some(ParseEvent::VariantTag(name_token.text)));
                            }
                        }
                        // Just @ - unit/null
                        return Ok(Some(ParseEvent::Scalar(ScalarValue::Null)));
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
                            return Ok(Some(ParseEvent::Scalar(ScalarValue::Str(Cow::Owned(
                                content,
                            )))));
                        } else {
                            token.text
                        };

                        let scalar = self.parse_scalar(text, kind);
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
                                return Ok(Some(ParseEvent::StructEnd));
                            }
                            ContextState::Sequence => {
                                return Ok(Some(ParseEvent::SequenceEnd));
                            }
                        }
                    }
                    return Ok(None);
                }
                TokenKind::RBrace => {
                    self.next_token();
                    if let Some(ContextState::Object { implicit: false }) = self.stack.pop() {
                        return Ok(Some(ParseEvent::StructEnd));
                    }
                    // Mismatched brace - error
                    return Err(self.error(StyxErrorKind::UnexpectedToken {
                        got: "}".to_string(),
                        expected: "key or value",
                    }));
                }
                TokenKind::RParen => {
                    self.next_token();
                    if let Some(ContextState::Sequence) = self.stack.pop() {
                        return Ok(Some(ParseEvent::SequenceEnd));
                    }
                    return Err(self.error(StyxErrorKind::UnexpectedToken {
                        got: ")".to_string(),
                        expected: "value",
                    }));
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
                    // Skip doc comments for now
                    self.next_token();
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

                        return Ok(Some(ParseEvent::FieldKey(FieldKey::new(
                            key,
                            FieldLocationHint::KeyValue,
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
                        self.next_token();
                        if let Some(next) = self.peek_token() {
                            if next.kind == TokenKind::BareScalar
                                && next.span.start == token.span.end
                            {
                                let name_token = self.next_token();
                                return Ok(Some(ParseEvent::VariantTag(name_token.text)));
                            }
                        }
                        return Ok(Some(ParseEvent::Scalar(ScalarValue::Null)));
                    }
                    _ => {}
                }
            }
        }

        Ok(None)
    }

    fn peek_event(&mut self) -> Result<Option<ParseEvent<'de>>, Self::Error> {
        if self.peeked_event.is_none() {
            self.peeked_event = self.next_event()?;
        }
        Ok(self.peeked_event.clone())
    }

    fn skip_value(&mut self) -> Result<(), Self::Error> {
        // Consume the next value, handling nested structures
        let mut depth = 0;
        loop {
            let event = self.next_event()?;
            match event {
                Some(ParseEvent::StructStart(_)) | Some(ParseEvent::SequenceStart(_)) => {
                    depth += 1;
                }
                Some(ParseEvent::StructEnd) | Some(ParseEvent::SequenceEnd) => {
                    if depth == 0 {
                        break;
                    }
                    depth -= 1;
                }
                Some(ParseEvent::Scalar(_)) | Some(ParseEvent::VariantTag(_)) => {
                    if depth == 0 {
                        break;
                    }
                }
                Some(ParseEvent::FieldKey(_)) | Some(ParseEvent::OrderedField) => {
                    // Continue
                }
                None => break,
            }
        }
        Ok(())
    }

    fn begin_probe(&mut self) -> Result<Self::Probe<'_>, Self::Error> {
        Ok(StyxProbe { parser: self })
    }

    fn current_span(&self) -> Option<facet_reflect::Span> {
        self.current_span.map(|s| facet_reflect::Span {
            offset: s.start as usize,
            len: (s.end - s.start) as usize,
        })
    }
}

/// Probe for untagged enum resolution.
pub struct StyxProbe<'a, 'de> {
    parser: &'a mut StyxParser<'de>,
}

impl<'a, 'de> ProbeStream<'de> for StyxProbe<'a, 'de> {
    type Error = StyxError;

    fn next(&mut self) -> Result<Option<FieldEvidence<'de>>, Self::Error> {
        // Peek at next event to gather evidence
        let event = self.parser.peek_event()?;
        match event {
            Some(ParseEvent::FieldKey(key)) => Ok(Some(FieldEvidence::new(
                key.name,
                FieldLocationHint::KeyValue,
                Some(ValueTypeHint::Map),
                None,
            ))),
            Some(ParseEvent::Scalar(ScalarValue::Bool(_))) => Ok(Some(FieldEvidence::new(
                "",
                FieldLocationHint::KeyValue,
                Some(ValueTypeHint::Bool),
                None,
            ))),
            Some(ParseEvent::Scalar(
                ScalarValue::I64(_) | ScalarValue::U64(_) | ScalarValue::F64(_),
            )) => Ok(Some(FieldEvidence::new(
                "",
                FieldLocationHint::KeyValue,
                Some(ValueTypeHint::Number),
                None,
            ))),
            Some(ParseEvent::Scalar(ScalarValue::Str(_) | ScalarValue::StringlyTyped(_))) => {
                Ok(Some(FieldEvidence::new(
                    "",
                    FieldLocationHint::KeyValue,
                    Some(ValueTypeHint::String),
                    None,
                )))
            }
            Some(ParseEvent::Scalar(ScalarValue::Null)) => Ok(Some(FieldEvidence::new(
                "",
                FieldLocationHint::KeyValue,
                Some(ValueTypeHint::Null),
                None,
            ))),
            Some(ParseEvent::SequenceStart(_)) => Ok(Some(FieldEvidence::new(
                "",
                FieldLocationHint::KeyValue,
                Some(ValueTypeHint::Sequence),
                None,
            ))),
            Some(ParseEvent::StructStart(_)) => Ok(Some(FieldEvidence::new(
                "",
                FieldLocationHint::KeyValue,
                Some(ValueTypeHint::Map),
                None,
            ))),
            _ => Ok(None),
        }
    }
}
