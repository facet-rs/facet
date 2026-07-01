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
use gingembre::{Context, VArray, VObject, VString, Value};
use gingembre::ast::{
    BinaryExpr, BinaryOp, BlockNode, BoolLit, CallExpr, CommentNode, ElifBranch, Expr, ExtendsNode,
    FieldExpr, FilterExpr, FloatLit, ForNode, Ident, IfNode, IncludeNode, IntLit, ListLit, Literal,
    Node, PrintNode, SetNode, SetValue, StringLit, Target, Template, TextNode, span,
};
use snark::{
    grammar::RawGrammarJson,
    lexical::LexicalFacts,
    lower::weavy::{RuntimeWeavyPlan, parse_prepared_weavy_with_report},
    parser::{ParseTable, ParserGrammar, RuntimeResolvedNode},
    validated::ValidatedGrammar,
};

/// First slice: context-free templates (text + interpolation + numeric/bool
/// literals + binary ops), so the render oracle doesn't depend on a data model.
const SAMPLES: &[(&str, &str)] = &[
    ("text", "hello world"),
    ("int add", "{{ 1 + 2 }}"),
    ("precedence", "{{ 1 + 2 * 3 }}"),
    ("nested paren", "{{ (1 + 2) * 3 }}"),
    ("float", "{{ 3.5 }}"),
    ("bool", "{{ true }}"),
    ("string", "{{ \"hello\" }}"),
    ("concat", "{{ \"a\" ~ \"b\" }}"),
    ("comparison", "{{ 5 >= 5 }}"),
    ("logical", "{{ true or false }}"),
    ("filter", "{{ \"hi\" | upper }}"),
    ("comment", "{# c #}A{# d #}B"),
    ("if true", "{% if true %}yes{% endif %}"),
    ("if else", "{% if false %}a{% else %}b{% endif %}"),
    ("if elif", "{% if 1 > 2 %}a{% elif 2 > 1 %}b{% else %}c{% endif %}"),
    ("for list", "{% for x in [1, 2, 3] %}{{ x }};{% endfor %}"),
    ("for else empty", "{% for x in [] %}a{% else %}empty{% endfor %}"),
    ("set", "{% set n = 2 * 3 %}{{ n }}"),
    // Precedence + associativity stress — does snark's grammar prec ladder agree
    // with gingembre's across operator classes?
    ("arith chain", "{{ 1 + 2 * 3 - 4 }}"),
    ("sub left-assoc", "{{ 10 - 2 - 3 }}"),
    ("div left-assoc", "{{ 100 / 10 / 2 }}"),
    ("mul+add mix", "{{ 2 * 3 + 4 * 5 }}"),
    ("add vs eq", "{{ 1 + 1 == 2 }}"),
    ("mul vs cmp", "{{ 2 * 3 > 5 }}"),
    ("cmp and cmp", "{{ 1 < 2 and 3 < 4 }}"),
    ("and vs or", "{{ true and false or true }}"),
    ("add vs cmp", "{{ 1 + 2 < 2 + 2 }}"),
    ("concat chain", "{{ \"a\" ~ \"b\" ~ \"c\" }}"),
    ("pow assoc", "{{ 2 ** 3 ** 2 }}"),
    // Filter-with-args (the grammar tweak): `f(args)` after `|` binds to the filter.
    ("filter args join", "{{ [1, 2, 3] | join(\"-\") }}"),
    ("filter args default", "{{ \"x\" | default(\"y\") }}"),
    ("filter chain args", "{{ [3, 1, 2] | sort | join(\",\") }}"),
    // Data-driven: read `name`, `user`, `items` from a shared facet Context.
    ("var", "{{ name }}"),
    ("field", "{{ user.name }}"),
    ("field in if", "{% if user.active %}on{% else %}off{% endif %}"),
    ("for data", "{% for i in items %}{{ i }};{% endfor %}"),
    ("field concat", "{{ user.name ~ \"!\" }}"),
    ("var filter", "{{ name | upper }}"),
    ("field filter", "{{ user.name | length }}"),
];

fn main() {
    // Repro mode: `gingembre-snark-spike <grammar.js> <input>` dumps snark's
    // resolved tree for one grammar + input (for comparing against tree-sitter).
    let args: Vec<String> = env::args().collect();
    if let (Some(grammar), Some(input)) = (args.get(1), args.get(2)) {
        repro(grammar, input);
        return;
    }

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

    let ctx = build_context();
    let mut pass = 0usize;
    let mut fail = 0usize;
    for (label, src) in SAMPLES {
        let report =
            match parse_prepared_weavy_with_report(&plan, &validated, &parser, &table, src) {
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

        let snark_out = render(lower_template(&resolved), src, &ctx);
        let native_out = render(gingembre::parse_template_recovered(src), src, &ctx);

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

fn repro(grammar_path: &str, input: &str) {
    let grammar_json =
        snark_dsl::emit_with_boa(std::path::Path::new(grammar_path)).expect("emit");
    let raw = RawGrammarJson::from_tree_sitter_json_str(&grammar_json).expect("import");
    let validated = ValidatedGrammar::from_raw(&raw).expect("validate");
    let lexical = LexicalFacts::from_grammar(&validated);
    let normalized =
        ParserGrammar::normalize_from_validated(&validated, &lexical).expect("normalize");
    let parser = normalized
        .prepare_productions_for_items()
        .expect("prepare");
    let table = ParseTable::from_grammar(&parser).expect("table");
    let plan = RuntimeWeavyPlan::new(&validated, &parser, &table).expect("plan");
    match parse_prepared_weavy_with_report(&plan, &validated, &parser, &table, input) {
        Ok(report) => match report.accepted_resolved_tree(&parser, input) {
            Some(tree) => dump(&tree, 0),
            None => println!("(no resolved tree)"),
        },
        Err(e) => println!("parse error: {e:?}"),
    }
}

fn render(ast: Template, src: &str, ctx: &Context) -> Result<String, String> {
    let template = gingembre::Template::from_ast(ast, "spike", src);
    block_on(template.render(ctx)).map_err(|e| format!("{e:?}"))
}

/// Shared facet-backed context for the data-driven templates. Context-free
/// templates ignore it; data-driven ones read `name`, `user`, `items`.
fn build_context() -> Context {
    let mut user = VObject::new();
    user.insert(VString::from("name"), Value::from("Ada"));
    user.insert(VString::from("active"), Value::from(true));
    let items = VArray::from_iter([Value::from(1i64), Value::from(2i64), Value::from(3i64)]);
    let mut ctx = Context::new();
    ctx.set("name", "Ada");
    ctx.set("user", Value::from(user));
    ctx.set("items", Value::from(items));
    ctx
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
        "interpolation" => Some(Node::Print(PrintNode {
            expr: node.children().iter().find_map(lower_expr)?,
            span: span(0, 0),
        })),
        "comment" => Some(Node::Comment(CommentNode {
            text: String::new(),
            span: span(0, 0),
        })),
        "if_statement" => lower_if(node),
        "for_statement" => lower_for(node),
        "set_statement" => Some(Node::Set(SetNode {
            name: named_child(node, "identifier").and_then(ident)?,
            value: SetValue::Expr(node.children().iter().find_map(lower_expr)?),
            span: span(0, 0),
        })),
        "block_statement" => Some(Node::Block(BlockNode {
            name: named_child(node, "identifier").and_then(ident)?,
            body: find_body(node),
            span: span(0, 0),
        })),
        "extends_statement" => Some(Node::Extends(ExtendsNode {
            path: named_child(node, "literal").and_then(string_value)?,
            span: span(0, 0),
        })),
        "include_statement" => Some(Node::Include(IncludeNode {
            path: named_child(node, "literal").and_then(string_value)?,
            context: None,
            span: span(0, 0),
        })),
        _ => None,
    }
}

fn lower_if(node: &RuntimeResolvedNode) -> Option<Node> {
    let condition = node.children().iter().find_map(lower_expr)?;
    let mut elif_branches = Vec::new();
    let mut else_body = None;
    for child in node.children() {
        match child.kind() {
            "elif_clause" => elif_branches.push(ElifBranch {
                condition: child.children().iter().find_map(lower_expr)?,
                body: find_body(child),
                span: span(0, 0),
            }),
            "else_clause" => else_body = Some(find_body(child)),
            _ => {}
        }
    }
    Some(Node::If(IfNode {
        condition,
        then_body: find_body(node),
        elif_branches,
        else_body,
        span: span(0, 0),
    }))
}

fn lower_for(node: &RuntimeResolvedNode) -> Option<Node> {
    let idents: Vec<&RuntimeResolvedNode> = node
        .children()
        .iter()
        .filter(|c| c.kind() == "identifier")
        .collect();
    let target = match idents.as_slice() {
        [one] => Target::Single {
            name: leaf_text(one)?,
            span: span(0, 0),
        },
        many => Target::Tuple {
            names: many.iter().filter_map(|i| Some((leaf_text(i)?, span(0, 0)))).collect(),
            span: span(0, 0),
        },
    };
    // The iterable is the first lowerable expression that isn't a bare loop ident.
    let iter = node
        .children()
        .iter()
        .filter(|c| c.kind() != "identifier")
        .find_map(lower_expr)?;
    let else_body = node
        .children()
        .iter()
        .find(|c| c.kind() == "else_clause")
        .map(find_body);
    Some(Node::For(ForNode {
        target,
        iter,
        body: find_body(node),
        else_body,
        span: span(0, 0),
    }))
}

fn lower_expr(node: &RuntimeResolvedNode) -> Option<Expr> {
    match node.kind() {
        "literal" => lower_literal(node).map(Expr::Literal),
        "list" => Some(Expr::Literal(Literal::List(ListLit {
            elements: node.children().iter().filter_map(lower_expr).collect(),
            span: span(0, 0),
        }))),
        "binary" => lower_binary(node),
        // A parenthesized expression is just its inner expression.
        "paren" => node.children().iter().find_map(lower_expr),
        "variable" => Some(Expr::Var(Ident {
            name: leaf_text(node)?,
            span: span(0, 0),
        })),
        "field" => Some(Expr::Field(FieldExpr {
            base: Box::new(node.children().iter().find_map(lower_expr)?),
            field: named_child(node, "identifier").and_then(ident)?,
            span: span(0, 0),
        })),
        "filter" => {
            let (args, kwargs) = lower_args(node);
            Some(Expr::Filter(FilterExpr {
                expr: Box::new(node.children().iter().find_map(lower_expr)?),
                filter: named_child(node, "identifier").and_then(ident)?,
                args,
                kwargs,
                span: span(0, 0),
            }))
        }
        "call" => {
            let (args, kwargs) = lower_args(node);
            let callee = node
                .children()
                .iter()
                .find(|c| c.named() && c.kind() != "arg_list")?;
            // snark parses `x | f(args)` as `call(filter(x, f), args)`, but
            // gingembre's AST wants `Filter { expr, filter, args }`. Turn it
            // inside out: hoist the call's args into the filter. (The clean fix
            // lives in the grammar — a real precedence puzzle, see notes — this
            // is the lowering doing it instead.)
            if callee.kind() == "filter" {
                Some(Expr::Filter(FilterExpr {
                    expr: Box::new(callee.children().iter().find_map(lower_expr)?),
                    filter: named_child(callee, "identifier").and_then(ident)?,
                    args,
                    kwargs,
                    span: span(0, 0),
                }))
            } else {
                Some(Expr::Call(CallExpr {
                    func: Box::new(lower_expr(callee)?),
                    args,
                    kwargs,
                    span: span(0, 0),
                }))
            }
        }
        _ => None,
    }
}

/// Extract `(positional args, kwargs)` from the `arg_list` child of `node`.
fn lower_args(node: &RuntimeResolvedNode) -> (Vec<Expr>, Vec<(Ident, Expr)>) {
    let mut args = Vec::new();
    let mut kwargs = Vec::new();
    let Some(arg_list) = named_child(node, "arg_list") else {
        return (args, kwargs);
    };
    for child in arg_list.children() {
        match child.kind() {
            "argument" => {
                if let Some(expr) = child.children().iter().find_map(lower_expr) {
                    args.push(expr);
                }
            }
            "kwarg" => {
                if let (Some(name), Some(value)) = (
                    named_child(child, "identifier").and_then(ident),
                    child.children().iter().find_map(lower_expr),
                ) {
                    kwargs.push((name, value));
                }
            }
            _ => {}
        }
    }
    (args, kwargs)
}

fn named_child<'a>(node: &'a RuntimeResolvedNode, kind: &str) -> Option<&'a RuntimeResolvedNode> {
    node.children().iter().find(|c| c.kind() == kind)
}

fn ident(node: &RuntimeResolvedNode) -> Option<Ident> {
    Some(Ident {
        name: leaf_text(node)?,
        span: span(0, 0),
    })
}

fn string_value(node: &RuntimeResolvedNode) -> Option<StringLit> {
    Some(StringLit {
        value: full_text(node)
            .trim_matches(|c| c == '"' || c == '\'')
            .to_string(),
        span: span(0, 0),
    })
}

fn find_body(node: &RuntimeResolvedNode) -> Vec<Node> {
    node.children()
        .iter()
        .find(|c| c.kind() == "body")
        .map(|b| b.children().iter().filter_map(lower_node).collect())
        .unwrap_or_default()
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
