use std::collections::BTreeMap;

use vix::engine::Engine;
use vix::oracle::{Event, Oracle, Payload, Value};

fn sample(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/../playgrounds/snark/src/bundled/vix/samples/{name}",
        env!("CARGO_MANIFEST_DIR"),
    ))
    .expect("read sample")
}

fn artifact_object(path: &str) -> Value {
    Value::Variant {
        enum_name: "Artifact".into(),
        index: 0,
        name: "Object".into(),
        payload: Payload::Tuple(vec![Value::Path(path.into())]),
    }
}

fn miss_multiset(events: &[Event]) -> BTreeMap<String, usize> {
    let mut out = BTreeMap::new();
    for event in events {
        if let Event::Miss { func, .. } = event {
            *out.entry(func.clone()).or_insert(0) += 1;
        }
    }
    out
}

fn hit_count(events: &[Event], name: &str) -> usize {
    events
        .iter()
        .filter(|event| matches!(event, Event::Hit { func, .. } if func == name))
        .count()
}

fn miss_count(events: &[Event], name: &str) -> usize {
    events
        .iter()
        .filter(|event| matches!(event, Event::Miss { func, .. } if func == name))
        .count()
}

fn assert_misses_subset(engine: &[Event], oracle: &[Event]) {
    let engine = miss_multiset(engine);
    let oracle = miss_multiset(oracle);
    for (func, engine_count) in engine {
        let oracle_count = oracle.get(&func).copied().unwrap_or(0);
        assert!(
            engine_count <= oracle_count,
            "engine miss count for {func}: {engine_count}, oracle: {oracle_count}"
        );
    }
}

fn assert_matches(source: &str, func: &str, args: &[(&str, Value)]) {
    let oracle = Oracle::load(source).expect("oracle load");
    let mut engine = Engine::load(source).expect("engine load");
    let oracle_value = oracle.call(func, args).expect("oracle call");
    let engine_value = engine.call(func, args).expect("engine call");
    assert_eq!(engine_value, oracle_value);
    assert_misses_subset(&engine.events(), &oracle.events());
}

#[test]
fn engine_matches_oracle_on_eval_vix() {
    assert_matches(&sample("eval.vix"), "demo", &[]);
}

#[test]
fn engine_matches_oracle_on_types_vix() {
    let src = sample("types.vix");
    assert_matches(&src, "partials", &[]);
    assert_matches(&src, "depths", &[]);
    assert_matches(&src, "classify", &[("a", artifact_object("lua.o"))]);
    assert_matches(&src, "classify", &[("a", artifact_object("lapi.o"))]);
}

#[test]
fn unused_binding_is_never_demanded() {
    let src = r#"
fn expensive() -> Int {
    41
}

pub fn main() -> Int {
    let x = expensive();
    7
}
"#;
    let oracle = Oracle::load(src).expect("oracle load");
    let mut engine = Engine::load(src).expect("engine load");

    assert_eq!(oracle.call("main", &[]).unwrap(), Value::Int(7));
    assert_eq!(engine.call("main", &[]).unwrap(), Value::Int(7));
    assert_eq!(miss_count(&oracle.events(), "expensive"), 1);
    assert_eq!(miss_count(&engine.events(), "expensive"), 0);
}

#[test]
fn shared_binding_computes_once() {
    let src = r#"
fn f(x: Int) -> Int {
    x + 1
}

pub fn main() -> Int {
    let x = f(20);
    x + x
}
"#;
    let mut engine = Engine::load(src).expect("engine load");

    assert_eq!(engine.call("main", &[]).unwrap(), Value::Int(42));
    let events = engine.events();
    assert_eq!(miss_count(&events, "f"), 1);
    assert_eq!(hit_count(&events, "f"), 0);
}

#[test]
fn unselected_match_arm_never_evaluates() {
    let src = r#"
fn boom() -> Int {
    1 / 0
}

pub fn main() -> Int {
    match 0 {
        0 => 42,
        _ => boom(),
    }
}
"#;
    let oracle = Oracle::load(src).expect("oracle load");
    let mut engine = Engine::load(src).expect("engine load");

    // Oracle parity note: the eager oracle also leaves unselected match arms
    // alone; this is parity, not an engine-lazier divergence.
    assert_eq!(oracle.call("main", &[]).unwrap(), Value::Int(42));
    assert_eq!(engine.call("main", &[]).unwrap(), Value::Int(42));
    assert_eq!(miss_count(&oracle.events(), "boom"), 0);
    assert_eq!(miss_count(&engine.events(), "boom"), 0);
}

#[test]
fn memo_hits_across_calls() {
    let src = r#"
fn f(x: Int) -> Int {
    x + 1
}

fn a() -> Int {
    f(20)
}

fn b() -> Int {
    f(20)
}

pub fn main() -> Int {
    a() + b()
}
"#;
    let mut engine = Engine::load(src).expect("engine load");

    assert_eq!(engine.call("main", &[]).unwrap(), Value::Int(42));
    let events = engine.events();
    assert_eq!(miss_count(&events, "f"), 1);
    assert!(hit_count(&events, "f") >= 1, "{events:?}");
}
