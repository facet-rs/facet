//! Typed views over the lossless CST.
//!
//! These are thin, zero-copy wrappers over [`crate::ResolvedNode`] (à la rust-analyzer's
//! `ast` layer): each wrapper is just a tagged node, and its accessor methods navigate the
//! CST. There is **no separate owned AST** — the engine and the LSP both evaluate directly
//! off these typed views, so there is a single source of truth. Leaf values (int/float/bool
//! literals, string-escape resolution, operator → enum) are decoded in the accessors.

use crate::ResolvedNode;
use crate::SyntaxKind::{self, *};

/// A typed view over a CST node of a known [`SyntaxKind`].
pub trait AstNode: Sized {
    fn can_cast(kind: SyntaxKind) -> bool;
    fn cast(node: ResolvedNode) -> Option<Self>;
    fn syntax(&self) -> &ResolvedNode;
}

macro_rules! ast_node {
    ($(#[$m:meta])* $name:ident = $kind:ident) => {
        $(#[$m])*
        #[derive(Debug, Clone)]
        pub struct $name(ResolvedNode);
        impl AstNode for $name {
            fn can_cast(k: SyntaxKind) -> bool { k == SyntaxKind::$kind }
            fn cast(node: ResolvedNode) -> Option<Self> {
                if node.kind() == SyntaxKind::$kind { Some(Self(node)) } else { None }
            }
            fn syntax(&self) -> &ResolvedNode { &self.0 }
        }
    };
}

// ----- helpers on raw nodes -----

/// Iterate the child *nodes* that cast to `T`.
fn typed_children<T: AstNode>(node: &ResolvedNode) -> impl Iterator<Item = T> + '_ {
    node.children().filter_map(|c| T::cast(c.clone()))
}

/// First child node that casts to `T`.
fn typed_child<T: AstNode>(node: &ResolvedNode) -> Option<T> {
    typed_children(node).next()
}

/// Text of the first child token of `kind`, if any.
fn token_text(node: &ResolvedNode, kind: SyntaxKind) -> Option<&str> {
    node.children_with_tokens().find_map(|e| {
        let t = e.into_token()?;
        (t.kind() == kind).then(|| t.text())
    })
}

/// Kind of the first child token matching any operator kind in `kinds`.
fn first_token_kind(node: &ResolvedNode, kinds: &[SyntaxKind]) -> Option<SyntaxKind> {
    node.children_with_tokens().find_map(|e| {
        let t = e.into_token()?;
        kinds.contains(&t.kind()).then(|| t.kind())
    })
}

// ===== expressions =====

ast_node!(Literal = Literal);
ast_node!(VarRef = VarRef);
ast_node!(FieldExpr = FieldExpr);
ast_node!(IndexExpr = IndexExpr);
ast_node!(CallExpr = CallExpr);
ast_node!(MacroCall = MacroCallExpr);
ast_node!(FilterExpr = FilterExpr);
ast_node!(TestExpr = TestExpr);
ast_node!(TernaryExpr = TernaryExpr);
ast_node!(BinaryExpr = BinaryExpr);
ast_node!(UnaryExpr = UnaryExpr);
ast_node!(OptionalExpr = OptionalExpr);
ast_node!(ListLit = ListLit);
ast_node!(DictLit = DictLit);
ast_node!(ParenExpr = ParenExpr);
ast_node!(ArgList = ArgList);
ast_node!(Arg = Arg);
ast_node!(KwArg = KwArg);

/// Any expression node.
#[derive(Debug, Clone)]
pub enum Expr {
    Literal(Literal),
    Var(VarRef),
    Field(FieldExpr),
    Index(IndexExpr),
    Call(CallExpr),
    MacroCall(MacroCall),
    Filter(FilterExpr),
    Test(TestExpr),
    Ternary(TernaryExpr),
    Binary(BinaryExpr),
    Unary(UnaryExpr),
    Optional(OptionalExpr),
    List(ListLit),
    Dict(DictLit),
    Paren(ParenExpr),
}

impl Expr {
    pub fn cast(node: ResolvedNode) -> Option<Expr> {
        Some(match node.kind() {
            SyntaxKind::Literal => Expr::Literal(self::Literal(node)),
            SyntaxKind::VarRef => Expr::Var(self::VarRef(node)),
            SyntaxKind::FieldExpr => Expr::Field(self::FieldExpr(node)),
            SyntaxKind::IndexExpr => Expr::Index(self::IndexExpr(node)),
            SyntaxKind::CallExpr => Expr::Call(self::CallExpr(node)),
            SyntaxKind::MacroCallExpr => Expr::MacroCall(self::MacroCall(node)),
            SyntaxKind::FilterExpr => Expr::Filter(self::FilterExpr(node)),
            SyntaxKind::TestExpr => Expr::Test(self::TestExpr(node)),
            SyntaxKind::TernaryExpr => Expr::Ternary(self::TernaryExpr(node)),
            SyntaxKind::BinaryExpr => Expr::Binary(self::BinaryExpr(node)),
            SyntaxKind::UnaryExpr => Expr::Unary(self::UnaryExpr(node)),
            SyntaxKind::OptionalExpr => Expr::Optional(self::OptionalExpr(node)),
            SyntaxKind::ListLit => Expr::List(self::ListLit(node)),
            SyntaxKind::DictLit => Expr::Dict(self::DictLit(node)),
            SyntaxKind::ParenExpr => Expr::Paren(self::ParenExpr(node)),
            _ => return None,
        })
    }

    pub fn syntax(&self) -> &ResolvedNode {
        match self {
            Expr::Literal(n) => n.syntax(),
            Expr::Var(n) => n.syntax(),
            Expr::Field(n) => n.syntax(),
            Expr::Index(n) => n.syntax(),
            Expr::Call(n) => n.syntax(),
            Expr::MacroCall(n) => n.syntax(),
            Expr::Filter(n) => n.syntax(),
            Expr::Test(n) => n.syntax(),
            Expr::Ternary(n) => n.syntax(),
            Expr::Binary(n) => n.syntax(),
            Expr::Unary(n) => n.syntax(),
            Expr::Optional(n) => n.syntax(),
            Expr::List(n) => n.syntax(),
            Expr::Dict(n) => n.syntax(),
            Expr::Paren(n) => n.syntax(),
        }
    }
}

fn first_expr(node: &ResolvedNode) -> Option<Expr> {
    node.children().find_map(|c| Expr::cast(c.clone()))
}

fn nth_expr(node: &ResolvedNode, n: usize) -> Option<Expr> {
    node.children().filter_map(|c| Expr::cast(c.clone())).nth(n)
}

/// The kind of literal a [`Literal`] node holds.
#[derive(Debug, Clone, PartialEq)]
pub enum LitValue {
    Str(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    None,
}

impl Literal {
    pub fn value(&self) -> LitValue {
        let tok = self.0.first_token();
        let Some(tok) = tok else {
            return LitValue::None;
        };
        match tok.kind() {
            Int => LitValue::Int(tok.text().parse().unwrap_or(0)),
            Float => LitValue::Float(tok.text().parse().unwrap_or(0.0)),
            True => LitValue::Bool(true),
            False => LitValue::Bool(false),
            NoneKw => LitValue::None,
            Str => LitValue::Str(unquote(tok.text())),
            _ => LitValue::None,
        }
    }
}

impl VarRef {
    pub fn name(&self) -> Option<&str> {
        token_text(&self.0, Ident)
    }
}

impl FieldExpr {
    pub fn base(&self) -> Option<Expr> {
        first_expr(&self.0)
    }
    pub fn field(&self) -> Option<&str> {
        token_text(&self.0, Ident)
    }
}

impl IndexExpr {
    pub fn base(&self) -> Option<Expr> {
        first_expr(&self.0)
    }
    /// `true` when this is a slice (`a[:n]` / `a[a:b]`) rather than a plain index.
    pub fn is_slice(&self) -> bool {
        self.0
            .children_with_tokens()
            .any(|e| e.into_token().is_some_and(|t| t.kind() == Colon))
    }
    /// The index expression (for a plain index), i.e. the second expression child.
    pub fn index(&self) -> Option<Expr> {
        nth_expr(&self.0, 1)
    }
}

impl CallExpr {
    pub fn callee(&self) -> Option<Expr> {
        first_expr(&self.0)
    }
    pub fn args(&self) -> Option<ArgList> {
        typed_child(&self.0)
    }
}

impl MacroCall {
    fn idents(&self) -> impl Iterator<Item = &str> + '_ {
        self.0
            .children_with_tokens()
            .filter_map(|e| e.into_token())
            .filter(|t| t.kind() == Ident)
            .map(|t| t.text())
    }
    pub fn namespace(&self) -> Option<&str> {
        self.idents().next()
    }
    pub fn name(&self) -> Option<&str> {
        self.idents().nth(1)
    }
    pub fn args(&self) -> Option<ArgList> {
        typed_child(&self.0)
    }
}

impl ArgList {
    pub fn positional(&self) -> impl Iterator<Item = Expr> + '_ {
        typed_children::<Arg>(&self.0).filter_map(|a| first_expr(a.syntax()))
    }
    pub fn keyword(&self) -> impl Iterator<Item = (String, Expr)> + '_ {
        typed_children::<KwArg>(&self.0).filter_map(|k| {
            Some((
                token_text(k.syntax(), Ident)?.to_owned(),
                first_expr(k.syntax())?,
            ))
        })
    }
}

impl FilterExpr {
    pub fn base(&self) -> Option<Expr> {
        first_expr(&self.0)
    }
    pub fn name(&self) -> Option<&str> {
        // `expr | NAME` — the filter name is the Ident token directly under this node.
        token_text(&self.0, Ident)
    }
    pub fn args(&self) -> Option<ArgList> {
        typed_child(&self.0)
    }
}

impl TestExpr {
    pub fn base(&self) -> Option<Expr> {
        first_expr(&self.0)
    }
    pub fn negated(&self) -> bool {
        self.0
            .children_with_tokens()
            .any(|e| e.into_token().is_some_and(|t| t.kind() == NotKw))
    }
    pub fn name(&self) -> Option<&str> {
        // Test name is an Ident, or the `none` keyword.
        self.0.children_with_tokens().find_map(|e| {
            let t = e.into_token()?;
            matches!(t.kind(), Ident | NoneKw).then(|| t.text())
        })
    }
    pub fn args(&self) -> Option<ArgList> {
        typed_child(&self.0)
    }
}

impl TernaryExpr {
    /// `value if cond else otherwise`: the three expression children in source order.
    pub fn value(&self) -> Option<Expr> {
        nth_expr(&self.0, 0)
    }
    pub fn condition(&self) -> Option<Expr> {
        nth_expr(&self.0, 1)
    }
    pub fn otherwise(&self) -> Option<Expr> {
        nth_expr(&self.0, 2)
    }
}

/// A binary operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    FloorDiv,
    Mod,
    Pow,
    Concat,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
    In,
    NotIn,
}

impl BinaryExpr {
    pub fn lhs(&self) -> Option<Expr> {
        nth_expr(&self.0, 0)
    }
    pub fn rhs(&self) -> Option<Expr> {
        nth_expr(&self.0, 1)
    }
    pub fn op(&self) -> Option<BinOp> {
        // `not in` is the only binary carrying a `not` token; detect it first.
        let has = |k: SyntaxKind| {
            self.0
                .children_with_tokens()
                .any(|e| e.into_token().is_some_and(|t| t.kind() == k))
        };
        if has(NotKw) && has(InKw) {
            return Some(BinOp::NotIn);
        }
        let k = first_token_kind(
            &self.0,
            &[
                Plus, Minus, Star, Slash, SlashSlash, Percent, StarStar, Tilde, EqEq, Neq, Lt, Le,
                Gt, Ge, AndKw, OrKw, InKw,
            ],
        )?;
        Some(match k {
            Plus => BinOp::Add,
            Minus => BinOp::Sub,
            Star => BinOp::Mul,
            Slash => BinOp::Div,
            SlashSlash => BinOp::FloorDiv,
            Percent => BinOp::Mod,
            StarStar => BinOp::Pow,
            Tilde => BinOp::Concat,
            EqEq => BinOp::Eq,
            Neq => BinOp::Ne,
            Lt => BinOp::Lt,
            Le => BinOp::Le,
            Gt => BinOp::Gt,
            Ge => BinOp::Ge,
            AndKw => BinOp::And,
            OrKw => BinOp::Or,
            InKw => BinOp::In,
            _ => return None,
        })
    }
}

/// A unary operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Not,
    Neg,
}

impl UnaryExpr {
    pub fn op(&self) -> Option<UnOp> {
        match first_token_kind(&self.0, &[NotKw, Minus])? {
            NotKw => Some(UnOp::Not),
            Minus => Some(UnOp::Neg),
            _ => None,
        }
    }
    pub fn operand(&self) -> Option<Expr> {
        first_expr(&self.0)
    }
}

impl OptionalExpr {
    pub fn operand(&self) -> Option<Expr> {
        first_expr(&self.0)
    }
}

impl ListLit {
    pub fn elements(&self) -> impl Iterator<Item = Expr> + '_ {
        self.0.children().filter_map(|c| Expr::cast(c.clone()))
    }
}

impl DictLit {
    /// (key, value) pairs in source order.
    pub fn entries(&self) -> Vec<(Expr, Expr)> {
        let exprs: Vec<Expr> = self
            .0
            .children()
            .filter_map(|c| Expr::cast(c.clone()))
            .collect();
        exprs
            .chunks_exact(2)
            .map(|c| (c[0].clone(), c[1].clone()))
            .collect()
    }
}

impl ParenExpr {
    pub fn inner(&self) -> Option<Expr> {
        first_expr(&self.0)
    }
}

// ===== statements / template structure =====

ast_node!(Template = Template);
ast_node!(Interpolation = Interpolation);
ast_node!(Body = Body);
ast_node!(IfStmt = IfStmt);
ast_node!(ElifClause = ElifClause);
ast_node!(ElseClause = ElseClause);
ast_node!(ForStmt = ForStmt);
ast_node!(SetStmt = SetStmt);
ast_node!(BlockStmt = BlockStmt);
ast_node!(MacroStmt = MacroStmt);
ast_node!(ExtendsStmt = ExtendsStmt);
ast_node!(IncludeStmt = IncludeStmt);
ast_node!(ImportStmt = ImportStmt);
ast_node!(ParamList = ParamList);
ast_node!(Param = Param);
ast_node!(BreakStmt = BreakStmt);
ast_node!(ContinueStmt = ContinueStmt);

/// One item in a template body: literal text/trivia, an interpolation, or a statement.
#[derive(Debug, Clone)]
pub enum Item {
    /// A run of literal text or trivia token (kept for lossless output; render trims it
    /// per adjacent `{%- -%}` markers).
    Text(crate::ResolvedToken),
    Interpolation(Interpolation),
    If(IfStmt),
    For(ForStmt),
    Set(SetStmt),
    Block(BlockStmt),
    Macro(MacroStmt),
    Extends(ExtendsStmt),
    Include(IncludeStmt),
    Import(ImportStmt),
    Break(BreakStmt),
    Continue(ContinueStmt),
}

/// Iterate the items of a `Template` or `Body` node in source order.
fn items_of(node: &ResolvedNode) -> Vec<Item> {
    let mut out = Vec::new();
    for el in node.children_with_tokens() {
        match el {
            cstree::syntax::ResolvedElementRef::Token(t) => {
                // Text / Whitespace / Comment tokens become Text items (lossless).
                out.push(Item::Text(t.clone()));
            }
            cstree::syntax::ResolvedElementRef::Node(n) => {
                let item = match n.kind() {
                    SyntaxKind::Interpolation => Item::Interpolation(Interpolation(n.clone())),
                    SyntaxKind::IfStmt => Item::If(IfStmt(n.clone())),
                    SyntaxKind::ForStmt => Item::For(ForStmt(n.clone())),
                    SyntaxKind::SetStmt => Item::Set(SetStmt(n.clone())),
                    SyntaxKind::BlockStmt => Item::Block(BlockStmt(n.clone())),
                    SyntaxKind::MacroStmt => Item::Macro(MacroStmt(n.clone())),
                    SyntaxKind::ExtendsStmt => Item::Extends(ExtendsStmt(n.clone())),
                    SyntaxKind::IncludeStmt => Item::Include(IncludeStmt(n.clone())),
                    SyntaxKind::ImportStmt => Item::Import(ImportStmt(n.clone())),
                    SyntaxKind::BreakStmt => Item::Break(BreakStmt(n.clone())),
                    SyntaxKind::ContinueStmt => Item::Continue(ContinueStmt(n.clone())),
                    // Error/other nodes are skipped by the typed walk.
                    _ => continue,
                };
                out.push(item);
            }
        }
    }
    out
}

impl Item {
    /// The item's node, for non-text items.
    fn node(&self) -> Option<&ResolvedNode> {
        Some(match self {
            Item::Text(_) => return None,
            Item::Interpolation(n) => &n.0,
            Item::If(n) => &n.0,
            Item::For(n) => &n.0,
            Item::Set(n) => &n.0,
            Item::Block(n) => &n.0,
            Item::Macro(n) => &n.0,
            Item::Extends(n) => &n.0,
            Item::Include(n) => &n.0,
            Item::Import(n) => &n.0,
            Item::Break(n) => &n.0,
            Item::Continue(n) => &n.0,
        })
    }

    /// Whether this item's leading delimiter is a trim variant (`{%-`/`{{-`) —
    /// trims trailing whitespace of the text run before it.
    pub fn opens_with_trim(&self) -> bool {
        self.node().and_then(|n| n.first_token()).is_some_and(|t| {
            matches!(
                t.kind(),
                SyntaxKind::OpenStmtTrim | SyntaxKind::OpenExprTrim
            )
        })
    }

    /// Whether this item's trailing delimiter is a trim variant (`-%}`/`-}}`) —
    /// trims leading whitespace of the text run after it.
    pub fn closes_with_trim(&self) -> bool {
        self.node().and_then(|n| n.last_token()).is_some_and(|t| {
            matches!(
                t.kind(),
                SyntaxKind::CloseStmtTrim | SyntaxKind::CloseExprTrim
            )
        })
    }
}

impl Template {
    pub fn items(&self) -> Vec<Item> {
        items_of(&self.0)
    }
}

impl Body {
    pub fn items(&self) -> Vec<Item> {
        items_of(&self.0)
    }
}

impl Interpolation {
    pub fn expr(&self) -> Option<Expr> {
        first_expr(&self.0)
    }
}

impl IfStmt {
    pub fn condition(&self) -> Option<Expr> {
        first_expr(&self.0)
    }
    /// The `then` body is the first direct `Body` child.
    pub fn then_body(&self) -> Option<Body> {
        typed_child(&self.0)
    }
    pub fn elif_clauses(&self) -> impl Iterator<Item = ElifClause> + '_ {
        typed_children(&self.0)
    }
    pub fn else_clause(&self) -> Option<ElseClause> {
        typed_child(&self.0)
    }
}

impl ElifClause {
    pub fn condition(&self) -> Option<Expr> {
        first_expr(&self.0)
    }
    pub fn body(&self) -> Option<Body> {
        typed_child(&self.0)
    }
}

impl ElseClause {
    pub fn body(&self) -> Option<Body> {
        typed_child(&self.0)
    }
}

impl ForStmt {
    /// Loop target names (one, or several for tuple unpacking).
    pub fn targets(&self) -> Vec<String> {
        // The Ident tokens before `in` are the targets; the iterator is an Expr child.
        let mut names = Vec::new();
        for el in self.0.children_with_tokens() {
            if let Some(t) = el.into_token() {
                match t.kind() {
                    SyntaxKind::Ident => names.push(t.text().to_owned()),
                    SyntaxKind::InKw => break,
                    _ => {}
                }
            } else {
                break;
            }
        }
        names
    }
    pub fn iter_expr(&self) -> Option<Expr> {
        first_expr(&self.0)
    }
    pub fn body(&self) -> Option<Body> {
        typed_child(&self.0)
    }
    pub fn else_body(&self) -> Option<Body> {
        // The for-else body lives inside an ElseClause child.
        typed_child::<ElseClause>(&self.0).and_then(|e| e.body())
    }
}

impl SetStmt {
    pub fn name(&self) -> Option<String> {
        token_text(&self.0, Ident).map(str::to_owned)
    }
    /// `{% set x = EXPR %}` — present for the assignment form.
    pub fn value(&self) -> Option<Expr> {
        first_expr(&self.0)
    }
    /// `{% set x %}…{% endset %}` — present for the block form.
    pub fn body(&self) -> Option<Body> {
        typed_child(&self.0)
    }
}

impl BlockStmt {
    pub fn name(&self) -> Option<String> {
        token_text(&self.0, Ident).map(str::to_owned)
    }
    pub fn body(&self) -> Option<Body> {
        typed_child(&self.0)
    }
}

impl MacroStmt {
    pub fn name(&self) -> Option<String> {
        token_text(&self.0, Ident).map(str::to_owned)
    }
    pub fn params(&self) -> Vec<(String, Option<Expr>)> {
        let Some(list) = typed_child::<ParamList>(&self.0) else {
            return Vec::new();
        };
        typed_children::<Param>(list.syntax())
            .filter_map(|p| {
                Some((
                    token_text(p.syntax(), Ident)?.to_owned(),
                    first_expr(p.syntax()),
                ))
            })
            .collect()
    }
    pub fn body(&self) -> Option<Body> {
        typed_child(&self.0)
    }
}

impl ExtendsStmt {
    pub fn path(&self) -> Option<Expr> {
        first_expr(&self.0)
    }
}

impl IncludeStmt {
    pub fn path(&self) -> Option<Expr> {
        first_expr(&self.0)
    }
}

impl ImportStmt {
    pub fn path(&self) -> Option<Expr> {
        first_expr(&self.0)
    }
    pub fn alias(&self) -> Option<String> {
        // `import "p" as NAME` — the trailing Ident.
        self.0
            .children_with_tokens()
            .filter_map(|e| e.into_token())
            .filter(|t| t.kind() == Ident)
            .last()
            .map(|t| t.text().to_owned())
    }
}

/// Resolve a quoted string literal to its value (strip quotes, process `\` escapes).
fn unquote(raw: &str) -> String {
    let bytes = raw.as_bytes();
    if bytes.len() < 2 {
        return raw.to_string();
    }
    let inner = &raw[1..raw.len() - 1];
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('\\') => out.push('\\'),
                Some('"') => out.push('"'),
                Some('\'') => out.push('\''),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_expr_str;

    /// Cast the single interpolation's expression out of a `{{ … }}` parse.
    fn expr_of(src: &str) -> Expr {
        let p = parse_expr_str(src);
        assert!(p.errors.is_empty(), "errors: {:?}", p.errors);
        // Template → Interpolation → Expr
        let interp = p
            .syntax()
            .children()
            .find(|n| n.kind() == SyntaxKind::Interpolation)
            .unwrap();
        first_expr(interp).expect("an expression")
    }

    #[test]
    fn literals() {
        assert!(matches!(expr_of("42"), Expr::Literal(l) if l.value() == LitValue::Int(42)));
        assert!(
            matches!(expr_of("\"a\\nb\""), Expr::Literal(l) if l.value() == LitValue::Str("a\nb".into()))
        );
        assert!(matches!(expr_of("true"), Expr::Literal(l) if l.value() == LitValue::Bool(true)));
    }

    #[test]
    fn binary_op_and_operands() {
        let Expr::Binary(b) = expr_of("a + b") else {
            panic!()
        };
        assert_eq!(b.op(), Some(BinOp::Add));
        assert!(matches!(b.lhs(), Some(Expr::Var(_))));
        assert!(matches!(b.rhs(), Some(Expr::Var(_))));
    }

    #[test]
    fn not_in_operator() {
        let Expr::Binary(b) = expr_of("x not in xs") else {
            panic!()
        };
        assert_eq!(b.op(), Some(BinOp::NotIn));
    }

    #[test]
    fn call_with_field_arg_and_kwarg() {
        let Expr::Call(c) = expr_of("f(a.b, k=c)") else {
            panic!()
        };
        let args = c.args().unwrap();
        assert_eq!(args.positional().count(), 1);
        let kw: Vec<_> = args.keyword().collect();
        assert_eq!(kw.len(), 1);
        assert_eq!(kw[0].0, "k");
        assert!(matches!(args.positional().next(), Some(Expr::Field(_))));
    }

    #[test]
    fn field_and_optional() {
        let Expr::Field(f) = expr_of("page.title") else {
            panic!()
        };
        assert_eq!(f.field(), Some("title"));
        assert!(matches!(expr_of("width?"), Expr::Optional(_)));
    }

    #[test]
    fn slice_detected() {
        let Expr::Index(i) = expr_of("xs[:3]") else {
            panic!()
        };
        assert!(i.is_slice());
    }

    fn template(src: &str) -> Template {
        let p = crate::parse(src);
        assert!(p.errors.is_empty(), "errors: {:?}", p.errors);
        Template::cast(p.syntax().clone()).expect("root is a Template")
    }

    #[test]
    fn if_elif_else_structure() {
        let t = template("{% if a %}x{% elif b %}y{% else %}z{% endif %}");
        let Item::If(iff) = &t.items()[0] else {
            panic!("{:?}", t.items())
        };
        assert!(matches!(iff.condition(), Some(Expr::Var(_))));
        assert!(iff.then_body().is_some());
        assert_eq!(iff.elif_clauses().count(), 1);
        assert!(iff.else_clause().is_some());
    }

    #[test]
    fn for_targets_and_iter() {
        let t = template("{% for a, b in items %}{{ a }}{% endfor %}");
        let Item::For(f) = &t.items()[0] else {
            panic!()
        };
        assert_eq!(f.targets(), vec!["a".to_string(), "b".to_string()]);
        assert!(matches!(f.iter_expr(), Some(Expr::Var(_))));
        assert!(f.body().is_some());
    }

    #[test]
    fn macro_params() {
        let t = template("{% macro m(a, b=1) %}{{ a }}{% endmacro %}");
        let Item::Macro(m) = &t.items()[0] else {
            panic!()
        };
        assert_eq!(m.name().as_deref(), Some("m"));
        let params = m.params();
        assert_eq!(params.len(), 2);
        assert_eq!(params[0].0, "a");
        assert!(params[0].1.is_none());
        assert_eq!(params[1].0, "b");
        assert!(params[1].1.is_some());
    }

    #[test]
    fn set_assign_vs_block() {
        let t = template("{% set x = 1 %}");
        let Item::Set(s) = &t.items()[0] else {
            panic!()
        };
        assert_eq!(s.name().as_deref(), Some("x"));
        assert!(s.value().is_some());

        let t2 = template("{% set y %}hi{% endset %}");
        let Item::Set(s2) = &t2.items()[0] else {
            panic!()
        };
        assert!(s2.value().is_none());
        assert!(s2.body().is_some());
    }
}
