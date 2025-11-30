//! AST nodes for the template language
//!
//! Every node carries a [`Span`] for precise error reporting.
//! The AST is designed to be parsed once and evaluated many times.

use miette::SourceSpan;

/// A span in the source (re-export from miette)
pub type Span = SourceSpan;

/// Create a span from offset and length
pub fn span(offset: usize, len: usize) -> Span {
    SourceSpan::new(offset.into(), len)
}

/// A complete parsed template
#[derive(Debug, Clone)]
pub struct Template {
    /// The template body (sequence of nodes)
    pub body: Vec<Node>,
    /// Full span of the template
    pub span: Span,
}

/// A node in the template AST
#[derive(Debug, Clone)]
pub enum Node {
    /// Raw text (passed through unchanged)
    Text(TextNode),
    /// Expression interpolation: {{ expr }}
    Print(PrintNode),
    /// If statement: {% if cond %}...{% endif %}
    If(IfNode),
    /// For loop: {% for item in items %}...{% endfor %}
    For(ForNode),
    /// Include: {% include "path" %}
    Include(IncludeNode),
    /// Block definition: {% block name %}...{% endblock %}
    Block(BlockNode),
    /// Extends: {% extends "path" %}
    Extends(ExtendsNode),
    /// Comment: {# comment #}
    Comment(CommentNode),
    /// Set statement: {% set var = expr %}
    Set(SetNode),
    /// Import: {% import "path" as name %}
    Import(ImportNode),
    /// Macro definition: {% macro name(args) %}...{% endmacro %}
    Macro(MacroNode),
}

impl Node {
    pub fn span(&self) -> Span {
        match self {
            Node::Text(n) => n.span,
            Node::Print(n) => n.span,
            Node::If(n) => n.span,
            Node::For(n) => n.span,
            Node::Include(n) => n.span,
            Node::Block(n) => n.span,
            Node::Extends(n) => n.span,
            Node::Comment(n) => n.span,
            Node::Set(n) => n.span,
            Node::Import(n) => n.span,
            Node::Macro(n) => n.span,
        }
    }
}

/// Raw text node
#[derive(Debug, Clone)]
pub struct TextNode {
    pub text: String,
    pub span: Span,
}

/// Expression print: {{ expr }}
#[derive(Debug, Clone)]
pub struct PrintNode {
    pub expr: Expr,
    pub span: Span,
}

/// If statement
#[derive(Debug, Clone)]
pub struct IfNode {
    /// The condition expression
    pub condition: Expr,
    /// Body if condition is true
    pub then_body: Vec<Node>,
    /// Optional elif branches
    pub elif_branches: Vec<ElifBranch>,
    /// Optional else body
    pub else_body: Option<Vec<Node>>,
    /// Full span from {% if %} to {% endif %}
    pub span: Span,
}

/// An elif branch
#[derive(Debug, Clone)]
pub struct ElifBranch {
    pub condition: Expr,
    pub body: Vec<Node>,
    pub span: Span,
}

/// For loop
#[derive(Debug, Clone)]
pub struct ForNode {
    /// Loop variable name (or names for tuple unpacking)
    pub target: Target,
    /// Expression to iterate over
    pub iter: Expr,
    /// Loop body
    pub body: Vec<Node>,
    /// Optional else body (if iter is empty)
    pub else_body: Option<Vec<Node>>,
    /// Full span from {% for %} to {% endfor %}
    pub span: Span,
}

/// Loop target (variable binding)
#[derive(Debug, Clone)]
pub enum Target {
    /// Single variable: `for item in items`
    Single { name: String, span: Span },
    /// Tuple unpacking: `for key, value in items`
    Tuple {
        names: Vec<(String, Span)>,
        span: Span,
    },
}

impl Target {
    pub fn span(&self) -> Span {
        match self {
            Target::Single { span, .. } => *span,
            Target::Tuple { span, .. } => *span,
        }
    }
}

/// Include another template
#[derive(Debug, Clone)]
pub struct IncludeNode {
    pub path: StringLit,
    /// Optional context override
    pub context: Option<Expr>,
    pub span: Span,
}

/// Block definition (for inheritance)
#[derive(Debug, Clone)]
pub struct BlockNode {
    pub name: Ident,
    pub body: Vec<Node>,
    pub span: Span,
}

/// Extends a parent template
#[derive(Debug, Clone)]
pub struct ExtendsNode {
    pub path: StringLit,
    pub span: Span,
}

/// Comment node (for AST completeness, usually ignored)
#[derive(Debug, Clone)]
pub struct CommentNode {
    pub text: String,
    pub span: Span,
}

/// Set statement: {% set var = expr %}
#[derive(Debug, Clone)]
pub struct SetNode {
    pub name: Ident,
    pub value: Expr,
    pub span: Span,
}

/// Import statement: {% import "path" as name %}
#[derive(Debug, Clone)]
pub struct ImportNode {
    pub path: StringLit,
    pub alias: Ident,
    pub span: Span,
}

/// Macro definition: {% macro name(args) %}...{% endmacro %}
#[derive(Debug, Clone)]
pub struct MacroNode {
    pub name: Ident,
    pub params: Vec<MacroParam>,
    pub body: Vec<Node>,
    pub span: Span,
}

/// A macro parameter with optional default value
#[derive(Debug, Clone)]
pub struct MacroParam {
    pub name: Ident,
    pub default: Option<Expr>,
}

// ============================================================================
// Expressions
// ============================================================================

/// An expression
#[derive(Debug, Clone)]
pub enum Expr {
    /// Literal value
    Literal(Literal),
    /// Variable reference
    Var(Ident),
    /// Field access: expr.field
    Field(FieldExpr),
    /// Index access: `expr[index]`
    Index(IndexExpr),
    /// Filter application: expr | filter
    Filter(FilterExpr),
    /// Binary operation: expr op expr
    Binary(BinaryExpr),
    /// Unary operation: op expr
    Unary(UnaryExpr),
    /// Function/method call: func(args)
    Call(CallExpr),
    /// Ternary: expr if cond else expr
    Ternary(TernaryExpr),
    /// Test expression: expr is test_name or expr is not test_name
    Test(TestExpr),
    /// Macro call: namespace::macro(args) or self::macro(args)
    MacroCall(MacroCallExpr),
}

impl Expr {
    pub fn span(&self) -> Span {
        match self {
            Expr::Literal(l) => l.span(),
            Expr::Var(i) => i.span,
            Expr::Field(f) => f.span,
            Expr::Index(i) => i.span,
            Expr::Filter(f) => f.span,
            Expr::Binary(b) => b.span,
            Expr::Unary(u) => u.span,
            Expr::Call(c) => c.span,
            Expr::Ternary(t) => t.span,
            Expr::Test(t) => t.span,
            Expr::MacroCall(m) => m.span,
        }
    }
}

/// A literal value
#[derive(Debug, Clone)]
pub enum Literal {
    String(StringLit),
    Int(IntLit),
    Float(FloatLit),
    Bool(BoolLit),
    None(NoneLit),
    List(ListLit),
    Dict(DictLit),
}

impl Literal {
    pub fn span(&self) -> Span {
        match self {
            Literal::String(l) => l.span,
            Literal::Int(l) => l.span,
            Literal::Float(l) => l.span,
            Literal::Bool(l) => l.span,
            Literal::None(l) => l.span,
            Literal::List(l) => l.span,
            Literal::Dict(l) => l.span,
        }
    }
}

/// String literal
#[derive(Debug, Clone)]
pub struct StringLit {
    pub value: String,
    pub span: Span,
}

/// Integer literal
#[derive(Debug, Clone)]
pub struct IntLit {
    pub value: i64,
    pub span: Span,
}

/// Float literal
#[derive(Debug, Clone)]
pub struct FloatLit {
    pub value: f64,
    pub span: Span,
}

/// Boolean literal
#[derive(Debug, Clone)]
pub struct BoolLit {
    pub value: bool,
    pub span: Span,
}

/// None literal
#[derive(Debug, Clone)]
pub struct NoneLit {
    pub span: Span,
}

/// List literal: [a, b, c]
#[derive(Debug, Clone)]
pub struct ListLit {
    pub elements: Vec<Expr>,
    pub span: Span,
}

/// Dict literal: {a: b, c: d}
#[derive(Debug, Clone)]
pub struct DictLit {
    pub entries: Vec<(Expr, Expr)>,
    pub span: Span,
}

/// An identifier
#[derive(Debug, Clone)]
pub struct Ident {
    pub name: String,
    pub span: Span,
}

/// Field access: expr.field
#[derive(Debug, Clone)]
pub struct FieldExpr {
    pub base: Box<Expr>,
    pub field: Ident,
    pub span: Span,
}

/// Index access: `expr[index]`
#[derive(Debug, Clone)]
pub struct IndexExpr {
    pub base: Box<Expr>,
    pub index: Box<Expr>,
    pub span: Span,
}

/// Filter application: expr | filter or expr | filter(args, key=value)
#[derive(Debug, Clone)]
pub struct FilterExpr {
    pub expr: Box<Expr>,
    pub filter: Ident,
    pub args: Vec<Expr>,
    pub kwargs: Vec<(Ident, Expr)>,
    pub span: Span,
}

/// Binary expression
#[derive(Debug, Clone)]
pub struct BinaryExpr {
    pub left: Box<Expr>,
    pub op: BinaryOp,
    pub right: Box<Expr>,
    pub span: Span,
}

/// Binary operators
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    // Arithmetic
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    FloorDiv,
    Pow,
    // Comparison
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    // Logical
    And,
    Or,
    // Membership
    In,
    NotIn,
    // String
    Concat,
}

/// Unary expression
#[derive(Debug, Clone)]
pub struct UnaryExpr {
    pub op: UnaryOp,
    pub expr: Box<Expr>,
    pub span: Span,
}

/// Unary operators
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Not,
    Neg,
    Pos,
}

/// Function/method call
#[derive(Debug, Clone)]
pub struct CallExpr {
    pub func: Box<Expr>,
    pub args: Vec<Expr>,
    pub kwargs: Vec<(Ident, Expr)>,
    pub span: Span,
}

/// Ternary expression: value if cond else other
#[derive(Debug, Clone)]
pub struct TernaryExpr {
    pub value: Box<Expr>,
    pub condition: Box<Expr>,
    pub otherwise: Box<Expr>,
    pub span: Span,
}

/// Test expression: expr is test_name or expr is not test_name(args)
#[derive(Debug, Clone)]
pub struct TestExpr {
    pub expr: Box<Expr>,
    pub test_name: Ident,
    pub args: Vec<Expr>,
    pub negated: bool,
    pub span: Span,
}

/// Macro call expression: namespace::macro_name(args) or self::macro_name(args)
#[derive(Debug, Clone)]
pub struct MacroCallExpr {
    /// The namespace (e.g., "macros" or "self")
    pub namespace: Ident,
    /// The macro name
    pub macro_name: Ident,
    /// Positional arguments
    pub args: Vec<Expr>,
    /// Keyword arguments
    pub kwargs: Vec<(Ident, Expr)>,
    pub span: Span,
}
