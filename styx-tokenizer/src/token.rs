//! Token types for the Styx lexer.

use crate::Span;

/// The kind of a token.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenKind {
    // Structural tokens
    /// `{`
    LBrace,
    /// `}`
    RBrace,
    /// `(`
    LParen,
    /// `)`
    RParen,
    /// `,`
    Comma,
    /// `>`
    Gt,
    /// `@` (standalone unit)
    At,
    /// `@name` (tag with identifier)
    Tag,

    // Scalar tokens
    /// Bare (unquoted) scalar: `hello`, `42`, `true`
    BareScalar,
    /// Quoted scalar: `"hello world"`
    QuotedScalar,
    /// Raw scalar: `r#"..."#`
    RawScalar,
    /// Heredoc start marker: `<<DELIM` (includes the newline)
    HeredocStart,
    /// Heredoc content (the actual text)
    HeredocContent,
    /// Heredoc end marker: the closing delimiter
    HeredocEnd,

    // Comment tokens
    /// Line comment: `// ...`
    LineComment,
    /// Doc comment line: `/// ...`
    DocComment,

    // Whitespace tokens (significant for separator detection)
    /// Horizontal whitespace: spaces and tabs
    Whitespace,
    /// Newline: `\n` or `\r\n`
    Newline,

    // Special tokens
    /// End of file
    Eof,
    /// Lexer error (unrecognized input)
    Error,
}

impl TokenKind {
    /// Whether this token is trivia (whitespace or comments).
    pub fn is_trivia(&self) -> bool {
        matches!(
            self,
            TokenKind::Whitespace | TokenKind::Newline | TokenKind::LineComment
        )
    }

    /// Whether this token starts a scalar value.
    pub fn is_scalar_start(&self) -> bool {
        matches!(
            self,
            TokenKind::BareScalar
                | TokenKind::QuotedScalar
                | TokenKind::RawScalar
                | TokenKind::HeredocStart
        )
    }
}

/// A token with its kind, span, and source text slice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token<'src> {
    /// The kind of token.
    pub kind: TokenKind,
    /// The span in the source text.
    pub span: Span,
    /// The source text of this token.
    pub text: &'src str,
}

impl<'src> Token<'src> {
    /// Create a new token.
    pub fn new(kind: TokenKind, span: Span, text: &'src str) -> Self {
        Self { kind, span, text }
    }
}
