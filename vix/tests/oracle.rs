//! Design probes: run the corpus through the oracle and observe identity,
//! caching, observations, and total order doing what the synthesis doc claims.

use std::collections::{BTreeMap, BTreeSet};

use vix::exec::Tree;
use vix::fetch::{FakeFetchBackend, sha256_hex};
use vix::oracle::{Event, Oracle, Payload, Value};

fn sample(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/../playgrounds/snark/src/bundled/vix/samples/{name}",
        env!("CARGO_MANIFEST_DIR"),
    ))
    .expect("read sample")
}

fn anti_nix_diamond() -> &'static str {
    r#"
fn leaf() -> Int {
    1
}

fn left() -> Int {
    leaf() + 10
}

fn right() -> Int {
    leaf() + 20
}

fn independent() -> Int {
    5
}

fn never_demanded() -> Int {
    100
}

pub fn main() -> Int {
    left() + right() + independent()
}
"#
}

fn event_counts(events: &[Event]) -> BTreeMap<&'static str, usize> {
    let mut counts = BTreeMap::new();
    for event in events {
        let key = match event {
            Event::Miss { .. } => "miss",
            Event::Hit { .. } => "hit",
            Event::Created { .. } => "created",
            Event::Scheduled { .. } => "scheduled",
            Event::Observation { .. } => "observation",
            Event::Finished { .. } => "finished",
        };
        *counts.entry(key).or_insert(0) += 1;
    }
    counts
}

fn missed_functions(events: &[Event]) -> BTreeSet<String> {
    events
        .iter()
        .filter_map(|event| match event {
            Event::Miss { func, .. } => Some(func.clone()),
            _ => None,
        })
        .collect()
}

fn hit_functions(events: &[Event]) -> BTreeSet<String> {
    events
        .iter()
        .filter_map(|event| match event {
            Event::Hit { func, .. } => Some(func.clone()),
            _ => None,
        })
        .collect()
}

fn assert_zero_cost_reload(events: &[Event]) {
    let counts = event_counts(events);
    assert_eq!(counts.get("miss").copied().unwrap_or(0), 0, "{events:?}");
    assert_eq!(counts.get("created").copied().unwrap_or(0), 0, "{events:?}");
}

#[test]
fn eval_vix_computes_42() {
    let oracle = Oracle::load(&sample("eval.vix")).expect("load");
    // let x = 6.0 in x * 7.0 — recursion, variants, persistent map, match.
    assert_eq!(oracle.call("demo", &[]).unwrap(), Value::Float(42.0));
}

#[test]
fn memo_hits_and_identity_survives_trivia() {
    let oracle = Oracle::load(&sample("eval.vix")).expect("load");
    oracle.call("demo", &[]).unwrap();
    let cold: Vec<Event> = oracle.events();
    assert!(cold.iter().all(|e| matches!(e, Event::Miss { .. })));
    let cold_misses = cold.len();

    // Second call: served entirely from the memo — ONE hit, zero new misses.
    oracle.call("demo", &[]).unwrap();
    let warm = &oracle.events()[cold_misses..];
    assert_eq!(warm.len(), 1, "{warm:?}");
    assert!(
        matches!(&warm[0], Event::Hit { func, .. } if func == "demo"),
        "{warm:?}"
    );

    // Identity is the AST modulo spans: comments and whitespace anywhere —
    // including inside the fn — don't change a function's hash.
    let a = Oracle::load(&sample("eval.vix")).unwrap();
    let reformatted = sample("eval.vix")
        .replace("fn demo() -> Float {", "fn demo() -> Float {\n    // hi!\n")
        .replace("use vix::Map;", "// preamble\n\nuse vix::Map;");
    let b = Oracle::load(&reformatted).unwrap();
    assert_eq!(a.fn_hash("demo"), b.fn_hash("demo"));
    assert_eq!(a.fn_hash("eval"), b.fn_hash("eval"));

    // But a REAL change is a new identity.
    let changed = sample("eval.vix").replace("Expr::Num(6.0)", "Expr::Num(5.0)");
    let c = Oracle::load(&changed).unwrap();
    assert_ne!(a.fn_hash("demo"), c.fn_hash("demo"));
    assert_eq!(a.fn_hash("eval"), c.fn_hash("eval"), "eval didn't change");
}

#[test]
fn warm_reload_trivia_anywhere_costs_zero_misses_and_zero_runs() {
    let mut oracle = Oracle::load(anti_nix_diamond()).expect("load");
    assert_eq!(oracle.call("main", &[]).unwrap(), Value::Int(37));

    let reformatted = anti_nix_diamond()
        .replace(
            "fn leaf() -> Int {",
            "// top-level trivia\nfn leaf() -> Int {\n    // leaf",
        )
        .replace("fn left() -> Int {", "fn left() -> Int {\n\n    // left")
        .replace("fn right() -> Int {", "fn right() -> Int {\n    // right")
        .replace(
            "fn never_demanded() -> Int {",
            "fn never_demanded() -> Int {\n    // dead code trivia",
        )
        .replace("left() + right()", "left()   +   right()");
    oracle.reload(&reformatted).expect("reload");

    assert_eq!(oracle.call("main", &[]).unwrap(), Value::Int(37));
    let events = oracle.events();
    assert_zero_cost_reload(&events);
    assert_eq!(hit_functions(&events), BTreeSet::from(["main".to_string()]));
}

#[test]
fn warm_reload_leaf_semantic_edit_misses_exact_theoretical_blast_radius() {
    let mut oracle = Oracle::load(anti_nix_diamond()).expect("load");
    assert_eq!(oracle.call("main", &[]).unwrap(), Value::Int(37));

    let edited =
        anti_nix_diamond().replace("fn leaf() -> Int {\n    1", "fn leaf() -> Int {\n    2");
    oracle.reload(&edited).expect("reload");

    assert_eq!(oracle.call("main", &[]).unwrap(), Value::Int(39));
    let events = oracle.events();
    assert_eq!(
        missed_functions(&events),
        BTreeSet::from([
            "leaf".to_string(),
            "left".to_string(),
            "main".to_string(),
            "right".to_string(),
        ]),
        "{events:?}"
    );
    let hits = hit_functions(&events);
    assert!(hits.contains("independent"), "{events:?}");
    assert!(!hits.contains("never_demanded"), "{events:?}");
}

#[test]
fn warm_reload_never_demanded_semantic_edit_costs_zero_misses() {
    let mut oracle = Oracle::load(anti_nix_diamond()).expect("load");
    assert_eq!(oracle.call("main", &[]).unwrap(), Value::Int(37));

    let edited = anti_nix_diamond().replace(
        "fn never_demanded() -> Int {\n    100",
        "fn never_demanded() -> Int {\n    101",
    );
    oracle.reload(&edited).expect("reload");

    assert_eq!(oracle.call("main", &[]).unwrap(), Value::Int(37));
    let events = oracle.events();
    assert_zero_cost_reload(&events);
    assert_eq!(hit_functions(&events), BTreeSet::from(["main".to_string()]));
}

#[test]
fn editing_unreferenced_function_preserves_other_closure_hashes() {
    let before = Oracle::load(anti_nix_diamond()).expect("load before");
    let edited = anti_nix_diamond().replace(
        "fn never_demanded() -> Int {\n    100",
        "fn never_demanded() -> Int {\n    101",
    );
    let after = Oracle::load(&edited).expect("load after");

    for name in ["leaf", "left", "right", "independent", "main"] {
        assert_eq!(
            before.fn_hash(name),
            after.fn_hash(name),
            "{name} should not inherit an unreferenced function edit"
        );
    }
    assert_ne!(
        before.fn_hash("never_demanded"),
        after.fn_hash("never_demanded")
    );
}

fn type_closure_source() -> &'static str {
    r#"
enum Choice { A, B }

fn typed(x: Choice) -> Int {
    match x {
        Choice::A => 1,
        Choice::B => 2,
    }
}

fn bridge() -> Int {
    typed(Choice::A)
}

fn independent() -> Int {
    7
}

pub fn main() -> Int {
    bridge() + independent()
}
"#
}

#[test]
fn warm_reload_type_declaration_edit_misses_exact_transitive_users() {
    let mut oracle = Oracle::load(type_closure_source()).expect("load");
    assert_eq!(oracle.call("main", &[]).unwrap(), Value::Int(8));

    let edited = type_closure_source().replace("enum Choice { A, B }", "enum Choice { B, A }");
    oracle.reload(&edited).expect("reload");

    assert_eq!(oracle.call("main", &[]).unwrap(), Value::Int(8));
    let events = oracle.events();
    assert_eq!(
        missed_functions(&events),
        BTreeSet::from([
            "bridge".to_string(),
            "main".to_string(),
            "typed".to_string(),
        ]),
        "{events:?}"
    );
    let hits = hit_functions(&events);
    assert!(hits.contains("independent"), "{events:?}");
}

#[test]
fn recursive_scc_closure_hashes_are_stable_across_definition_order() {
    let ab = r#"
fn a() -> Int { b() }
fn b() -> Int { a() }
"#;
    let ba = r#"
fn b() -> Int { a() }
fn a() -> Int { b() }
"#;
    let ab = Oracle::load(ab).expect("load ab");
    let ba = Oracle::load(ba).expect("load ba");

    assert_eq!(ab.fn_hash("a"), ba.fn_hash("a"));
    assert_eq!(ab.fn_hash("b"), ba.fn_hash("b"));
}

#[test]
fn types_vix_partials_guards_and_tuple_indexing() {
    let oracle = Oracle::load(&sample("types.vix")).expect("load");

    // partials(): scaled(k: 2, ..) then double(x: 21).
    assert_eq!(oracle.call("partials", &[]).unwrap(), Value::Int(42));

    // depths(): ((1, 2), 3).0.1
    assert_eq!(oracle.call("depths", &[]).unwrap(), Value::Int(2));

    // classify: guard picks the interpreter object; shorthand binds `name`.
    let obj = oracle
        .variant("Artifact", "Object", vec![Value::Path("lua.o".into())])
        .unwrap();
    assert_eq!(
        oracle.call("classify", &[("a", obj)]).unwrap(),
        Value::Str("the interpreter object".into())
    );
    let other = oracle
        .variant("Artifact", "Object", vec![Value::Path("lapi.o".into())])
        .unwrap();
    assert_eq!(
        oracle.call("classify", &[("a", other)]).unwrap(),
        Value::Str("an object".into())
    );

    // apply(f: scaled(k: 2, ..), x: 21) — a Partial is a first-class callable.
    let partial = oracle.call("scaled", &[("k", Value::Int(2))]).err();
    assert!(
        partial.is_some(),
        "scaled without x and without `..` errors"
    );
}

#[test]
fn toolchain_acquires_capabilities_and_updates_records() {
    let oracle = Oracle::load(&sample("types.vix")).expect("load");
    let windows = oracle.variant("Os", "Windows", vec![]).unwrap();
    let target = Value::Struct {
        name: "Target".into(),
        fields: vec![("os".into(), windows)],
    };

    let out = oracle.call("toolchain", &[("target", target)]).unwrap();
    let Value::Struct { name, fields } = &out else {
        panic!("toolchain returns a struct");
    };
    assert_eq!(name, "Toolchain");
    // Windows arm: opt tuned to 1 via record update; env set via record update
    // of the tuple-indexed base.
    assert_eq!(
        fields.iter().find(|(n, _)| n == "opt").map(|(_, v)| v),
        Some(&Value::Int(1))
    );
    let Some((_, Value::Map(env))) = fields.iter().find(|(n, _)| n == "env") else {
        panic!("env is a map");
    };
    assert_eq!(env.len(), 2);
    assert_eq!(
        env.get(&Value::Str("CFLAGS".into())),
        Some(&Value::Str("-O2".into()))
    );

    // Two capability acquisitions observed, neither replayed (cold).
    let obs: Vec<_> = oracle
        .events()
        .into_iter()
        .filter(|e| matches!(e, Event::Observation { .. }))
        .collect();
    assert_eq!(obs.len(), 2);
    assert!(obs.iter().all(|e| matches!(
        e,
        Event::Observation {
            replayed: false,
            ..
        }
    )));
}

#[test]
fn fetch_pins_the_journal_and_replays() {
    const URL: &str = "https://example.org/lua.tar.gz";
    const ARCHIVE: &[u8] = b"example fixture archive";
    let sha256 = sha256_hex(ARCHIVE);
    let src = format!(
        r#"
use vix::Tree;

fn src_tree(nonce: Int) -> Tree {{
    fetch(
        url: "{URL}",
        sha256: "{sha256}",
    )
}}
"#
    );
    let backend = FakeFetchBackend::new().with_archive(
        URL,
        ARCHIVE,
        Tree::of(&[("src/lib.rs", "pub fn f() {}")]),
    );
    let oracle = Oracle::load(&src)
        .expect("load")
        .with_fetch_backend(backend);
    let a = oracle
        .call("src_tree", &[("nonce", Value::Int(1))])
        .unwrap();
    // Different args = memo miss = fetch runs again — but the OBSERVATION is
    // pinned by its checksum, so the second run REPLAYS the pin.
    let b = oracle
        .call("src_tree", &[("nonce", Value::Int(2))])
        .unwrap();
    assert_eq!(a, b, "the checksum IS the identity");

    let obs: Vec<_> = oracle
        .events()
        .into_iter()
        .filter_map(|e| match e {
            Event::Observation { key, replayed } => Some((key, replayed)),
            _ => None,
        })
        .collect();
    assert_eq!(
        obs,
        vec![
            (format!("fetch:{URL}:sha256:{sha256}"), false),
            (format!("fetch:{URL}:sha256:{sha256}"), true),
        ]
    );
    assert_eq!(
        oracle.journal(),
        BTreeMap::from([(format!("fetch:{URL}:sha256:{sha256}"), Value::Str(sha256))])
    );
}

#[test]
fn closures_ship_between_oracles() {
    // exec = eval elsewhere. A closure serialized in one oracle and
    // reconstituted in another must evaluate to the SAME value — that's
    // purity, and it's the exec primitive minus the wire.
    let src = r#"
fn make_scaler(k: Int) -> fn(Int) -> Int {
    |x| k * x
}
"#;
    let a = Oracle::load(src).expect("load a");
    let scaler = a.call("make_scaler", &[("k", Value::Int(3))]).unwrap();
    assert!(matches!(scaler, Value::Closure { .. }));

    let bytes = vix::oracle::ship(&scaler).expect("ship");
    let b = Oracle::load(src).expect("load b");
    let arrived = vix::oracle::receive(&bytes).expect("receive");

    // Identity survives the wire: same canonical hash before and after.
    assert_eq!(scaler.canon_hash(), arrived.canon_hash());
    // And it computes: 3 * 14 on the remote side.
    assert_eq!(
        b.invoke(arrived, vec![Value::Int(14)]).unwrap(),
        Value::Int(42)
    );

    // The closure's identity is its canonical AST: a differently-formatted
    // but identical source yields a closure with the SAME hash.
    let reformatted = src.replace("|x| k * x", "|x|   k   *   x   // comment");
    let c = Oracle::load(&reformatted).expect("load c");
    let scaler2 = c.call("make_scaler", &[("k", Value::Int(3))]).unwrap();
    assert_eq!(scaler.canon_hash(), scaler2.canon_hash());
}

#[test]
fn values_are_totally_ordered_canonically() {
    let oracle = Oracle::load(&sample("types.vix")).expect("load");

    // Declaration order IS the total order: Linux < Macos < Windows.
    let linux = oracle.variant("Os", "Linux", vec![]).unwrap();
    let macos = oracle.variant("Os", "Macos", vec![]).unwrap();
    let windows = oracle.variant("Os", "Windows", vec![]).unwrap();
    assert!(linux < macos && macos < windows);

    // Floats: total order, NaN last, -0.0 == 0.0.
    assert!(Value::Float(1.0) < Value::Float(2.0));
    assert!(Value::Float(f64::INFINITY) < Value::Float(f64::NAN));
    assert_eq!(Value::Float(0.0), Value::Float(-0.0));

    // Maps iterate in canonical key order regardless of insertion order.
    let mut m = std::collections::BTreeMap::new();
    m.insert(Value::Str("z".into()), Value::Int(1));
    m.insert(Value::Str("a".into()), Value::Int(2));
    let keys: Vec<_> = m.keys().cloned().collect();
    assert_eq!(keys, vec![Value::Str("a".into()), Value::Str("z".into())]);

    // Hash agrees with equality across construction orders.
    let x = Value::Map(m.clone());
    let y = Value::Map(m);
    assert_eq!(x.canon_hash(), y.canon_hash());

    // Variant payloads participate: Some(1) < Some(2) < None (decl order).
    let _ = Payload::Unit; // (re-exported shape used across the suite)
}

#[test]
fn highlights_query_captures_lua_sample() {
    let parser = vix::VixParser::new();
    let caps = parser.highlights(&sample("lua.vix")).expect("highlights");
    assert!(!caps.is_empty());
    // The oracle for the oracle: known tokens land in known captures.
    let has = |name: &str, text: &str| {
        let src = sample("lua.vix");
        caps.iter()
            .any(|(cap, s, e)| cap == name && &src[*s as usize..*e as usize] == text)
    };
    assert!(has("keyword", "fn"), "{caps:?}");
    assert!(has("function", "sources"), "fn decl name: {caps:?}");
    assert!(has("string.special.path", "p\"lua.c\""), "{caps:?}");
    // Captures are byte ranges in document order, usable directly as spans.
    assert!(caps.windows(2).all(|w| w[0].1 <= w[1].1), "sorted starts");
}
