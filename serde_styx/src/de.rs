//! Serde deserializer for Styx.

use serde::de::{self, Visitor};
use styx_parse::{Lexer, ScalarKind, Span, Token, TokenKind};

use crate::error::{Error, Result};

/// Styx deserializer implementing serde::Deserializer.
pub struct Deserializer<'de> {
    lexer: Lexer<'de>,
    /// Peeked token (if any).
    peeked_token: Option<Token<'de>>,
    /// Current span for error reporting.
    current_span: Option<Span>,
    /// Whether we've started the implicit root struct.
    root_started: bool,
}

impl<'de> Deserializer<'de> {
    /// Create a new deserializer for the given source.
    pub fn new(source: &'de str) -> Self {
        Self {
            lexer: Lexer::new(source),
            peeked_token: None,
            current_span: None,
            root_started: false,
        }
    }

    /// Peek at the next token without consuming it.
    fn peek_token(&mut self) -> Option<&Token<'de>> {
        if self.peeked_token.is_none() {
            loop {
                let token = self.lexer.next_token();
                match token.kind {
                    TokenKind::Whitespace | TokenKind::LineComment | TokenKind::Newline => continue,
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
                TokenKind::Whitespace | TokenKind::LineComment | TokenKind::Newline => continue,
                _ => {
                    self.current_span = Some(token.span);
                    return token;
                }
            }
        }
    }

    /// Skip commas between entries.
    fn skip_comma(&mut self) {
        if let Some(token) = self.peek_token()
            && token.kind == TokenKind::Comma
        {
            self.next_token();
        }
    }

    /// Parse a scalar value.
    fn parse_scalar(&self, text: &'de str, kind: ScalarKind) -> ScalarValue<'de> {
        match kind {
            ScalarKind::Bare => {
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
                    ScalarValue::Str(text)
                }
            }
            ScalarKind::Quoted => {
                let inner = self.unescape_quoted(text);
                ScalarValue::String(inner)
            }
            ScalarKind::Raw | ScalarKind::Heredoc => ScalarValue::Str(text),
        }
    }

    /// Unescape a quoted string.
    fn unescape_quoted(&self, text: &'de str) -> String {
        // Remove surrounding quotes
        let inner = if text.starts_with('"') && text.ends_with('"') && text.len() >= 2 {
            &text[1..text.len() - 1]
        } else {
            text
        };

        styx_format::unescape_quoted(inner).into_owned()
    }

    fn error(&self, msg: impl Into<String>) -> Error {
        Error::new(msg)
    }
}

/// Internal scalar value representation.
enum ScalarValue<'de> {
    Null,
    Bool(bool),
    I64(i64),
    U64(u64),
    F64(f64),
    Str(&'de str),
    String(String),
}

impl<'de> de::Deserializer<'de> for &mut Deserializer<'de> {
    type Error = Error;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let token = self.peek_token().cloned();
        match token {
            Some(token) => match token.kind {
                TokenKind::LBrace => self.deserialize_map(visitor),
                TokenKind::LParen => self.deserialize_seq(visitor),
                TokenKind::At => {
                    self.next_token();
                    // Check if it's a tag or unit
                    if let Some(next) = self.peek_token() {
                        if next.kind == TokenKind::BareScalar && next.span.start == token.span.end {
                            // It's a tag - treat as enum
                            let name_token = self.next_token();
                            visitor.visit_enum(EnumAccess {
                                de: self,
                                variant: name_token.text,
                            })
                        } else {
                            visitor.visit_unit()
                        }
                    } else {
                        visitor.visit_unit()
                    }
                }
                TokenKind::BareScalar => {
                    let token = self.next_token();
                    match self.parse_scalar(token.text, ScalarKind::Bare) {
                        ScalarValue::Null => visitor.visit_unit(),
                        ScalarValue::Bool(v) => visitor.visit_bool(v),
                        ScalarValue::I64(v) => visitor.visit_i64(v),
                        ScalarValue::U64(v) => visitor.visit_u64(v),
                        ScalarValue::F64(v) => visitor.visit_f64(v),
                        ScalarValue::Str(s) => visitor.visit_borrowed_str(s),
                        ScalarValue::String(s) => visitor.visit_string(s),
                    }
                }
                TokenKind::QuotedScalar => {
                    let token = self.next_token();
                    let s = self.unescape_quoted(token.text);
                    visitor.visit_string(s)
                }
                TokenKind::RawScalar => {
                    let token = self.next_token();
                    // Extract content from r#"..."#
                    let text = token.text;
                    let start = text.find('"').map(|i| i + 1).unwrap_or(0);
                    let end = text.rfind('"').unwrap_or(text.len());
                    visitor.visit_borrowed_str(&text[start..end])
                }
                TokenKind::HeredocStart => {
                    self.next_token();
                    let mut content = String::new();
                    loop {
                        let next = self.next_token();
                        match next.kind {
                            TokenKind::HeredocContent => content.push_str(next.text),
                            TokenKind::HeredocEnd => break,
                            _ => break,
                        }
                    }
                    visitor.visit_string(content)
                }
                TokenKind::Eof => {
                    // At EOF, try to deserialize as empty struct if at root
                    if !self.root_started {
                        self.root_started = true;
                        visitor.visit_map(MapAccess { de: self })
                    } else {
                        Err(self.error("unexpected end of input"))
                    }
                }
                _ => {
                    // At root level, start implicit struct
                    if !self.root_started {
                        self.root_started = true;
                        visitor.visit_map(MapAccess { de: self })
                    } else {
                        Err(self.error(format!("unexpected token: {:?}", token.kind)))
                    }
                }
            },
            None => Err(self.error("unexpected end of input")),
        }
    }

    fn deserialize_bool<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let token = self.next_token();
        match token.text {
            "true" => visitor.visit_bool(true),
            "false" => visitor.visit_bool(false),
            _ => Err(self.error(format!("expected bool, got {:?}", token.text))),
        }
    }

    fn deserialize_i8<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i16<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i32<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let token = self.next_token();
        let n: i64 = token
            .text
            .parse()
            .map_err(|_| self.error(format!("expected integer, got {:?}", token.text)))?;
        visitor.visit_i64(n)
    }

    fn deserialize_u8<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u16<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u32<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let token = self.next_token();
        let n: u64 = token
            .text
            .parse()
            .map_err(|_| self.error(format!("expected unsigned integer, got {:?}", token.text)))?;
        visitor.visit_u64(n)
    }

    fn deserialize_f32<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        self.deserialize_f64(visitor)
    }

    fn deserialize_f64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let token = self.next_token();
        let n: f64 = token
            .text
            .parse()
            .map_err(|_| self.error(format!("expected float, got {:?}", token.text)))?;
        visitor.visit_f64(n)
    }

    fn deserialize_char<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let token = self.next_token();
        let s = if token.kind == TokenKind::QuotedScalar {
            self.unescape_quoted(token.text)
        } else {
            token.text.to_string()
        };
        let c = s
            .chars()
            .next()
            .ok_or_else(|| self.error("expected char"))?;
        visitor.visit_char(c)
    }

    fn deserialize_str<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let token = self.next_token();
        match token.kind {
            TokenKind::QuotedScalar => {
                let s = self.unescape_quoted(token.text);
                visitor.visit_string(s)
            }
            TokenKind::RawScalar => {
                let text = token.text;
                let start = text.find('"').map(|i| i + 1).unwrap_or(0);
                let end = text.rfind('"').unwrap_or(text.len());
                visitor.visit_borrowed_str(&text[start..end])
            }
            _ => visitor.visit_borrowed_str(token.text),
        }
    }

    fn deserialize_string<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        self.deserialize_str(visitor)
    }

    fn deserialize_bytes<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        // Decode hex string
        let token = self.next_token();
        let text = if token.kind == TokenKind::QuotedScalar {
            &token.text[1..token.text.len() - 1]
        } else {
            token.text
        };

        let mut bytes = Vec::with_capacity(text.len() / 2);
        let mut chars = text.chars();
        while let (Some(a), Some(b)) = (chars.next(), chars.next()) {
            let byte = u8::from_str_radix(&format!("{}{}", a, b), 16)
                .map_err(|_| self.error("invalid hex in bytes"))?;
            bytes.push(byte);
        }
        visitor.visit_bytes(&bytes)
    }

    fn deserialize_byte_buf<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        self.deserialize_bytes(visitor)
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        if let Some(token) = self.peek_token()
            && token.kind == TokenKind::At
        {
            // Check if it's just @ (unit/none) or @tag
            let token = self.next_token();
            if let Some(next) = self.peek_token()
                && next.kind == TokenKind::BareScalar
                && next.span.start == token.span.end
            {
                // It's a tag - put back and deserialize as Some
                self.peeked_token = Some(Token {
                    kind: TokenKind::At,
                    text: "@",
                    span: token.span,
                });
                return visitor.visit_some(self);
            }
            return visitor.visit_none();
        }
        visitor.visit_some(self)
    }

    fn deserialize_unit<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let token = self.next_token();
        if token.kind == TokenKind::At {
            visitor.visit_unit()
        } else {
            Err(self.error("expected unit (@)"))
        }
    }

    fn deserialize_unit_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value> {
        self.deserialize_unit(visitor)
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value> {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_seq<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let token = self.next_token();
        if token.kind != TokenKind::LParen {
            return Err(self.error("expected sequence ("));
        }
        visitor.visit_seq(SeqAccess { de: self })
    }

    fn deserialize_tuple<V: Visitor<'de>>(self, _len: usize, visitor: V) -> Result<V::Value> {
        self.deserialize_seq(visitor)
    }

    fn deserialize_tuple_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value> {
        self.deserialize_seq(visitor)
    }

    fn deserialize_map<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        if let Some(token) = self.peek_token()
            && token.kind == TokenKind::LBrace
        {
            self.next_token();
        }
        visitor.visit_map(MapAccess { de: self })
    }

    fn deserialize_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value> {
        // Check if we're at root level (no braces) or explicit struct
        if let Some(token) = self.peek_token()
            && token.kind == TokenKind::LBrace
        {
            self.next_token();
            return visitor.visit_map(MapAccess { de: self });
        }

        // Implicit root struct
        if !self.root_started {
            self.root_started = true;
        }
        visitor.visit_map(MapAccess { de: self })
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value> {
        let token = self.next_token();
        if token.kind == TokenKind::At {
            // Tagged enum
            let name_token = self.next_token();
            if name_token.kind != TokenKind::BareScalar {
                return Err(self.error("expected variant name after @"));
            }
            visitor.visit_enum(EnumAccess {
                de: self,
                variant: name_token.text,
            })
        } else if token.kind == TokenKind::BareScalar {
            // Untagged string variant
            visitor.visit_enum(EnumAccess {
                de: self,
                variant: token.text,
            })
        } else {
            Err(self.error("expected enum variant"))
        }
    }

    fn deserialize_identifier<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let token = self.next_token();
        match token.kind {
            TokenKind::BareScalar => visitor.visit_borrowed_str(token.text),
            TokenKind::QuotedScalar => {
                let s = self.unescape_quoted(token.text);
                visitor.visit_string(s)
            }
            _ => Err(self.error("expected identifier")),
        }
    }

    fn deserialize_ignored_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        // Skip the next value
        self.skip_value()?;
        visitor.visit_unit()
    }
}

impl<'de> Deserializer<'de> {
    fn skip_value(&mut self) -> Result<()> {
        let token = self.next_token();
        match token.kind {
            TokenKind::LBrace => {
                let mut depth = 1;
                while depth > 0 {
                    let t = self.next_token();
                    match t.kind {
                        TokenKind::LBrace => depth += 1,
                        TokenKind::RBrace => depth -= 1,
                        TokenKind::Eof => break,
                        _ => {}
                    }
                }
            }
            TokenKind::LParen => {
                let mut depth = 1;
                while depth > 0 {
                    let t = self.next_token();
                    match t.kind {
                        TokenKind::LParen => depth += 1,
                        TokenKind::RParen => depth -= 1,
                        TokenKind::Eof => break,
                        _ => {}
                    }
                }
            }
            TokenKind::At => {
                // Could be unit or tag with value
                if let Some(next) = self.peek_token()
                    && next.kind == TokenKind::BareScalar
                {
                    self.next_token();
                    // Check for payload
                    if let Some(next) = self.peek_token()
                        && !matches!(
                            next.kind,
                            TokenKind::RBrace
                                | TokenKind::RParen
                                | TokenKind::Comma
                                | TokenKind::Eof
                        )
                    {
                        self.skip_value()?;
                    }
                }
            }
            _ => {
                // Scalar - already consumed
            }
        }
        Ok(())
    }
}

/// Sequence access for serde.
struct SeqAccess<'a, 'de> {
    de: &'a mut Deserializer<'de>,
}

impl<'a, 'de> de::SeqAccess<'de> for SeqAccess<'a, 'de> {
    type Error = Error;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>>
    where
        T: de::DeserializeSeed<'de>,
    {
        self.de.skip_comma();

        if let Some(token) = self.de.peek_token() {
            if token.kind == TokenKind::RParen {
                self.de.next_token();
                return Ok(None);
            }
            if token.kind == TokenKind::Eof {
                return Err(self.de.error("unexpected end of sequence"));
            }
        }

        seed.deserialize(&mut *self.de).map(Some)
    }
}

/// Map access for serde.
struct MapAccess<'a, 'de> {
    de: &'a mut Deserializer<'de>,
}

impl<'a, 'de> de::MapAccess<'de> for MapAccess<'a, 'de> {
    type Error = Error;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>>
    where
        K: de::DeserializeSeed<'de>,
    {
        self.de.skip_comma();

        let token_kind = self.de.peek_token().map(|t| t.kind);
        match token_kind {
            Some(TokenKind::RBrace) => {
                self.de.next_token();
                return Ok(None);
            }
            Some(TokenKind::Eof) | None => {
                return Ok(None);
            }
            Some(TokenKind::BareScalar) | Some(TokenKind::QuotedScalar) => {
                // This is a key
            }
            Some(TokenKind::DocComment) => {
                // Skip doc comments
                self.de.next_token();
                return self.next_key_seed(seed);
            }
            Some(kind) => {
                return Err(self.de.error(format!("expected key, got {:?}", kind)));
            }
        }

        seed.deserialize(&mut *self.de).map(Some)
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value>
    where
        V: de::DeserializeSeed<'de>,
    {
        seed.deserialize(&mut *self.de)
    }
}

/// Enum access for serde.
struct EnumAccess<'a, 'de> {
    de: &'a mut Deserializer<'de>,
    variant: &'de str,
}

impl<'a, 'de> de::EnumAccess<'de> for EnumAccess<'a, 'de> {
    type Error = Error;
    type Variant = VariantAccess<'a, 'de>;

    fn variant_seed<V>(self, seed: V) -> Result<(V::Value, Self::Variant)>
    where
        V: de::DeserializeSeed<'de>,
    {
        let variant = seed.deserialize(de::value::BorrowedStrDeserializer::new(self.variant))?;
        Ok((variant, VariantAccess { de: self.de }))
    }
}

/// Variant access for serde.
struct VariantAccess<'a, 'de> {
    de: &'a mut Deserializer<'de>,
}

impl<'a, 'de> de::VariantAccess<'de> for VariantAccess<'a, 'de> {
    type Error = Error;

    fn unit_variant(self) -> Result<()> {
        Ok(())
    }

    fn newtype_variant_seed<T>(self, seed: T) -> Result<T::Value>
    where
        T: de::DeserializeSeed<'de>,
    {
        seed.deserialize(self.de)
    }

    fn tuple_variant<V>(self, _len: usize, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_seq(self.de, visitor)
    }

    fn struct_variant<V>(self, _fields: &'static [&'static str], visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_map(self.de, visitor)
    }
}
