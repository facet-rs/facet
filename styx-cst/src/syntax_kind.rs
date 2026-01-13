//! Syntax node and token kinds for the Styx CST.

use styx_parse::TokenKind;

/// The kind of a syntax element (node or token).
///
/// Tokens are terminal elements (leaves), while nodes are non-terminal
/// (contain children). The distinction is made by value: tokens have
/// lower values than `__LAST_TOKEN`.
///
/// The SCREAMING_CASE naming convention is used to match rowan/rust-analyzer
/// conventions for syntax kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u16)]
#[allow(non_camel_case_types)]
#[allow(clippy::manual_non_exhaustive)] // __LAST_TOKEN is used for token/node distinction
pub enum SyntaxKind {
    // ========== TOKENS (terminals) ==========
    // Structural tokens
    /// `{`
    L_BRACE = 0,
    /// `}`
    R_BRACE,
    /// `(`
    L_PAREN,
    /// `)`
    R_PAREN,
    /// `,`
    COMMA,
    /// `=`
    EQ,
    /// `@`
    AT,

    // Scalar tokens
    /// Bare (unquoted) scalar: `hello`, `42`, `true`
    BARE_SCALAR,
    /// Quoted scalar: `"hello world"`
    QUOTED_SCALAR,
    /// Raw scalar: `r#"..."#`
    RAW_SCALAR,
    /// Heredoc start marker: `<<DELIM\n`
    HEREDOC_START,
    /// Heredoc content
    HEREDOC_CONTENT,
    /// Heredoc end marker
    HEREDOC_END,

    // Comment tokens
    /// Line comment: `// ...`
    LINE_COMMENT,
    /// Doc comment: `/// ...`
    DOC_COMMENT,

    // Whitespace tokens
    /// Horizontal whitespace (spaces, tabs)
    WHITESPACE,
    /// Newline (`\n` or `\r\n`)
    NEWLINE,

    // Special tokens
    /// End of file
    EOF,
    /// Lexer/parser error
    ERROR,

    // Marker for end of tokens
    #[doc(hidden)]
    __LAST_TOKEN,

    // ========== NODES (non-terminals) ==========
    /// Root document node
    DOCUMENT,
    /// An entry (key-value pair or sequence element)
    ENTRY,
    /// An explicit object `{ ... }`
    OBJECT,
    /// A sequence `( ... )`
    SEQUENCE,
    /// A scalar value wrapper
    SCALAR,
    /// Unit value `@`
    UNIT,
    /// A tag `@name` with optional payload
    TAG,
    /// Tag name (without @)
    TAG_NAME,
    /// Tag payload (the value after the tag name)
    TAG_PAYLOAD,
    /// Key in an entry
    KEY,
    /// Value in an entry
    VALUE,
    /// A heredoc (groups start, content, end)
    HEREDOC,
}

impl SyntaxKind {
    /// Whether this is a token (terminal) kind.
    pub fn is_token(self) -> bool {
        (self as u16) < (Self::__LAST_TOKEN as u16)
    }

    /// Whether this is a node (non-terminal) kind.
    pub fn is_node(self) -> bool {
        (self as u16) > (Self::__LAST_TOKEN as u16)
    }

    /// Whether this is trivia (whitespace or comments).
    pub fn is_trivia(self) -> bool {
        matches!(self, Self::WHITESPACE | Self::NEWLINE | Self::LINE_COMMENT)
    }
}

impl From<TokenKind> for SyntaxKind {
    fn from(kind: TokenKind) -> Self {
        match kind {
            TokenKind::LBrace => Self::L_BRACE,
            TokenKind::RBrace => Self::R_BRACE,
            TokenKind::LParen => Self::L_PAREN,
            TokenKind::RParen => Self::R_PAREN,
            TokenKind::Comma => Self::COMMA,
            TokenKind::Eq => Self::EQ,
            TokenKind::At => Self::AT,
            TokenKind::BareScalar => Self::BARE_SCALAR,
            TokenKind::QuotedScalar => Self::QUOTED_SCALAR,
            TokenKind::RawScalar => Self::RAW_SCALAR,
            TokenKind::HeredocStart => Self::HEREDOC_START,
            TokenKind::HeredocContent => Self::HEREDOC_CONTENT,
            TokenKind::HeredocEnd => Self::HEREDOC_END,
            TokenKind::LineComment => Self::LINE_COMMENT,
            TokenKind::DocComment => Self::DOC_COMMENT,
            TokenKind::Whitespace => Self::WHITESPACE,
            TokenKind::Newline => Self::NEWLINE,
            TokenKind::Eof => Self::EOF,
            TokenKind::Error => Self::ERROR,
        }
    }
}

impl From<SyntaxKind> for rowan::SyntaxKind {
    fn from(kind: SyntaxKind) -> Self {
        rowan::SyntaxKind(kind as u16)
    }
}

/// Language definition for Styx, used by rowan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StyxLanguage {}

impl rowan::Language for StyxLanguage {
    type Kind = SyntaxKind;

    fn kind_from_raw(raw: rowan::SyntaxKind) -> Self::Kind {
        assert!(raw.0 <= SyntaxKind::HEREDOC as u16);
        // SAFETY: We've checked the value is in range
        unsafe { std::mem::transmute(raw.0) }
    }

    fn kind_to_raw(kind: Self::Kind) -> rowan::SyntaxKind {
        rowan::SyntaxKind(kind as u16)
    }
}

/// A syntax node in the Styx CST.
pub type SyntaxNode = rowan::SyntaxNode<StyxLanguage>;

/// A syntax token in the Styx CST.
pub type SyntaxToken = rowan::SyntaxToken<StyxLanguage>;

/// A syntax element (either node or token) in the Styx CST.
pub type SyntaxElement = rowan::SyntaxElement<StyxLanguage>;

#[cfg(test)]
mod tests {
    use super::*;
    use rowan::Language;

    #[test]
    fn token_vs_node() {
        assert!(SyntaxKind::L_BRACE.is_token());
        assert!(SyntaxKind::WHITESPACE.is_token());
        assert!(SyntaxKind::ERROR.is_token());

        assert!(SyntaxKind::DOCUMENT.is_node());
        assert!(SyntaxKind::ENTRY.is_node());
        assert!(SyntaxKind::OBJECT.is_node());
    }

    #[test]
    fn trivia() {
        assert!(SyntaxKind::WHITESPACE.is_trivia());
        assert!(SyntaxKind::NEWLINE.is_trivia());
        assert!(SyntaxKind::LINE_COMMENT.is_trivia());

        assert!(!SyntaxKind::DOC_COMMENT.is_trivia());
        assert!(!SyntaxKind::BARE_SCALAR.is_trivia());
    }

    #[test]
    fn token_kind_conversion() {
        assert_eq!(SyntaxKind::from(TokenKind::LBrace), SyntaxKind::L_BRACE);
        assert_eq!(
            SyntaxKind::from(TokenKind::BareScalar),
            SyntaxKind::BARE_SCALAR
        );
        assert_eq!(SyntaxKind::from(TokenKind::Newline), SyntaxKind::NEWLINE);
    }

    #[test]
    fn rowan_roundtrip() {
        let kind = SyntaxKind::DOCUMENT;
        let raw = StyxLanguage::kind_to_raw(kind);
        let back = StyxLanguage::kind_from_raw(raw);
        assert_eq!(kind, back);
    }
}
