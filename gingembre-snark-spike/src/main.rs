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

    // Item 3b: JIT the SAME op program via copy-and-patch (weavy::jit) and materialize again.
    let via_jit: Expr = run_ops_via_weavy_jit(&ops);
    assert_eq!(via_jit, expected, "JIT-compiled ops must reproduce the reflected AST");
    println!(
        "✓ item 3b: same {} ops COPY-AND-PATCH JIT-compiled (native={}) -> identical AST",
        ops.len(),
        weavy::jit::NATIVE_COPY_PATCH_AVAILABLE,
    );
}

/// Item 3: the reflection ops as a flat Weavy program. Emitting these (instead of driving
/// `Partial` directly) is what lets materialization be lowered to the copy-and-patch JIT —
/// the same substrate as facet-json's `weavy_deser`. Here we run them through weavy's
/// interpreter (`weavy::run`); the JIT compiles the identical op stream.
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
/// interpreter (`AstBuilder::step`) and the copy-and-patch JIT host intrinsic, so both
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

/// Materialize `T` by COPY-AND-PATCH JIT-compiling the build-op program: each op becomes a
/// copied `HOSTCALL` stencil in one native chain (control flow is machine code, patched
/// site-to-site), and each site dispatches to `jit_apply`. This is the same substrate as
/// facet-json's `from_str_weavy_jit`; here the ops build a `Partial` instead of decoding
/// JSON. Falls back to the interpreter where native copy-and-patch isn't available.
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
fn build<'f>(
    mut p: facet_reflect::Partial<'f, false>,
    node: &RuntimeResolvedNode,
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
fn select_child<'a>(node: &'a RuntimeResolvedNode, selector: &str) -> &'a RuntimeResolvedNode {
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
fn find_expr(node: &RuntimeResolvedNode) -> Option<&RuntimeResolvedNode> {
    let interp = find_kind(node, "interpolation")?;
    interp.children().iter().find(|c| c.named())
}

fn find_kind<'a>(node: &'a RuntimeResolvedNode, kind: &str) -> Option<&'a RuntimeResolvedNode> {
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
            name: leaf_text(one)?,
            span: span(0, 0),
        },
        many => Target::Tuple {
            names: many
                .iter()
                .filter_map(|i| Some((leaf_text(i)?, span(0, 0))))
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
