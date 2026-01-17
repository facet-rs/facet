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
    /// Duplicate key in object. Contains the span of the first occurrence.
    // parser[impl entry.key-equality]
    DuplicateKey { original: Span },
    /// Invalid tag name (must match pattern).
    InvalidTagName,
    /// Invalid key (e.g., heredoc used as key).
    InvalidKey,
    /// Dangling doc comment (not followed by entry).
    DanglingDocComment,
    /// Too many atoms in entry (expected at most 2: key and value).
    // parser[impl entry.toomany]
    TooManyAtoms,
    /// Attempted to reopen a path that was closed when a sibling appeared.
    // parser[impl entry.path.reopen]
    ReopenedPath {
        /// The closed path that was attempted to be reopened.
        closed_path: Vec<String>,
    },
    /// Attempted to nest into a path that has a terminal value (scalar/sequence/tag/unit).
    NestIntoTerminal {
        /// The path that has a terminal value.
        terminal_path: Vec<String>,
    },
}

impl std::fmt::Display for ParseErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseErrorKind::UnexpectedToken => write!(f, "unexpected token"),
            ParseErrorKind::UnclosedObject => write!(f, "unclosed object (missing `}}`)"),
            ParseErrorKind::UnclosedSequence => write!(f, "unclosed sequence (missing `)`)"),
            ParseErrorKind::MixedSeparators => {
                write!(f, "mixed separators (use either commas or newlines)")
            }
            ParseErrorKind::InvalidEscape(seq) => write!(f, "invalid escape sequence: {}", seq),
            ParseErrorKind::ExpectedKey => write!(f, "expected a key"),
            ParseErrorKind::ExpectedValue => write!(f, "expected a value"),
            ParseErrorKind::UnexpectedEof => write!(f, "unexpected end of input"),
            ParseErrorKind::DuplicateKey { .. } => write!(f, "duplicate key"),
            ParseErrorKind::InvalidTagName => write!(f, "invalid tag name"),
            ParseErrorKind::InvalidKey => write!(f, "invalid key"),
            ParseErrorKind::DanglingDocComment => {
                write!(f, "doc comment not followed by an entry")
            }
            ParseErrorKind::TooManyAtoms => {
                write!(f, "unexpected atom after value (entry has too many atoms)")
            }
            ParseErrorKind::ReopenedPath { closed_path } => {
                write!(
                    f,
                    "cannot reopen path `{}` after sibling appeared",
                    closed_path.join(".")
                )
            }
            ParseErrorKind::NestIntoTerminal { terminal_path } => {
                write!(
                    f,
                    "cannot nest into `{}` which has a terminal value",
                    terminal_path.join(".")
                )
            }
        }
    }
}
