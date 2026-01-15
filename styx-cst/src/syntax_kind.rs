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
    /// A group of attributes (key=value pairs)
    ATTRIBUTES,
    /// A single attribute (key=value)
    ATTRIBUTE,
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
        Self::Kind::from_raw(raw.0).expect("invalid SyntaxKind value from rowan")
    }

    fn kind_to_raw(kind: Self::Kind) -> rowan::SyntaxKind {
        rowan::SyntaxKind(kind as u16)
    }
}

impl SyntaxKind {
    /// Convert from a raw u16 value to SyntaxKind.
    /// Returns None if the value is out of range or corresponds to __LAST_TOKEN.
    pub const fn from_raw(raw: u16) -> Option<Self> {
        match raw {
            0 => Some(Self::L_BRACE),
            1 => Some(Self::R_BRACE),
            2 => Some(Self::L_PAREN),
            3 => Some(Self::R_PAREN),
            4 => Some(Self::COMMA),
            5 => Some(Self::EQ),
            6 => Some(Self::AT),
            7 => Some(Self::BARE_SCALAR),
            8 => Some(Self::QUOTED_SCALAR),
            9 => Some(Self::RAW_SCALAR),
            10 => Some(Self::HEREDOC_START),
            11 => Some(Self::HEREDOC_CONTENT),
            12 => Some(Self::HEREDOC_END),
            13 => Some(Self::LINE_COMMENT),
            14 => Some(Self::DOC_COMMENT),
            15 => Some(Self::WHITESPACE),
            16 => Some(Self::NEWLINE),
            17 => Some(Self::EOF),
            18 => Some(Self::ERROR),
            // 19 is __LAST_TOKEN - skip it
            20 => Some(Self::DOCUMENT),
            21 => Some(Self::ENTRY),
            22 => Some(Self::OBJECT),
            23 => Some(Self::SEQUENCE),
            24 => Some(Self::SCALAR),
            25 => Some(Self::UNIT),
            26 => Some(Self::TAG),
            27 => Some(Self::TAG_NAME),
            28 => Some(Self::TAG_PAYLOAD),
            29 => Some(Self::KEY),
            30 => Some(Self::VALUE),
            31 => Some(Self::HEREDOC),
            32 => Some(Self::ATTRIBUTES),
            33 => Some(Self::ATTRIBUTE),
            _ => None,
        }
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
