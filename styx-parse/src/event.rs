//! Event types for the Styx event-based parser.

use std::borrow::Cow;

use crate::Span;

/// Events emitted by the parser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event<'src> {
    // Document boundaries
    /// Start of document.
    DocumentStart,
    /// End of document.
    DocumentEnd,

    // Objects
    /// Start of an object `{ ... }`.
    ObjectStart {
        /// Span of the opening brace.
        span: Span,
        /// Detected separator mode.
        separator: Separator,
    },
    /// End of an object.
    ObjectEnd {
        /// Span of the closing brace.
        span: Span,
    },

    // Sequences
    /// Start of a sequence `( ... )`.
    SequenceStart {
        /// Span of the opening paren.
        span: Span,
    },
    /// End of a sequence.
    SequenceEnd {
        /// Span of the closing paren.
        span: Span,
    },

    // Entry structure (within objects)
    /// Start of an entry (key-value pair).
    EntryStart,
    /// A key in an entry.
    ///
    /// Keys can be scalars or unit, optionally tagged.
    /// Objects, sequences, and heredocs are not allowed as keys.
    Key {
        /// Span of the key.
        span: Span,
        /// Tag name if this key is tagged (without @).
        tag: Option<&'src str>,
        /// Scalar payload after escape processing. None means unit.
        payload: Option<Cow<'src, str>>,
        /// Kind of scalar used for the key. Only meaningful if payload is Some.
        kind: ScalarKind,
    },
    /// End of an entry.
    EntryEnd,

    // Values
    /// A scalar value.
    Scalar {
        /// Span of the scalar.
        span: Span,
        /// Value after escape processing.
        value: Cow<'src, str>,
        /// Kind of scalar.
        kind: ScalarKind,
    },
    /// Unit value `@`.
    Unit {
        /// Span of the unit.
        span: Span,
    },

    // Tags
    /// Start of a tag `@name`.
    TagStart {
        /// Span of the tag (including @).
        span: Span,
        /// Tag name (without @).
        name: &'src str,
    },
    /// End of a tag.
    TagEnd,

    // Comments
    /// Line comment `// ...`.
    Comment {
        /// Span of the comment.
        span: Span,
        /// Comment text (including //).
        text: &'src str,
    },
    /// Doc comment `/// ...`.
    DocComment {
        /// Span of the doc comment.
        span: Span,
        /// Doc comment text (including ///).
        text: &'src str,
    },

    // Errors
    /// Parse error.
    Error {
        /// Span where error occurred.
        span: Span,
        /// Kind of error.
        kind: ParseErrorKind,
    },
}

/// Separator mode for object entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Separator {
    /// Entries separated by newlines.
    #[default]
    Newline,
    /// Entries separated by commas.
    Comma,
}

/// Kind of scalar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScalarKind {
    /// Bare (unquoted) scalar.
    Bare,
    /// Quoted string `"..."`.
    Quoted,
    /// Raw string `r#"..."#`.
    Raw,
    /// Heredoc `<<DELIM...DELIM`.
    Heredoc,
}

/// Parse error kinds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseErrorKind {
    /// Unexpected token.
    UnexpectedToken,
    /// Unclosed object (missing `}`).
    UnclosedObject,
    /// Unclosed sequence (missing `)`).
    UnclosedSequence,
    /// Mixed separators in object (some commas, some newlines).
    MixedSeparators,
    /// Invalid escape sequence in quoted string.
    InvalidEscape(String),
    /// Expected a key.
    ExpectedKey,
    /// Expected a value.
    ExpectedValue,
    /// Unexpected end of input.
    UnexpectedEof,
    /// Duplicate key in object.
    // parser[impl entry.key-equality]
    DuplicateKey,
    /// Invalid tag name (must match pattern).
    InvalidTagName,
    /// Invalid key (e.g., heredoc used as key).
    InvalidKey,
    /// Dangling doc comment (not followed by entry).
    DanglingDocComment,
}
