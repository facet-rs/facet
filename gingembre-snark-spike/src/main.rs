//! ============================================================================
//!  S P I K E  —  T H R O W A W A Y.  Do not build on this. Do not let it live.
//! ============================================================================
//! Goal: answer "can snark be gingembre's front end?" Nothing more.
//!
//! It parses gingembre with snark's Weavy runtime, hand-lowers the resolved CST
//! (`RuntimeResolvedNode`) into `gingembre::ast::Template`, renders it through
//! gingembre's REAL evaluator, and diffs the output against the native
//! `cst_lower` path (`gingembre::parse_template_recovered`). Byte-identical
//! render = the surface carries the semantics.
//!
//! The hand-lowering below is the disposable part. The real version generates
//! typed views from the grammar; the keeper here is the ANSWER and this oracle
//! harness. When the answer lands, delete the crate.

use std::{env, path::PathBuf};

use futures::executor::block_on;
use gingembre::Context;
use gingembre::ast::{
    BinaryExpr, BinaryOp, BoolLit, Expr, FloatLit, Ident, IntLit, Literal, Node, PrintNode,
    StringLit, Template, TextNode, span,
};
use snark::{
    grammar::RawGrammarJson,
    lexical::LexicalFacts,
    lower::weavy::{RuntimeWeavyPlan, parse_prepared_runtime_with_report},
    parser::{ParseTable, ParserGrammar, RuntimeResolvedNode},
    validated::ValidatedGrammar,
};

/// First slice: context-free templates (text + interpolation + numeric/bool
/// literals + binary ops), so the render oracle doesn't depend on a data model.
const SAMPLES: &[(&str, &str)] = &[
    ("text", "hello world"),
    ("int add", "{{ 1 + 2 }}"),
    ("text+interp", "a {{ 2 * 3 }} b"),
    ("precedence", "{{ 1 + 2 * 3 }}"),
    ("sub", "{{ 10 - 4 }}"),
    ("nested", "{{ (1 + 2) * 3 }}"),
    ("float", "{{ 3.5 }}"),
    ("bool", "{{ true }}"),
];

fn main() {
    let repo = env::var_os("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .and_then(|p| p.parent().map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."));
    let grammar_js = repo.join("playgrounds/snark/src/bundled/gingembre/grammar.js");

    let grammar_json = snark_dsl::emit_with_boa(&grammar_js).expect("emit grammar.js -> json");
    let raw = RawGrammarJson::from_tree_sitter_json_str(&grammar_json).expect("import json");
    let validated = ValidatedGrammar::from_raw(&raw).expect("validate");
    let lexical = LexicalFacts::from_grammar(&validated);
    let normalized =
        ParserGrammar::normalize_from_validated(&validated, &lexical).expect("normalize");
    let parser = normalized
        .prepare_productions_for_items()
        .expect("prepare productions");
    let table = ParseTable::from_grammar(&parser).expect("build parse table");
    let plan = RuntimeWeavyPlan::new(&validated, &parser, &table).expect("weavy plan");

    let mut pass = 0usize;
    let mut fail = 0usize;
    for (label, src) in SAMPLES {
        let report =
            match parse_prepared_runtime_with_report(&plan, &validated, &parser, &table, src) {
                Ok(report) => report,
                Err(e) => {
                    println!("✗ {label}: snark parse error: {e:?}");
                    fail += 1;
                    continue;
                }
            };
        let Some(resolved) = report.accepted_resolved_tree(&parser, src) else {
            println!("✗ {label}: no accepted resolved tree");
            fail += 1;
            continue;
        };

        let snark_out = render(lower_template(&resolved), src);
        let native_out = render(gingembre::parse_template_recovered(src), src);

        if snark_out == native_out {
            println!("✓ {label}: {snark_out:?}");
            pass += 1;
        } else {
            println!("✗ {label}: snark={snark_out:?}  native={native_out:?}");
            dump(&resolved, 2);
            fail += 1;
        }
    }
    println!("\n{pass} pass / {fail} fail");
}

fn render(ast: Template, src: &str) -> Result<String, String> {
    let template = gingembre::Template::from_ast(ast, "spike", src);
    block_on(template.render(&Context::new())).map_err(|e| format!("{e:?}"))
}

// ---------------------------------------------------------------------------
// Hand-lowering: RuntimeResolvedNode -> gingembre::ast (THROWAWAY).
// ---------------------------------------------------------------------------

fn lower_template(node: &RuntimeResolvedNode) -> Template {
    let body = node.children().iter().filter_map(lower_node).collect();
    Template {
        body,
        span: span(0, 0),
    }
}

fn lower_node(node: &RuntimeResolvedNode) -> Option<Node> {
    match node.kind() {
        "text" => Some(Node::Text(TextNode {
            text: full_text(node),
            span: span(0, 0),
        })),
        "interpolation" => {
            let expr = node.children().iter().find_map(lower_expr)?;
            Some(Node::Print(PrintNode {
                expr,
                span: span(0, 0),
            }))
        }
        _ => None,
    }
}

fn lower_expr(node: &RuntimeResolvedNode) -> Option<Expr> {
    match node.kind() {
        "literal" => lower_literal(node).map(Expr::Literal),
        "binary" => lower_binary(node),
        // A parenthesized expression is just its inner expression.
        "paren" => node.children().iter().find_map(lower_expr),
        "variable" => Some(Expr::Var(Ident {
            name: leaf_text(node)?,
            span: span(0, 0),
        })),
        _ => None,
    }
}

fn lower_literal(node: &RuntimeResolvedNode) -> Option<Literal> {
    let inner = node.children().iter().find(|c| c.named())?;
    match inner.kind() {
        "number" => {
            let text = leaf_text(inner)?;
            if text.contains('.') {
                Some(Literal::Float(FloatLit {
                    value: text.parse().ok()?,
                    span: span(0, 0),
                }))
            } else {
                Some(Literal::Int(IntLit {
                    value: text.parse().ok()?,
                    span: span(0, 0),
                }))
            }
        }
        "boolean" => Some(Literal::Bool(BoolLit {
            value: leaf_text(inner)? == "true",
            span: span(0, 0),
        })),
        "string" => Some(Literal::String(StringLit {
            value: full_text(inner)
                .trim_matches(|c| c == '"' || c == '\'')
                .to_string(),
            span: span(0, 0),
        })),
        _ => None,
    }
}

fn lower_binary(node: &RuntimeResolvedNode) -> Option<Expr> {
    let op_text = node
        .children()
        .iter()
        .find(|c| !c.named())
        .and_then(|c| c.text())?;
    let op = binary_op(op_text)?;
    let mut operands = node.children().iter().filter(|c| c.named());
    let left = lower_expr(operands.next()?)?;
    let right = lower_expr(operands.next()?)?;
    Some(Expr::Binary(BinaryExpr {
        left: Box::new(left),
        op,
        right: Box::new(right),
        span: span(0, 0),
    }))
}

fn binary_op(text: &str) -> Option<BinaryOp> {
    Some(match text {
        "+" => BinaryOp::Add,
        "-" => BinaryOp::Sub,
        "*" => BinaryOp::Mul,
        "/" => BinaryOp::Div,
        "%" => BinaryOp::Mod,
        "//" => BinaryOp::FloorDiv,
        "**" => BinaryOp::Pow,
        "==" => BinaryOp::Eq,
        "!=" => BinaryOp::Ne,
        "<" => BinaryOp::Lt,
        "<=" => BinaryOp::Le,
        ">" => BinaryOp::Gt,
        ">=" => BinaryOp::Ge,
        "and" => BinaryOp::And,
        "or" => BinaryOp::Or,
        "~" => BinaryOp::Concat,
        _ => return None,
    })
}

/// Concatenate every terminal's source text under this node.
fn full_text(node: &RuntimeResolvedNode) -> String {
    if let Some(text) = node.text() {
        return text.to_string();
    }
    node.children().iter().map(full_text).collect()
}

/// First terminal text under this node (for single-token leaves).
fn leaf_text(node: &RuntimeResolvedNode) -> Option<String> {
    if let Some(text) = node.text() {
        return Some(text.to_string());
    }
    node.children().iter().find_map(leaf_text)
}

/// Debug dump of the resolved tree (only printed on an oracle mismatch).
fn dump(node: &RuntimeResolvedNode, depth: usize) {
    let indent = "  ".repeat(depth);
    let field = node.field().map(|f| format!("{f}: ")).unwrap_or_default();
    let anon = if node.named() { "" } else { " (anon)" };
    let text = node.text().map(|t| format!("  {t:?}")).unwrap_or_default();
    println!("{indent}{field}{}{anon}{text}", node.kind());
    for child in node.children() {
        dump(child, depth + 1);
    }
}
