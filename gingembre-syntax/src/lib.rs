//! Lossless concrete syntax for gingembre templates.
//!
//! This crate defines the [`SyntaxKind`] tag set and cstree language for gingembre's
//! template grammar. It is the single source of truth for parsing, shared by the engine
//! (which lowers the CST to its AST) and the authoring LSP (which consumes the CST
//! directly for completion / hover / diagnostics with full error recovery).
//!
//! See `notes/gingembre-cstree-parser.md` for the design and the grammar catalogue.

mod lexer;
pub use lexer::{Lexeme, lex};

mod parser;
pub use parser::{Parse, ParseError, parse, parse_expr_str};

pub mod ast;

/// A node in the resolved (text-bearing) syntax tree.
pub type ResolvedNode = cstree::syntax::ResolvedNode<SyntaxKind>;
/// A token in the resolved syntax tree.
pub type ResolvedToken = cstree::syntax::ResolvedToken<SyntaxKind>;

/// The kind of every token and node in a gingembre syntax tree.
///
/// A single flat tag set: variants up to (and including) [`SyntaxKind::Error`] are
/// **tokens** (terminals produced by the lexer); the rest are **nodes** (interior).
/// Ordering matters only for the token/node split documented by [`SyntaxKind::is_token`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, cstree::Syntax)]
#[repr(u32)]
pub enum SyntaxKind {
    // ===== Tokens: trivia & raw text =====
    /// Whitespace inside code (`{{ }}` / `{% %}`).
    Whitespace,
    /// A run of literal template text (outside delimiters).
    Text,
    /// A whole `{# … #}` comment (lexed as one token; nesting handled by the lexer).
    Comment,

    // ===== Tokens: delimiters (trim variants carry the `-`) =====
    #[static_text("{{")]
    OpenExpr,
    #[static_text("{{-")]
    OpenExprTrim,
    #[static_text("}}")]
    CloseExpr,
    #[static_text("-}}")]
    CloseExprTrim,
    #[static_text("{%")]
    OpenStmt,
    #[static_text("{%-")]
    OpenStmtTrim,
    #[static_text("%}")]
    CloseStmt,
    #[static_text("-%}")]
    CloseStmtTrim,

    // ===== Tokens: literals & identifiers =====
    Ident,
    Int,
    Float,
    /// A quoted string literal (single or double quoted).
    Str,
    #[static_text("true")]
    True,
    #[static_text("false")]
    False,
    #[static_text("none")]
    NoneKw,

    // ===== Tokens: statement keywords =====
    #[static_text("if")]
    IfKw,
    #[static_text("elif")]
    ElifKw,
    #[static_text("else")]
    ElseKw,
    #[static_text("endif")]
    EndifKw,
    #[static_text("for")]
    ForKw,
    #[static_text("endfor")]
    EndforKw,
    #[static_text("set")]
    SetKw,
    #[static_text("endset")]
    EndsetKw,
    #[static_text("block")]
    BlockKw,
    #[static_text("endblock")]
    EndblockKw,
    #[static_text("extends")]
    ExtendsKw,
    #[static_text("include")]
    IncludeKw,
    #[static_text("import")]
    ImportKw,
    #[static_text("macro")]
    MacroKw,
    #[static_text("endmacro")]
    EndmacroKw,
    #[static_text("break")]
    BreakKw,
    #[static_text("continue")]
    ContinueKw,

    // ===== Tokens: operator keywords =====
    #[static_text("as")]
    AsKw,
    #[static_text("in")]
    InKw,
    #[static_text("is")]
    IsKw,
    #[static_text("not")]
    NotKw,
    #[static_text("and")]
    AndKw,
    #[static_text("or")]
    OrKw,

    // ===== Tokens: punctuation & operators =====
    #[static_text(".")]
    Dot,
    #[static_text(",")]
    Comma,
    #[static_text(":")]
    Colon,
    #[static_text("::")]
    ColonColon,
    #[static_text("|")]
    Pipe,
    #[static_text("?")]
    Question,
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
    #[static_text("*")]
    Star,
    #[static_text("**")]
    StarStar,
    #[static_text("/")]
    Slash,
    #[static_text("//")]
    SlashSlash,
    #[static_text("%")]
    Percent,
    #[static_text("~")]
    Tilde,
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

    /// Lexer/parser error token (unrecognised input). Last token kind.
    Error,

    // ===== Nodes: document structure =====
    /// Root of the tree.
    Template,
    /// A `{{ expr }}` interpolation.
    Interpolation,
    /// A `{% … %}` statement (specific kind given by the child statement node).
    Statement,

    // ===== Nodes: statements =====
    IfStmt,
    ElifClause,
    ElseClause,
    ForStmt,
    SetStmt,
    BlockStmt,
    ExtendsStmt,
    IncludeStmt,
    ImportStmt,
    MacroStmt,
    BreakStmt,
    ContinueStmt,
    /// Body of a block-bearing statement (if/for/block/macro/set-block).
    Body,
    /// Macro parameter list `(a, b="x")` and a single parameter.
    ParamList,
    Param,

    // ===== Nodes: expressions =====
    Literal,
    ListLit,
    DictLit,
    /// A bare identifier reference.
    VarRef,
    /// `base.field`
    FieldExpr,
    /// `base[index]`
    IndexExpr,
    /// `base[a:b]`
    SliceExpr,
    /// `func(args)`
    CallExpr,
    /// `namespace::macro(args)` — explicit macro call.
    MacroCallExpr,
    /// Argument list of a call, and its positional / keyword args.
    ArgList,
    Arg,
    KwArg,
    /// `expr | filter` / `expr | filter(args)`
    FilterExpr,
    /// `expr is [not] test`
    TestExpr,
    /// `a if cond else b`
    TernaryExpr,
    /// Binary operation.
    BinaryExpr,
    /// Unary operation (`not`, `-`).
    UnaryExpr,
    /// Postfix lenient access `expr?`.
    OptionalExpr,
    /// Parenthesised expression.
    ParenExpr,
}

// `SyntaxKind` itself is the cstree `Syntax` type (via the derive above), so the tree
// types are parameterised directly on it.

/// A node in the (red) syntax tree.
pub type SyntaxNode = cstree::syntax::SyntaxNode<SyntaxKind>;
/// A token in the syntax tree.
pub type SyntaxToken = cstree::syntax::SyntaxToken<SyntaxKind>;
/// Either a node or a token.
pub type SyntaxElement = cstree::syntax::SyntaxElement<SyntaxKind>;

impl SyntaxKind {
    /// Whether this kind is a token (terminal). Everything from [`SyntaxKind::Template`]
    /// onward is an interior node.
    pub fn is_token(self) -> bool {
        self <= SyntaxKind::Error
    }

    /// Whether this kind is trivia (whitespace or comment) — skipped by the typed AST.
    pub fn is_trivia(self) -> bool {
        matches!(self, SyntaxKind::Whitespace | SyntaxKind::Comment)
    }
}
