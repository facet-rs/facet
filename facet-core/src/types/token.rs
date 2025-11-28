//! Token types for extension attribute arguments.
//!
//! These types represent the raw tokens from `#[facet(ns::key(args))]` attributes
//! in a `Sync + Send + 'static` form that can be stored in static data.

/// Source location information for error reporting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    /// The source file path (from `file!()`)
    pub file: &'static str,
    /// Line number (1-indexed, from `line!()`)
    pub line: u32,
    /// Column number (1-indexed, from `column!()`)
    pub column: u32,
}

impl Span {
    /// Create a new span with the given location.
    pub const fn new(file: &'static str, line: u32, column: u32) -> Self {
        Self { file, line, column }
    }

    /// A dummy span for when location information is not available.
    pub const DUMMY: Span = Span {
        file: "<unknown>",
        line: 0,
        column: 0,
    };
}

impl core::fmt::Display for Span {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}:{}:{}", self.file, self.line, self.column)
    }
}

/// The delimiter for a group of tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delimiter {
    /// `( ... )`
    Parenthesis,
    /// `{ ... }`
    Brace,
    /// `[ ... ]`
    Bracket,
    /// No delimiter (e.g., `$( ... )` in macro_rules)
    None,
}

/// The kind of a literal token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiteralKind {
    /// A string literal: `"hello"`
    String,
    /// A byte string literal: `b"hello"`
    ByteString,
    /// A character literal: `'a'`
    Char,
    /// A byte character literal: `b'a'`
    Byte,
    /// An integer literal: `42`, `0x2A`, `0b101010`
    Integer,
    /// A floating-point literal: `3.14`, `1e10`
    Float,
}

/// A token from an extension attribute's arguments.
///
/// This is a `Sync + Send + 'static` representation of proc-macro tokens,
/// suitable for storage in static data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Token {
    /// An identifier: `foo`, `Bar`, `_baz`
    Ident {
        /// The identifier text
        name: &'static str,
        /// Source location
        span: Span,
    },
    /// A literal value: `"string"`, `42`, `3.14`, `'c'`
    Literal {
        /// What kind of literal this is
        kind: LiteralKind,
        /// The raw text of the literal (including quotes for strings)
        text: &'static str,
        /// Source location
        span: Span,
    },
    /// A punctuation character: `=`, `,`, `:`, etc.
    Punct {
        /// The punctuation character
        ch: char,
        /// Whether this is joined to the next punct (e.g., `::` or `->`)
        joint: bool,
        /// Source location
        span: Span,
    },
    /// A group of tokens with delimiters: `(...)`, `{...}`, `[...]`
    Group {
        /// The delimiter type
        delimiter: Delimiter,
        /// The tokens inside the group
        tokens: &'static [Token],
        /// Source location (of the opening delimiter)
        span: Span,
    },
}

impl Token {
    /// Get the span of this token.
    pub const fn span(&self) -> Span {
        match self {
            Token::Ident { span, .. } => *span,
            Token::Literal { span, .. } => *span,
            Token::Punct { span, .. } => *span,
            Token::Group { span, .. } => *span,
        }
    }

    /// Check if this token is an identifier with the given name.
    pub fn is_ident(&self, name: &str) -> bool {
        matches!(self, Token::Ident { name: n, .. } if *n == name)
    }

    /// Check if this token is a specific punctuation character.
    pub fn is_punct(&self, ch: char) -> bool {
        matches!(self, Token::Punct { ch: c, .. } if *c == ch)
    }

    /// Get the identifier name if this is an Ident token.
    pub const fn as_ident(&self) -> Option<&'static str> {
        match self {
            Token::Ident { name, .. } => Some(*name),
            _ => None,
        }
    }

    /// Get the literal text if this is a Literal token.
    pub const fn as_literal(&self) -> Option<(&'static str, LiteralKind)> {
        match self {
            Token::Literal { text, kind, .. } => Some((*text, *kind)),
            _ => None,
        }
    }
}

#[cfg(feature = "alloc")]
mod parsed_args {
    extern crate alloc;
    use alloc::collections::BTreeMap;
    use alloc::string::String;
    use alloc::vec::Vec;

    use super::{LiteralKind, Span, Token};

    /// A parsed value from extension attribute arguments.
    ///
    /// This is a simple value type that represents the parsed content of tokens
    /// from extension attributes like `#[facet(ns::key(arg1, name = "value"))]`.
    #[derive(Debug, Clone, PartialEq)]
    pub enum TokenValue {
        /// A string value (from string literals or identifiers)
        String(String),
        /// A static string reference (from identifiers)
        StaticStr(&'static str),
        /// A signed integer
        I64(i64),
        /// An unsigned integer
        U64(u64),
        /// A floating-point number
        F64(f64),
        /// A boolean
        Bool(bool),
        /// A character
        Char(char),
        /// A list of values (from `[...]`)
        List(Vec<TokenValue>),
        /// A map of values (from `{...}`)
        Map(BTreeMap<String, TokenValue>),
    }

    impl TokenValue {
        /// Create a string value from an owned String.
        pub fn string(s: String) -> Self {
            TokenValue::String(s)
        }

        /// Create a string value from a static str.
        pub fn str(s: &'static str) -> Self {
            TokenValue::StaticStr(s)
        }

        /// Create an i64 value.
        pub fn i64(n: i64) -> Self {
            TokenValue::I64(n)
        }

        /// Create a u64 value.
        pub fn u64(n: u64) -> Self {
            TokenValue::U64(n)
        }

        /// Create an f64 value.
        pub fn f64(n: f64) -> Self {
            TokenValue::F64(n)
        }

        /// Create a boolean value.
        pub fn bool(b: bool) -> Self {
            TokenValue::Bool(b)
        }

        /// Create a char value.
        pub fn char(c: char) -> Self {
            TokenValue::Char(c)
        }

        /// Create a list value.
        pub fn list(v: Vec<TokenValue>) -> Self {
            TokenValue::List(v)
        }

        /// Create a map value.
        pub fn map(m: BTreeMap<String, TokenValue>) -> Self {
            TokenValue::Map(m)
        }

        /// Try to get as a string slice.
        pub fn as_str(&self) -> Option<&str> {
            match self {
                TokenValue::String(s) => Some(s.as_str()),
                TokenValue::StaticStr(s) => Some(*s),
                _ => None,
            }
        }

        /// Try to get as an i64.
        pub fn as_i64(&self) -> Option<i64> {
            match self {
                TokenValue::I64(n) => Some(*n),
                TokenValue::U64(n) => (*n).try_into().ok(),
                _ => None,
            }
        }

        /// Try to get as a u64.
        pub fn as_u64(&self) -> Option<u64> {
            match self {
                TokenValue::U64(n) => Some(*n),
                TokenValue::I64(n) => (*n).try_into().ok(),
                _ => None,
            }
        }

        /// Try to get as an f64.
        pub fn as_f64(&self) -> Option<f64> {
            match self {
                TokenValue::F64(n) => Some(*n),
                TokenValue::I64(n) => Some(*n as f64),
                TokenValue::U64(n) => Some(*n as f64),
                _ => None,
            }
        }

        /// Try to get as a bool.
        pub fn as_bool(&self) -> Option<bool> {
            match self {
                TokenValue::Bool(b) => Some(*b),
                _ => None,
            }
        }

        /// Try to get as a char.
        pub fn as_char(&self) -> Option<char> {
            match self {
                TokenValue::Char(c) => Some(*c),
                _ => None,
            }
        }

        /// Try to get as a list.
        pub fn as_list(&self) -> Option<&[TokenValue]> {
            match self {
                TokenValue::List(v) => Some(v.as_slice()),
                _ => None,
            }
        }

        /// Try to get as a map.
        pub fn as_map(&self) -> Option<&BTreeMap<String, TokenValue>> {
            match self {
                TokenValue::Map(m) => Some(m),
                _ => None,
            }
        }
    }

    /// Parsed arguments from an extension attribute.
    ///
    /// This is the result of parsing `#[facet(ns::key(arg1, arg2, name = value))]`
    /// into a structured form. Positional arguments come first, followed by named arguments.
    #[derive(Debug, Clone)]
    pub struct ParsedArgs {
        /// Positional arguments (in order)
        pub positional: Vec<TokenValue>,
        /// Named arguments (key = value pairs)
        pub named: BTreeMap<String, TokenValue>,
    }

    impl ParsedArgs {
        /// Parse a token slice into structured arguments.
        ///
        /// Expected format: `arg1, arg2, ..., name1 = value1, name2 = value2, ...`
        /// Positional arguments must come before named arguments.
        pub fn parse(tokens: &'static [Token]) -> Result<Self, TokenParseError> {
            let mut positional = Vec::new();
            let mut named = BTreeMap::new();
            let mut seen_named = false;

            let mut iter = tokens.iter().peekable();

            while iter.peek().is_some() {
                // Check if this is a named argument (ident = value)
                let is_named = {
                    let mut lookahead = iter.clone();
                    matches!(
                        (lookahead.next(), lookahead.next()),
                        (
                            Some(Token::Ident { .. }),
                            Some(Token::Punct { ch: '=', .. })
                        )
                    )
                };

                if is_named {
                    seen_named = true;
                    // Parse: ident = value
                    let name = match iter.next() {
                        Some(Token::Ident { name, .. }) => *name,
                        _ => unreachable!(), // We checked above
                    };
                    // Skip the '='
                    iter.next();
                    // Parse the value
                    let value = parse_value(&mut iter)?;
                    named.insert(String::from(name), value);
                } else {
                    if seen_named {
                        let span = iter.peek().map(|t| t.span()).unwrap_or(Span::DUMMY);
                        return Err(TokenParseError::PositionalAfterNamed { span });
                    }
                    // Parse positional argument
                    let value = parse_value(&mut iter)?;
                    positional.push(value);
                }

                // Skip comma if present
                if let Some(Token::Punct { ch: ',', .. }) = iter.peek() {
                    iter.next();
                }
            }

            Ok(ParsedArgs { positional, named })
        }

        /// Get a positional argument by index.
        pub fn get_positional(&self, index: usize) -> Option<&TokenValue> {
            self.positional.get(index)
        }

        /// Get a named argument by key.
        pub fn get_named(&self, key: &str) -> Option<&TokenValue> {
            self.named.get(key)
        }
    }

    /// Error type for token argument parsing.
    #[derive(Debug, Clone)]
    pub enum TokenParseError {
        /// A positional argument appeared after a named argument
        PositionalAfterNamed {
            /// The span of the positional argument
            span: Span,
        },
        /// Unexpected token
        UnexpectedToken {
            /// The span of the unexpected token
            span: Span,
            /// A description of what was expected
            message: String,
        },
        /// Unexpected end of tokens
        UnexpectedEnd,
    }

    impl core::fmt::Display for TokenParseError {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            match self {
                TokenParseError::PositionalAfterNamed { span } => {
                    write!(f, "positional argument after named argument at {span}")
                }
                TokenParseError::UnexpectedToken { span, message } => {
                    write!(f, "unexpected token at {span}: {message}")
                }
                TokenParseError::UnexpectedEnd => {
                    write!(f, "unexpected end of tokens")
                }
            }
        }
    }

    #[cfg(feature = "std")]
    impl core::error::Error for TokenParseError {}

    /// Parse a single value from the token iterator.
    fn parse_value<'a, I>(iter: &mut core::iter::Peekable<I>) -> Result<TokenValue, TokenParseError>
    where
        I: Iterator<Item = &'a Token> + Clone,
    {
        match iter.next() {
            Some(Token::Literal { kind, text, span }) => parse_literal(*kind, text, *span),
            Some(Token::Ident { name, .. }) => {
                // Could be a boolean or just treat as string
                match *name {
                    "true" => Ok(TokenValue::bool(true)),
                    "false" => Ok(TokenValue::bool(false)),
                    _ => Ok(TokenValue::str(name)),
                }
            }
            Some(Token::Group {
                delimiter: super::Delimiter::Bracket,
                tokens,
                ..
            }) => {
                // Parse as array
                let args = ParsedArgs::parse(tokens)?;
                Ok(TokenValue::list(args.positional))
            }
            Some(Token::Group {
                delimiter: super::Delimiter::Brace,
                tokens,
                ..
            }) => {
                // Parse as map
                let args = ParsedArgs::parse(tokens)?;
                Ok(TokenValue::map(args.named))
            }
            Some(other) => Err(TokenParseError::UnexpectedToken {
                span: other.span(),
                message: alloc::format!("expected value, got {other:?}"),
            }),
            None => Err(TokenParseError::UnexpectedEnd),
        }
    }

    /// Parse a literal token into a TokenValue.
    fn parse_literal(
        kind: LiteralKind,
        text: &str,
        span: Span,
    ) -> Result<TokenValue, TokenParseError> {
        match kind {
            LiteralKind::String => {
                // Strip quotes and unescape
                let inner = text.trim_start_matches('"').trim_end_matches('"');
                // TODO: proper unescaping
                Ok(TokenValue::string(String::from(inner)))
            }
            LiteralKind::Integer => {
                // Try parsing as i64 first, then u64
                if let Ok(n) = text.parse::<i64>() {
                    Ok(TokenValue::i64(n))
                } else if let Ok(n) = text.parse::<u64>() {
                    Ok(TokenValue::u64(n))
                } else {
                    Err(TokenParseError::UnexpectedToken {
                        span,
                        message: alloc::format!("invalid integer: {text}"),
                    })
                }
            }
            LiteralKind::Float => {
                if let Ok(n) = text.parse::<f64>() {
                    Ok(TokenValue::f64(n))
                } else {
                    Err(TokenParseError::UnexpectedToken {
                        span,
                        message: alloc::format!("invalid float: {text}"),
                    })
                }
            }
            LiteralKind::Char => {
                let inner = text.trim_start_matches('\'').trim_end_matches('\'');
                let ch = inner.chars().next().unwrap_or('\0');
                Ok(TokenValue::char(ch))
            }
            LiteralKind::ByteString | LiteralKind::Byte => {
                // For now, treat as string
                let inner = text
                    .trim_start_matches("b\"")
                    .trim_start_matches("b'")
                    .trim_end_matches('"')
                    .trim_end_matches('\'');
                Ok(TokenValue::string(String::from(inner)))
            }
        }
    }
}

#[cfg(feature = "alloc")]
pub use parsed_args::{ParsedArgs, TokenParseError, TokenValue};
