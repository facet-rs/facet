//! Generate the vix typed AST — and its lowering — FROM THE GRAMMAR at build time.
//!
//! The grammar fields every AST-relevant child, so structure derives mechanically:
//!   - hidden choice rules (`_expr`, `_item`, …) become enums (names from ast()),
//!   - fielded visible rules become structs; a field's type comes from what the
//!     field can contain, its cardinality from the optional/repeat context around it
//!     (bare -> T, optional -> Option<T>, repeat/sepBy -> Vec<T>),
//!   - mixed-alternative fields (e.g. array elements: flag | expr) become ad-hoc
//!     enums named by ast() annotations,
//!   - unfielded leaves (identifier, string, …) decode per their annotation.
//!
//! Output: `$OUT_DIR/vix_ast.rs` (types + ResolvedCstNode lowering, `include!`d by
//! src/lib.rs) and `$OUT_DIR/vix_grammar.json` (embedded so the runtime parser needs
//! no JS engine). Nobody hand-writes the AST.
//!
//! Known snark gap this codegen routes around: fields on anonymous TOKEN steps
//! (`field("op", …)` on operators, `field("vis", "pub")`) never reach the resolved
//! tree — token steps emit `Field { child: None }`, which the resolved builder
//! discards. Token-valued fields are therefore lowered by scanning the node's
//! anonymous children against the token set derived from the grammar.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::{env, fs, path::PathBuf};

use snark::grammar::{RawGrammarJson, RawRuleJson};

/// One ast() annotation, keyed by node kind. Only enrichment the grammar can't express.
#[derive(facet::Facet, Default, Debug)]
struct NodeAnn {
    /// Variant name when this node appears inside an enum.
    #[facet(rename = "as", default)]
    as_variant: Option<String>,
    /// Enum name for hidden choice rules.
    #[facet(rename = "enum", default)]
    enum_name: Option<String>,
    /// Struct name override (default: CamelCase of the node kind).
    #[facet(rename = "struct", default)]
    struct_name: Option<String>,
    /// Leaf decode choice: "text" | "string" | "path" | "bool".
    #[facet(default)]
    decode: Option<String>,
    /// Per-field enrichment (ad-hoc enum names).
    #[facet(default)]
    fields: BTreeMap<String, FieldAnn>,
}

#[derive(facet::Facet, Default, Debug)]
struct FieldAnn {
    /// Name for the ad-hoc enum generated from a mixed-alternative field.
    #[facet(rename = "enum", default)]
    enum_name: Option<String>,
}

type Annotations = BTreeMap<String, NodeAnn>;

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let repo = manifest.parent().unwrap().to_path_buf();
    let grammar_js = repo.join("playgrounds/snark/src/bundled/vix/grammar.js");
    let ann_js = manifest.join("vix_ast.snark.js");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={}", grammar_js.display());
    println!("cargo:rerun-if-changed={}", ann_js.display());

    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    let grammar_json = snark_dsl::emit_with_boa(&grammar_js).expect("emit grammar.json");
    fs::write(out.join("vix_grammar.json"), &grammar_json).expect("write embedded grammar");

    let ann_src = fs::read_to_string(&ann_js).expect("read annotation source");
    let ann_json =
        snark_dsl::annotations_from_source(&ann_src, "vix_ast.snark.js").expect("annotations");
    let raw =
        RawGrammarJson::from_tree_sitter_json_str(&grammar_json).expect("import grammar json");
    let anns: Annotations = facet_json::from_str(&ann_json).expect("decode annotations");

    let generated = Model::build(&raw, &anns).generate();
    fs::write(out.join("vix_ast.rs"), generated).expect("write generated ast");
}

// ---------------------------------------------------------------------------
// Grammar walking: fields + cardinality.
// ---------------------------------------------------------------------------

/// How many times a field can occur in a node.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mult {
    One,
    Opt,
    Many,
}

/// What a field can contain, before shape resolution.
#[derive(Clone, PartialEq, Debug)]
enum Alt {
    /// Reference to another rule (hidden or visible).
    Rule(String),
    /// Anonymous literal token(s).
    Token(String),
}

/// Ordered (by first appearance) field accumulator for one rule.
#[derive(Default, Clone, Debug)]
struct Fields(Vec<(String, Vec<Alt>, Mult)>);

impl Fields {
    fn entry(&mut self, name: &str) -> Option<&mut (String, Vec<Alt>, Mult)> {
        self.0.iter_mut().find(|(n, _, _)| n == name)
    }

    /// Sequential composition: a field seen again in the same sequence repeats.
    fn seq(mut self, other: Fields) -> Fields {
        for (name, alts, _mult) in other.0 {
            if let Some((_, existing, mult)) = self.entry(&name) {
                merge_alts(existing, alts);
                *mult = Mult::Many;
            } else {
                self.0.push((name, alts, _mult));
            }
        }
        self
    }

    /// Alternative composition: a field missing from some arms becomes optional.
    fn choice(arms: Vec<Fields>) -> Fields {
        let mut out = Fields::default();
        for arm in &arms {
            for (name, alts, _) in &arm.0 {
                if let Some((_, existing, _)) = out.entry(name) {
                    merge_alts(existing, alts.clone());
                } else {
                    out.0.push((name.clone(), alts.clone(), Mult::One));
                }
            }
        }
        for (name, _, mult) in &mut out.0 {
            let occurrences: Vec<Option<Mult>> = arms
                .iter()
                .map(|arm| {
                    arm.0
                        .iter()
                        .find(|(n, _, _)| n == name)
                        .map(|(_, _, m)| *m)
                })
                .collect();
            let any_many = occurrences.iter().flatten().any(|m| *m == Mult::Many);
            let everywhere_one = occurrences.iter().all(|m| *m == Some(Mult::One));
            *mult = if any_many {
                Mult::Many
            } else if everywhere_one {
                Mult::One
            } else {
                Mult::Opt
            };
        }
        out
    }

    fn repeated(mut self) -> Fields {
        for (_, _, mult) in &mut self.0 {
            *mult = Mult::Many;
        }
        self
    }
}

fn merge_alts(existing: &mut Vec<Alt>, incoming: Vec<Alt>) {
    for alt in incoming {
        if !existing.contains(&alt) {
            existing.push(alt);
        }
    }
}

/// Walk a rule body collecting its fields. Field contents are NOT walked: nested
/// fields inside field content are out of scope for vix (and unused by the grammar).
fn walk(rule: &RawRuleJson) -> Fields {
    match rule {
        RawRuleJson::Field { name, content } => {
            Fields(vec![(name.clone(), field_alts(content), Mult::One)])
        }
        RawRuleJson::Seq { members } => members
            .iter()
            .fold(Fields::default(), |acc, m| acc.seq(walk(m))),
        RawRuleJson::Choice { members } => Fields::choice(members.iter().map(walk).collect()),
        RawRuleJson::Repeat { content } | RawRuleJson::Repeat1 { content } => {
            walk(content).repeated()
        }
        RawRuleJson::Prec { content, .. }
        | RawRuleJson::PrecLeft { content, .. }
        | RawRuleJson::PrecRight { content, .. }
        | RawRuleJson::PrecDynamic { content, .. }
        | RawRuleJson::Token { content, .. }
        | RawRuleJson::ImmediateToken { content, .. }
        | RawRuleJson::Alias { content, .. }
        | RawRuleJson::Reserved { content, .. } => walk(content),
        _ => Fields::default(),
    }
}

/// The alternatives a field's content can produce.
fn field_alts(content: &RawRuleJson) -> Vec<Alt> {
    match unwrap_transparent(content) {
        RawRuleJson::Symbol { name } => vec![Alt::Rule(name.clone())],
        RawRuleJson::String { value } => vec![Alt::Token(value.clone())],
        RawRuleJson::Choice { members } => {
            let mut out = Vec::new();
            for m in members {
                merge_alts(&mut out, field_alts(m));
            }
            out
        }
        RawRuleJson::Blank => Vec::new(),
        other => panic!("unsupported field content in vix grammar: {other:?}"),
    }
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

// ---------------------------------------------------------------------------
// Model: classify rules, resolve field shapes.
// ---------------------------------------------------------------------------

/// How a leaf node decodes into a Rust value.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Decode {
    Text,
    Str,
    Path,
    Bool,
    /// The node is a single fixed literal — a unit variant, nothing to carry.
    Unit,
}

impl Decode {
    fn rust_type(self) -> &'static str {
        match self {
            Decode::Bool => "crate::support::Spanned<bool>",
            // Unit leaves still carry WHERE they were (`_` patterns, hover).
            Decode::Unit => "crate::support::Span",
            _ => "crate::support::Spanned<String>",
        }
    }

    fn lower_fn(self) -> &'static str {
        match self {
            Decode::Text => "crate::support::node_text",
            Decode::Str => "crate::support::decode_string",
            Decode::Path => "crate::support::decode_path",
            Decode::Bool => "crate::support::decode_bool",
            Decode::Unit => "crate::support::span",
        }
    }

    fn lower(self, node: &str) -> String {
        format!("{}({node})", self.lower_fn())
    }
}

/// Resolved type/lowering shape for one field.
#[derive(Clone, Debug)]
enum Shape {
    /// Anonymous literal token(s) — lowered by token-set scan (see module docs).
    TokenSet(Vec<String>),
    /// A hidden choice rule — the generated enum.
    Enum(String),
    /// A fielded visible rule — the generated struct (by node kind).
    Struct(String),
    /// An unfielded visible rule — decoded scalar.
    Leaf(Decode),
    /// Mixed alternatives — a generated ad-hoc enum (by index into Model::adhocs).
    AdHoc(usize),
}

#[derive(Clone, Debug)]
struct AdHocDef {
    name: String,
    /// (variant name, dispatch) per alternative, in grammar order.
    alts: Vec<AdHocAlt>,
}

#[derive(Clone, Debug)]
enum AdHocAlt {
    /// Visible node kind.
    Visible(String),
    /// Hidden enum rule (kind, enum name).
    Hidden(String, String),
}

#[derive(Clone, Debug)]
struct FieldDef {
    grammar_name: String,
    rust_name: String,
    shape: Shape,
    mult: Mult,
}

#[derive(Clone, Debug)]
struct StructDef {
    kind: String,
    name: String,
    fields: Vec<FieldDef>,
}

#[derive(Clone, Debug)]
struct EnumDef {
    kind: String,
    name: String,
    member_kinds: Vec<String>,
}

struct Model<'a> {
    raw: &'a RawGrammarJson,
    anns: &'a Annotations,
    enums: Vec<EnumDef>,
    structs: Vec<StructDef>,
    adhocs: Vec<AdHocDef>,
}

impl<'a> Model<'a> {
    fn build(raw: &'a RawGrammarJson, anns: &'a Annotations) -> Self {
        let mut model = Model {
            raw,
            anns,
            enums: Vec::new(),
            structs: Vec::new(),
            adhocs: Vec::new(),
        };

        // Hidden enums first (shape resolution needs them), in grammar order.
        for (name, rule) in raw.rules.iter() {
            let kind = name.as_str();
            if !kind.starts_with('_') {
                continue;
            }
            let enum_name = anns
                .get(kind)
                .and_then(|a| a.enum_name.clone())
                .unwrap_or_else(|| panic!("hidden rule `{kind}` needs an `enum` annotation"));
            let RawRuleJson::Choice { members } = unwrap_transparent(rule) else {
                panic!("hidden enum rule `{kind}` must be a choice");
            };
            let member_kinds = members
                .iter()
                .map(|m| match unwrap_transparent(m) {
                    RawRuleJson::Symbol { name } => name.clone(),
                    other => panic!("hidden enum `{kind}` member must be a symbol: {other:?}"),
                })
                .collect();
            model.enums.push(EnumDef {
                kind: kind.to_string(),
                name: enum_name,
                member_kinds,
            });
        }

        // Structs: every fielded visible rule, in grammar order.
        let struct_kinds: Vec<(String, Fields)> = raw
            .rules
            .iter()
            .filter(|(name, _)| !name.as_str().starts_with('_'))
            .filter_map(|(name, rule)| {
                let fields = walk(rule);
                (!fields.0.is_empty()).then(|| (name.as_str().to_string(), fields))
            })
            .collect();
        for (kind, fields) in struct_kinds {
            let name = model.struct_name(&kind);
            let fields = fields
                .0
                .into_iter()
                .map(|(fname, alts, mult)| {
                    let shape = model.resolve_shape(&kind, &fname, alts);
                    FieldDef {
                        rust_name: rust_field_name(&fname, mult),
                        grammar_name: fname,
                        shape,
                        mult,
                    }
                })
                .collect();
            model.structs.push(StructDef { kind, name, fields });
        }

        model
    }

    fn ann(&self, kind: &str) -> Option<&NodeAnn> {
        self.anns.get(kind)
    }

    fn struct_name(&self, kind: &str) -> String {
        self.ann(kind)
            .and_then(|a| a.struct_name.clone())
            .unwrap_or_else(|| camel(kind))
    }

    fn variant_name(&self, kind: &str) -> String {
        self.ann(kind)
            .and_then(|a| a.as_variant.clone())
            .unwrap_or_else(|| camel(kind))
    }

    fn hidden_enum(&self, kind: &str) -> Option<&EnumDef> {
        self.enums.iter().find(|e| e.kind == kind)
    }

    /// The visible node kinds an enum can dispatch on, hidden members expanded.
    fn dispatch_kinds(&self, e: &EnumDef) -> Vec<String> {
        let mut out = Vec::new();
        for kind in &e.member_kinds {
            match self.hidden_enum(kind) {
                Some(inner) => out.extend(self.dispatch_kinds(inner)),
                None => out.push(kind.clone()),
            }
        }
        out
    }

    fn is_struct_kind(&self, kind: &str) -> bool {
        self.raw
            .rules
            .get(kind)
            .is_some_and(|rule| !walk(rule).0.is_empty())
    }

    /// Decode for an unfielded (leaf) visible rule.
    fn leaf_decode(&self, kind: &str) -> Decode {
        match self.ann(kind).and_then(|a| a.decode.as_deref()) {
            Some("text") => Decode::Text,
            Some("string") => Decode::Str,
            Some("path") => Decode::Path,
            Some("bool") => Decode::Bool,
            Some(other) => panic!("unknown decode `{other}` on `{kind}`"),
            None => {
                // A single fixed literal is a unit; anything else is raw text.
                let rule = self
                    .raw
                    .rules
                    .get(kind)
                    .unwrap_or_else(|| panic!("unknown rule `{kind}`"));
                match unwrap_transparent(rule) {
                    RawRuleJson::String { .. } => Decode::Unit,
                    _ => Decode::Text,
                }
            }
        }
    }

    fn resolve_shape(&mut self, kind: &str, fname: &str, alts: Vec<Alt>) -> Shape {
        let tokens: Vec<String> = alts
            .iter()
            .filter_map(|a| match a {
                Alt::Token(t) => Some(t.clone()),
                Alt::Rule(_) => None,
            })
            .collect();
        let rules: Vec<String> = alts
            .iter()
            .filter_map(|a| match a {
                Alt::Rule(r) => Some(r.clone()),
                Alt::Token(_) => None,
            })
            .collect();

        if rules.is_empty() {
            assert!(
                !tokens.is_empty(),
                "field `{fname}` on `{kind}` has no alternatives"
            );
            return Shape::TokenSet(tokens);
        }
        assert!(
            tokens.is_empty(),
            "field `{fname}` on `{kind}` mixes tokens and rules — unsupported"
        );
        if rules.len() == 1 {
            let r = &rules[0];
            return if let Some(e) = self.hidden_enum(r) {
                Shape::Enum(e.name.clone())
            } else if self.is_struct_kind(r) {
                Shape::Struct(r.clone())
            } else {
                Shape::Leaf(self.leaf_decode(r))
            };
        }

        // Mixed alternatives: an ad-hoc enum, named by annotation.
        let name = self
            .ann(kind)
            .and_then(|a| a.fields.get(fname))
            .and_then(|f| f.enum_name.clone())
            .unwrap_or_else(|| {
                panic!("field `{fname}` on `{kind}` mixes alternatives; needs an `enum` annotation")
            });
        if let Some(idx) = self.adhocs.iter().position(|a| a.name == name) {
            return Shape::AdHoc(idx);
        }
        let alts = rules
            .iter()
            .map(|r| {
                if let Some(e) = self.hidden_enum(r) {
                    AdHocAlt::Hidden(r.clone(), e.name.clone())
                } else {
                    AdHocAlt::Visible(r.clone())
                }
            })
            .collect();
        self.adhocs.push(AdHocDef { name, alts });
        Shape::AdHoc(self.adhocs.len() - 1)
    }

    // -----------------------------------------------------------------------
    // Emission.
    // -----------------------------------------------------------------------

    fn generate(&self) -> String {
        let mut out = String::new();
        out.push_str(
            "// @generated by vix/build.rs from the vix grammar + vix_ast.snark.js.\n\
             // Structure derived from grammar fields + cardinality; names/decodes from ast().\n\n\
             use snark::parser::ResolvedCstNode;\n\n\
             pub use crate::support::{Span, Spanned};\n\n",
        );
        for e in &self.enums {
            self.emit_enum(&mut out, e);
        }
        for (idx, a) in self.adhocs.iter().enumerate() {
            self.emit_adhoc(&mut out, idx, a);
        }
        for s in &self.structs {
            self.emit_struct(&mut out, s);
        }
        out
    }

    fn variant_payload(&self, kind: &str) -> String {
        if self.is_struct_kind(kind) {
            format!("(Box<{}>)", self.struct_name(kind))
        } else {
            format!("({})", self.leaf_decode(kind).rust_type())
        }
    }

    /// The expression lowering node `c` into the variant payload for `kind`.
    fn variant_lower(&self, enum_name: &str, kind: &str, c: &str) -> String {
        let variant = self.variant_name(kind);
        if self.is_struct_kind(kind) {
            format!("{enum_name}::{variant}(Box::new(lower_{kind}({c})))")
        } else {
            format!(
                "{enum_name}::{variant}({})",
                self.leaf_decode(kind).lower(c)
            )
        }
    }

    fn emit_enum(&self, out: &mut String, e: &EnumDef) {
        writeln!(
            out,
            "#[derive(facet::Facet, Debug, Clone, PartialEq)]\n#[repr(u8)]\npub enum {} {{",
            e.name
        )
        .unwrap();
        for kind in &e.member_kinds {
            // A hidden member (e.g. `_expr` inside `_arg`) nests its enum.
            if let Some(inner) = self.hidden_enum(kind) {
                let variant = self
                    .ann(kind)
                    .and_then(|a| a.as_variant.clone())
                    .unwrap_or_else(|| inner.name.clone());
                writeln!(out, "    {variant}({}),", inner.name).unwrap();
            } else {
                writeln!(
                    out,
                    "    {}{},",
                    self.variant_name(kind),
                    self.variant_payload(kind)
                )
                .unwrap();
            }
        }
        writeln!(out, "}}\n").unwrap();

        writeln!(
            out,
            "pub fn lower_{}(n: &ResolvedCstNode) -> {} {{\n    match n.kind() {{",
            snake(&e.name),
            e.name
        )
        .unwrap();
        // Visible members dispatch on their exact kind; hidden members guard on
        // their enum's kind set, so put them last.
        for kind in &e.member_kinds {
            if self.hidden_enum(kind).is_none() {
                writeln!(
                    out,
                    "        {kind:?} => {},",
                    self.variant_lower(&e.name, kind, "n")
                )
                .unwrap();
            }
        }
        for kind in &e.member_kinds {
            if let Some(inner) = self.hidden_enum(kind) {
                let variant = self
                    .ann(kind)
                    .and_then(|a| a.as_variant.clone())
                    .unwrap_or_else(|| inner.name.clone());
                let kinds = self
                    .dispatch_kinds(inner)
                    .iter()
                    .map(|k| format!("{k:?}"))
                    .collect::<Vec<_>>()
                    .join(" | ");
                writeln!(
                    out,
                    "        {kinds} => {}::{variant}(lower_{}(n)),",
                    e.name,
                    snake(&inner.name)
                )
                .unwrap();
            }
        }
        writeln!(
            out,
            "        other => panic!(\"unexpected `{}` node kind `{{other}}`\"),\n    }}\n}}\n",
            e.kind
        )
        .unwrap();
    }

    fn emit_adhoc(&self, out: &mut String, _idx: usize, a: &AdHocDef) {
        writeln!(
            out,
            "#[derive(facet::Facet, Debug, Clone, PartialEq)]\n#[repr(u8)]\npub enum {} {{",
            a.name
        )
        .unwrap();
        for alt in &a.alts {
            match alt {
                AdHocAlt::Visible(kind) => writeln!(
                    out,
                    "    {}{},",
                    self.variant_name(kind),
                    self.variant_payload(kind)
                )
                .unwrap(),
                AdHocAlt::Hidden(_, enum_name) => {
                    writeln!(out, "    {enum_name}({enum_name}),").unwrap()
                }
            }
        }
        writeln!(out, "}}\n").unwrap();

        writeln!(
            out,
            "pub fn lower_{}(n: &ResolvedCstNode) -> {} {{\n    match n.kind() {{",
            snake(&a.name),
            a.name
        )
        .unwrap();
        // Visible alternatives dispatch on their exact kind; hidden ones guard on
        // the enum's kind set, so put them last.
        for alt in &a.alts {
            if let AdHocAlt::Visible(kind) = alt {
                writeln!(
                    out,
                    "        {kind:?} => {},",
                    self.variant_lower(&a.name, kind, "n")
                )
                .unwrap();
            }
        }
        for alt in &a.alts {
            if let AdHocAlt::Hidden(hidden_kind, enum_name) = alt {
                let e = self.hidden_enum(hidden_kind).unwrap();
                let kinds = self
                    .dispatch_kinds(e)
                    .iter()
                    .map(|k| format!("{k:?}"))
                    .collect::<Vec<_>>()
                    .join(" | ");
                writeln!(
                    out,
                    "        {kinds} => {}::{enum_name}(lower_{}(n)),",
                    a.name,
                    snake(enum_name)
                )
                .unwrap();
            }
        }
        writeln!(
            out,
            "        other => panic!(\"unexpected `{}` node kind `{{other}}`\"),\n    }}\n}}\n",
            a.name
        )
        .unwrap();
    }

    fn field_type(&self, f: &FieldDef) -> String {
        let base = match &f.shape {
            Shape::TokenSet(_) => "String".to_string(),
            Shape::Enum(name) => name.clone(),
            Shape::Struct(kind) => self.struct_name(kind),
            Shape::Leaf(decode) => decode.rust_type().to_string(),
            Shape::AdHoc(idx) => self.adhocs[*idx].name.clone(),
        };
        match f.mult {
            Mult::One => base,
            Mult::Opt => format!("Option<{base}>"),
            Mult::Many => format!("Vec<{base}>"),
        }
    }

    /// The single-argument function lowering one child node of this field.
    fn field_lower_fn(&self, f: &FieldDef) -> String {
        match &f.shape {
            Shape::TokenSet(_) => unreachable!("token fields lower via token_field"),
            Shape::Enum(name) => format!("lower_{}", snake(name)),
            Shape::Struct(kind) => format!("lower_{kind}"),
            Shape::Leaf(decode) => decode.lower_fn().to_string(),
            Shape::AdHoc(idx) => format!("lower_{}", snake(&self.adhocs[*idx].name)),
        }
    }

    fn emit_struct(&self, out: &mut String, s: &StructDef) {
        writeln!(
            out,
            "#[derive(facet::Facet, Debug, Clone, PartialEq)]\npub struct {} {{\n    \
             pub span: crate::support::Span,",
            s.name
        )
        .unwrap();
        for f in &s.fields {
            writeln!(out, "    pub {}: {},", f.rust_name, self.field_type(f)).unwrap();
        }
        writeln!(out, "}}\n").unwrap();

        writeln!(
            out,
            "pub fn lower_{}(n: &ResolvedCstNode) -> {} {{\n    {} {{\n        \
             span: crate::support::span(n),",
            s.kind, s.name, s.name
        )
        .unwrap();
        for f in &s.fields {
            let expr = if let Shape::TokenSet(tokens) = &f.shape {
                let set = tokens
                    .iter()
                    .map(|t| format!("{t:?}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                match f.mult {
                    Mult::Opt => format!("crate::support::token_field(n, &[{set}])"),
                    Mult::One => format!(
                        "crate::support::token_field(n, &[{set}])\n            \
                         .unwrap_or_else(|| panic!(\"missing token field `{}` on `{}`\"))",
                        f.grammar_name, s.kind
                    ),
                    Mult::Many => panic!(
                        "repeated token field `{}` on `{}` — unsupported",
                        f.grammar_name, s.kind
                    ),
                }
            } else {
                let lower = self.field_lower_fn(f);
                match f.mult {
                    Mult::One => format!(
                        "{lower}(crate::support::field_one(n, {:?}, {:?}))",
                        f.grammar_name, s.kind
                    ),
                    Mult::Opt => format!(
                        "crate::support::field_opt(n, {:?}).map({lower})",
                        f.grammar_name
                    ),
                    Mult::Many => format!(
                        "crate::support::fields(n, {:?}).map({lower}).collect()",
                        f.grammar_name
                    ),
                }
            };
            writeln!(out, "        {}: {expr},", f.rust_name).unwrap();
        }
        writeln!(out, "    }}\n}}\n").unwrap();
    }
}

/// Grammar field names are singular (they label one child each); Vec-shaped fields
/// pluralize (`stmt` -> `stmts`, `leaf` -> `leaves`), and `type` dodges the keyword.
fn rust_field_name(name: &str, mult: Mult) -> String {
    let singular = match name {
        "type" => "ty",
        other => other,
    };
    match (mult, singular) {
        (Mult::Many, "leaf") => "leaves".to_string(),
        (Mult::Many, s) if !s.ends_with('s') => format!("{s}s"),
        (_, s) => s.to_string(),
    }
}

fn camel(kind: &str) -> String {
    kind.split('_')
        .filter(|s| !s.is_empty())
        .map(|s| {
            let mut chars = s.chars();
            match chars.next() {
                Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

fn snake(name: &str) -> String {
    let mut out = String::new();
    for (i, ch) in name.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}
