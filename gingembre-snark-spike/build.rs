//! Generate the gingembre expression AST FROM THE GRAMMAR at build time.
//!
//! Structure (which children a node has, cardinality, the expression enum's variants) is
//! DERIVED from the grammar rules in-snark — no tree-sitter-cli. The `ast()` annotations add
//! only enrichment the grammar can't express: field names (gingembre uses no `field()`),
//! node->variant rename, and leaf decode types. The result is written to
//! `$OUT_DIR/gingembre_ast.rs` and `include!`d by the crate — nobody hand-writes the AST.

use std::collections::BTreeMap;
use std::{env, fs, path::PathBuf};

use snark::grammar::{RawGrammarJson, RawRuleJson};

/// One AST annotation, keyed by node kind. Only enrichment the grammar can't express:
/// variant/enum names, the struct name, the scalar-decode choice, and field names. Field
/// TYPES are NOT here — derived from the grammar (named child -> the enum; token -> String).
#[derive(facet::Facet, Default, Debug)]
struct NodeAnn {
    #[facet(rename = "as", default)]
    as_variant: Option<String>,
    #[facet(rename = "enum", default)]
    enum_name: Option<String>,
    #[facet(rename = "struct", default)]
    struct_name: Option<String>,
    #[facet(default)]
    scalar: Option<String>,
    #[facet(default)]
    transparent: bool,
    #[facet(default)]
    fields: BTreeMap<String, FieldAnn>,
}

/// A field's annotation: only the child selector (`named:N` | `token`). The Rust type is
/// derived from the grammar via that selector.
#[derive(facet::Facet, Default, Debug)]
struct FieldAnn {
    from: String,
}

type Annotations = BTreeMap<String, NodeAnn>;

/// A derived child slot of a node, from walking its grammar rule.
#[derive(Debug, Clone, PartialEq)]
enum Slot {
    /// A child that is itself an expression (a `_expr` reference) — typed as the enum.
    Expr,
    /// An anonymous operator/keyword token — typed as `String`.
    Token,
}

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let repo = manifest.parent().unwrap().to_path_buf();
    let grammar_js = repo.join("playgrounds/snark/src/bundled/gingembre/grammar.js");
    let ann_js = manifest.join("gingembre_ast.snark.js");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={}", grammar_js.display());
    println!("cargo:rerun-if-changed={}", ann_js.display());

    let ann_src = fs::read_to_string(&ann_js).expect("read annotation source");
    let grammar_json = snark_dsl::emit_with_boa(&grammar_js).expect("emit grammar.json");
    let ann_json = snark_dsl::annotations_from_source(&ann_src, "gingembre_ast.snark.js")
        .expect("annotations");
    let raw =
        RawGrammarJson::from_tree_sitter_json_str(&grammar_json).expect("import grammar json");
    let anns: Annotations = facet_json::from_str(&ann_json).expect("decode annotations");

    let generated = generate(&raw, &anns);
    let out = PathBuf::from(env::var("OUT_DIR").unwrap()).join("gingembre_ast.rs");
    fs::write(&out, generated).expect("write generated ast");

    build_intop_stencils(&manifest);
}

/// Compile + extract the copy-and-patch stencils for the unboxed integer eval lane, mirroring
/// weavy/phon-jit: `rustc --emit=obj` on `stencils/intop.rs`, then pull each op's machine code
/// and its `weavy_cont` relocation by symbol. Emits `$OUT_DIR/intop_stencils.rs`.
fn build_intop_stencils(manifest: &std::path::Path) {
    use std::fmt::Write as _;
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    let generated = out.join("intop_stencils.rs");
    let src = manifest.join("stencils/intop.rs");
    println!("cargo:rerun-if-changed={}", src.display());

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let native = matches!(
        (target_os.as_str(), target_arch.as_str()),
        ("macos", "aarch64") | ("linux", "x86_64")
    );
    if !native {
        // Empty stencils -> the runtime lane falls back to the interpreter.
        let mut empty = ["PUSH", "ADD", "SUB", "MUL", "FADD", "FSUB", "FMUL", "DONE"]
            .iter()
            .map(|n| format!("pub const {n}: &[u8] = &[];\npub const {n}_CONT: &[usize] = &[];\n"))
            .collect::<String>();
        for g in ["GUARD", "GUARD_F64"] {
            empty.push_str(&format!(
                "pub const {g}: &[u8] = &[];\npub const {g}_FAST_CONT: &[usize] = &[];\n\
                 pub const {g}_DEOPT_CONT: &[usize] = &[];\n"
            ));
        }
        fs::write(&generated, empty).expect("write empty stencils");
        return;
    }

    let target = env::var("TARGET").expect("TARGET set by cargo");
    let obj = out.join("intop_stencils.o");
    let rustc = env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string());

    // Prefer guaranteed tail calls (`become`) so a stencil that can't tail-call fails loud.
    // RUSTC_BOOTSTRAP=1 unlocks `explicit_tail_calls` on the STABLE toolchain (no nightly
    // install); it only affects the stencil rustc we spawn here, never downstream crates.
    // At -O the shipped bytes are identical to the stable call (LLVM already TCOs) — this is
    // a correctness guard. Fall back to a plain stable compile if `become` won't build.
    unsafe { env::set_var("RUSTC_BOOTSTRAP", "1") };
    let tailcall = copypatch::extract::compile_object(&rustc, &[], &src, &obj, &target, true);
    println!(
        "cargo:warning=intop stencils compiled with {}",
        if tailcall {
            "become (guaranteed tail calls)"
        } else {
            "stable call (-O TCO)"
        }
    );
    if !tailcall {
        assert!(
            copypatch::extract::compile_object(&rustc, &[], &src, &obj, &target, false),
            "rustc failed to compile intop stencils"
        );
    }
    let bytes = fs::read(&obj).expect("read stencil object");
    let symbols = [
        "weavy_intop_push",
        "weavy_intop_add",
        "weavy_intop_sub",
        "weavy_intop_mul",
        "weavy_intop_fadd",
        "weavy_intop_fsub",
        "weavy_intop_fmul",
        "weavy_intop_done",
    ];
    let get = |sym: &str| copypatch::extract::extract_stencil(&bytes, &symbols, sym, "weavy_cont");

    let mut s = String::new();
    s.push_str("// @generated by build.rs from stencils/intop.rs (rustc --emit=obj, real copy-and-patch).\n");
    for (name, sym) in [
        ("PUSH", "weavy_intop_push"),
        ("ADD", "weavy_intop_add"),
        ("SUB", "weavy_intop_sub"),
        ("MUL", "weavy_intop_mul"),
        ("FADD", "weavy_intop_fadd"),
        ("FSUB", "weavy_intop_fsub"),
        ("FMUL", "weavy_intop_fmul"),
        ("DONE", "weavy_intop_done"),
    ] {
        let st = get(sym);
        writeln!(s, "pub const {name}: &[u8] = &{:?};", st.bytes).unwrap();
        // DONE is a lone `ret` with no continuation; only emit CONT relocs for chaining ops.
        if name != "DONE" {
            writeln!(
                s,
                "pub const {name}_CONT: &[usize] = &{:?};",
                st.cont_relocs
            )
            .unwrap();
        }
    }

    // The two-successor speculation guard: extract BOTH continuation holes (fast/deopt).
    let guard_src = manifest.join("stencils/guard.rs");
    println!("cargo:rerun-if-changed={}", guard_src.display());
    let guard_obj = out.join("guard_stencils.o");
    let tail =
        copypatch::extract::compile_object(&rustc, &[], &guard_src, &guard_obj, &target, true);
    if !tail {
        assert!(
            copypatch::extract::compile_object(&rustc, &[], &guard_src, &guard_obj, &target, false),
            "rustc failed to compile guard stencil"
        );
    }
    let guard_bytes = fs::read(&guard_obj).expect("read guard object");
    let guard_syms = ["weavy_guard_i64", "weavy_guard_f64"];
    for (name, sym) in [
        ("GUARD", "weavy_guard_i64"),
        ("GUARD_F64", "weavy_guard_f64"),
    ] {
        let g = copypatch::extract::extract_stencil_n(
            &guard_bytes,
            &guard_syms,
            sym,
            &["weavy_cont", "weavy_deopt"],
        );
        writeln!(s, "pub const {name}: &[u8] = &{:?};", g.bytes).unwrap();
        writeln!(
            s,
            "pub const {name}_FAST_CONT: &[usize] = &{:?};",
            g.cont_relocs[0]
        )
        .unwrap();
        writeln!(
            s,
            "pub const {name}_DEOPT_CONT: &[usize] = &{:?};",
            g.cont_relocs[1]
        )
        .unwrap();
    }

    fs::write(&generated, s).expect("write intop stencils");
}

/// Look up a rule by name and strip transparent wrappers (prec/token/alias/reserved).
fn rule<'a>(raw: &'a RawGrammarJson, name: &str) -> Option<&'a RawRuleJson> {
    raw.rules.get(name).map(unwrap_transparent)
}

fn unwrap_transparent(mut r: &RawRuleJson) -> &RawRuleJson {
    loop {
        r = match r {
            RawRuleJson::Prec { content, .. }
            | RawRuleJson::PrecLeft { content, .. }
            | RawRuleJson::PrecRight { content, .. }
            | RawRuleJson::PrecDynamic { content, .. }
            | RawRuleJson::Token { content, .. }
            | RawRuleJson::ImmediateToken { content, .. }
            | RawRuleJson::Reserved { content, .. }
            | RawRuleJson::Alias { content, .. } => content,
            other => return other,
        };
    }
}

/// The enum's variants: the `_expr` CHOICE members that we have an annotation for.
fn enum_variants(raw: &RawGrammarJson, anns: &Annotations) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if let Some(RawRuleJson::Choice { members }) = rule(raw, "_expr") {
        for m in members {
            if let RawRuleJson::Symbol { name } = unwrap_transparent(m) {
                let kind = resolve_transparent_kind(raw, anns, name);
                if let Some(variant) = anns.get(&kind).and_then(|a| a.as_variant.clone()) {
                    out.push((variant, kind));
                }
            }
        }
    }
    out
}

/// Follow `transparent` annotations to the concrete node kind (e.g. `literal` -> `number`).
fn resolve_transparent_kind(raw: &RawGrammarJson, anns: &Annotations, kind: &str) -> String {
    if anns.get(kind).is_some_and(|a| a.transparent) {
        // A transparent node is a CHOICE; take its first member as the representative.
        if let Some(RawRuleJson::Choice { members }) = rule(raw, kind)
            && let Some(RawRuleJson::Symbol { name }) = members.first().map(unwrap_transparent)
        {
            return resolve_transparent_kind(raw, anns, name);
        }
    }
    kind.to_string()
}

/// Derive a struct node's ordered child slots (Expr / Token) from its grammar rule.
fn derive_slots(raw: &RawGrammarJson, kind: &str) -> Vec<Slot> {
    // Find a representative SEQ (binary is CHOICE[PREC_LEFT[SEQ[...]]] over prec levels).
    fn find_seq(r: &RawRuleJson) -> Option<&[RawRuleJson]> {
        match r {
            RawRuleJson::Seq { members } => Some(members),
            RawRuleJson::Choice { members } => {
                members.iter().find_map(|m| find_seq(unwrap_transparent(m)))
            }
            RawRuleJson::Prec { content, .. }
            | RawRuleJson::PrecLeft { content, .. }
            | RawRuleJson::PrecRight { content, .. }
            | RawRuleJson::PrecDynamic { content, .. } => find_seq(content),
            _ => None,
        }
    }
    let Some(rule) = rule(raw, kind) else {
        return Vec::new();
    };
    let Some(seq) = find_seq(rule) else {
        return Vec::new();
    };
    seq.iter()
        .filter_map(|m| match unwrap_transparent(m) {
            // A reference into the expression grammar -> an Expr child.
            RawRuleJson::Symbol { name } if name == "_expr" || name.ends_with("expr") => {
                Some(Slot::Expr)
            }
            // A literal operator/keyword -> a token slot.
            RawRuleJson::String { .. } | RawRuleJson::Choice { .. } => Some(Slot::Token),
            _ => None,
        })
        .collect()
}

fn generate(raw: &RawGrammarJson, anns: &Annotations) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    out.push_str("// @generated from the gingembre grammar + ast() annotations by build.rs.\n");
    out.push_str(
        "// Structure derived from the grammar rules; names/decodes from annotations.\n\n",
    );

    let variants = enum_variants(raw, anns);
    let enum_name = anns
        .get("_expr")
        .and_then(|a| a.enum_name.clone())
        .unwrap_or_else(|| "Expr".into());

    // The enum.
    writeln!(
        out,
        "#[derive(facet::Facet, Debug, Clone, PartialEq)]\n#[repr(u8)]\npub enum {enum_name} {{"
    )
    .unwrap();
    let mut structs: Vec<(String, String)> = Vec::new(); // (struct name, source node kind)
    for (variant, kind) in &variants {
        let ann = &anns[kind];
        let payload = if let Some(st) = &ann.struct_name {
            structs.push((st.clone(), kind.clone()));
            format!("Box<{st}>")
        } else if let Some(scalar) = &ann.scalar {
            scalar.clone()
        } else {
            // A single-token leaf node (e.g. variable -> identifier) decodes to String.
            "String".into()
        };
        writeln!(out, "    {variant}({payload}),").unwrap();
    }
    writeln!(out, "}}\n").unwrap();

    // The structs — fields DERIVED from the grammar (slots), NAMED by annotations.
    for (st_name, kind) in structs {
        let ann = &anns[&kind];
        let slots = derive_slots(raw, &kind);
        writeln!(
            out,
            "#[derive(facet::Facet, Debug, Clone, PartialEq)]\npub struct {st_name} {{"
        )
        .unwrap();
        // Map each annotated field to its selector -> slot -> Rust type derived from the grammar.
        for (fname, field) in &ann.fields {
            let ty = field_type(&enum_name, &slots, &field.from);
            writeln!(out, "    pub {fname}: {ty},").unwrap();
        }
        writeln!(out, "}}\n").unwrap();
    }
    out
}

/// Type for a field, from its selector resolved against the grammar-derived slots. A
/// `named:N` selector picks the Nth *named* child; in the tree the named children of these
/// expression nodes are the `_expr` operands (Expr slots) — so the type is the enum. A
/// `token` selector picks an anonymous operator token — a `String`.
fn field_type(enum_name: &str, slots: &[Slot], selector: &str) -> String {
    if let Some(n) = selector.strip_prefix("named:") {
        let n: usize = n.parse().unwrap_or(0);
        // The Nth named slot must be an Expr per the grammar.
        let named: Vec<&Slot> = slots.iter().filter(|s| matches!(s, Slot::Expr)).collect();
        assert!(
            named.get(n).is_some(),
            "grammar has no named (expr) child #{n} for this node; slots={slots:?}"
        );
        enum_name.to_string()
    } else if selector == "token" {
        "String".to_string()
    } else {
        panic!("unknown field selector {selector:?}")
    }
}
