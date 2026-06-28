//! Bridge: lower the cstree typed views (`gingembre_syntax::ast`) into the engine's
//! existing `ast::Expr`/`Node` so the new parser drives the unchanged eval/render.
//!
//! INTERIM: the correctness bar is rendered output, not AST parity, so spans are derived
//! from CST text ranges (more accurate than the old parser's) and need not match the old
//! ones. The end state is to evaluate directly off the typed views and delete `ast.rs`;
//! this bridge gets the engine green on the cstree parser first, then that follows.

use gingembre_syntax::ResolvedNode;
use gingembre_syntax::ast as cst;

use crate::ast::{self, BinaryOp, Expr, Ident, Span, UnaryOp};

/// Span covering a CST node.
fn sp(node: &ResolvedNode) -> Span {
    let r = node.text_range();
    ast::span(usize::from(r.start()), usize::from(r.len()))
}

fn ident(name: &str, node: &ResolvedNode) -> Ident {
    // Use the matching Ident token's own range (the semantic index / LSP keys references
    // on ident spans), not the enclosing node's.
    let span = node
        .children_with_tokens()
        .filter_map(|e| e.into_token())
        .find(|t| t.kind() == gingembre_syntax::SyntaxKind::Ident && t.text() == name)
        .map(|t| ast::span(usize::from(t.text_range().start()), t.text().len()))
        .unwrap_or_else(|| sp(node));
    Ident {
        name: name.to_string(),
        span,
    }
}

/// Lower a typed-CST expression to an engine `Expr`.
pub fn lower_expr(e: &cst::Expr) -> Expr {
    let node = e.syntax();
    let span = sp(node);
    match e {
        cst::Expr::Literal(l) => Expr::Literal(lower_literal(l, span)),
        cst::Expr::Var(v) => Expr::Var(Ident {
            name: v.name().unwrap_or_default().to_string(),
            span,
        }),
        cst::Expr::Paren(p) => p
            .inner()
            .map(|i| lower_expr(&i))
            .unwrap_or(Expr::Literal(ast::Literal::None(ast::NoneLit { span }))),
        cst::Expr::Field(f) => Expr::Field(ast::FieldExpr {
            base: Box::new(opt_expr(f.base(), span)),
            field: ident(f.field().unwrap_or_default(), node),
            span,
        }),
        cst::Expr::Index(i) => Expr::Index(ast::IndexExpr {
            base: Box::new(opt_expr(i.base(), span)),
            // Slices aren't a first-class engine Expr; use the (end) index expression, or
            // a None placeholder for a bare `[:]`. (Refine when porting eval off the CST.)
            index: Box::new(opt_expr(i.index(), span)),
            span,
        }),
        cst::Expr::Call(c) => {
            let (args, kwargs) = lower_args(c.args());
            Expr::Call(ast::CallExpr {
                func: Box::new(opt_expr(c.callee(), span)),
                args,
                kwargs,
                span,
            })
        }
        cst::Expr::MacroCall(m) => {
            let (args, kwargs) = lower_args(m.args());
            Expr::MacroCall(ast::MacroCallExpr {
                namespace: ident(m.namespace().unwrap_or_default(), node),
                macro_name: ident(m.name().unwrap_or_default(), node),
                args,
                kwargs,
                span,
            })
        }
        cst::Expr::Filter(f) => {
            let (args, kwargs) = lower_args(f.args());
            Expr::Filter(ast::FilterExpr {
                expr: Box::new(opt_expr(f.base(), span)),
                filter: ident(f.name().unwrap_or_default(), node),
                args,
                kwargs,
                span,
            })
        }
        cst::Expr::Test(t) => {
            let (args, _) = lower_args(t.args());
            Expr::Test(ast::TestExpr {
                expr: Box::new(opt_expr(t.base(), span)),
                test_name: ident(t.name().unwrap_or_default(), node),
                args,
                negated: t.negated(),
                span,
            })
        }
        cst::Expr::Ternary(t) => Expr::Ternary(ast::TernaryExpr {
            value: Box::new(opt_expr(t.value(), span)),
            condition: Box::new(opt_expr(t.condition(), span)),
            otherwise: Box::new(opt_expr(t.otherwise(), span)),
            span,
        }),
        cst::Expr::Binary(b) => Expr::Binary(ast::BinaryExpr {
            left: Box::new(opt_expr(b.lhs(), span)),
            op: lower_binop(b.op()),
            right: Box::new(opt_expr(b.rhs(), span)),
            span,
        }),
        cst::Expr::Unary(u) => Expr::Unary(ast::UnaryExpr {
            op: match u.op() {
                Some(cst::UnOp::Not) => UnaryOp::Not,
                _ => UnaryOp::Neg,
            },
            expr: Box::new(opt_expr(u.operand(), span)),
            span,
        }),
        cst::Expr::Optional(o) => Expr::Optional(ast::OptionalExpr {
            expr: Box::new(opt_expr(o.operand(), span)),
            span,
        }),
        cst::Expr::List(l) => Expr::Literal(ast::Literal::List(ast::ListLit {
            elements: l.elements().map(|x| lower_expr(&x)).collect(),
            span,
        })),
        cst::Expr::Dict(d) => Expr::Literal(ast::Literal::Dict(ast::DictLit {
            entries: d
                .entries()
                .iter()
                .map(|(k, v)| (lower_expr(k), lower_expr(v)))
                .collect(),
            span,
        })),
    }
}

fn opt_expr(e: Option<cst::Expr>, span: Span) -> Expr {
    e.map(|x| lower_expr(&x))
        .unwrap_or(Expr::Literal(ast::Literal::None(ast::NoneLit { span })))
}

fn lower_literal(l: &cst::Literal, span: Span) -> ast::Literal {
    match l.value() {
        cst::LitValue::Str(s) => ast::Literal::String(ast::StringLit { value: s, span }),
        cst::LitValue::Int(i) => ast::Literal::Int(ast::IntLit { value: i, span }),
        cst::LitValue::Float(f) => ast::Literal::Float(ast::FloatLit { value: f, span }),
        cst::LitValue::Bool(b) => ast::Literal::Bool(ast::BoolLit { value: b, span }),
        cst::LitValue::None => ast::Literal::None(ast::NoneLit { span }),
    }
}

fn lower_args(args: Option<cst::ArgList>) -> (Vec<Expr>, Vec<(Ident, Expr)>) {
    let Some(args) = args else {
        return (Vec::new(), Vec::new());
    };
    let pos = args.positional().map(|e| lower_expr(&e)).collect();
    let kw = args
        .keyword()
        .map(|(name, e)| {
            let span = sp(e.syntax());
            (Ident { name, span }, lower_expr(&e))
        })
        .collect();
    (pos, kw)
}

use crate::ast::{
    BlockNode, ElifBranch, ExtendsNode, ForNode, IfNode, ImportNode, IncludeNode, MacroNode,
    MacroParam, Node, PrintNode, SetNode, SetValue, StringLit, Target, TextNode,
};

use std::cell::RefCell;
use std::collections::HashMap;

thread_local! {
    /// Per-parse map: byte offset of each Text run → (trim_leading, trim_trailing),
    /// computed from flat token adjacency (whitespace control crosses tree boundaries).
    static TRIM: RefCell<HashMap<usize, (bool, bool)>> = RefCell::new(HashMap::new());
}

/// Compute whitespace-control trim flags for every Text run from the flat lexeme stream.
fn compute_trim(src: &str) -> HashMap<usize, (bool, bool)> {
    use gingembre_syntax::SyntaxKind::*;
    let lex = gingembre_syntax::lex(src);
    let mut map = HashMap::new();
    let mut offset = 0usize;
    for (i, lx) in lex.iter().enumerate() {
        if lx.kind == Text {
            let lead = i > 0 && matches!(lex[i - 1].kind, CloseStmtTrim | CloseExprTrim);
            let trail = i + 1 < lex.len() && matches!(lex[i + 1].kind, OpenStmtTrim | OpenExprTrim);
            map.insert(offset, (lead, trail));
        }
        offset += lx.text.len();
    }
    map
}

/// Parse a template via the cstree front-end, returning the engine AST or the first
/// parse error as a `TemplateError`. This is the single parse entry shared by the engine
/// (`Template::parse`) and the authoring LSP — there is no longer a separate hand-written
/// parser doing the work.
pub fn parse_template(
    name: impl Into<String>,
    src: impl AsRef<str>,
) -> Result<crate::ast::Template, crate::error::TemplateError> {
    let src = src.as_ref();
    let (ast, errors) = parse_to_template(src);
    if let Some(e) = errors.first() {
        let ts = crate::error::TemplateSource::new(name.into(), src);
        return Err(crate::error::SyntaxError {
            found: "end of input".to_string(),
            expected: e.message.clone(),
            loc: crate::error::SourceLocation::new(ast::span(e.offset, 1), ts.named_source()),
        }
        .into());
    }
    Ok(ast)
}

/// Parse a standalone expression (REPL / `eval_expression`) via the cstree parser.
pub fn parse_expression(src: &str) -> Result<Expr, crate::error::TemplateError> {
    use gingembre_syntax::ast::AstNode;
    let p = gingembre_syntax::parse_expr_str(src);
    let expr = cst::Template::cast(p.syntax().clone()).and_then(|t| {
        t.items().into_iter().find_map(|i| match i {
            cst::Item::Interpolation(interp) => interp.expr(),
            _ => None,
        })
    });
    match expr {
        Some(e) => Ok(lower_expr(&e)),
        None => {
            let ts = crate::error::TemplateSource::new("<expr>", src);
            Err(crate::error::SyntaxError {
                found: "end of input".to_string(),
                expected: "an expression".to_string(),
                loc: crate::error::SourceLocation::new(ast::span(0, src.len()), ts.named_source()),
            }
            .into())
        }
    }
}

/// Parse leniently (recovery): always returns a (best-effort) template, ignoring parse
/// errors. Replaces the old `Parser::parse_recovered().template`.
pub fn parse_template_recovered(src: impl AsRef<str>) -> crate::ast::Template {
    parse_to_template(src.as_ref()).0
}

/// Parse `src` with the cstree parser and lower it to the engine's template AST.
///
/// The parser recovers (produces a usable tree even with errors); error *surfacing* to
/// the engine's `TemplateError` is a follow-up. The bar is render output, and the real
/// ftl corpus parses clean.
pub fn parse_to_template(src: &str) -> (crate::ast::Template, Vec<gingembre_syntax::ParseError>) {
    use gingembre_syntax::ast::AstNode;
    let parse = gingembre_syntax::parse(src);
    TRIM.with(|t| *t.borrow_mut() = compute_trim(src));
    let body = cst::Template::cast(parse.syntax().clone())
        .map(|t| lower_items(&t.items()))
        .unwrap_or_default();
    TRIM.with(|t| t.borrow_mut().clear());
    (
        crate::ast::Template {
            body,
            span: ast::span(0, src.len()),
        },
        parse.errors,
    )
}

fn lower_items(items: &[cst::Item]) -> Vec<Node> {
    let mut out = Vec::new();
    for (i, item) in items.iter().enumerate() {
        match item {
            cst::Item::Text(tok) => {
                if tok.kind() == gingembre_syntax::SyntaxKind::Comment {
                    continue; // comments stripped from output
                }
                // Whitespace control via flat-adjacency trim flags (keyed by offset).
                let off = usize::from(tok.text_range().start());
                let (lead, trail) =
                    TRIM.with(|t| t.borrow().get(&off).copied().unwrap_or((false, false)));
                let mut text = tok.text();
                if lead {
                    text = text.trim_start();
                }
                if trail {
                    text = text.trim_end();
                }
                let _ = i;
                if text.is_empty() {
                    continue;
                }
                out.push(Node::Text(TextNode {
                    text: text.to_string(),
                    span: ast::span(0, text.len()),
                }));
            }
            other => {
                if let Some(node) = lower_item(other) {
                    out.push(node);
                }
            }
        }
    }
    out
}

fn lower_body(body: Option<cst::Body>) -> Vec<Node> {
    body.map(|b| lower_items(&b.items())).unwrap_or_default()
}

fn lower_item(item: &cst::Item) -> Option<Node> {
    use gingembre_syntax::ast::AstNode;
    Some(match item {
        // Whitespace-control trimming (`{%- -%}`) is applied as a follow-up; text is
        // passed through verbatim for now (affects only surrounding whitespace).
        cst::Item::Text(tok) => {
            // Comments are stripped from output (not rendered).
            if tok.kind() == gingembre_syntax::SyntaxKind::Comment {
                return None;
            }
            Node::Text(TextNode {
                text: tok.text().to_string(),
                span: ast::span(0, tok.text().len()),
            })
        }
        cst::Item::Interpolation(i) => {
            let span = sp(i.syntax());
            Node::Print(PrintNode {
                expr: opt_expr(i.expr(), span),
                span,
            })
        }
        cst::Item::If(iff) => {
            let span = sp(iff.syntax());
            Node::If(IfNode {
                condition: opt_expr(iff.condition(), span),
                then_body: lower_body(iff.then_body()),
                elif_branches: iff
                    .elif_clauses()
                    .map(|e| ElifBranch {
                        condition: opt_expr(e.condition(), sp(e.syntax())),
                        body: lower_body(e.body()),
                        span: sp(e.syntax()),
                    })
                    .collect(),
                else_body: iff.else_clause().map(|e| lower_body(e.body())),
                span,
            })
        }
        cst::Item::For(f) => {
            let span = sp(f.syntax());
            let names = f.targets();
            let target = if names.len() == 1 {
                Target::Single {
                    name: names[0].clone(),
                    span,
                }
            } else {
                Target::Tuple {
                    names: names.into_iter().map(|n| (n, span)).collect(),
                    span,
                }
            };
            Node::For(ForNode {
                target,
                iter: opt_expr(f.iter_expr(), span),
                body: lower_body(f.body()),
                else_body: f.else_body().map(|b| lower_items(&b.items())),
                span,
            })
        }
        cst::Item::Set(s) => {
            let span = sp(s.syntax());
            // `{% set x = expr %}` binds an expression; `{% set x %}…{% endset %}`
            // binds the rendered body.
            let value = match s.value() {
                Some(expr) => SetValue::Expr(lower_expr(&expr)),
                None => SetValue::Body(lower_body(s.body())),
            };
            Node::Set(SetNode {
                name: ident(&s.name().unwrap_or_default(), s.syntax()),
                value,
                span,
            })
        }
        cst::Item::Block(b) => {
            let span = sp(b.syntax());
            Node::Block(BlockNode {
                name: ident(&b.name().unwrap_or_default(), b.syntax()),
                body: lower_body(b.body()),
                span,
            })
        }
        cst::Item::Macro(m) => {
            let span = sp(m.syntax());
            Node::Macro(MacroNode {
                name: ident(&m.name().unwrap_or_default(), m.syntax()),
                params: m
                    .params()
                    .into_iter()
                    .map(|(name, default)| MacroParam {
                        name: Ident { name, span },
                        default: default.as_ref().map(lower_expr),
                    })
                    .collect(),
                body: lower_body(m.body()),
                span,
            })
        }
        cst::Item::Extends(e) => Node::Extends(ExtendsNode {
            path: str_lit(e.path(), sp(e.syntax())),
            span: sp(e.syntax()),
        }),
        cst::Item::Include(i) => Node::Include(IncludeNode {
            path: str_lit(i.path(), sp(i.syntax())),
            context: None,
            span: sp(i.syntax()),
        }),
        cst::Item::Import(im) => {
            let span = sp(im.syntax());
            Node::Import(ImportNode {
                path: str_lit(im.path(), span),
                alias: Ident {
                    name: im.alias().unwrap_or_default(),
                    span,
                },
                span,
            })
        }
        cst::Item::Break(b) => Node::Break(crate::ast::BreakNode {
            span: sp(b.syntax()),
        }),
        cst::Item::Continue(c) => Node::Continue(crate::ast::ContinueNode {
            span: sp(c.syntax()),
        }),
    })
}

/// Extract a `StringLit` from a path expression (extends/include/import take string paths).
fn str_lit(e: Option<cst::Expr>, span: Span) -> StringLit {
    let value = match e {
        Some(cst::Expr::Literal(l)) => match l.value() {
            cst::LitValue::Str(s) => s,
            _ => String::new(),
        },
        _ => String::new(),
    };
    StringLit { value, span }
}

fn lower_binop(op: Option<cst::BinOp>) -> BinaryOp {
    match op {
        Some(cst::BinOp::Add) => BinaryOp::Add,
        Some(cst::BinOp::Sub) => BinaryOp::Sub,
        Some(cst::BinOp::Mul) => BinaryOp::Mul,
        Some(cst::BinOp::Div) => BinaryOp::Div,
        Some(cst::BinOp::FloorDiv) => BinaryOp::FloorDiv,
        Some(cst::BinOp::Mod) => BinaryOp::Mod,
        Some(cst::BinOp::Pow) => BinaryOp::Pow,
        Some(cst::BinOp::Concat) => BinaryOp::Concat,
        Some(cst::BinOp::Eq) => BinaryOp::Eq,
        Some(cst::BinOp::Ne) => BinaryOp::Ne,
        Some(cst::BinOp::Lt) => BinaryOp::Lt,
        Some(cst::BinOp::Le) => BinaryOp::Le,
        Some(cst::BinOp::Gt) => BinaryOp::Gt,
        Some(cst::BinOp::Ge) => BinaryOp::Ge,
        Some(cst::BinOp::And) => BinaryOp::And,
        Some(cst::BinOp::Or) => BinaryOp::Or,
        Some(cst::BinOp::In) => BinaryOp::In,
        Some(cst::BinOp::NotIn) => BinaryOp::NotIn,
        None => BinaryOp::Add,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gingembre_syntax::SyntaxKind;
    use gingembre_syntax::ast::AstNode;

    /// Lower the expression inside a `{{ … }}` and check it lowers without panicking,
    /// matching the expected engine Expr variant.
    fn lower(src: &str) -> Expr {
        let p = gingembre_syntax::parse(&format!("{{{{ {src} }}}}"));
        assert!(p.errors.is_empty(), "{:?}", p.errors);
        let interp = p
            .syntax()
            .children()
            .find(|n| n.kind() == SyntaxKind::Interpolation)
            .unwrap();
        let cst_expr = cst::Interpolation::cast(interp.clone())
            .unwrap()
            .expr()
            .unwrap();
        lower_expr(&cst_expr)
    }

    #[test]
    fn lowers_core_exprs() {
        assert!(matches!(lower("42"), Expr::Literal(ast::Literal::Int(i)) if i.value == 42));
        assert!(matches!(lower("a + b"), Expr::Binary(b) if b.op == BinaryOp::Add));
        assert!(matches!(lower("x not in xs"), Expr::Binary(b) if b.op == BinaryOp::NotIn));
        assert!(matches!(lower("page.title"), Expr::Field(_)));
        assert!(
            matches!(lower("f(a.b, k=c)"), Expr::Call(c) if c.args.len() == 1 && c.kwargs.len() == 1)
        );
        assert!(matches!(lower("x | upper | safe"), Expr::Filter(_)));
        assert!(matches!(lower("width?"), Expr::Optional(_)));
        assert!(matches!(lower("a if c else b"), Expr::Ternary(_)));
        assert!(
            matches!(lower("[1, 2]"), Expr::Literal(ast::Literal::List(l)) if l.elements.len() == 2)
        );
    }
}
