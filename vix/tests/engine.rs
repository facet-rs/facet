use std::collections::{BTreeMap, BTreeSet};

use vix::engine::Engine;
use vix::exec::ExecEvent;
use vix::oracle::{Event, Oracle, Payload, Value};

fn sample(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/../playgrounds/snark/src/bundled/vix/samples/{name}",
        env!("CARGO_MANIFEST_DIR"),
    ))
    .expect("read sample")
}

fn sample_fixture(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/../playgrounds/snark/src/bundled/vix/samples/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR"),
    ))
    .expect("read sample fixture")
}

fn artifact_object(path: &str) -> Value {
    Value::Variant {
        enum_name: "Artifact".into(),
        index: 0,
        name: "Object".into(),
        payload: Payload::Tuple(vec![Value::Path(path.into())]),
    }
}

fn target() -> Value {
    Value::Struct {
        name: "Target".into(),
        fields: vec![("os".into(), Value::Str("linux-x86_64".into()))],
    }
}

fn windows_target() -> Value {
    Value::Struct {
        name: "Target".into(),
        fields: vec![(
            "os".into(),
            Value::Variant {
                enum_name: "Os".into(),
                index: 2,
                name: "Windows".into(),
                payload: Payload::Unit,
            },
        )],
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

fn finished_multiset(events: &[Event]) -> BTreeMap<(String, String), usize> {
    let mut out = BTreeMap::new();
    for event in events {
        if let Event::Finished { command, event, .. } = event {
            let class = match event {
                ExecEvent::Tier1Hit => "tier1-hit".to_string(),
                ExecEvent::Tier2Cutoff { .. } => "tier2-cutoff".to_string(),
                ExecEvent::Ran => "ran".to_string(),
                ExecEvent::Joined => "joined".to_string(),
            };
            *out.entry((command.clone(), class)).or_insert(0) += 1;
        }
    }
    out
}

fn created_set(events: &[Event]) -> BTreeSet<(String, Vec<String>)> {
    events
        .iter()
        .filter_map(|event| match event {
            Event::Created { command, argv, .. } => Some((command.clone(), argv.clone())),
            _ => None,
        })
        .collect()
}

fn observation_keys(events: &[Event]) -> BTreeSet<String> {
    events
        .iter()
        .filter_map(|event| match event {
            Event::Observation { key, .. } => Some(key.clone()),
            _ => None,
        })
        .collect()
}

fn scheduled_count(events: &[Event]) -> usize {
    events
        .iter()
        .filter(|event| matches!(event, Event::Scheduled { .. }))
        .count()
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

fn assert_created_subset(engine: &[Event], oracle: &[Event]) {
    let engine = created_set(engine);
    let oracle = created_set(oracle);
    for created in &engine {
        assert!(
            oracle.contains(created),
            "engine created run absent from oracle: {created:?}\nengine={engine:?}\noracle={oracle:?}"
        );
    }
}

fn assert_full_contract(source: &str, func: &str, args: &[(&str, Value)], exact_created: bool) {
    let oracle = Oracle::load(source).expect("oracle load");
    let mut engine = Engine::load(source).expect("engine load");
    let oracle_value = oracle.call(func, args).expect("oracle call");
    let engine_value = engine.call(func, args).expect("engine call");
    assert_eq!(engine_value, oracle_value);
    let engine_events = engine.events();
    let oracle_events = oracle.events();
    assert_eq!(
        finished_multiset(&engine_events),
        finished_multiset(&oracle_events),
        "Finished command/event-class multiset differs\nengine={engine_events:?}\noracle={oracle_events:?}"
    );
    assert_eq!(
        scheduled_count(&engine_events),
        scheduled_count(&oracle_events),
        "Scheduled counts differ\nengine={engine_events:?}\noracle={oracle_events:?}"
    );
    assert_eq!(
        observation_keys(&engine_events),
        observation_keys(&oracle_events),
        "Observation key sets differ\nengine={engine_events:?}\noracle={oracle_events:?}"
    );
    assert_misses_subset(&engine_events, &oracle_events);
    if exact_created {
        assert_eq!(
            created_set(&engine_events),
            created_set(&oracle_events),
            "Created sets differ\nengine={engine_events:?}\noracle={oracle_events:?}"
        );
    } else {
        assert_created_subset(&engine_events, &oracle_events);
    }
}

#[test]
fn engine_matches_oracle_on_eval_vix() {
    assert_full_contract(&sample("eval.vix"), "demo", &[], false);
}

#[test]
fn engine_matches_oracle_on_types_vix() {
    let src = sample("types.vix");
    assert_full_contract(&src, "partials", &[], false);
    assert_full_contract(&src, "depths", &[], false);
    assert_full_contract(&src, "classify", &[("a", artifact_object("lua.o"))], false);
    assert_full_contract(&src, "classify", &[("a", artifact_object("lapi.o"))], false);
    assert_full_contract(&src, "toolchain", &[("target", windows_target())], false);
}

#[test]
fn engine_matches_oracle_on_lua_vix_exec_seam() {
    let src = sample("lua.vix");
    let oracle = Oracle::load(&src).expect("oracle load");
    let mut engine = Engine::load(&src).expect("engine load");

    let oracle_value = oracle
        .call("lua", &[("target", target())])
        .expect("oracle call");
    let engine_value = engine
        .call("lua", &[("target", target())])
        .expect("engine call");
    assert_eq!(engine_value, oracle_value);

    let engine_events = engine.events();
    let oracle_events = oracle.events();
    assert_eq!(
        finished_multiset(&engine_events),
        finished_multiset(&oracle_events)
    );
    assert_eq!(scheduled_count(&engine_events), 5, "{engine_events:?}");
    assert_eq!(scheduled_count(&oracle_events), 5, "{oracle_events:?}");
    assert_eq!(created_set(&engine_events), created_set(&oracle_events));
    assert_eq!(
        observation_keys(&engine_events),
        observation_keys(&oracle_events)
    );
    assert_misses_subset(&engine_events, &oracle_events);
}

#[test]
fn engine_matches_oracle_on_cargo_toml_projection() {
    let manifest = sample_fixture("Cargo.toml");
    let tree = Value::Tree(vix::exec::Tree::of(&[("Cargo.toml", &manifest)]));
    assert_full_contract(
        &sample("cargo.vix"),
        "cargo_manifest",
        &[("manifest", tree)],
        false,
    );
}

#[test]
fn engine_matches_oracle_on_json_structural_values() {
    let src = r#"
pub fn parse(input: String) -> (String, Int, Bool) {
    let doc = json(input);
    let package = doc.get("package").unwrap();
    (
        package.get("name").unwrap(),
        package.get("version").unwrap(),
        doc.get("publish").unwrap(),
    )
}
"#;
    assert_full_contract(
        src,
        "parse",
        &[(
            "input",
            Value::Str(
                r#"{"package":{"name":"mini-real-crate","version":3},"publish":false}"#.into(),
            ),
        )],
        false,
    );
}

#[test]
fn warm_engine_lua_second_call_is_one_hit() {
    let src = sample("lua.vix");
    let mut engine = Engine::load(&src).expect("engine load");

    let first = engine.call("lua", &[("target", target())]).unwrap();
    let before = engine.events().len();
    let second = engine.call("lua", &[("target", target())]).unwrap();

    assert_eq!(first, second);
    let warm = &engine.events()[before..];
    assert_eq!(warm.len(), 1, "{warm:?}");
    assert!(
        matches!(&warm[0], Event::Hit { func, .. } if func == "lua"),
        "{warm:?}"
    );
}

#[test]
fn unused_command_binding_is_never_created_by_engine() {
    let src = r#"
use vix::Target;
use caps::Cc;

pub fn main(target: Target) -> Int {
    let cc = Cc::acquire(target);
    let dead = cc! { -o dead };
    7
}
"#;
    let oracle = Oracle::load(src).expect("oracle load");
    let mut engine = Engine::load(src).expect("engine load");

    assert_eq!(
        oracle.call("main", &[("target", target())]).unwrap(),
        Value::Int(7)
    );
    assert_eq!(
        engine.call("main", &[("target", target())]).unwrap(),
        Value::Int(7)
    );
    assert_eq!(
        created_set(&oracle.events()).len(),
        1,
        "{:?}",
        oracle.events()
    );
    assert!(
        created_set(&engine.events()).is_empty(),
        "{:?}",
        engine.events()
    );
    assert_eq!(finished_multiset(&engine.events()), BTreeMap::new());
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

#[test]
fn oracle_rejects_duplicate_named_arguments() {
    let src = r#"
fn f(x: Int) -> Int {
    x
}

pub fn main() -> Int {
    f(x: 1, x: 2)
}
"#;
    let oracle = Oracle::load(src).expect("oracle load");
    let err = oracle
        .call("main", &[])
        .expect_err("duplicate argument errors");
    assert!(err.contains("duplicate argument `x`"), "{err}");
}

#[test]
fn engine_rejects_duplicate_named_arguments() {
    let src = r#"
fn f(x: Int) -> Int {
    x
}

pub fn main() -> Int {
    f(x: 1, x: 2)
}
"#;
    let mut engine = Engine::load(src).expect("engine load");
    let err = engine
        .call("main", &[])
        .expect_err("duplicate argument errors");
    assert!(err.contains("duplicate argument `x`"), "{err}");
}
