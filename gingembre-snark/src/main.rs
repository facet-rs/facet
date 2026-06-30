//! gingembre-snark: parse gingembre with snark's **Weavy** runtime and inspect the
//! resolved CST (`RuntimeResolvedNode`) — kinds, fields, ranges, and terminal text
//! including anonymous operators and `{%- -%}` trim markers.
//!
//! This is the input surface for lowering to `gingembre::ast::Template`. The point of
//! this dump is to confirm the resolved tree carries everything the lowering needs
//! (notably the anonymous-terminal text the bare s-expression dropped).

use std::{env, path::PathBuf};

use snark::{
    grammar::RawGrammarJson,
    lexical::LexicalFacts,
    lower::weavy::{RuntimeWeavyPlan, parse_prepared_runtime_with_report},
    parser::{ParseTable, ParserGrammar, RuntimeResolvedNode},
    validated::ValidatedGrammar,
};

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

    let samples: &[(&str, &str)] = &[
        ("text", "hello world"),
        ("print + field + filter", "{{ user.name | upper }}"),
        ("print + arithmetic", "{{ 1 + 2 * 3 }}"),
        ("filter with args", "{{ value | truncate(10, true) }}"),
        ("if / elif / else", "{% if a %}x{% elif b %}y{% else %}z{% endif %}"),
        ("for", "{% for item in items %}{{ item }}{% endfor %}"),
        ("for tuple", "{% for k, v in map %}{{ k }}={{ v }}{% endfor %}"),
        (
            "extends + block",
            "{% extends \"base.html\" %}{% block content %}hi{% endblock %}",
        ),
        ("set", "{% set n = 1 + 2 %}{{ n }}"),
        ("comment", "{# a comment #}"),
        ("include", "{% include \"foo.html\" %}"),
        (
            "macro",
            "{% macro greet(name) %}hi {{ name }}{% endmacro %}",
        ),
        ("whitespace control", "{%- if a -%}x{%- endif -%}"),
    ];

    for (label, src) in samples {
        println!("### {label}\n  src: {src:?}");
        match parse_prepared_runtime_with_report(&plan, &validated, &parser, &table, src) {
            Ok(report) => match report.accepted_resolved_tree(&parser, src) {
                Some(tree) => dump(&tree, 1),
                None => println!("  (no accepted resolved tree)"),
            },
            Err(e) => println!("  PARSE ERROR: {e:?}"),
        }
        println!();
    }
}

/// Print the resolved tree: `field: kind (anon) "text"` per node, indented by depth.
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
