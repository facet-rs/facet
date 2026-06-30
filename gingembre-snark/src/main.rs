//! Structural fitness probe: parse representative gingembre templates with the real
//! snark RuntimeParser and print the resolved s-expression, so we can eyeball whether
//! the grammar's tree structure is rich enough to map onto `gingembre::ast::Template`
//! — independent of (and prior to) solving leaf-text extraction.

use std::{env, path::PathBuf};

use snark::{
    grammar::RawGrammarJson,
    lexical::LexicalFacts,
    parser::{ParseTable, ParserGrammar, RuntimeParser},
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
    let runtime = RuntimeParser::new(&validated, &parser, &table).expect("runtime");

    // One template per AST node kind we care about, smallest form that exercises it.
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
        match runtime.parse_compact_with_report(src) {
            Ok(report) => {
                let sexp = report.tree().to_sexp();
                let flag = if sexp.contains("ERROR") || sexp.contains("MISSING") {
                    "  ⚠ contains ERROR/MISSING"
                } else {
                    "  clean"
                };
                println!("{flag}\n  {sexp}\n");
            }
            Err(e) => println!("  PARSE ERROR: {e:?}\n"),
        }
    }
}
