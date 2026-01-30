#![doc = include_str!("../README.md")]

// Re-export rowan types for convenience
pub use rowan::{NodeOrToken, TextRange, TextSize, TokenAtOffset};

mod syntax_kind;
pub use syntax_kind::{StyxLanguage, SyntaxElement, SyntaxKind, SyntaxNode, SyntaxToken};

mod ast;
pub use ast::{
    AstNode, Document, Entry, Heredoc, Key, Object, Scalar, ScalarKind, Separator, Sequence, Tag,
    Unit, Value, ValueKind,
};

mod parser;
pub use parser::{Parse, ParseError, parse};

mod validation;
pub use validation::{Diagnostic, Severity, validate, validate_document};
