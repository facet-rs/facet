//! Lossless concrete syntax for Fable.
//!
//! Fable is the tiny typed language intended to inspect and mutate
//! Facet-reflected Rust values, then lower toward canonical Weavy IR. This crate
//! currently owns only the syntax layer: a lossless lexer/parser, the cstree
//! language tags, and a small typed AST facade for the first grammar slice.

mod lexer;
pub use lexer::{Lexeme, lex};

mod parser;
pub use parser::{Parse, ParseError, parse};

pub mod ast;

mod lowering;
pub use lowering::{
    FableError, FableField, FableFieldBoolUnary, FableFieldMut, FableFieldMutUnary,
    FableFieldStringUnary, FableFloatUnary, FableIntrinsics, FablePlan, FableSignedUnary,
    FableStringBinaryPredicate, FableStringUnary, FableUnsignedUnary, apply, apply_with_intrinsics,
};

/// A node in the resolved, text-bearing syntax tree.
pub type ResolvedNode = cstree::syntax::ResolvedNode<SyntaxKind>;
/// A token in the resolved, text-bearing syntax tree.
pub type ResolvedToken = cstree::syntax::ResolvedToken<SyntaxKind>;

/// The kind of every token and node in a Fable concrete syntax tree.
///
/// Variants up to and including [`SyntaxKind::Error`] are tokens. Later variants
/// are interior nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, cstree::Syntax)]
#[repr(u32)]
pub enum SyntaxKind {
    // Tokens: trivia.
    Whitespace,
    Comment,

    // Tokens: literals and identifiers.
    Ident,
    Int,
    Float,
    Str,
    #[static_text("true")]
    True,
    #[static_text("false")]
    False,
    #[static_text("null")]
    Null,

    // Tokens: keywords.
    #[static_text("if")]
    IfKw,
    #[static_text("else")]
    ElseKw,
    #[static_text("let")]
    LetKw,
    #[static_text("and")]
    AndKw,
    #[static_text("or")]
    OrKw,
    #[static_text("not")]
    NotKw,

    // Tokens: punctuation and operators.
    #[static_text(".")]
    Dot,
    #[static_text(",")]
    Comma,
    #[static_text(";")]
    Semicolon,
    #[static_text("(")]
    LParen,
    #[static_text(")")]
    RParen,
    #[static_text("[")]
    LBracket,
    #[static_text("]")]
    RBracket,
    #[static_text("{")]
    LBrace,
    #[static_text("}")]
    RBrace,
    #[static_text("=")]
    Assign,
    #[static_text("+")]
    Plus,
    #[static_text("-")]
    Minus,
    #[static_text("==")]
    EqEq,
    #[static_text("!=")]
    Neq,
    #[static_text("<")]
    Lt,
    #[static_text(">")]
    Gt,
    #[static_text("<=")]
    Le,
    #[static_text(">=")]
    Ge,

    /// Lexer/parser error token. This is the final token kind.
    Error,

    // Nodes: documents and statements.
    Root,
    Block,
    AssignStmt,
    LetStmt,
    ExprStmt,
    IfStmt,
    ElseClause,

    // Nodes: expressions.
    Literal,
    VarRef,
    FieldExpr,
    IndexExpr,
    CallExpr,
    ArgList,
    Arg,
    BinaryExpr,
    UnaryExpr,
    ParenExpr,
}

/// A red syntax-tree node.
pub type SyntaxNode = cstree::syntax::SyntaxNode<SyntaxKind>;
/// A red syntax-tree token.
pub type SyntaxToken = cstree::syntax::SyntaxToken<SyntaxKind>;
/// Either a red node or token.
pub type SyntaxElement = cstree::syntax::SyntaxElement<SyntaxKind>;

impl SyntaxKind {
    /// Whether this kind is a token emitted by the lexer.
    #[must_use]
    pub fn is_token(self) -> bool {
        self <= SyntaxKind::Error
    }

    /// Whether this kind is trivia skipped by parser decisions.
    #[must_use]
    pub fn is_trivia(self) -> bool {
        matches!(self, SyntaxKind::Whitespace | SyntaxKind::Comment)
    }
}
