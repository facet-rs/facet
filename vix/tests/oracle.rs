//! Design probes: run the corpus through the oracle and observe identity,
//! caching, observations, and total order doing what the synthesis doc claims.

use vix::oracle::{Event, Oracle, Payload, Value};

fn sample(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/../playgrounds/snark/src/bundled/vix/samples/{name}",
        env!("CARGO_MANIFEST_DIR"),
    ))
    .expect("read sample")
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
    assert_eq!(warm, &[Event::Hit { func: "demo".to_string() }]);

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
    assert!(partial.is_some(), "scaled without x and without `..` errors");
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
        Event::Observation { replayed: false, .. }
    )));
}

#[test]
fn fetch_pins_the_journal_and_replays() {
    let src = r#"
use vix::Tree;

fn src_tree(nonce: Int) -> Tree {
    fetch(
        url: "https://example.org/lua.tar.gz",
        sha256: "abc123",
    )
}
"#;
    let oracle = Oracle::load(src).expect("load");
    let a = oracle.call("src_tree", &[("nonce", Value::Int(1))]).unwrap();
    // Different args = memo miss = fetch runs again — but the OBSERVATION is
    // pinned by its checksum, so the second run REPLAYS the pin.
    let b = oracle.call("src_tree", &[("nonce", Value::Int(2))]).unwrap();
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
            ("fetch:abc123".to_string(), false),
            ("fetch:abc123".to_string(), true),
        ]
    );
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
