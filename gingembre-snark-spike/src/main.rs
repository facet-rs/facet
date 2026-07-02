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

use std::{collections::BTreeMap, env, path::PathBuf};

use futures::executor::block_on;
use gingembre::ast::{
    BinaryExpr, BinaryOp, BlockNode, BoolLit, CallExpr, CommentNode, ElifBranch, Expr, ExtendsNode,
    FieldExpr, FilterExpr, FloatLit, ForNode, Ident, IfNode, IncludeNode, IntLit, ListLit, Literal,
    Node, PrintNode, SetNode, SetValue, StringLit, Target, Template, TextNode, span,
};
use gingembre::{Context, VArray, VObject, VString, Value};
use snark::{
    grammar::RawGrammarJson,
    lexical::LexicalFacts,
    lower::weavy::{WeavyParsePlan, parse_prepared_weavy_with_report},
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
    (
        "if elif",
        "{% if 1 > 2 %}a{% elif 2 > 1 %}b{% else %}c{% endif %}",
    ),
    ("for list", "{% for x in [1, 2, 3] %}{{ x }};{% endfor %}"),
    (
        "for else empty",
        "{% for x in [] %}a{% else %}empty{% endfor %}",
    ),
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
    (
        "field in if",
        "{% if user.active %}on{% else %}off{% endif %}",
    ),
    ("for data", "{% for i in items %}{{ i }};{% endfor %}"),
    ("field concat", "{{ user.name ~ \"!\" }}"),
    ("var filter", "{{ name | upper }}"),
    ("field filter", "{{ user.name | length }}"),
];

fn main() {
    // Repro mode: `gingembre-snark-spike <grammar.js> <input>` dumps snark's
    // resolved tree for one grammar + input (for comparing against tree-sitter).
    let args: Vec<String> = env::args().collect();
    if args.get(1).map(|s| s == "--ast").unwrap_or(false) {
        ast_proof();
        return;
    }
    if args.get(1).map(|s| s == "--corpus").unwrap_or(false) {
        corpus(
            args.get(2)
                .map(String::as_str)
                .unwrap_or("/Users/amos/bearcove/fasterthanli.me/templates"),
        );
        return;
    }
    if args.get(1).map(|s| s == "--perf").unwrap_or(false) {
        perf();
        return;
    }
    if args.get(1).map(|s| s == "--diag").unwrap_or(false) {
        diag();
        return;
    }
    if args.get(1).map(|s| s == "--eval").unwrap_or(false) {
        eval_weavy_proof();
        return;
    }
    if args.get(1).map(|s| s == "--specialize").unwrap_or(false) {
        specialize();
        return;
    }
    if args.get(1).map(|s| s == "--speculate").unwrap_or(false) {
        speculate();
        return;
    }
    if args.get(1).map(|s| s == "--fuse").unwrap_or(false) {
        fuse();
        return;
    }
    if args.get(1).map(|s| s == "--ic").unwrap_or(false) {
        inline_cache();
        return;
    }
    if args.get(1).map(|s| s == "--jitmap").unwrap_or(false) {
        jitmap();
        return;
    }
    if args.get(1).map(|s| s == "--hot").unwrap_or(false) {
        let secs = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(5);
        hot(secs);
        return;
    }
    if args.get(1).map(|s| s == "--serialize").unwrap_or(false) {
        serialize();
        return;
    }
    if args.get(1).map(|s| s == "--profile").unwrap_or(false) {
        profile();
        return;
    }
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
    let plan = WeavyParsePlan::new(&validated, &parser, &table).expect("weavy plan");

    let ctx = build_context();
    let mut pass = 0usize;
    let mut fail = 0usize;
    for (label, src) in SAMPLES {
        let report = match parse_prepared_weavy_with_report(&plan, &parser, &table, src) {
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

/// Corpus mode: parse every *.jinja under `dir` with snark's gingembre grammar and
/// bucket by clean-parse / no-tree / hard-fail. First-cut GRAMMAR coverage of the real
/// ftl templates (the render/lowering oracle is a separate, harder pass that needs the
/// site's data + template loader).
fn corpus(dir: &str) {
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
    let plan = WeavyParsePlan::new(&validated, &parser, &table).expect("weavy plan");

    let mut files = Vec::new();
    collect_jinja(std::path::Path::new(dir), &mut files);
    files.sort();
    let (mut clean, mut notree, mut failed) = (0usize, 0usize, 0usize);
    for f in &files {
        let src = std::fs::read_to_string(f).unwrap_or_default();
        let name = f
            .strip_prefix(dir)
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| f.display().to_string());
        match parse_prepared_weavy_with_report(&plan, &parser, &table, &src) {
            Ok(report) => match report.accepted_resolved_tree(&parser, &src) {
                Some(_) => clean += 1,
                None => {
                    println!("~ {name}: parsed, no accepted tree");
                    notree += 1;
                }
            },
            Err(e) => {
                let msg = format!("{e:?}");
                println!("✗ {name} ({} B): {}", src.len(), &msg[..msg.len().min(160)]);
                failed += 1;
            }
        }
    }
    println!("\n{clean} clean / {notree} no-tree / {failed} failed   (of {} templates)", files.len());
}

fn collect_jinja(dir: &std::path::Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_jinja(&path, out);
        } else if path.extension().is_some_and(|e| e == "jinja") {
            out.push(path);
        }
    }
}

// The gingembre expression AST — FULLY GENERATED from the grammar + `ast()` annotations by
// build.rs, written to `$OUT_DIR/gingembre_ast.rs`, and included here. Nobody hand-writes
// these: `enum Expr { Binary(Box<Binary>), Number(i64), Variable(String) }` and
// `struct Binary { left: Expr, op: String, right: Expr }` come out of the grammar rules
// (2 `_expr` operands + an operator token) with names/decodes from the annotation file.
/// The generated AST lives in its own module so its `Expr` doesn't collide with
/// `gingembre::ast::Expr` (which the hand-lowering oracle path still uses).
pub mod gen_ast {
    include!(concat!(env!("OUT_DIR"), "/gingembre_ast.rs"));
}

/// The generated AST source, for showing what came out of the grammar.
const GENERATED_AST_SRC: &str = include_str!(concat!(env!("OUT_DIR"), "/gingembre_ast.rs"));

/// The grammar's AST annotations — what the `ast({...})` DSL helper records, keyed by node
/// kind. In the real thing these live inline in grammar.js; here they're loaded through the
/// same DSL channel (`snark_dsl::annotations_from_source`) and decoded with facet-json.
type Annotations = BTreeMap<String, NodeAnn>;

#[derive(facet::Facet, Default, Debug)]
struct NodeAnn {
    /// Enum variant this node maps to (`as` in the DSL).
    #[facet(rename = "as", default)]
    as_variant: Option<String>,
    /// The generated enum name (only on the `_expr` supertype entry).
    #[facet(rename = "enum", default)]
    enum_name: Option<String>,
    /// Structural noise — descend to the inner named child before mapping.
    #[facet(default)]
    transparent: bool,
    /// Generated struct name when the variant's payload is a struct.
    #[facet(rename = "struct", default)]
    struct_name: Option<String>,
    /// Scalar Rust type when the variant's payload is a leaf (`i64` | `String`).
    #[facet(default)]
    scalar: Option<String>,
    /// Struct field name -> child selector. The field TYPE is grammar-derived, not here.
    #[facet(default)]
    fields: BTreeMap<String, FieldAnn>,
}

#[derive(facet::Facet, Default, Debug)]
struct FieldAnn {
    /// Child selector (`named:N` | `token`).
    from: String,
}

/// AST enrichment for the gingembre expression nodes, in the `ast()` DSL — the SAME file
/// build.rs consumes to codegen the AST. Loaded through the DSL channel here to drive the
/// reflection/Weavy builder, keeping one source of truth.
const ANN_SRC: &str = include_str!("../gingembre_ast.snark.js");

/// PROOF: ONE generic reflection builder dispatching on the target facet `Shape`, driven
/// from snark's resolved tree — enum→variant, struct→fields, scalar→set. No facet-format.
/// The gingembre grammar has NO field labels, so the field/variant mapping comes from
/// `hint_*` (the stand-in for grammar annotations) — which is the whole point: annotations
/// are load-bearing, not sugar.
fn ast_proof() {
    // The generated AST types (shadow `gingembre::ast::Expr` within this fn only).
    use gen_ast::{Binary, Expr};

    let repo = env::var_os("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .and_then(|p| p.parent().map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."));
    let grammar_js = repo.join("playgrounds/snark/src/bundled/gingembre/grammar.js");
    let grammar_json = snark_dsl::emit_with_boa(&grammar_js).expect("emit grammar");
    let raw = RawGrammarJson::from_tree_sitter_json_str(&grammar_json).expect("import json");
    let validated = ValidatedGrammar::from_raw(&raw).expect("validate");
    let lexical = LexicalFacts::from_grammar(&validated);
    let normalized =
        ParserGrammar::normalize_from_validated(&validated, &lexical).expect("normalize");
    let parser = normalized.prepare_productions_for_items().expect("prepare");
    let table = ParseTable::from_grammar(&parser).expect("table");
    let plan = WeavyParsePlan::new(&validated, &parser, &table).expect("plan");

    // Load the grammar's AST annotations through the DSL channel, then decode them into a
    // Facet type via the WEAVY deserializer (dogfooding — no facet-format).
    let ann_json = snark_dsl::annotations_from_source(ANN_SRC, "gingembre.ast.js")
        .expect("emit annotations");
    let anns: Annotations = facet_json::from_str(&ann_json).expect("decode annotations");

    // Item 2: the AST types Expr/Binary are GENERATED by build.rs from the grammar + the same
    // annotation file, written to $OUT_DIR/gingembre_ast.rs, and `include!`d above — this is
    // that file's contents (nobody hand-wrote them, nobody copied stdout into a .rs).
    println!("--- $OUT_DIR/gingembre_ast.rs (generated by build.rs from the grammar) ---");
    print!("{GENERATED_AST_SRC}");
    println!("--- end ---");
    let _ = &anns; // annotations drive the builder below

    for src in ["{{ 1 + 2 }}", "{{ 1 + 2 * 3 }}", "{{ x }}"] {
        let report = parse_prepared_weavy_with_report(&plan, &parser, &table, src).expect("parse");
        let resolved = report
            .accepted_resolved_tree(&parser, src)
            .expect("resolved tree");
        let expr_node = find_expr(&resolved).expect("no expr under interpolation");

        let partial = facet_reflect::Partial::alloc_owned::<Expr>().expect("alloc");
        let mut ops = Vec::new();
        let value: Expr = build(partial, expr_node, &anns, &mut ops)
            .expect("build via reflection")
            .build()
            .expect("finalize")
            .materialize::<Expr>()
            .expect("materialize");
        println!("snark {src:<16?} -> {value:?}");
    }

    // Oracle: the precedence one must nest right (`*` binds tighter than `+`).
    let report = parse_prepared_weavy_with_report(&plan, &parser, &table, "{{ 1 + 2 * 3 }}").unwrap();
    let resolved = report.accepted_resolved_tree(&parser, "{{ 1 + 2 * 3 }}").unwrap();
    let mut ops = Vec::new();
    let value: Expr = build(
        facet_reflect::Partial::alloc_owned::<Expr>().unwrap(),
        find_expr(&resolved).unwrap(),
        &anns,
        &mut ops,
    )
    .unwrap()
    .build()
    .unwrap()
    .materialize::<Expr>()
    .unwrap();
    let expected = Expr::Binary(Box::new(Binary {
        left: Expr::Number(1),
        op: "+".into(),
        right: Expr::Binary(Box::new(Binary {
            left: Expr::Number(2),
            op: "*".into(),
            right: Expr::Number(3),
        })),
    }));
    assert_eq!(value, expected);
    println!("✓ generic Shape-driven reflection builder: grammar surface -> nested #[derive(Facet)] AST");

    // Item 3: run the SAME build ops as a Weavy program through the weavy interpreter.
    let via_weavy: Expr = run_ops_via_weavy(&ops);
    assert_eq!(via_weavy, expected, "weavy-run ops must reproduce the reflected AST");
    println!(
        "✓ item 3a: {} build ops as a Weavy program, run through weavy::run (interpreter) -> identical AST",
        ops.len()
    );

    // Item 3b: JIT the SAME op program via a weavy HOSTCALL chain and materialize again.
    // (NB: hostcall chain — op bodies stay interpreted Rust; see build_intop_native/--specialize
    // for REAL copy-and-patch with dedicated per-op stencils.)
    let via_jit: Expr = run_ops_via_weavy_jit(&ops);
    assert_eq!(via_jit, expected, "JIT-compiled ops must reproduce the reflected AST");
    println!(
        "✓ item 3b: same {} ops HOSTCALL-chain JIT-compiled (native={}) -> identical AST",
        ops.len(),
        weavy::jit::NATIVE_COPY_PATCH_AVAILABLE,
    );

    // Item 4: implement gingembre SEMANTICS on the fully generated AST. For each expression:
    // parse with snark -> JIT-materialize gen_ast::Expr -> lower into gingembre's AST ->
    // render through gingembre's REAL evaluator. Oracle: byte-identical to gingembre's own
    // parse+render of the same source. If they match, the generated AST carries the semantics.
    let mut ctx = Context::new();
    ctx.set("x", Value::from(42i64));
    let cases = [
        "{{ 1 + 2 }}",
        "{{ 1 + 2 * 3 }}",
        "{{ 10 - 2 - 3 }}",
        "{{ 100 / 10 / 2 }}",
        "{{ 2 * 3 + 4 * 5 }}",
        "{{ 2 * 3 > 5 }}",
        "{{ x }}",
        "{{ x + 1 }}",
    ];
    for src in cases {
        let report = parse_prepared_weavy_with_report(&plan, &parser, &table, src).expect("parse");
        let resolved = report.accepted_resolved_tree(&parser, src).expect("resolved tree");
        let node = find_expr(&resolved).expect("no expr under interpolation");

        // Grammar surface -> generated AST (via JIT) -> gingembre AST -> single-print template.
        let generated: Expr = expr_via_jit(node, &anns);
        let lowered = gen_expr_to_gingembre(&generated);
        let template = Template {
            body: vec![Node::Print(PrintNode {
                expr: lowered,
                span: span(0, 0),
            })],
            span: span(0, 0),
        };

        let via_generated = render(template, src, &ctx);
        let native = render(gingembre::parse_template_recovered(src), src, &ctx);
        assert_eq!(
            via_generated, native,
            "generated-AST render must match gingembre's own for {src:?}"
        );
        println!("✓ item 4 {src:<20} -> {via_generated:?} (== gingembre native)");
    }
    println!("✓ item 4: gingembre semantics implemented on the FULLY GENERATED AST (oracle-matched)");
}

/// Build the snark parse plan once (shared by perf/diag).
fn build_plan() -> (ParserGrammar, ParseTable, WeavyParsePlan, Annotations) {
    let repo = env::var_os("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .and_then(|p| p.parent().map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."));
    let grammar_js = repo.join("playgrounds/snark/src/bundled/gingembre/grammar.js");
    let grammar_json = snark_dsl::emit_with_boa(&grammar_js).expect("emit grammar");
    let raw = RawGrammarJson::from_tree_sitter_json_str(&grammar_json).expect("import json");
    let validated = ValidatedGrammar::from_raw(&raw).expect("validate");
    let lexical = LexicalFacts::from_grammar(&validated);
    let normalized =
        ParserGrammar::normalize_from_validated(&validated, &lexical).expect("normalize");
    let parser = normalized.prepare_productions_for_items().expect("prepare");
    let table = ParseTable::from_grammar(&parser).expect("table");
    let plan = WeavyParsePlan::new(&validated, &parser, &table).expect("plan");
    let ann_json = snark_dsl::annotations_from_source(ANN_SRC, "gingembre.ast.js").expect("ann");
    let anns: Annotations = facet_json::from_str(&ann_json).expect("decode ann");
    (parser, table, plan, anns)
}

/// PERF: front-end parse (snark-CST vs gingembre native) and materialize (reflection vs
/// weavy-interp vs weavy-JIT). Coarse wall-clock — the real profile is stax; this is a
/// first honest sense of magnitude.
fn perf() {
    use std::time::Instant;

    let setup = Instant::now();
    let (parser, table, plan, anns) = build_plan();
    let setup_us = setup.elapsed().as_micros();
    println!("snark plan setup (one-time): {setup_us} us\n");

    // ---- parse throughput: snark resolved CST vs gingembre native AST ----
    let cases = ["{{ 1 + 2 * 3 }}", "{% if user.active %}on{% endif %}", "{{ x | upper }}"];
    let n = 20_000u32;
    println!("parse (avg over {n} iters):        snark-CST      gingembre-native");
    for src in cases {
        let t = Instant::now();
        for _ in 0..n {
            let report = parse_prepared_weavy_with_report(&plan, &parser, &table, src).unwrap();
            std::hint::black_box(report.accepted_resolved_tree(&parser, src));
        }
        let snark_ns = t.elapsed().as_nanos() / n as u128;
        let t = Instant::now();
        for _ in 0..n {
            std::hint::black_box(gingembre::parse_template_recovered(src));
        }
        let ging_ns = t.elapsed().as_nanos() / n as u128;
        println!("  {src:<34} {snark_ns:>7} ns   {ging_ns:>7} ns");
    }

    // ---- materialize `1 + 2 * 3` into the generated AST, three ways ----
    let src = "{{ 1 + 2 * 3 }}";
    let report = parse_prepared_weavy_with_report(&plan, &parser, &table, src).unwrap();
    let resolved = report.accepted_resolved_tree(&parser, src).unwrap();
    let node = find_expr(&resolved).unwrap();
    let mut ops = Vec::new();
    let _ = build(
        facet_reflect::Partial::alloc_owned::<gen_ast::Expr>().unwrap(),
        node,
        &anns,
        &mut ops,
    )
    .unwrap();

    let m = 200_000u32;
    // (a) reflection straight into Partial (no ops).
    let t = Instant::now();
    for _ in 0..m {
        let mut o = Vec::new();
        let v: gen_ast::Expr = build(
            facet_reflect::Partial::alloc_owned::<gen_ast::Expr>().unwrap(),
            node,
            &anns,
            &mut o,
        )
        .unwrap()
        .build()
        .unwrap()
        .materialize()
        .unwrap();
        std::hint::black_box(v);
    }
    let refl_ns = t.elapsed().as_nanos() / m as u128;

    // (b) weavy interpreter over recorded ops.
    let t = Instant::now();
    for _ in 0..m {
        std::hint::black_box(run_ops_via_weavy::<gen_ast::Expr>(&ops));
    }
    let interp_ns = t.elapsed().as_nanos() / m as u128;

    // (c) weavy JIT: compile + run each call (what run_ops_via_weavy_jit does).
    let t = Instant::now();
    for _ in 0..m {
        std::hint::black_box(run_ops_via_weavy_jit::<gen_ast::Expr>(&ops));
    }
    let jit_ns = t.elapsed().as_nanos() / m as u128;

    // (c') weavy JIT compiled ONCE, run many — isolates run cost from compile cost.
    use weavy::jit::{HostCallCtx, HostCallInfo, NativeProgram, StencilLayout};
    let calls: Vec<HostCallInfo> = ops
        .iter()
        .map(|op| HostCallInfo {
            info: core::ptr::from_ref(op).cast(),
            call: jit_apply,
        })
        .collect();
    let compile = Instant::now();
    let mut layout = StencilLayout::new();
    let root = layout.start_chain();
    let mut sites = Vec::with_capacity(calls.len());
    for call in &calls {
        sites.push(layout.emit_hostcall(root, core::ptr::from_ref(call)));
    }
    let done = layout.emit_done();
    for i in 0..sites.len() {
        layout.patch_hostcall_continuation(sites[i], sites.get(i + 1).copied().unwrap_or(done));
    }
    let native = NativeProgram::new(layout, root);
    let compile_ns = compile.elapsed().as_nanos();
    let entry = unsafe { native.entry_fn::<HostCallCtx<JitState>>() };
    let t = Instant::now();
    for _ in 0..m {
        let mut state = JitState {
            partial: Some(facet_reflect::Partial::alloc_owned::<gen_ast::Expr>().unwrap()),
            error: None,
        };
        let mut cx = HostCallCtx::new(native.entry_prog(), &mut state);
        unsafe { entry(&mut cx) };
        let v: gen_ast::Expr = state.partial.take().unwrap().build().unwrap().materialize().unwrap();
        std::hint::black_box(v);
    }
    let jit_run_ns = t.elapsed().as_nanos() / m as u128;

    println!("\nmaterialize `1 + 2 * 3` -> gen_ast::Expr (avg over {m} iters):");
    println!("  (a) reflection direct into Partial : {refl_ns:>6} ns");
    println!("  (b) weavy interpreter over ops     : {interp_ns:>6} ns");
    println!("  (c) weavy JIT (compile + run)      : {jit_ns:>6} ns");
    println!("  (c') weavy JIT (compiled once, run): {jit_run_ns:>6} ns  (compile once = {compile_ns} ns)");
    println!(
        "\nNote: (c) recompiles the op stream every call. The ops encode DATA (SetI64(1))\n\
         as well as structure, so the native program is not reused across inputs — the real\n\
         win needs structure compiled once per grammar-shape with data in prog-stream slots\n\
         (facet-json's per-TYPE plan model). That separation is the next perf step."
    );
}

/// DIAG: feed malformed templates to both front ends and show what each reports. snark either
/// recovers (ERROR/MISSING nodes) or hard-errors with the exact expected-terminal set + byte
/// position; gingembre emits a construct-aware human message. Different shapes — see takeaway.
fn diag() {
    let (parser, table, plan, _anns) = build_plan();
    let bad = ["{{ 1 + }}", "{% if x %}no end", "{{ 1 +* 2 }}"];
    for src in bad {
        println!("\n=== input: {src:?} ===");

        // gingembre: a structured TemplateError (rendered by ariadne for humans).
        match gingembre::parse_template("diag", src) {
            Ok(_) => println!("gingembre: parsed OK (no error)"),
            Err(e) => println!("gingembre: {e}"),
        }

        // snark: recovered tree with ERROR/MISSING nodes + failure count.
        match parse_prepared_weavy_with_report(&plan, &parser, &table, src) {
            Ok(report) => {
                println!("snark: parsed with recovery (see tree; ERROR/MISSING mark the gap)");
                if let Some(tree) = report.accepted_resolved_tree(&parser, src) {
                    dump(&tree, 3);
                } else {
                    println!("   (no accepted tree)");
                }
            }
            Err(e) => println!("snark: hard parse error: {e:?}"),
        }
    }
    println!(
        "\nTakeaway: snark's error carries the EXACT expected-terminal set + byte position\n\
         (richer raw material than gingembre's), but unshaped: raw regexes not friendly names,\n\
         byte offset not line:col, no construct context (gingembre hand-writes 'unclosed if,\n\
         expected endif'). gingembre's are human-tuned but coarser ('expected an expression')\n\
         and here even slightly wrong. snark has the better DATA, gingembre the better PROSE.\n\
         Not reconciled yet."
    );
}

// ===========================================================================================
// #3: the TEMPLATE ITSELF lowers to Weavy too. Materialization (tree -> AST) was one Weavy
// program; EVALUATION (AST -> Value) is ANOTHER. gingembre's evaluator is an async, boxed-
// future tree-walk over a SECOND AST; here we lower the ONE generated AST into a flat
// stack-machine Weavy program and run it through the interpreter AND a weavy HOSTCALL chain.
// Oracle: gingembre's own `eval_expression`. (Subset: int arithmetic + comparison + logical +
// variable load — the operators the generated Expr/Binary covers. The int-arith intrinsic is
// the disposable stand-in; in the real thing it'd be gingembre's own Value ops as intrinsics.)
// ===========================================================================================

/// A Weavy op for evaluating an expression as a stack machine.
#[derive(Clone, Debug)]
enum EvalOp {
    PushInt(i64),
    LoadVar(String),
    /// Pop b, pop a, push (a `op` b).
    Bin(String),
}

/// Post-order lowering of the generated AST into a stack program (this is the "eval lowers to
/// Weavy" step — the analogue of `build` for materialization).
fn lower_eval(e: &gen_ast::Expr, out: &mut Vec<EvalOp>) {
    match e {
        gen_ast::Expr::Number(n) => out.push(EvalOp::PushInt(*n)),
        gen_ast::Expr::Variable(v) => out.push(EvalOp::LoadVar(v.clone())),
        gen_ast::Expr::Binary(b) => {
            lower_eval(&b.left, out);
            lower_eval(&b.right, out);
            out.push(EvalOp::Bin(b.op.clone()));
        }
    }
}

/// The single source of eval-op semantics — shared by interpreter and JIT (mirrors `apply_op`).
fn eval_apply(stack: &mut Vec<Value>, ctx: &BTreeMap<String, Value>, op: &EvalOp) -> Result<(), String> {
    match op {
        EvalOp::PushInt(n) => stack.push(Value::from(*n)),
        EvalOp::LoadVar(v) => {
            let val = ctx.get(v).cloned().ok_or_else(|| format!("undefined variable {v:?}"))?;
            stack.push(val);
        }
        EvalOp::Bin(o) => {
            let b = stack.pop().ok_or("stack underflow")?;
            let a = stack.pop().ok_or("stack underflow")?;
            stack.push(eval_binop(&a, o, &b)?);
        }
    }
    Ok(())
}

fn eval_binop(a: &Value, op: &str, b: &Value) -> Result<Value, String> {
    let int = |v: &Value| v.as_number().and_then(|n| n.to_i64());
    let (ai, bi) = (int(a), int(b));
    let need = |x: Option<i64>| x.ok_or_else(|| format!("non-integer operand for {op:?}"));
    Ok(match op {
        "+" => Value::from(need(ai)? + need(bi)?),
        "-" => Value::from(need(ai)? - need(bi)?),
        "*" => Value::from(need(ai)? * need(bi)?),
        "==" => Value::from(ai == bi),
        "!=" => Value::from(ai != bi),
        "<" => Value::from(need(ai)? < need(bi)?),
        "<=" => Value::from(need(ai)? <= need(bi)?),
        ">" => Value::from(need(ai)? > need(bi)?),
        ">=" => Value::from(need(ai)? >= need(bi)?),
        "and" => Value::from(a.as_bool().unwrap_or(false) && b.as_bool().unwrap_or(false)),
        "or" => Value::from(a.as_bool().unwrap_or(false) || b.as_bool().unwrap_or(false)),
        other => return Err(format!("unsupported operator {other:?}")),
    })
}

/// Stack-machine evaluator (owns its context so the JIT can cast a plain `*mut EvalMachine`).
struct EvalMachine {
    stack: Vec<Value>,
    ctx: BTreeMap<String, Value>,
}

impl<'p> weavy::Step<'p, u32, EvalOp> for EvalMachine {
    type Error = String;
    type Continuation = ();
    fn step(&mut self, op: &'p EvalOp) -> Result<weavy::Control<'p, u32, EvalOp, ()>, String> {
        eval_apply(&mut self.stack, &self.ctx, op)?;
        Ok(weavy::Control::Continue)
    }
}

/// Evaluate the stack program through the weavy interpreter.
fn run_eval_via_weavy(ops: &[EvalOp], ctx: &BTreeMap<String, Value>) -> Value {
    let mut m = EvalMachine { stack: Vec::new(), ctx: ctx.clone() };
    let lowered = weavy::Lowered::<u32, EvalOp>::new(ops.to_vec());
    weavy::run(&lowered, &mut m).expect("eval run");
    m.stack.pop().expect("result on stack")
}

/// The JIT intrinsic: apply one eval op to the stack machine.
unsafe extern "C" fn eval_jit_apply(cx: *mut (), info: *const ()) -> bool {
    let m = unsafe { &mut *cx.cast::<EvalMachine>() };
    let op = unsafe { &*info.cast::<EvalOp>() };
    eval_apply(&mut m.stack, &m.ctx, op).is_ok()
}

/// Evaluate the stack program via a weavy HOSTCALL chain (same harness as the materializer).
/// NB: hostcall lane — the op body is still an indirect call into `eval_apply`. For REAL
/// copy-and-patch (dedicated stencils, native add, no call) see `build_intop_native`.
fn run_eval_via_weavy_jit(ops: &[EvalOp], ctx: &BTreeMap<String, Value>) -> Value {
    use weavy::jit::{HostCallCtx, HostCallInfo, NativeProgram, StencilLayout};
    if !weavy::jit::NATIVE_COPY_PATCH_AVAILABLE {
        return run_eval_via_weavy(ops, ctx);
    }
    let calls: Vec<HostCallInfo> = ops
        .iter()
        .map(|op| HostCallInfo {
            info: core::ptr::from_ref(op).cast(),
            call: eval_jit_apply,
        })
        .collect();
    let mut layout = StencilLayout::new();
    let root = layout.start_chain();
    let mut sites = Vec::with_capacity(calls.len());
    for call in &calls {
        sites.push(layout.emit_hostcall(root, core::ptr::from_ref(call)));
    }
    let done = layout.emit_done();
    for i in 0..sites.len() {
        layout.patch_hostcall_continuation(sites[i], sites.get(i + 1).copied().unwrap_or(done));
    }
    let native = NativeProgram::new(layout, root);
    let mut m = EvalMachine { stack: Vec::new(), ctx: ctx.clone() };
    let mut cx = HostCallCtx::new(native.entry_prog(), &mut m);
    let entry = unsafe { native.entry_fn::<HostCallCtx<EvalMachine>>() };
    unsafe { entry(&mut cx) };
    m.stack.pop().expect("result on stack")
}

/// #3 PROOF: one generated AST, and EVALUATION is also a Weavy program (interp + JIT), oracle'd
/// against gingembre's own evaluator.
fn eval_weavy_proof() {
    let (parser, table, plan, anns) = build_plan();

    let mut my_ctx = BTreeMap::new();
    my_ctx.insert("x".to_string(), Value::from(42i64));
    let mut g_ctx = Context::new();
    g_ctx.set("x", Value::from(42i64));

    let exprs = [
        "1 + 2",
        "1 + 2 * 3",
        "2 * 3 + 4 * 5",
        "10 - 2 - 3",
        "2 * 3 > 5",
        "1 + 1 == 2",
        "1 < 2 and 3 < 4",
        "x",
        "x + 1",
    ];
    for expr in exprs {
        let src = format!("{{{{ {expr} }}}}");
        let report = parse_prepared_weavy_with_report(&plan, &parser, &table, &src).expect("parse");
        let resolved = report.accepted_resolved_tree(&parser, &src).expect("resolved tree");
        let node = find_expr(&resolved).expect("no expr");

        // ONE AST (generated, via JIT) -> eval Weavy program.
        let ast = expr_via_jit(node, &anns);
        let mut ops = Vec::new();
        lower_eval(&ast, &mut ops);

        let via_interp = run_eval_via_weavy(&ops, &my_ctx);
        let via_jit = run_eval_via_weavy_jit(&ops, &my_ctx);
        let native = block_on(gingembre::eval_expression(expr, &g_ctx)).expect("gingembre eval");

        assert_eq!(via_interp, native, "weavy-interp eval must match gingembre for {expr:?}");
        assert_eq!(via_jit, native, "weavy-JIT eval must match gingembre for {expr:?}");
        println!("✓ {expr:<20} = {native:?}  ({} eval ops; interp==jit==gingembre)", ops.len());
    }
    println!(
        "\n✓ evaluation ALSO lowers to Weavy: the ONE generated AST -> stack program -> interp\n\
         AND hostcall-chain JIT, oracle-matched to gingembre's evaluator. No second AST, no\n\
         tree-walk. Next: text/if/for as emit + control-flow ops (Weavy blocks) for full render."
    );
}

// ===========================================================================================
// V8-style type specialization: when the generated AST proves a subtree is integer (all
// `number` leaves + arithmetic ops — the grammar annotations already type the leaves), lower
// it to UNBOXED i64 ops over an i64 stack. No `Value` construction, no as_number()/re-box on
// every op, no dynamic dispatch through the arithmetic tower. Measured against the boxed path.
// (Variables are the not-statically-known case → the guard+deopt / inline-cache story, next.)
// ===========================================================================================

/// Unboxed integer op — the monomorphic fast path. `Facet` so it can be phon-serialized as
/// portable bytecode (see `--serialize`).
#[derive(facet::Facet, Clone, Debug, PartialEq)]
#[repr(u8)]
enum IntOp {
    Push(i64),
    Add,
    Sub,
    Mul,
}

/// Is this subtree statically integer? (number literals + int arithmetic). This is the type
/// fact the grammar hands us for free — `number` decodes to i64.
fn is_static_int(e: &gen_ast::Expr) -> bool {
    match e {
        gen_ast::Expr::Number(_) => true,
        gen_ast::Expr::Binary(b) => {
            matches!(b.op.as_str(), "+" | "-" | "*")
                && is_static_int(&b.left)
                && is_static_int(&b.right)
        }
        gen_ast::Expr::Variable(_) => false,
    }
}

/// Lower a statically-integer subtree to unboxed i64 ops (assumes `is_static_int`).
fn lower_int(e: &gen_ast::Expr, out: &mut Vec<IntOp>) {
    match e {
        gen_ast::Expr::Number(n) => out.push(IntOp::Push(*n)),
        gen_ast::Expr::Binary(b) => {
            lower_int(&b.left, out);
            lower_int(&b.right, out);
            out.push(match b.op.as_str() {
                "+" => IntOp::Add,
                "-" => IntOp::Sub,
                "*" => IntOp::Mul,
                other => unreachable!("non-int op {other:?} slipped past is_static_int"),
            });
        }
        gen_ast::Expr::Variable(_) => unreachable!("variable slipped past is_static_int"),
    }
}

/// Unboxed i64 stack machine — no `Value`, no tag checks.
struct IntMachine {
    stack: Vec<i64>,
}

#[inline]
fn int_apply(stack: &mut Vec<i64>, op: &IntOp) {
    match op {
        IntOp::Push(n) => stack.push(*n),
        IntOp::Add => {
            let b = stack.pop().unwrap();
            *stack.last_mut().unwrap() += b;
        }
        IntOp::Sub => {
            let b = stack.pop().unwrap();
            *stack.last_mut().unwrap() -= b;
        }
        IntOp::Mul => {
            let b = stack.pop().unwrap();
            *stack.last_mut().unwrap() *= b;
        }
    }
}

impl<'p> weavy::Step<'p, u32, IntOp> for IntMachine {
    type Error = std::convert::Infallible;
    type Continuation = ();
    fn step(&mut self, op: &'p IntOp) -> Result<weavy::Control<'p, u32, IntOp, ()>, Self::Error> {
        int_apply(&mut self.stack, op);
        Ok(weavy::Control::Continue)
    }
}

unsafe extern "C" fn int_jit_apply(cx: *mut (), info: *const ()) -> bool {
    let m = unsafe { &mut *cx.cast::<IntMachine>() };
    let op = unsafe { &*info.cast::<IntOp>() };
    int_apply(&mut m.stack, op);
    true
}

/// Assemble a native program from a caller-owned host-call array (which must outlive the
/// returned program — the copied code reads its `info` pointers). No borrow is tracked because
/// the program stores raw pointers, so keep `calls` alive in the same scope.
fn build_native(calls: &[weavy::jit::HostCallInfo]) -> weavy::jit::NativeProgram {
    use weavy::jit::{NativeProgram, StencilLayout};
    let mut layout = StencilLayout::new();
    let root = layout.start_chain();
    let mut sites = Vec::with_capacity(calls.len());
    for call in calls {
        sites.push(layout.emit_hostcall(root, core::ptr::from_ref(call)));
    }
    let done = layout.emit_done();
    for i in 0..sites.len() {
        layout.patch_hostcall_continuation(sites[i], sites.get(i + 1).copied().unwrap_or(done));
    }
    NativeProgram::new(layout, root)
}

/// Host-call metadata for an unboxed int program (kept alive by the caller).
fn int_calls(ops: &[IntOp]) -> Vec<weavy::jit::HostCallInfo> {
    ops.iter()
        .map(|op| weavy::jit::HostCallInfo {
            info: core::ptr::from_ref(op).cast(),
            call: int_jit_apply,
        })
        .collect()
}

// --- REAL copy-and-patch: dedicated per-op stencils, no hostcalls (mirrors phon-jit). -------

/// Extracted stencil bytes + continuation relocs, generated by build.rs from stencils/intop.rs.
mod intop_stencils {
    include!(concat!(env!("OUT_DIR"), "/intop_stencils.rs"));
}

/// Threaded state — MUST match `Ctx` in stencils/intop.rs (repr(C), same field order).
#[repr(C)]
struct IntCtx {
    prog: *const u64,
    sp: *mut i64,
}

/// Assemble the `IntOp` program as REAL copy-and-patch: copy one dedicated compiled stencil per
/// op into a chain, patch each stencil's `weavy_cont` hole to the next (the last to `DONE`),
/// immediates in the prog stream. No hostcalls, no indirect dispatch — the add is a native add.
fn build_intop_native(ops: &[IntOp]) -> Option<weavy::jit::NativeProgram> {
    use weavy::jit::StencilLayout;
    if intop_stencils::PUSH.is_empty() || !weavy::jit::NATIVE_COPY_PATCH_AVAILABLE {
        return None; // stencils unavailable on this target -> caller uses the interpreter
    }
    let mut layout = StencilLayout::new();
    let root = layout.start_chain();
    let mut sites: Vec<(usize, &'static [usize])> = Vec::with_capacity(ops.len());
    for op in ops {
        let (bytes, cont): (&[u8], &'static [usize]) = match op {
            IntOp::Push(n) => {
                layout.push_prog_word(root.prog_index, *n as u64);
                (intop_stencils::PUSH, intop_stencils::PUSH_CONT)
            }
            IntOp::Add => (intop_stencils::ADD, intop_stencils::ADD_CONT),
            IntOp::Sub => (intop_stencils::SUB, intop_stencils::SUB_CONT),
            IntOp::Mul => (intop_stencils::MUL, intop_stencils::MUL_CONT),
        };
        let start = layout.emit_stencil(bytes);
        sites.push((start, cont));
    }
    let done = layout.emit_stencil(intop_stencils::DONE);
    for i in 0..sites.len() {
        let (start, cont) = sites[i];
        let target = sites.get(i + 1).map(|(s, _)| *s).unwrap_or(done);
        for &rel in cont {
            layout.patch_continuation(start + rel, target);
        }
    }
    Some(weavy::jit::NativeProgram::new(layout, root))
}

/// Run a compiled `IntOp` copy-and-patch program over a scratch i64 stack, returning the result.
fn run_intop_native(native: &weavy::jit::NativeProgram, stack: &mut [i64]) -> i64 {
    let mut ctx = IntCtx {
        prog: native.entry_prog(),
        sp: stack.as_mut_ptr(),
    };
    let entry = unsafe { native.entry_fn::<IntCtx>() };
    unsafe { entry(&mut ctx) };
    unsafe { *ctx.sp.sub(1) }
}

// --- Type SPECULATION + deopt: a guarded fast path, falling back to the real evaluator. ------

/// A speculative op: like `IntOp` but variables become `Guard`s that bet the var is an integer.
#[derive(Clone, Debug)]
enum SpecOp {
    Guard(usize), // speculate variable #idx is i64: push it (fast) or deopt (slow)
    Push(i64),
    Add,
    Sub,
    Mul,
}

/// Lower the generated AST into a speculative program, collecting variable names in first-seen
/// order (each `Variable` becomes a `Guard` on its slot).
fn lower_spec(e: &gen_ast::Expr, vars: &mut Vec<String>, out: &mut Vec<SpecOp>) {
    match e {
        gen_ast::Expr::Number(n) => out.push(SpecOp::Push(*n)),
        gen_ast::Expr::Variable(v) => {
            let idx = vars.iter().position(|x| x == v).unwrap_or_else(|| {
                vars.push(v.clone());
                vars.len() - 1
            });
            out.push(SpecOp::Guard(idx));
        }
        gen_ast::Expr::Binary(b) => {
            lower_spec(&b.left, vars, out);
            lower_spec(&b.right, vars, out);
            out.push(match b.op.as_str() {
                "+" => SpecOp::Add,
                "-" => SpecOp::Sub,
                "*" => SpecOp::Mul,
                other => unreachable!("non-int op {other:?} in speculative lane"),
            });
        }
    }
}

/// A resolved variable slot — MUST match `VarSlot` in stencils/guard.rs.
#[repr(C)]
struct SpecVarSlot {
    tag: i64,
    bits: i64,
}

const TAG_I64: i64 = 0;
const TAG_F64: i64 = 1;
const TAG_OTHER: i64 = 2;

/// Classify a gingembre value into a guard tag + unboxed bits.
fn tag_of(v: &Value) -> SpecVarSlot {
    if let Some(i) = v.as_number().and_then(|n| n.to_i64()) {
        SpecVarSlot { tag: TAG_I64, bits: i }
    } else if let Some(f) = v.as_number().and_then(|n| n.to_f64()) {
        SpecVarSlot { tag: TAG_F64, bits: f.to_bits() as i64 }
    } else {
        SpecVarSlot { tag: TAG_OTHER, bits: 0 }
    }
}

/// Threaded state — MUST match `Ctx` in stencils/guard.rs (prog/sp share layout with IntCtx).
#[repr(C)]
struct SpecCtx {
    prog: *const u64,
    sp: *mut i64,
    vars: *const SpecVarSlot,
    deopt: *mut u64,
}

/// Assemble a speculative program: guard stencils (two successors: fast chain / deopt exit)
/// mixed with the unboxed IntOp stencils. Every guard's deopt hole and the final op both patch
/// to `DONE`; the fast holes chain linearly.
fn build_spec_native(ops: &[SpecOp]) -> Option<weavy::jit::NativeProgram> {
    use weavy::jit::StencilLayout;
    if intop_stencils::GUARD.is_empty() || !weavy::jit::NATIVE_COPY_PATCH_AVAILABLE {
        return None;
    }
    let mut layout = StencilLayout::new();
    let root = layout.start_chain();
    // One emitted stencil: (code offset, fast/next cont relocs, optional deopt cont relocs).
    type Site = (usize, &'static [usize], Option<&'static [usize]>);
    let mut sites: Vec<Site> = Vec::with_capacity(ops.len());
    for op in ops {
        match op {
            SpecOp::Guard(idx) => {
                layout.push_prog_word(root.prog_index, *idx as u64);
                let start = layout.emit_stencil(intop_stencils::GUARD);
                sites.push((start, intop_stencils::GUARD_FAST_CONT, Some(intop_stencils::GUARD_DEOPT_CONT)));
            }
            SpecOp::Push(n) => {
                layout.push_prog_word(root.prog_index, *n as u64);
                let start = layout.emit_stencil(intop_stencils::PUSH);
                sites.push((start, intop_stencils::PUSH_CONT, None));
            }
            SpecOp::Add => sites.push((layout.emit_stencil(intop_stencils::ADD), intop_stencils::ADD_CONT, None)),
            SpecOp::Sub => sites.push((layout.emit_stencil(intop_stencils::SUB), intop_stencils::SUB_CONT, None)),
            SpecOp::Mul => sites.push((layout.emit_stencil(intop_stencils::MUL), intop_stencils::MUL_CONT, None)),
        }
    }
    let done = layout.emit_stencil(intop_stencils::DONE);
    for i in 0..sites.len() {
        let (start, fast, deopt) = sites[i];
        let next = sites.get(i + 1).map(|(s, _, _)| *s).unwrap_or(done);
        for &rel in fast {
            layout.patch_continuation(start + rel, next);
        }
        if let Some(deopt_relocs) = deopt {
            for &rel in deopt_relocs {
                layout.patch_continuation(start + rel, done);
            }
        }
    }
    Some(weavy::jit::NativeProgram::new(layout, root))
}

/// Run a speculative program. Returns `Some(result)` if every guard's bet held (fast path), or
/// `None` if any guard deopted — the caller then falls back to the full evaluator.
fn run_spec_native(
    native: &weavy::jit::NativeProgram,
    vars: &[SpecVarSlot],
    stack: &mut [i64],
) -> Option<i64> {
    let mut deopt = 0u64;
    let mut ctx = SpecCtx {
        prog: native.entry_prog(),
        sp: stack.as_mut_ptr(),
        vars: vars.as_ptr(),
        deopt: &mut deopt,
    };
    let entry = unsafe { native.entry_fn::<SpecCtx>() };
    unsafe { entry(&mut ctx) };
    if deopt != 0 {
        None
    } else {
        Some(unsafe { *ctx.sp.sub(1) })
    }
}

/// Assemble a guarded program specialized to ONE type profile `ty` (TAG_I64 or TAG_F64): the
/// guard, arithmetic stencils, and push-immediate encoding are all chosen by `ty`. This is what
/// an inline cache compiles per observed type.
fn build_ic_native(ops: &[SpecOp], ty: i64) -> Option<weavy::jit::NativeProgram> {
    use weavy::jit::StencilLayout;
    let float = ty == TAG_F64;
    if intop_stencils::GUARD.is_empty() || !weavy::jit::NATIVE_COPY_PATCH_AVAILABLE {
        return None;
    }
    let (guard, guard_fast, guard_deopt) = if float {
        (intop_stencils::GUARD_F64, intop_stencils::GUARD_F64_FAST_CONT, intop_stencils::GUARD_F64_DEOPT_CONT)
    } else {
        (intop_stencils::GUARD, intop_stencils::GUARD_FAST_CONT, intop_stencils::GUARD_DEOPT_CONT)
    };
    let (add, add_c, sub, sub_c, mul, mul_c) = if float {
        (
            intop_stencils::FADD, intop_stencils::FADD_CONT,
            intop_stencils::FSUB, intop_stencils::FSUB_CONT,
            intop_stencils::FMUL, intop_stencils::FMUL_CONT,
        )
    } else {
        (
            intop_stencils::ADD, intop_stencils::ADD_CONT,
            intop_stencils::SUB, intop_stencils::SUB_CONT,
            intop_stencils::MUL, intop_stencils::MUL_CONT,
        )
    };

    let mut layout = StencilLayout::new();
    let root = layout.start_chain();
    type Site = (usize, &'static [usize], Option<&'static [usize]>);
    let mut sites: Vec<Site> = Vec::with_capacity(ops.len());
    for op in ops {
        match op {
            SpecOp::Guard(idx) => {
                layout.push_prog_word(root.prog_index, *idx as u64);
                sites.push((layout.emit_stencil(guard), guard_fast, Some(guard_deopt)));
            }
            SpecOp::Push(n) => {
                // In the float program, integer literals become f64 bits.
                let imm = if float { (*n as f64).to_bits() } else { *n as u64 };
                layout.push_prog_word(root.prog_index, imm);
                sites.push((layout.emit_stencil(intop_stencils::PUSH), intop_stencils::PUSH_CONT, None));
            }
            SpecOp::Add => sites.push((layout.emit_stencil(add), add_c, None)),
            SpecOp::Sub => sites.push((layout.emit_stencil(sub), sub_c, None)),
            SpecOp::Mul => sites.push((layout.emit_stencil(mul), mul_c, None)),
        }
    }
    let done = layout.emit_stencil(intop_stencils::DONE);
    for i in 0..sites.len() {
        let (start, fast, deopt) = sites[i];
        let next = sites.get(i + 1).map(|(s, _, _)| *s).unwrap_or(done);
        for &rel in fast {
            layout.patch_continuation(start + rel, next);
        }
        if let Some(deopt_relocs) = deopt {
            for &rel in deopt_relocs {
                layout.patch_continuation(start + rel, done);
            }
        }
    }
    Some(weavy::jit::NativeProgram::new(layout, root))
}

/// A polymorphic inline cache: caches one compiled native program per observed type profile.
/// Hit -> run the cached native code; miss -> compile for the new type, cache, run.
#[derive(Default)]
struct InlineCache {
    entries: Vec<(i64, weavy::jit::NativeProgram)>,
    hits: usize,
    misses: usize,
}

impl InlineCache {
    /// Evaluate `ops` for a variable of runtime type `ty`. Returns the raw result bits (i64 value
    /// or f64 bits), or `None` if the guard deopted (unknown type).
    fn eval(&mut self, ops: &[SpecOp], ty: i64, slots: &[SpecVarSlot], stack: &mut [i64]) -> Option<i64> {
        if let Some((_, prog)) = self.entries.iter().find(|(t, _)| *t == ty) {
            self.hits += 1;
            return run_spec_native(prog, slots, stack);
        }
        self.misses += 1;
        let prog = build_ic_native(ops, ty)?; // compile the specialization for this type
        let r = run_spec_native(&prog, slots, stack);
        self.entries.push((ty, prog));
        r
    }
}

/// PROOF + MEASUREMENT: a polymorphic INLINE CACHE. The generated AST is compiled per observed
/// variable type (int lane / float lane) on the first sighting, cached, and reused; a type change
/// triggers one recompile + a new cache entry. Oracle: every result matches gingembre.
fn inline_cache() {
    use std::time::Instant;
    let (parser, table, plan, anns) = build_plan();

    let expr = "x * 3 + 1";
    let src = format!("{{{{ {expr} }}}}");
    let report = parse_prepared_weavy_with_report(&plan, &parser, &table, &src).unwrap();
    let resolved = report.accepted_resolved_tree(&parser, &src).unwrap();
    let ast = expr_via_jit(find_expr(&resolved).unwrap(), &anns);
    let mut vars = Vec::new();
    let mut ops = Vec::new();
    lower_spec(&ast, &mut vars, &mut ops);
    println!("expr: {expr}   program: {ops:?}   vars: {vars:?}\n");

    let mut ic = InlineCache::default();
    let mut stack = vec![0i64; 64];

    // A stream of calls with varying runtime types (monomorphic int, then a float shows up).
    // (tag_of uses to_i64 as a stand-in, so a WHOLE float would land in the int lane; a
    // production guard reads the Value's real tag. Fractional floats classify as float here.)
    let stream = [
        Value::from(10i64),
        Value::from(20i64),
        Value::from(30i64),
        Value::from(2.5f64),
        Value::from(4.5f64),
        Value::from(7i64),
    ];
    for x in &stream {
        let slot = tag_of(x);
        let before = (ic.hits, ic.misses);
        let bits = ic.eval(&ops, slot.tag, std::slice::from_ref(&slot), &mut stack).expect("guard held");

        let mut gctx = Context::new();
        gctx.set("x", x.clone());
        let gingembre = block_on(gingembre::eval_expression(expr, &gctx)).unwrap();

        let (kind, got, ok) = if slot.tag == TAG_I64 {
            let g = gingembre.as_number().and_then(|n| n.to_i64()).unwrap();
            ("int", format!("{bits}"), bits == g)
        } else {
            let f = f64::from_bits(bits as u64);
            let g = gingembre.as_number().and_then(|n| n.to_f64()).unwrap();
            ("flt", format!("{f}"), (f - g).abs() < 1e-9)
        };
        assert!(ok, "IC result must match gingembre for x={x:?}");
        let ev = if ic.hits > before.0 { "HIT " } else { "MISS→compile" };
        println!("x = {x:<10?} [{kind}]  {ev}  -> {got}  (== gingembre {gingembre:?})");
    }
    println!(
        "\nIC state: {} cache entries (one native program per type), {} hits, {} misses/compiles.",
        ic.entries.len(),
        ic.hits,
        ic.misses,
    );

    // Measure a cached HIT (no compile) vs a cold MISS (compile + run) vs gingembre.
    let x = Value::from(10i64);
    let slot = tag_of(&x);
    let mut warm = InlineCache::default();
    warm.eval(&ops, slot.tag, std::slice::from_ref(&slot), &mut stack); // prime
    let m = 200_000u32;
    let t = Instant::now();
    for _ in 0..m {
        std::hint::black_box(warm.eval(&ops, slot.tag, std::slice::from_ref(&slot), &mut stack));
    }
    let hit_ns = t.elapsed().as_nanos() / m as u128;
    let t = Instant::now();
    for _ in 0..m {
        let mut cold = InlineCache::default();
        std::hint::black_box(cold.eval(&ops, slot.tag, std::slice::from_ref(&slot), &mut stack));
    }
    let miss_ns = t.elapsed().as_nanos() / m as u128;
    println!(
        "\ncached HIT: {hit_ns} ns   cold MISS (compile+run): {miss_ns} ns   (compile amortizes after ~{} calls)",
        miss_ns / hit_ns.max(1)
    );
    println!(
        "\nThis is a polymorphic inline cache: first sighting of a type compiles a specialized\n\
         native program (int lane vs float lane, different guard + arithmetic stencils); repeats\n\
         hit the cache and run compiled code; a new type recompiles once. The guard is the IC's\n\
         type check — same conditional-branch stencil, now selecting the cached specialization."
    );
}

/// PROOF + MEASUREMENT: type speculation with deopt. The generated AST -> a guarded copy-and-
/// patch program that BETS every variable is an integer. When the bet holds it runs the unboxed
/// fast path; when it misses, a guard branches to the deopt exit and we fall back to gingembre's
/// evaluator. Oracle: both paths match gingembre.
fn speculate() {
    use std::time::Instant;
    let (parser, table, plan, anns) = build_plan();

    let expr = "x * 3 + 1";
    let src = format!("{{{{ {expr} }}}}");
    let report = parse_prepared_weavy_with_report(&plan, &parser, &table, &src).unwrap();
    let resolved = report.accepted_resolved_tree(&parser, &src).unwrap();
    let ast = expr_via_jit(find_expr(&resolved).unwrap(), &anns);

    let mut vars = Vec::new();
    let mut ops = Vec::new();
    lower_spec(&ast, &mut vars, &mut ops);
    let native = build_spec_native(&ops).expect("guard stencils available");
    println!("expr: {expr}   speculative program: {ops:?}   vars: {vars:?}\n");

    // Resolve each variable to a guard slot (tag + unboxed bits) from a gingembre value.
    let slots_for = |x: &Value| -> Vec<SpecVarSlot> { vars.iter().map(|_| tag_of(x)).collect() };

    for (label, x) in [("x = 10 (int)", Value::from(10i64)), ("x = 2.5 (float)", Value::from(2.5f64))] {
        let mut gctx = Context::new();
        gctx.set("x", x.clone());
        let gingembre = block_on(gingembre::eval_expression(expr, &gctx)).unwrap();

        let slots = slots_for(&x);
        let mut stack = vec![0i64; 64];
        match run_spec_native(&native, &slots, &mut stack) {
            Some(fast) => {
                assert_eq!(Some(fast), gingembre.as_number().and_then(|n| n.to_i64()));
                println!("{label:<16} -> guard HELD  -> fast JIT path = {fast}  (== gingembre {gingembre:?})");
            }
            None => {
                // Deopt: the bet missed; fall back to the real evaluator.
                println!("{label:<16} -> guard MISSED -> DEOPT -> gingembre = {gingembre:?}");
            }
        }
    }

    // Measure the win when the bet holds: fast JIT path vs the deopt (full evaluator) path.
    let x_int = Value::from(10i64);
    let slots = slots_for(&x_int);
    let mut gctx = Context::new();
    gctx.set("x", x_int);
    let m = 200_000u32;
    let mut stack = vec![0i64; 64];
    let t = Instant::now();
    for _ in 0..m {
        std::hint::black_box(run_spec_native(&native, &slots, &mut stack));
    }
    let fast_ns = t.elapsed().as_nanos() / m as u128;
    let t = Instant::now();
    for _ in 0..m {
        std::hint::black_box(block_on(gingembre::eval_expression(expr, &gctx)).unwrap());
    }
    let deopt_ns = t.elapsed().as_nanos() / m as u128;
    println!(
        "\nfast JIT path (guard held): {fast_ns} ns   deopt/full evaluator: {deopt_ns} ns   ({:.0}x)",
        deopt_ns as f64 / fast_ns.max(1) as f64
    );
    println!(
        "\nThis is speculation + deopt: the guard stencil is a real conditional branch (cbz on\n\
         the type tag) with TWO patched continuations — fast chain vs deopt exit. Bet holds ->\n\
         unboxed native path; bet misses -> fall back to gingembre. Inline-cache feedback would\n\
         pick which guard to bet on; here we always speculate i64."
    );
}

/// PROOF + MEASUREMENT: type-specialized unboxed i64 path vs the boxed `Value` path, for a
/// statically-integer expression. Same result (oracle vs gingembre), very different cost.
fn specialize() {
    use std::time::Instant;
    use weavy::jit::HostCallCtx;

    let (parser, table, plan, anns) = build_plan();
    let ctx: BTreeMap<String, Value> = BTreeMap::new();
    let g_ctx = Context::new();

    // A chunky statically-integer expression, so per-op cost dominates loop overhead.
    let expr = "1 + 2 * 3 + 4 * 5 - 6 + 7 * 8 - 9 * 2 + 3 * 4";
    let src = format!("{{{{ {expr} }}}}");
    let report = parse_prepared_weavy_with_report(&plan, &parser, &table, &src).unwrap();
    let resolved = report.accepted_resolved_tree(&parser, &src).unwrap();
    let ast = expr_via_jit(find_expr(&resolved).unwrap(), &anns);

    assert!(is_static_int(&ast), "expr should be statically integer");

    // Boxed path (Value ops) and unboxed path (i64 ops) from the SAME generated AST.
    let mut boxed_ops = Vec::new();
    lower_eval(&ast, &mut boxed_ops);
    let mut int_ops = Vec::new();
    lower_int(&ast, &mut int_ops);

    // Host-call arrays — kept alive in this fn scope for as long as the programs can run.
    let boxed_calls: Vec<weavy::jit::HostCallInfo> = boxed_ops
        .iter()
        .map(|op| weavy::jit::HostCallInfo { info: core::ptr::from_ref(op).cast(), call: eval_jit_apply })
        .collect();
    let boxed_prog = build_native(&boxed_calls);
    let int_calls = int_calls(&int_ops);
    let int_prog = build_native(&int_calls);

    // Oracle: both agree with gingembre.
    let native = block_on(gingembre::eval_expression(expr, &g_ctx)).unwrap();
    let native_i64 = native.as_number().and_then(|n| n.to_i64()).unwrap();
    let boxed_val = run_eval_via_weavy_jit(&boxed_ops, &ctx);
    let unboxed = {
        let mut m = IntMachine { stack: Vec::new() };
        let mut cx = HostCallCtx::new(int_prog.entry_prog(), &mut m);
        let entry = unsafe { int_prog.entry_fn::<HostCallCtx<IntMachine>>() };
        unsafe { entry(&mut cx) };
        m.stack.pop().unwrap()
    };
    assert_eq!(boxed_val.as_number().and_then(|n| n.to_i64()), Some(native_i64));
    assert_eq!(unboxed, native_i64);
    println!("expr: {expr}");
    println!("  = {native_i64}  (unboxed i64 == boxed Value == gingembre)\n");

    // Run each JIT program (compiled once above) many times, time run-only.
    let m = 1_000_000u32;

    let boxed_entry = unsafe { boxed_prog.entry_fn::<HostCallCtx<EvalMachine>>() };
    let t = Instant::now();
    for _ in 0..m {
        let mut mm = EvalMachine { stack: Vec::new(), ctx: ctx.clone() };
        let mut cx = HostCallCtx::new(boxed_prog.entry_prog(), &mut mm);
        unsafe { boxed_entry(&mut cx) };
        std::hint::black_box(mm.stack.pop());
    }
    let boxed_ns = t.elapsed().as_nanos() / m as u128;

    let int_entry = unsafe { int_prog.entry_fn::<HostCallCtx<IntMachine>>() };
    let t = Instant::now();
    for _ in 0..m {
        let mut mm = IntMachine { stack: Vec::new() };
        let mut cx = HostCallCtx::new(int_prog.entry_prog(), &mut mm);
        unsafe { int_entry(&mut cx) };
        std::hint::black_box(mm.stack.pop());
    }
    let int_hostcall_ns = t.elapsed().as_nanos() / m as u128;

    // (d) REAL copy-and-patch: dedicated per-op stencils, no hostcall. Oracle it, then time it.
    let cp = build_intop_native(&int_ops).expect("copy-and-patch stencils available");
    let mut stack = vec![0i64; 256];
    assert_eq!(run_intop_native(&cp, &mut stack), native_i64, "copy-and-patch result must match");
    let t = Instant::now();
    for _ in 0..m {
        std::hint::black_box(run_intop_native(&cp, &mut stack));
    }
    let cp_ns = t.elapsed().as_nanos() / m as u128;

    println!("JIT run-only, {} ops, avg over {m} iters:", int_ops.len());
    println!("  (a) boxed  Value ops, HOSTCALL chain   : {boxed_ns:>5} ns");
    println!("  (b) unboxed i64 ops,  HOSTCALL chain    : {int_hostcall_ns:>5} ns");
    println!("  (c) unboxed i64 ops,  COPY-AND-PATCH    : {cp_ns:>5} ns  <- dedicated stencils, no call");
    println!(
        "\n  unboxing (a->b): {:.1}x   copy-and-patch over hostcall (b->c): {:.1}x   total (a->c): {:.1}x",
        boxed_ns as f64 / int_hostcall_ns as f64,
        int_hostcall_ns as f64 / cp_ns as f64,
        boxed_ns as f64 / cp_ns as f64,
    );
    println!(
        "\n(a)->(b) is the V8 SMI win: the grammar proved these are integers, so no Value/tag.\n\
         (b)->(c) is REAL copy-and-patch: each op is a dedicated compiled stencil (native add,\n\
         no fn-pointer call back into Rust), chained by patching the weavy_cont hole. This is\n\
         the lane phon-jit uses; the earlier '--eval/--ast JIT' were HOSTCALL chains, not this."
    );
}

/// JITMAP: make JIT'd stencil code observable. A copy-and-patch program is otherwise an
/// anonymous executable blob (invisible to perf/lldb/stax); but we control the assembly, so we
/// emit a `code-offset -> op` symbol map — the foundation for BOTH profiling (symbolicate samples)
/// and debuggability (fault/deopt address -> which op / template expression). Also surfaces the
/// serialization property: a pure-stencil program has no absolute pointers, so it's a
/// self-contained relocatable blob (an AOT/JIT-cache artifact).
/// Lower a statically-integer expression to `(op, source byte range)` by walking the PARSE tree
/// (which carries byte ranges). A binary op's span is its whole sub-expression; a push's span is
/// the literal — so the JIT map nests like a flame graph over the template source.
fn lower_int_spanned<N: ParseNode>(node: &N, anns: &Annotations, out: &mut Vec<(IntOp, (usize, usize))>) {
    let mut node = node;
    while anns.get(node.kind()).is_some_and(|a| a.transparent) {
        node = node.children().iter().find(|c| c.named()).expect("transparent inner named child");
    }
    match node.kind() {
        "binary" => {
            lower_int_spanned(select_child(node, "named:0"), anns, out);
            lower_int_spanned(select_child(node, "named:1"), anns, out);
            let op = match leaf_text(select_child(node, "token")).as_deref() {
                Some("+") => IntOp::Add,
                Some("-") => IntOp::Sub,
                Some("*") => IntOp::Mul,
                other => panic!("non-int op {other:?} in spanned lowering"),
            };
            out.push((op, node.byte_range()));
        }
        "number" => {
            let n = leaf_text(node).and_then(|t| t.trim().parse::<i64>().ok()).unwrap_or(0);
            out.push((IntOp::Push(n), node.byte_range()));
        }
        other => panic!("unsupported node {other:?} in spanned int lowering"),
    }
}

/// One emitted stencil's JIT-map row: code offset, continuation relocs, op symbol, source span.
struct MapRow {
    start: usize,
    cont: &'static [usize],
    label: String,
    span: (usize, usize),
}

fn jitmap() {
    use weavy::jit::{NativeProgram, StencilLayout};
    let (parser, table, plan, anns) = build_plan();

    let expr = "1 + 2 * 3 + 4 * 5";
    let src = format!("{{{{ {expr} }}}}");
    let report = parse_prepared_weavy_with_report(&plan, &parser, &table, &src).unwrap();
    let resolved = report.accepted_resolved_tree(&parser, &src).unwrap();
    let node = find_expr(&resolved).unwrap();

    // Lower straight off the parse tree so every op carries its SOURCE byte range.
    let mut spanned = Vec::new();
    lower_int_spanned(node, &anns, &mut spanned);

    let mut layout = StencilLayout::new();
    let root = layout.start_chain();
    let mut rows: Vec<MapRow> = Vec::new();
    for (i, (op, span)) in spanned.iter().enumerate() {
        let (bytes, cont, label): (&[u8], &'static [usize], String) = match op {
            IntOp::Push(n) => {
                layout.push_prog_word(root.prog_index, *n as u64);
                (intop_stencils::PUSH, intop_stencils::PUSH_CONT, format!("op{i}_push_{n}"))
            }
            IntOp::Add => (intop_stencils::ADD, intop_stencils::ADD_CONT, format!("op{i}_add")),
            IntOp::Sub => (intop_stencils::SUB, intop_stencils::SUB_CONT, format!("op{i}_sub")),
            IntOp::Mul => (intop_stencils::MUL, intop_stencils::MUL_CONT, format!("op{i}_mul")),
        };
        let start = layout.emit_stencil(bytes);
        rows.push(MapRow { start, cont, label, span: *span });
    }
    let done = layout.emit_stencil(intop_stencils::DONE);
    for i in 0..rows.len() {
        let next = rows.get(i + 1).map(|r| r.start).unwrap_or(done);
        for &rel in rows[i].cont {
            layout.patch_continuation(rows[i].start + rel, next);
        }
    }
    let prog_words = layout.prog(root.prog_index).to_vec();
    let code_len = layout.code_len();
    let native = NativeProgram::new(layout, root);
    let base = native.code_ptr() as usize;

    println!("template: {src:?}\nexpr: {expr}   ({} ops, {code_len}B native @ {base:#x})\n", spanned.len());
    println!("--- source-mapped JIT symbols:  addr  size  symbol  ->  template source ---");
    let mut perf = String::new();
    for i in 0..rows.len() {
        let r = &rows[i];
        let end = rows.get(i + 1).map(|n| n.start).unwrap_or(done);
        let source = &src[r.span.0..r.span.1];
        println!("{:016x} {:<3x} jit::{:<12} -> {source:?}  @ {}..{}", base + r.start, end - r.start, r.label, r.span.0, r.span.1);
        // perf `/tmp/perf-<pid>.map` line: <hex addr> <hex size> <symbol>
        perf.push_str(&format!("{:x} {:x} jit::{} [{}]\n", base + r.start, end - r.start, r.label, source));
    }
    let pid = std::process::id();
    let path = format!("/tmp/perf-{pid}.map");
    let wrote = std::fs::write(&path, &perf).is_ok();

    println!(
        "\nWrote {path} ({}): `perf report` symbolicates JIT frames from this — a sampled PC in a\n\
         stencil's range resolves to the op AND the template sub-expression (nested spans = a\n\
         source flame graph). Same map answers a debugger: a fault/deopt PC -> exact template text.\n\
         stax can consume the same offset->source table.",
        if wrote { "ok" } else { "write failed" },
    );
    println!(
        "\nSerialization: prog stream is {} immediates ({prog_words:?}), ZERO host pointers, branches\n\
         PC-relative -> a self-contained relocatable blob (serialize once, reload anywhere as an\n\
         AOT/JIT cache). Hostcall lane can't (prog holds HostCallInfo pointers). Reload needs one\n\
         weavy API: NativeProgram::from_parts(code, progs, entry).",
        prog_words.len(),
    );

    let mut stack = vec![0i64; 64];
    println!("\n(run check: {expr} = {})", run_intop_native(&native, &mut stack));
}

/// SERIALIZE: cache a compiled template with phon. Bytecode vs native: we serialize the PORTABLE,
/// Facet-derived forms (the generated AST, and the op stream) with phon and RE-JIT on load — cheap
/// (~µs), portable across arch/OS, and safe (recompiled from trusted stencils, not trusting raw
/// code bytes). Native code is arch-locked and only a same-machine cache.
fn serialize() {
    use weavy::jit::StencilLayout;
    let (parser, table, plan, anns) = build_plan();

    let expr = "1 + 2 * 3 + 4 * 5 - 6 + 7 * 8";
    let src = format!("{{{{ {expr} }}}}");
    let report = parse_prepared_weavy_with_report(&plan, &parser, &table, &src).unwrap();
    let resolved = report.accepted_resolved_tree(&parser, &src).unwrap();
    let ast = expr_via_jit(find_expr(&resolved).unwrap(), &anns);
    let mut ops = Vec::new();
    lower_int(&ast, &mut ops);

    // phon-serialize the two portable, Facet-derived artifacts.
    let ast_bytes = phon::api::encode(&ast).expect("phon encode AST");
    let ops_bytes = phon::api::encode(&ops).expect("phon encode ops");

    // Round-trip both back through phon.
    let ast2: gen_ast::Expr = phon::api::decode(&ast_bytes).expect("phon decode AST");
    let ops2: Vec<IntOp> = phon::api::decode(&ops_bytes).expect("phon decode ops");
    assert_eq!(ast, ast2, "AST must survive phon round-trip");
    assert_eq!(ops, ops2, "op bytecode must survive phon round-trip");

    // Native code size, for the comparison (build a layout to measure).
    let mut layout = StencilLayout::new();
    let root = layout.start_chain();
    for op in &ops {
        let bytes = match op {
            IntOp::Push(n) => {
                layout.push_prog_word(root.prog_index, *n as u64);
                intop_stencils::PUSH
            }
            IntOp::Add => intop_stencils::ADD,
            IntOp::Sub => intop_stencils::SUB,
            IntOp::Mul => intop_stencils::MUL,
        };
        layout.emit_stencil(bytes);
    }
    layout.emit_stencil(intop_stencils::DONE);
    let native_len = layout.code_len();

    // The "reload" path: decode op bytecode -> re-JIT -> run. This is what a cache load does.
    let native = build_intop_native(&ops2).expect("re-JIT from decoded bytecode");
    let mut stack = vec![0i64; 64];
    let result = run_intop_native(&native, &mut stack);

    println!("expr: {expr}   ({} ops)\n", ops.len());
    println!("artifact sizes:");
    println!("  template source        : {:>4} B  (most portable; re-parse+lower+JIT on load)", src.len());
    println!("  phon AST bytecode      : {:>4} B  (Facet gen_ast::Expr; re-lower+JIT on load)", ast_bytes.len());
    println!("  phon op bytecode       : {:>4} B  (Facet Vec<IntOp>; re-JIT only, ~µs)", ops_bytes.len());
    println!("  native code (this arch): {:>4} B  (arch-locked; mmap+run, needs from_parts)", native_len);
    println!("\nreloaded phon op bytecode -> re-JIT -> {expr} = {result}  (round-trip verified)");
    println!(
        "\nbytecode vs native: serialize the PORTABLE Facet forms with phon (AST or ops) and re-JIT\n\
         on load. It's portable across arch/OS, SAFE (recompiled from trusted stencils, not\n\
         executing cached bytes), and re-JIT is ~µs (amortizes instantly). Native code is a\n\
         same-machine optimization only: arch/OS-locked and you'd be trusting raw bytes as code.\n\
         The op stream IS the bytecode; phon (itself a copy-and-patch codec) carries it as data.\n\
         Nice recursion: phon's JIT decodes the bytes that drive our JIT."
    );
}

/// Write a perf jitdump (`/tmp/jit-<pid>.dump`) that stax's jitdump tailer consumes to
/// symbolicate + annotate JIT'd code. One `JIT_CODE_LOAD` per op: name = source snippet, bytes =
/// the op's actual (patched) runtime code at its address. Format: 40-byte header (magic 0x4A695444)
/// then records `id(u32) total_size(u32) timestamp(u64)` + payload.
fn write_jitdump(path: &str, records: &[(u64, u64, String, Vec<u8>)]) -> std::io::Result<()> {
    let pid = std::process::id();
    let ts = || {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64
    };
    let mut out = Vec::new();
    // Header (40 bytes): magic, version, header_size, elf_mach(EM_AARCH64=183), pad, pid, ts, flags.
    out.extend_from_slice(&0x4A69_5444u32.to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&183u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&pid.to_le_bytes());
    out.extend_from_slice(&ts().to_le_bytes());
    out.extend_from_slice(&0u64.to_le_bytes());
    for (i, (addr, size, name, code)) in records.iter().enumerate() {
        let mut payload = Vec::new();
        payload.extend_from_slice(&pid.to_le_bytes()); // pid
        payload.extend_from_slice(&pid.to_le_bytes()); // tid
        payload.extend_from_slice(&addr.to_le_bytes()); // vma (stax uses this)
        payload.extend_from_slice(&addr.to_le_bytes()); // code_addr
        payload.extend_from_slice(&size.to_le_bytes()); // code_size
        payload.extend_from_slice(&(i as u64).to_le_bytes()); // code_index
        payload.extend_from_slice(name.as_bytes());
        payload.push(0);
        payload.extend_from_slice(code);
        let total = 16 + payload.len();
        out.extend_from_slice(&0u32.to_le_bytes()); // id = JIT_CODE_LOAD
        out.extend_from_slice(&(total as u32).to_le_bytes());
        out.extend_from_slice(&ts().to_le_bytes());
        out.extend_from_slice(&payload);
    }
    std::fs::write(path, &out)
}

/// Re-parse a jitdump (mirroring stax's `parse_code_load`) to self-verify it's well-formed.
fn validate_jitdump(bytes: &[u8]) -> Vec<(u64, u64, String)> {
    let mut out = Vec::new();
    let magic = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
    assert!(magic == 0x4A69_5444 || magic == 0x4474_694A, "bad jitdump magic {magic:#x}");
    let mut cur = 40;
    while cur + 16 <= bytes.len() {
        let id = u32::from_le_bytes(bytes[cur..cur + 4].try_into().unwrap());
        let total = u32::from_le_bytes(bytes[cur + 4..cur + 8].try_into().unwrap()) as usize;
        if total < 16 || cur + total > bytes.len() {
            break;
        }
        if id == 0 {
            let p = &bytes[cur + 16..cur + total];
            let avma = u64::from_le_bytes(p[8..16].try_into().unwrap());
            let size = u64::from_le_bytes(p[24..32].try_into().unwrap());
            let nul = p[40..].iter().position(|&b| b == 0).unwrap();
            let name = String::from_utf8_lossy(&p[40..40 + nul]).into_owned();
            out.push((avma, size, name));
        }
        cur += total;
    }
    out
}

/// HOT: JIT a chunky integer expression, emit a source-named jitdump, then run the JIT'd code in a
/// hot loop so stax (`stax record --pid <pid>`) can sample + symbolicate + annotate it.
fn hot(secs: u64) {
    use std::time::{Duration, Instant};
    use weavy::jit::{NativeProgram, StencilLayout};
    let (parser, table, plan, anns) = build_plan();

    let expr = "1 + 2 * 3 + 4 * 5 - 6 + 7 * 8 - 9 * 2 + 3 * 4 + 5 * 6 - 7 + 8 * 9 - 1 * 2 + 3 * 4";
    let src = format!("{{{{ {expr} }}}}");
    let report = parse_prepared_weavy_with_report(&plan, &parser, &table, &src).unwrap();
    let resolved = report.accepted_resolved_tree(&parser, &src).unwrap();
    let node = find_expr(&resolved).unwrap();
    let mut spanned = Vec::new();
    lower_int_spanned(node, &anns, &mut spanned);

    let mut layout = StencilLayout::new();
    let root = layout.start_chain();
    let mut rows: Vec<MapRow> = Vec::new();
    for (i, (op, span)) in spanned.iter().enumerate() {
        let (bytes, cont, label): (&[u8], &'static [usize], String) = match op {
            IntOp::Push(n) => {
                layout.push_prog_word(root.prog_index, *n as u64);
                (intop_stencils::PUSH, intop_stencils::PUSH_CONT, format!("op{i}_push"))
            }
            IntOp::Add => (intop_stencils::ADD, intop_stencils::ADD_CONT, format!("op{i}_add")),
            IntOp::Sub => (intop_stencils::SUB, intop_stencils::SUB_CONT, format!("op{i}_sub")),
            IntOp::Mul => (intop_stencils::MUL, intop_stencils::MUL_CONT, format!("op{i}_mul")),
        };
        let start = layout.emit_stencil(bytes);
        rows.push(MapRow { start, cont, label, span: *span });
    }
    let done = layout.emit_stencil(intop_stencils::DONE);
    for i in 0..rows.len() {
        let next = rows.get(i + 1).map(|r| r.start).unwrap_or(done);
        for &rel in rows[i].cont {
            layout.patch_continuation(rows[i].start + rel, next);
        }
    }
    let native = NativeProgram::new(layout, root);
    let base = native.code_ptr() as usize;

    // Build jitdump records from the ACTUAL patched runtime bytes at each op's address.
    let mut records: Vec<(u64, u64, String, Vec<u8>)> = Vec::new();
    for i in 0..rows.len() {
        let r = &rows[i];
        let end = rows.get(i + 1).map(|n| n.start).unwrap_or(done);
        let size = end - r.start;
        let code = unsafe { std::slice::from_raw_parts(native.code_ptr().add(r.start), size) }.to_vec();
        let name = format!("jit::{} [{}]", r.label, &src[r.span.0..r.span.1]);
        records.push(((base + r.start) as u64, size as u64, name, code));
    }
    let pid = std::process::id();
    let path = format!("/tmp/jit-{pid}.dump");
    write_jitdump(&path, &records).expect("write jitdump");

    // Self-verify: re-parse the dump the way stax does.
    let parsed = validate_jitdump(&std::fs::read(&path).unwrap());
    assert_eq!(parsed.len(), records.len(), "jitdump round-trip record count");
    println!(
        "wrote {path}: {} JIT_CODE_LOAD records, self-validated (stax-format).\n\
         first: {:#x} size {:#x} {:?}\n",
        parsed.len(),
        parsed[0].0,
        parsed[0].1,
        parsed[0].2,
    );
    println!(
        "pid = {pid}. In another shell, profile the JIT'd code:\n\
         \x20 stax record --pid {pid}\n\
         \x20 stax wait --for-samples 20000 && stax top -n 12 --sort self\n\
         \x20 stax annotate 'jit::op6_mul*'      # per-instruction samples on a JIT'd stencil\n"
    );

    // Hot loop: keep the JIT'd program executing so there's something to sample.
    let native = &native;
    let mut stack = vec![0i64; 128];
    let deadline = Instant::now() + Duration::from_secs(secs);
    let mut iters = 0u64;
    let mut acc = 0i64;
    while Instant::now() < deadline {
        for _ in 0..50_000 {
            acc = acc.wrapping_add(run_intop_native(native, &mut stack));
            iters += 1;
        }
    }
    println!("ran {iters} JIT iterations over {secs}s (checksum {acc}). {expr} = {}", run_intop_native(native, &mut stack));
}

/// FUSE: prove the AST materializer is decoupled from snark's rich tree — it needs only the
/// `ParseNode` interface (kind/named/text/children). Build the generated AST over BOTH the rich
/// `RuntimeResolvedNode` and a lightweight `LeanNode` (what a lean parse->AST driver emits) and
/// show they're identical. This is the concrete interface + proof for PL core's lean driver.
fn fuse() {
    let (parser, table, plan, anns) = build_plan();
    for expr in ["1 + 2 * 3", "x + 1", "10 - 2 - 3", "2 * 3 > 5"] {
        let src = format!("{{{{ {expr} }}}}");
        let report = parse_prepared_weavy_with_report(&plan, &parser, &table, &src).unwrap();
        let resolved = report.accepted_resolved_tree(&parser, &src).unwrap();
        let rich = find_expr(&resolved).expect("no expr");

        // Materialize over the RICH resolved tree, then over a LEAN projection of it.
        let via_rich = expr_via_jit(rich, &anns);
        let lean = to_lean(rich);
        let via_lean = expr_via_jit(&lean, &anns);

        assert_eq!(via_rich, via_lean, "lean-node materialization must match rich for {expr:?}");
        let (rich_nodes, lean_bytes) = (count_nodes(rich), lean_footprint(&lean));
        println!("✓ {expr:<12} rich == lean -> {via_rich:?}   (lean node: {rich_nodes} nodes, ~{lean_bytes}B)");
    }
    println!(
        "\n✓ materialization needs ONLY kind/named/text/children (the ParseNode trait).\n\
         RuntimeResolvedNode and the lightweight LeanNode yield the IDENTICAL generated AST.\n\
         A lean parse->AST driver builds LeanNode-shaped nodes directly on each reduce — no\n\
         sexp, no tree_store, no trace/tree events — and feeds this same Shape-driven build().\n\
         See notes/gingembre-snark-lean-parse.md for the full driver design + handoff."
    );
}

fn count_nodes<N: ParseNode>(n: &N) -> usize {
    1 + n.children().iter().map(count_nodes).sum::<usize>()
}

/// Rough heap footprint of a LeanNode subtree (illustrative — kind strings + child vecs).
fn lean_footprint(n: &LeanNode) -> usize {
    let self_bytes = std::mem::size_of::<LeanNode>()
        + n.kind.len()
        + n.text.as_ref().map(String::len).unwrap_or(0);
    self_bytes + n.children.iter().map(lean_footprint).sum::<usize>()
}

/// PROFILE: quantify what the rich parse collects, and split parse (report) vs resolved-tree
/// materialization. Tests the hypothesis that the parse is slow because it collects observ-
/// ability data (trace_events/tree_events/sexp) instead of building the AST efficiently.
fn profile() {
    use std::time::Instant;
    let (parser, table, plan, _anns) = build_plan();

    let cases = [
        "{{ 1 + 2 * 3 }}",
        "{% if user.active %}on{% else %}off{% endif %}",
        "{% for x in [1, 2, 3] %}{{ x }};{% endfor %}",
    ];
    let n = 20_000u32;
    println!(
        "{:<48} {:>6} {:>4} {:>6} {:>6} {:>9} {:>9}",
        "input", "steps", "fork", "trace", "tree", "parse_ns", "tree_ns"
    );
    for src in cases {
        // One-shot: collection volume + peak live GLR branches.
        let report = parse_prepared_weavy_with_report(&plan, &parser, &table, src).unwrap();
        let stats = report.stats();
        let n_trace = report.trace_events().len();
        let n_tree = report.tree_events().len();
        let forks = report.max_live_versions();

        // Phase A: parse -> report.
        let t = Instant::now();
        for _ in 0..n {
            std::hint::black_box(
                parse_prepared_weavy_with_report(&plan, &parser, &table, src).unwrap(),
            );
        }
        let parse_ns = t.elapsed().as_nanos() / n as u128;

        // Phase B: report -> resolved tree (what we actually materialize from).
        let t = Instant::now();
        for _ in 0..n {
            let report = parse_prepared_weavy_with_report(&plan, &parser, &table, src).unwrap();
            std::hint::black_box(report.accepted_resolved_tree(&parser, src));
        }
        let a_and_b_ns = t.elapsed().as_nanos() / n as u128;
        let tree_ns = a_and_b_ns.saturating_sub(parse_ns);

        println!(
            "{src:<48} {:>6} {forks:>4} {n_trace:>6} {n_tree:>6} {parse_ns:>9} {tree_ns:>9}",
            stats.step_count
        );
    }
    println!(
        "\nfork = peak live GLR branches. If fork is ~1, every per-step branch.clone() (full LR\n\
         stack copy) is pure overhead, and the trace/tree events are collected for nothing.\n\
         The rich GLR+observability path is paying for ambiguity/recovery that valid input\n\
         never uses -> a lean single-stack parse->AST lowering should win big; keep the rich\n\
         parse only as the error/ambiguity fallback."
    );
}

/// Item 3: the reflection ops as a flat Weavy program. Emitting these (instead of driving
/// `Partial` directly) is what lets materialization run through weavy — interpreter or a
/// hostcall chain here; a dedicated-stencil copy-and-patch backend (like `build_intop_native`)
/// is the path to true compilation. Same substrate as facet-json's `weavy_deser`.
#[derive(Clone, Debug)]
enum BuildOp {
    SelectVariant(String),
    BeginNthField(usize),
    BeginField(String),
    BeginSmartPtr,
    SetI64(i64),
    SetStr(String),
    End,
}

/// Apply one build op to the `Partial`. The SINGLE source of op semantics — shared by the
/// interpreter (`AstBuilder::step`) and the hostcall-chain JIT intrinsic, so both
/// backends run identical logic.
fn apply_op<'f>(
    p: facet_reflect::Partial<'f, false>,
    op: &BuildOp,
) -> Result<facet_reflect::Partial<'f, false>, facet_reflect::ReflectError> {
    match op {
        BuildOp::SelectVariant(v) => p.select_variant_named(v),
        BuildOp::BeginNthField(i) => p.begin_nth_field(*i),
        BuildOp::BeginField(f) => p.begin_field(f),
        BuildOp::BeginSmartPtr => p.begin_smart_ptr(),
        BuildOp::SetI64(v) => p.set(*v),
        BuildOp::SetStr(s) => p.set(s.clone()),
        BuildOp::End => p.end(),
    }
}

struct AstBuilder<'f> {
    partial: Option<facet_reflect::Partial<'f, false>>,
}

impl<'p, 'f> weavy::Step<'p, u32, BuildOp> for AstBuilder<'f> {
    type Error = facet_reflect::ReflectError;
    type Continuation = ();
    fn step(
        &mut self,
        op: &'p BuildOp,
    ) -> Result<weavy::Control<'p, u32, BuildOp, ()>, Self::Error> {
        let p = self.partial.take().expect("partial present");
        self.partial = Some(apply_op(p, op)?);
        Ok(weavy::Control::Continue)
    }
}

/// Run a flat Weavy program of build ops through the weavy runtime, materializing `T`.
fn run_ops_via_weavy<T: facet::Facet<'static>>(ops: &[BuildOp]) -> T {
    let mut builder = AstBuilder {
        partial: Some(facet_reflect::Partial::alloc_owned::<T>().expect("alloc")),
    };
    let lowered = weavy::Lowered::<u32, BuildOp>::new(ops.to_vec());
    weavy::run(&lowered, &mut builder).expect("weavy run");
    builder
        .partial
        .take()
        .unwrap()
        .build()
        .expect("build")
        .materialize::<T>()
        .expect("materialize")
}

/// Threaded state for the JIT chain: the moving `Partial`, plus a slot to stash a reflect
/// error (the host intrinsic can't unwind through the copied native code, so it returns
/// `false` and leaves the error here).
struct JitState<'f> {
    partial: Option<facet_reflect::Partial<'f, false>>,
    error: Option<facet_reflect::ReflectError>,
}

/// The consumer intrinsic copied at every host-call site: take the moving `Partial`, apply
/// this op, put it back. Returning `false` halts the copied chain (weavy's HOSTCALL ABI).
unsafe extern "C" fn jit_apply(cx: *mut (), info: *const ()) -> bool {
    let state = unsafe { &mut *cx.cast::<JitState>() };
    let op = unsafe { &*info.cast::<BuildOp>() };
    let Some(p) = state.partial.take() else {
        return false;
    };
    match apply_op(p, op) {
        Ok(p) => {
            state.partial = Some(p);
            true
        }
        Err(e) => {
            state.error = Some(e);
            false
        }
    }
}

/// Materialize `T` by JIT-compiling the build-op program as a weavy HOSTCALL chain: each op is
/// a copied `HOSTCALL` stencil in one native chain (control flow is machine code, patched
/// site-to-site), and each site dispatches to `jit_apply`. NB: this is the hostcall lane — op
/// bodies stay interpreted; it removes dispatch, not op cost. Same shape as facet-json's
/// `from_str_weavy_jit`. For TRUE copy-and-patch (dedicated per-op stencils, native op, no
/// call) see `build_intop_native`. Falls back to the interpreter where JIT isn't available.
fn run_ops_via_weavy_jit<T: facet::Facet<'static>>(ops: &[BuildOp]) -> T {
    use weavy::jit::{HostCallCtx, HostCallInfo, NativeProgram, StencilLayout};

    if !weavy::jit::NATIVE_COPY_PATCH_AVAILABLE {
        return run_ops_via_weavy(ops);
    }

    // Per-op host-call metadata: `info` points at the owned `BuildOp`, `call` is our
    // intrinsic. These vectors must outlive the native run (the copied code reads them).
    let calls: Vec<HostCallInfo> = ops
        .iter()
        .map(|op| HostCallInfo {
            info: core::ptr::from_ref(op).cast(),
            call: jit_apply,
        })
        .collect();

    // Copy one HOSTCALL stencil per op into a single chain, then a terminal DONE; patch each
    // site's continuation to the next site (the last one to DONE).
    let mut layout = StencilLayout::new();
    let root = layout.start_chain();
    let mut sites = Vec::with_capacity(calls.len());
    for call in &calls {
        sites.push(layout.emit_hostcall(root, core::ptr::from_ref(call)));
    }
    let done = layout.emit_done();
    for i in 0..sites.len() {
        let target = sites.get(i + 1).copied().unwrap_or(done);
        layout.patch_hostcall_continuation(sites[i], target);
    }

    let native = NativeProgram::new(layout, root);
    let mut state = JitState {
        partial: Some(facet_reflect::Partial::alloc_owned::<T>().expect("alloc")),
        error: None,
    };
    let mut cx = HostCallCtx::new(native.entry_prog(), &mut state);
    let entry = unsafe { native.entry_fn::<HostCallCtx<JitState>>() };
    unsafe { entry(&mut cx) };

    if let Some(e) = state.error {
        panic!("jit build op failed: {e}");
    }
    state
        .partial
        .take()
        .expect("partial present after jit chain")
        .build()
        .expect("build")
        .materialize::<T>()
        .expect("materialize")
}

/// The one generic builder. Dispatches on the target facet Shape (the type supplies
/// structure); the tree supplies data; the grammar `Annotations` supply the mapping. It
/// records a flat `BuildOp` program as it goes — that program IS the Weavy IR for the
/// materialization (see `run_ops_via_weavy`).
/// The MINIMAL node interface the AST materializer needs: kind, named-ness, leaf text, ordered
/// children. `RuntimeResolvedNode` (the rich parse's resolved tree) implements it, and so does
/// the lightweight `LeanNode` a lean parse->AST driver would emit — proving materialization is
/// decoupled from snark's rich tree machinery. THIS is the interface PL core's lean driver
/// implements: build LeanNode-shaped nodes on reduce (no sexp/tree_store/trace/tree events),
/// feed this same materializer.
trait ParseNode: Sized {
    fn kind(&self) -> &str;
    fn named(&self) -> bool;
    fn text(&self) -> Option<&str>;
    fn children(&self) -> &[Self];
    /// Half-open source byte range `[start, end)` — for source-mapped diagnostics/JIT profiling.
    fn byte_range(&self) -> (usize, usize);
}

impl ParseNode for RuntimeResolvedNode {
    fn kind(&self) -> &str {
        RuntimeResolvedNode::kind(self)
    }
    fn named(&self) -> bool {
        RuntimeResolvedNode::named(self)
    }
    fn text(&self) -> Option<&str> {
        RuntimeResolvedNode::text(self)
    }
    fn children(&self) -> &[Self] {
        RuntimeResolvedNode::children(self)
    }
    fn byte_range(&self) -> (usize, usize) {
        let b = RuntimeResolvedNode::bytes(self);
        (b.start().get() as usize, b.end().get() as usize)
    }
}

/// A lightweight parse node — kind + text + ordered children, nothing else. What a lean
/// deterministic parse->AST driver emits per reduce. Built here FROM a `RuntimeResolvedNode`
/// only to prove the materializer is byte-identical over it; the real driver builds it directly.
#[derive(Debug, Clone)]
struct LeanNode {
    kind: String,
    named: bool,
    text: Option<String>,
    range: (usize, usize),
    children: Vec<LeanNode>,
}

impl ParseNode for LeanNode {
    fn kind(&self) -> &str {
        &self.kind
    }
    fn named(&self) -> bool {
        self.named
    }
    fn text(&self) -> Option<&str> {
        self.text.as_deref()
    }
    fn children(&self) -> &[Self] {
        &self.children
    }
    fn byte_range(&self) -> (usize, usize) {
        self.range
    }
}

/// Project any `ParseNode` into a `LeanNode` (simulates what a lean driver would emit directly).
fn to_lean<N: ParseNode>(node: &N) -> LeanNode {
    LeanNode {
        kind: node.kind().to_string(),
        named: node.named(),
        text: node.text().map(str::to_string),
        range: node.byte_range(),
        children: node.children().iter().map(to_lean).collect(),
    }
}

fn build<'f, N: ParseNode>(
    mut p: facet_reflect::Partial<'f, false>,
    node: &N,
    anns: &Annotations,
    ops: &mut Vec<BuildOp>,
) -> Result<facet_reflect::Partial<'f, false>, facet_reflect::ReflectError> {
    use facet_core::{Def, Type, UserType};

    // `transparent` nodes (annotation) are structural noise — descend to the inner named child.
    let mut node = node;
    while anns.get(node.kind()).is_some_and(|a| a.transparent) {
        node = node
            .children()
            .iter()
            .find(|c| c.named())
            .expect("transparent node has no inner named child");
    }

    // Smart pointer (Box<...>): step through it, build the pointee, pop back.
    if matches!(p.shape().def, Def::Pointer(_)) {
        ops.push(BuildOp::BeginSmartPtr);
        p = p.begin_smart_ptr()?;
        p = build(p, node, anns, ops)?;
        ops.push(BuildOp::End);
        return p.end();
    }
    match p.shape().ty {
        Type::User(UserType::Enum(_)) => {
            let variant = anns
                .get(node.kind())
                .and_then(|a| a.as_variant.as_deref())
                .unwrap_or_else(|| panic!("no `as` variant annotation for node {:?}", node.kind()));
            ops.push(BuildOp::SelectVariant(variant.to_string()));
            p = p.select_variant_named(variant)?;
            ops.push(BuildOp::BeginNthField(0));
            p = p.begin_nth_field(0)?; // newtype variant payload
            p = build(p, node, anns, ops)?;
            ops.push(BuildOp::End);
            p = p.end()?;
        }
        Type::User(UserType::Struct(st)) => {
            let ann = anns
                .get(node.kind())
                .unwrap_or_else(|| panic!("no annotation for struct node {:?}", node.kind()));
            for field in st.fields.iter() {
                let field_ann = ann.fields.get(field.name).unwrap_or_else(|| {
                    panic!("no field mapping for {}.{}", node.kind(), field.name)
                });
                let child = select_child(node, &field_ann.from);
                ops.push(BuildOp::BeginField(field.name.to_string()));
                p = p.begin_field(field.name)?;
                p = build(p, child, anns, ops)?;
                ops.push(BuildOp::End);
                p = p.end()?;
            }
        }
        _ => {
            // Scalar leaf: set from the node's text per the target scalar type.
            let text = leaf_text(node).unwrap_or_default();
            if p.shape().is_type::<i64>() {
                let v = text.trim().parse::<i64>().unwrap_or(0);
                ops.push(BuildOp::SetI64(v));
                p = p.set(v)?;
            } else if p.shape().is_type::<String>() {
                ops.push(BuildOp::SetStr(text.clone()));
                p = p.set(text)?;
            } else {
                panic!("unsupported scalar shape {}", p.shape());
            }
        }
    }
    Ok(p)
}

/// Resolve an annotation child selector against a node: `named:N` = the Nth named child,
/// `token` = the (first) anonymous token child.
fn select_child<'a, N: ParseNode>(node: &'a N, selector: &str) -> &'a N {
    if let Some(idx) = selector.strip_prefix("named:") {
        let idx: usize = idx.parse().expect("bad named index");
        node.children()
            .iter()
            .filter(|c| c.named())
            .nth(idx)
            .unwrap_or_else(|| panic!("no named child {idx} on {:?}", node.kind()))
    } else if selector == "token" {
        node.children()
            .iter()
            .find(|c| !c.named())
            .unwrap_or_else(|| panic!("no token child on {:?}", node.kind()))
    } else {
        panic!("unknown child selector {selector:?}")
    }
}

// (The AST codegen lives in build.rs now — it writes $OUT_DIR/gingembre_ast.rs from the
// grammar + gingembre_ast.snark.js. Nothing hand-writes or copies the types anymore.)

/// The first named node under the interpolation (the expression).
fn find_expr<N: ParseNode>(node: &N) -> Option<&N> {
    let interp = find_kind(node, "interpolation")?;
    interp.children().iter().find(|c| c.named())
}

fn find_kind<'a, N: ParseNode>(node: &'a N, kind: &str) -> Option<&'a N> {
    if node.kind() == kind {
        return Some(node);
    }
    node.children().iter().find_map(|c| find_kind(c, kind))
}

fn repro(grammar_path: &str, input: &str) {
    let grammar_json = snark_dsl::emit_with_boa(std::path::Path::new(grammar_path)).expect("emit");
    let raw = RawGrammarJson::from_tree_sitter_json_str(&grammar_json).expect("import");
    let validated = ValidatedGrammar::from_raw(&raw).expect("validate");
    let lexical = LexicalFacts::from_grammar(&validated);
    let normalized =
        ParserGrammar::normalize_from_validated(&validated, &lexical).expect("normalize");
    let parser = normalized.prepare_productions_for_items().expect("prepare");
    let table = ParseTable::from_grammar(&parser).expect("table");
    let plan = WeavyParsePlan::new(&validated, &parser, &table).expect("plan");
    match parse_prepared_weavy_with_report(&plan, &parser, &table, input) {
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
            name: leaf_text(*one)?,
            span: span(0, 0),
        },
        many => Target::Tuple {
            names: many
                .iter()
                .filter_map(|i| Some((leaf_text(*i)?, span(0, 0))))
                .collect(),
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

/// Item 4: give the GENERATED AST meaning through gingembre's own evaluator. Lower the
/// grammar-generated `gen_ast::Expr` into `gingembre::ast::Expr` so gingembre's real
/// semantics (arithmetic, precedence-in-tree, comparison, variable lookup) render it. This
/// is the payoff — the fully generated types drive gingembre.
fn gen_expr_to_gingembre(e: &gen_ast::Expr) -> Expr {
    match e {
        gen_ast::Expr::Number(n) => Expr::Literal(Literal::Int(IntLit {
            value: *n,
            span: span(0, 0),
        })),
        gen_ast::Expr::Variable(name) => Expr::Var(Ident {
            name: name.clone(),
            span: span(0, 0),
        }),
        gen_ast::Expr::Binary(b) => Expr::Binary(BinaryExpr {
            left: Box::new(gen_expr_to_gingembre(&b.left)),
            op: binary_op(&b.op).unwrap_or_else(|| panic!("unknown operator {:?}", b.op)),
            right: Box::new(gen_expr_to_gingembre(&b.right)),
            span: span(0, 0),
        }),
    }
}

/// Materialize a `gen_ast::Expr` from a resolved expression node by JIT-compiling the build
/// ops (the whole pipeline: grammar surface -> reflection ops -> hostcall-chain JIT -> AST).
fn expr_via_jit<N: ParseNode>(node: &N, anns: &Annotations) -> gen_ast::Expr {
    let mut ops = Vec::new();
    let p = facet_reflect::Partial::alloc_owned::<gen_ast::Expr>().expect("alloc");
    // Drive a throwaway reflective build to RECORD the op program, then JIT the same ops.
    let _ = build(p, node, anns, &mut ops).expect("record build ops");
    run_ops_via_weavy_jit::<gen_ast::Expr>(&ops)
}

/// Concatenate every terminal's source text under this node.
fn full_text(node: &RuntimeResolvedNode) -> String {
    if let Some(text) = node.text() {
        return text.to_string();
    }
    node.children().iter().map(full_text).collect()
}

/// First terminal text under this node (for single-token leaves).
fn leaf_text<N: ParseNode>(node: &N) -> Option<String> {
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
