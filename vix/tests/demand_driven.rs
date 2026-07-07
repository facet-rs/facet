mod common;

use std::collections::BTreeSet;

use common::*;
use vix::machine::driver::Lane;
use vix::machine::{DriveEvent, MachineArg, NamedArg, RenderedValue, StoreHandle};

const CORPUS: &str = r#"
fn square(x: Int) -> Int { x * x }

fn twice_sq(n: Int) -> Int { square(n) + square(n) }

pub fn poly(n: Int) -> Int {
    let t = twice_sq(n + 1);
    t - n
}
"#;

fn assert_lane_traces_equal(traces: &[(Lane, Vec<DriveEvent>)]) {
    let Some((first_lane, first_trace)) = traces.first() else {
        return;
    };
    for (lane, trace) in &traces[1..] {
        assert_eq!(
            trace, first_trace,
            "driver trace diverged between {first_lane:?} and {lane:?}"
        );
    }
}

fn memo_projection_hit_count(machine: &vix::machine::Machine, name: &str) -> usize {
    let hash = machine.fn_hash(name).expect("function hash");
    machine
        .trace()
        .iter()
        .filter(|event| {
            matches!(event, DriveEvent::MemoProjectionHit { fn_hash, .. } if *fn_hash == hash)
        })
        .count()
}

fn projection_verified_count(machine: &vix::machine::Machine, name: &str) -> Vec<usize> {
    let hash = machine.fn_hash(name).expect("function hash");
    machine
        .trace()
        .iter()
        .filter_map(|event| match event {
            DriveEvent::MemoProjectionHit {
                fn_hash, verified, ..
            } if *fn_hash == hash => Some(*verified),
            _ => None,
        })
        .collect()
}

fn memo_semantic_hit_count(machine: &vix::machine::Machine, name: &str) -> usize {
    let hash = machine.fn_hash(name).expect("function hash");
    machine
        .trace()
        .iter()
        .filter(|event| {
            matches!(event, DriveEvent::MemoSemanticHit { fn_hash, .. } if *fn_hash == hash)
        })
        .count()
}

fn fn_event_count(
    machine: &vix::machine::Machine,
    name: &str,
    matches_event: impl Fn(&DriveEvent, u64) -> bool,
) -> usize {
    let hash = machine.fn_hash(name).expect("function hash");
    machine
        .trace()
        .iter()
        .filter(|event| matches_event(event, hash))
        .count()
}

fn demanded_count(machine: &vix::machine::Machine, name: &str) -> usize {
    fn_event_count(
        machine,
        name,
        |event, hash| matches!(event, DriveEvent::Demanded { fn_hash } if *fn_hash == hash),
    )
}

fn completed_count(machine: &vix::machine::Machine, name: &str) -> usize {
    fn_event_count(
        machine,
        name,
        |event, hash| matches!(event, DriveEvent::Completed { fn_hash } if *fn_hash == hash),
    )
}

fn spawned_invocation_count(machine: &vix::machine::Machine, name: &str) -> usize {
    fn_event_count(
        machine,
        name,
        |event, hash| matches!(event, DriveEvent::SpawnedInvocation { fn_hash, .. } if *fn_hash == hash),
    )
}

fn store_alloc_count(machine: &vix::machine::Machine) -> usize {
    machine
        .trace()
        .iter()
        .filter(|event| matches!(event, DriveEvent::StoreAlloc { .. }))
        .count()
}

fn strict_subsequence<T: PartialEq>(needle: &[T], haystack: &[T]) -> bool {
    if needle.len() >= haystack.len() {
        return false;
    }
    let mut next = 0usize;
    for item in haystack {
        if needle.get(next).is_some_and(|candidate| candidate == item) {
            next += 1;
            if next == needle.len() {
                return true;
            }
        }
    }
    false
}

fn countdown_tail_source() -> &'static str {
    r#"
pub fn countdown(n: Int, acc: Int) -> Int {
    match n {
        0 => acc,
        _ => countdown(n - 1, acc + 1),
    }
}
"#
}

fn load_forced_tail_invoke(source: &str, lane: Lane) -> vix::machine::Machine {
    let mut machine = load_with_lane(source, lane);
    machine.set_force_tail_invoke(true).unwrap();
    machine
}

fn semantic_cutoff_demo_source(comparator_name: &str) -> String {
    format!(
        r#"
use vix::{{Version, VersionSet}};

fn expensive(req: VersionSet) -> Int {{
    match req.contains(version("1.2.3")) {{
        true => 42,
        false => 0,
    }}
}}

pub fn derived(req: VersionSet) -> Int {{
    expensive(req)
}}

fn {comparator_name}(old: VersionSet, new: VersionSet) -> Bool {{
    new.subset(old)
}}
"#
    )
}

fn call_derived(machine: &mut vix::machine::Machine, req: &str) -> i64 {
    machine
        .call(
            "derived",
            &[NamedArg {
                name: "req".into(),
                value: MachineArg::String(req.into()),
            }],
        )
        .unwrap()
        .0
}

fn semantic_observable_trace(machine: &vix::machine::Machine) -> Vec<DriveEvent> {
    let recompute_hashes = [
        machine.fn_hash("derived").expect("derived hash"),
        machine.fn_hash("expensive").expect("expensive hash"),
        machine
            .fn_hash("derived__memo_verify_req")
            .or_else(|| machine.fn_hash("derived__not_a_memo_verify_req"))
            .expect("comparator hash"),
    ];
    machine
        .trace()
        .iter()
        .filter(|event| match event {
            DriveEvent::Demanded { fn_hash }
            | DriveEvent::MemoHit { fn_hash }
            | DriveEvent::MemoProjectionHit { fn_hash, .. }
            | DriveEvent::MemoSemanticHit { fn_hash, .. }
            | DriveEvent::Spawned { fn_hash }
            | DriveEvent::ParkedOn { fn_hash }
            | DriveEvent::Completed { fn_hash }
            | DriveEvent::SpawnedInvocation { fn_hash, .. } => !recompute_hashes.contains(fn_hash),
            _ => true,
        })
        .cloned()
        .collect()
}

#[test]
fn config_gen_template_renders_bytes_and_demands_only_used_holes() {
    let src = r#"
fn active() -> String { "enabled" }
fn unused() -> String { "never" }

pub fn config() -> String {
    let bindings: Map<String, String> = {};
    let bindings = bindings
        .insert("server", "api.local")
        .insert("env", "PROD")
        .insert("active", active())
        .insert("unused", unused())
        .insert("empty", "");
    render(tmpl"server {{ server | upper }}
env={{ env | lower }}
active={{ active }}
fallback={{ empty | default(\"8080\") }}
", bindings)
}
"#;
    let expected = "server API.LOCAL\nenv=prod\nactive=enabled\nfallback=8080\n";
    for lane in lanes() {
        let mut machine = load_with_lane(src, lane);
        let result = machine.demand_i64("config", vec![]).unwrap();
        let RenderedValue::String { value } = machine.render_result("config", result).unwrap()
        else {
            panic!("config did not render as String on {lane:?}");
        };
        assert_eq!(value, expected, "{lane:?}");
        assert_eq!(spawned_count(&machine, "active"), 1, "{lane:?}");
        assert_eq!(spawned_count(&machine, "unused"), 0, "{lane:?}");
    }
}

#[test]
fn shared_calls_spawn_once() {
    for lane in lanes() {
        let mut m = load_with_lane(CORPUS, lane);
        m.demand_i64("poly", vec![3]).unwrap();
        let spawns = m
            .trace()
            .iter()
            .filter(|e| matches!(e, DriveEvent::Spawned { .. }))
            .count();
        assert_eq!(spawns, 3, "{lane:?}");
    }
}

#[test]
fn warm_demand_is_two_events() {
    for lane in lanes() {
        let mut m = load_with_lane(CORPUS, lane);
        m.demand_i64("poly", vec![3]).unwrap();
        m.clear_trace();
        assert_eq!(m.demand_i64("poly", vec![3]).unwrap(), 29, "{lane:?}");
        assert_eq!(
            m.trace().len(),
            2,
            "Demanded + MemoHit, nothing else on {lane:?}"
        );
    }
}

#[test]
fn undemanded_functions_never_trace() {
    let source = format!("{CORPUS}\nfn never(z: Int) -> Int {{ z * 1000 }}\n");
    for lane in lanes() {
        let mut m = load_with_lane(&source, lane);
        m.demand_i64("poly", vec![5]).unwrap();
        let poly = m.demand_i64("poly", vec![5]).unwrap();
        assert_eq!(poly, (6 * 6) * 2 - 5, "{lane:?}");
        assert_eq!(
            m.trace()
                .iter()
                .filter(|e| matches!(e, DriveEvent::Spawned { .. }))
                .count(),
            3,
            "three spawns total; `never` never appears on {lane:?}"
        );
    }
}

#[test]
fn untaken_arms_never_spawn() {
    let src = r#"
fn cheap(x: Int) -> Int { x + 1 }
fn expensive(x: Int) -> Int { x * 1000000 }
fn pick(b: Int) -> Int {
    match b {
        0 => cheap(b),
        _ => expensive(b),
    }
}
"#;
    let mut traces = Vec::new();
    for lane in lanes() {
        let mut m = load_with_lane(src, lane);
        assert_eq!(m.demand_i64("pick", vec![0]).unwrap(), 1, "{lane:?}");
        let spawns = m
            .trace()
            .iter()
            .filter(|e| matches!(e, DriveEvent::Spawned { .. }))
            .count();
        assert_eq!(spawns, 2, "{lane:?}");
        traces.push((lane, m.trace().to_vec()));
    }
    assert_lane_traces_equal(&traces);
}

#[test]
fn structural_equal_handles_share_store_and_memo() {
    let src = r#"
enum Expr {
    Num(Int),
    Add(Expr, Expr),
}

fn make_a() -> Expr {
    Expr::Add(Expr::Num(1), Expr::Num(2))
}

fn make_b() -> Expr {
    Expr::Add(Expr::Num(1), Expr::Num(2))
}

fn eval(e: Expr) -> Int {
    match e {
        Expr::Num(n) => n,
        Expr::Add(a, b) => eval(a) + eval(b),
    }
}

fn main() -> Int {
    let a = make_a();
    let b = make_b();
    eval(a) + eval(b)
}
"#;
    for lane in lanes() {
        let mut m = load_with_lane(src, lane);
        assert_eq!(m.demand_i64("main", vec![]).unwrap(), 6, "{lane:?}");
        let eval_hash = m.fn_hash("eval").expect("eval hash");
        let eval_spawns = m
            .trace()
            .iter()
            .filter(|e| matches!(e, DriveEvent::Spawned { fn_hash } if *fn_hash == eval_hash))
            .count();
        let eval_hits = m
            .trace()
            .iter()
            .filter(|e| matches!(e, DriveEvent::MemoHit { fn_hash } if *fn_hash == eval_hash))
            .count();
        assert_eq!(
            eval_spawns, 3,
            "Add, Num(1), Num(2) each spawn once on {lane:?}"
        );
        assert!(
            eval_hits > 0,
            "second structurally equal tree hits memo on {lane:?}"
        );
        assert!(
            m.trace()
                .iter()
                .any(|e| matches!(e, DriveEvent::StoreAlloc { deduped: true, .. })),
            "second constructor path dedupes in the value store on {lane:?}"
        );
    }
}

#[test]
fn untaken_variant_arms_never_spawn() {
    let src = r#"
enum Choice { A, B }

fn expensive() -> Int { 999999 }

fn pick(c: Choice) -> Int {
    match c {
        Choice::A => 1,
        Choice::B => expensive(),
    }
}

fn main() -> Int {
    pick(Choice::A)
}
"#;
    let mut traces = Vec::new();
    for lane in lanes() {
        let mut m = load_with_lane(src, lane);
        assert_eq!(m.demand_i64("main", vec![]).unwrap(), 1, "{lane:?}");
        let expensive_hash = m.fn_hash("expensive").expect("expensive hash");
        assert!(
            !m.trace().iter().any(|e| matches!(
                e,
                DriveEvent::Demanded { fn_hash } | DriveEvent::Spawned { fn_hash }
                    if *fn_hash == expensive_hash
            )),
            "untaken variant arm never demands or spawns expensive on {lane:?}"
        );
        traces.push((lane, m.trace().to_vec()));
    }
    assert_lane_traces_equal(&traces);
}

#[test]
fn eval_vix_demo_returns_42_on_the_machine() {
    let src = include_str!("../../playgrounds/snark/src/bundled/vix/samples/eval.vix");
    let mut cold_traces = Vec::new();
    for lane in lanes() {
        let mut m = load_with_lane(src, lane);
        let bits = m.demand_i64("demo", vec![]).unwrap() as u64;
        assert_eq!(bits, 42.0f64.to_bits(), "{lane:?}");
        let demo_hash = m.fn_hash("demo").expect("demo hash");
        let spawns = m
            .trace()
            .iter()
            .filter(|event| matches!(event, DriveEvent::Spawned { .. }))
            .count();
        assert_eq!(
            spawns, 6,
            "demo plus five distinct eval invocations on {lane:?}"
        );
        cold_traces.push((lane, m.trace().to_vec()));

        m.clear_trace();
        let warm_bits = m.demand_i64("demo", vec![]).unwrap() as u64;
        assert_eq!(warm_bits, 42.0f64.to_bits(), "{lane:?}");
        assert_eq!(
            m.trace(),
            &[
                DriveEvent::Demanded { fn_hash: demo_hash },
                DriveEvent::MemoHit { fn_hash: demo_hash },
            ],
            "warm demo is exactly demand + memo hit on {lane:?}"
        );
    }
    assert_lane_traces_equal(&cold_traces);
}

#[test]
fn eval_vix_untaken_helper_never_appears() {
    let src = format!(
        "{}\n{}",
        include_str!("../../playgrounds/snark/src/bundled/vix/samples/eval.vix"),
        r#"
fn never_float() -> Float { 99.0 }

pub fn lazy_probe() -> Float {
    let e = Expr::Num(1.0);
    match e {
        Expr::Num(n) => n,
        Expr::Var(_) => never_float(),
        _ => 0.0,
    }
}
"#
    );
    for lane in lanes() {
        let mut m = load_with_lane(&src, lane);
        assert_eq!(
            (m.demand_i64("lazy_probe", vec![]).unwrap() as u64),
            1.0f64.to_bits(),
            "{lane:?}"
        );
        let never_hash = m.fn_hash("never_float").expect("never_float hash");
        assert!(
            !m.trace().iter().any(|event| matches!(
                event,
                DriveEvent::Demanded { fn_hash } | DriveEvent::Spawned { fn_hash }
                    if *fn_hash == never_hash
            )),
            "helper in untaken variant arm never demanded or spawned on {lane:?}"
        );
    }
}

#[test]
fn insertion_order_equal_maps_memoize_as_equal_arguments() {
    let src = r#"
fn ab() -> Map<String, Float> {
    let m: Map<String, Float> = {};
    m.insert("a", 1.0).insert("b", 2.0)
}

fn ba() -> Map<String, Float> {
    let m: Map<String, Float> = {};
    m.insert("b", 2.0).insert("a", 1.0)
}

fn consume(m: Map<String, Float>) -> Float {
    m.get("a").unwrap() + m.get("b").unwrap()
}

fn main() -> Float {
    consume(ab()) + consume(ba())
}
"#;
    for lane in lanes() {
        let mut m = load_with_lane(src, lane);
        assert_eq!(
            (m.demand_i64("main", vec![]).unwrap() as u64),
            6.0f64.to_bits(),
            "{lane:?}"
        );
        let consume_hash = m.fn_hash("consume").expect("consume hash");
        let consume_spawns = m
            .trace()
            .iter()
            .filter(|event| matches!(event, DriveEvent::Spawned { fn_hash } if *fn_hash == consume_hash))
            .count();
        let consume_hits = m
            .trace()
            .iter()
            .filter(|event| matches!(event, DriveEvent::MemoHit { fn_hash } if *fn_hash == consume_hash))
            .count();
        assert_eq!(consume_spawns, 1, "{lane:?}");
        assert_eq!(consume_hits, 1, "{lane:?}");
    }
}

#[test]
fn record_projection_hit_ignores_untouched_field_and_misses_touched_field() {
    let src = r#"
struct BigRecord {
    wanted: Int,
    untouched: String
}

pub fn make(wanted: Int, untouched: String) -> BigRecord {
    BigRecord { wanted: wanted, untouched: untouched }
}

pub fn pick(record: BigRecord) -> Int {
    record.wanted
}
"#;
    for lane in lanes() {
        let mut machine = load_with_lane(src, lane);
        let first = machine
            .call(
                "make",
                &[
                    NamedArg {
                        name: "wanted".to_string(),
                        value: MachineArg::Int(7),
                    },
                    NamedArg {
                        name: "untouched".to_string(),
                        value: MachineArg::String("first".to_string()),
                    },
                ],
            )
            .unwrap()
            .0;
        assert_eq!(
            machine.demand_i64("pick", vec![first]).unwrap(),
            7,
            "{lane:?}"
        );

        let untouched_changed = machine
            .call(
                "make",
                &[
                    NamedArg {
                        name: "wanted".to_string(),
                        value: MachineArg::Int(7),
                    },
                    NamedArg {
                        name: "untouched".to_string(),
                        value: MachineArg::String("edited".to_string()),
                    },
                ],
            )
            .unwrap()
            .0;
        machine.clear_trace();
        assert_eq!(
            machine.demand_i64("pick", vec![untouched_changed]).unwrap(),
            7,
            "{lane:?}"
        );
        assert_eq!(spawned_count(&machine, "pick"), 0, "{lane:?}");
        assert_eq!(memo_projection_hit_count(&machine, "pick"), 1, "{lane:?}");
        assert_eq!(
            projection_verified_count(&machine, "pick"),
            vec![1],
            "{lane:?}"
        );

        let touched_changed = machine
            .call(
                "make",
                &[
                    NamedArg {
                        name: "wanted".to_string(),
                        value: MachineArg::Int(8),
                    },
                    NamedArg {
                        name: "untouched".to_string(),
                        value: MachineArg::String("edited".to_string()),
                    },
                ],
            )
            .unwrap()
            .0;
        machine.clear_trace();
        assert_eq!(
            machine.demand_i64("pick", vec![touched_changed]).unwrap(),
            8,
            "{lane:?}"
        );
        assert_eq!(memo_projection_hit_count(&machine, "pick"), 0, "{lane:?}");
        assert_eq!(spawned_count(&machine, "pick"), 1, "{lane:?}");
    }
}

#[test]
fn map_projection_hit_ignores_untouched_entry_and_misses_touched_entry() {
    let src = r#"
pub fn make(wanted: String, untouched: String) -> Map<String, String> {
    let m: Map<String, String> = {};
    m.insert("wanted", wanted).insert("untouched", untouched)
}

pub fn pick(map: Map<String, String>) -> String {
    map.get("wanted").unwrap()
}
"#;
    for lane in lanes() {
        let mut machine = load_with_lane(src, lane);
        let first = machine
            .call(
                "make",
                &[
                    NamedArg {
                        name: "wanted".to_string(),
                        value: MachineArg::String("keep".to_string()),
                    },
                    NamedArg {
                        name: "untouched".to_string(),
                        value: MachineArg::String("first".to_string()),
                    },
                ],
            )
            .unwrap()
            .0;
        let first_value = machine.demand_i64("pick", vec![first]).unwrap();
        let RenderedValue::String { value } = machine.render_result("pick", first_value).unwrap()
        else {
            panic!("pick did not render as String on {lane:?}");
        };
        assert_eq!(value, "keep", "{lane:?}");

        let untouched_changed = machine
            .call(
                "make",
                &[
                    NamedArg {
                        name: "wanted".to_string(),
                        value: MachineArg::String("keep".to_string()),
                    },
                    NamedArg {
                        name: "untouched".to_string(),
                        value: MachineArg::String("edited".to_string()),
                    },
                ],
            )
            .unwrap()
            .0;
        machine.clear_trace();
        let second_value = machine.demand_i64("pick", vec![untouched_changed]).unwrap();
        let RenderedValue::String { value } = machine.render_result("pick", second_value).unwrap()
        else {
            panic!("pick did not render as String on {lane:?}");
        };
        assert_eq!(value, "keep", "{lane:?}");
        assert_eq!(spawned_count(&machine, "pick"), 0, "{lane:?}");
        assert_eq!(memo_projection_hit_count(&machine, "pick"), 1, "{lane:?}");
        assert_eq!(
            projection_verified_count(&machine, "pick"),
            vec![1],
            "{lane:?}"
        );

        let touched_changed = machine
            .call(
                "make",
                &[
                    NamedArg {
                        name: "wanted".to_string(),
                        value: MachineArg::String("changed".to_string()),
                    },
                    NamedArg {
                        name: "untouched".to_string(),
                        value: MachineArg::String("edited".to_string()),
                    },
                ],
            )
            .unwrap()
            .0;
        machine.clear_trace();
        let third_value = machine.demand_i64("pick", vec![touched_changed]).unwrap();
        let RenderedValue::String { value } = machine.render_result("pick", third_value).unwrap()
        else {
            panic!("pick did not render as String on {lane:?}");
        };
        assert_eq!(value, "changed", "{lane:?}");
        assert_eq!(memo_projection_hit_count(&machine, "pick"), 0, "{lane:?}");
        assert_eq!(spawned_count(&machine, "pick"), 1, "{lane:?}");
    }
}

#[test]
fn is_patron_ruling_uses_only_is_patron_projection() {
    let src = r#"
struct Session {
    is_patron: Bool,
    profile_note: String
}

pub fn session(is_patron: Bool, profile_note: String) -> Session {
    Session { is_patron: is_patron, profile_note: profile_note }
}

pub fn is_patron(session: Session) -> Bool {
    session.is_patron
}
"#;
    for lane in lanes() {
        let mut machine = load_with_lane(src, lane);
        let first = machine
            .call(
                "session",
                &[
                    NamedArg {
                        name: "is_patron".to_string(),
                        value: MachineArg::Bool(true),
                    },
                    NamedArg {
                        name: "profile_note".to_string(),
                        value: MachineArg::String("old profile".to_string()),
                    },
                ],
            )
            .unwrap()
            .0;
        assert_eq!(
            machine.demand_i64("is_patron", vec![first]).unwrap(),
            1,
            "{lane:?}"
        );

        let changed_profile = machine
            .call(
                "session",
                &[
                    NamedArg {
                        name: "is_patron".to_string(),
                        value: MachineArg::Bool(true),
                    },
                    NamedArg {
                        name: "profile_note".to_string(),
                        value: MachineArg::String("new profile".to_string()),
                    },
                ],
            )
            .unwrap()
            .0;
        machine.clear_trace();
        assert_eq!(
            machine
                .demand_i64("is_patron", vec![changed_profile])
                .unwrap(),
            1,
            "{lane:?}"
        );
        assert_eq!(spawned_count(&machine, "is_patron"), 0, "{lane:?}");
        assert_eq!(
            memo_projection_hit_count(&machine, "is_patron"),
            1,
            "{lane:?}"
        );
        assert_eq!(
            projection_verified_count(&machine, "is_patron"),
            vec![1],
            "{lane:?}"
        );

        let changed_patron = machine
            .call(
                "session",
                &[
                    NamedArg {
                        name: "is_patron".to_string(),
                        value: MachineArg::Bool(false),
                    },
                    NamedArg {
                        name: "profile_note".to_string(),
                        value: MachineArg::String("new profile".to_string()),
                    },
                ],
            )
            .unwrap()
            .0;
        machine.clear_trace();
        assert_eq!(
            machine
                .demand_i64("is_patron", vec![changed_patron])
                .unwrap(),
            0,
            "{lane:?}"
        );
        assert_eq!(
            memo_projection_hit_count(&machine, "is_patron"),
            0,
            "{lane:?}"
        );
        assert_eq!(spawned_count(&machine, "is_patron"), 1, "{lane:?}");
    }
}

#[test]
fn projection_read_sets_survive_warm_reload() {
    let src = r#"
struct Session {
    is_patron: Bool,
    profile_note: String
}

pub fn session(is_patron: Bool, profile_note: String) -> Session {
    Session { is_patron: is_patron, profile_note: profile_note }
}

pub fn is_patron(session: Session) -> Bool {
    session.is_patron
}
"#;
    for lane in lanes() {
        let mut machine = load_with_lane(src, lane);
        let first = machine
            .call(
                "session",
                &[
                    NamedArg {
                        name: "is_patron".to_string(),
                        value: MachineArg::Bool(true),
                    },
                    NamedArg {
                        name: "profile_note".to_string(),
                        value: MachineArg::String("old profile".to_string()),
                    },
                ],
            )
            .unwrap()
            .0;
        assert_eq!(
            machine.demand_i64("is_patron", vec![first]).unwrap(),
            1,
            "{lane:?}"
        );
        let reloaded = src.replace(
            "pub fn is_patron(session: Session) -> Bool {",
            "pub fn is_patron(session: Session) -> Bool {\n    // still only reads is_patron",
        );
        let diff = machine.reload(&reloaded).unwrap();
        assert!(diff.changed.is_empty(), "{lane:?}: {diff:?}");

        let changed_profile = machine
            .call(
                "session",
                &[
                    NamedArg {
                        name: "is_patron".to_string(),
                        value: MachineArg::Bool(true),
                    },
                    NamedArg {
                        name: "profile_note".to_string(),
                        value: MachineArg::String("after reload".to_string()),
                    },
                ],
            )
            .unwrap()
            .0;
        machine.clear_trace();
        assert_eq!(
            machine
                .demand_i64("is_patron", vec![changed_profile])
                .unwrap(),
            1,
            "{lane:?}"
        );
        assert_eq!(spawned_count(&machine, "is_patron"), 0, "{lane:?}");
        assert_eq!(
            memo_projection_hit_count(&machine, "is_patron"),
            1,
            "{lane:?}"
        );
    }
}

#[test]
fn unused_let_call_never_spawns() {
    let src = r#"
fn expensive() -> Int { 41 }

pub fn main() -> Int {
    let x = expensive();
    7
}
"#;
    for lane in lanes() {
        let mut machine = load_with_lane(src, lane);
        assert_eq!(machine.demand_i64("main", vec![]).unwrap(), 7, "{lane:?}");
        assert_eq!(spawned_count(&machine, "expensive"), 0, "{lane:?}");
    }
}

#[test]
fn let_binding_sinks_into_only_using_match_arm() {
    let src = r#"
fn expensive() -> Int { 41 }

pub fn main(n: Int) -> Int {
    let x = expensive();
    match n {
        0 => 7,
        _ => x + 1,
    }
}
"#;
    for lane in lanes() {
        let mut machine = load_with_lane(src, lane);
        assert_eq!(machine.demand_i64("main", vec![0]).unwrap(), 7, "{lane:?}");
        assert_eq!(spawned_count(&machine, "expensive"), 0, "{lane:?}");
        machine.clear_trace();
        assert_eq!(machine.demand_i64("main", vec![1]).unwrap(), 42, "{lane:?}");
        assert_eq!(spawned_count(&machine, "expensive"), 1, "{lane:?}");
    }
}

#[test]
fn shared_let_binding_computes_once() {
    let src = r#"
fn f(x: Int) -> Int { x + 1 }

pub fn main() -> Int {
    let x = f(20);
    x + x
}
"#;
    for lane in lanes() {
        let mut machine = load_with_lane(src, lane);
        assert_eq!(machine.demand_i64("main", vec![]).unwrap(), 42, "{lane:?}");
        assert_eq!(spawned_count(&machine, "f"), 1, "{lane:?}");
        assert_eq!(memo_hit_count(&machine, "f"), 0, "{lane:?}");
    }
}

#[test]
fn memo_hits_across_distinct_calls_exact_counts() {
    let src = r#"
fn f(x: Int) -> Int { x + 1 }
fn a() -> Int { f(20) }
fn b() -> Int { f(20) }

pub fn main() -> Int {
    a() + b()
}
"#;
    for lane in lanes() {
        let mut machine = load_with_lane(src, lane);
        assert_eq!(machine.demand_i64("main", vec![]).unwrap(), 42, "{lane:?}");
        assert_eq!(spawned_count(&machine, "f"), 1, "{lane:?}");
        assert_eq!(memo_hit_count(&machine, "f"), 1, "{lane:?}");
    }
}

#[test]
fn tail_loop_emits_one_demand_per_entry_zero_per_iteration() {
    for lane in lanes() {
        let mut machine = load_with_lane(countdown_tail_source(), lane);
        assert_eq!(machine.demand_i64("countdown", vec![12, 0]).unwrap(), 12);
        assert_eq!(demanded_count(&machine, "countdown"), 1, "{lane:?}");
        assert_eq!(spawned_count(&machine, "countdown"), 1, "{lane:?}");
        assert_eq!(
            spawned_invocation_count(&machine, "countdown"),
            1,
            "{lane:?}"
        );
        assert_eq!(completed_count(&machine, "countdown"), 1, "{lane:?}");
    }
}

#[test]
fn tail_loop_matches_forced_invoke_result_and_readset() {
    let src = r#"
struct Session {
    wanted: String,
    ignored: String
}

pub fn session(wanted: String, ignored: String) -> Session {
    Session { wanted: wanted, ignored: ignored }
}

pub fn tail_session(session: Session, n: Int) -> String {
    match n {
        0 => session.wanted,
        _ => tail_session(session, n - 1),
    }
}
"#;
    for lane in lanes() {
        let mut enabled = load_with_lane(src, lane);
        let mut disabled = load_forced_tail_invoke(src, lane);

        let enabled_session = enabled
            .call(
                "session",
                &[
                    NamedArg {
                        name: "wanted".to_string(),
                        value: MachineArg::String("keep".to_string()),
                    },
                    NamedArg {
                        name: "ignored".to_string(),
                        value: MachineArg::String("first".to_string()),
                    },
                ],
            )
            .unwrap()
            .0;
        let disabled_session = disabled
            .call(
                "session",
                &[
                    NamedArg {
                        name: "wanted".to_string(),
                        value: MachineArg::String("keep".to_string()),
                    },
                    NamedArg {
                        name: "ignored".to_string(),
                        value: MachineArg::String("first".to_string()),
                    },
                ],
            )
            .unwrap()
            .0;

        enabled.clear_trace();
        disabled.clear_trace();
        let enabled_result = enabled
            .demand_i64("tail_session", vec![enabled_session, 8])
            .unwrap();
        let disabled_result = disabled
            .demand_i64("tail_session", vec![disabled_session, 8])
            .unwrap();
        assert_eq!(enabled_result, disabled_result, "{lane:?}");
        assert_eq!(
            enabled
                .export_value(StoreHandle(enabled_result))
                .unwrap()
                .values,
            disabled
                .export_value(StoreHandle(disabled_result))
                .unwrap()
                .values,
            "{lane:?}"
        );
        assert!(
            strict_subsequence(enabled.trace(), disabled.trace()),
            "{lane:?}: enabled trace must be a strict subsequence of forced-INVOKE trace"
        );

        let enabled_changed = enabled
            .call(
                "session",
                &[
                    NamedArg {
                        name: "wanted".to_string(),
                        value: MachineArg::String("keep".to_string()),
                    },
                    NamedArg {
                        name: "ignored".to_string(),
                        value: MachineArg::String("changed".to_string()),
                    },
                ],
            )
            .unwrap()
            .0;
        let disabled_changed = disabled
            .call(
                "session",
                &[
                    NamedArg {
                        name: "wanted".to_string(),
                        value: MachineArg::String("keep".to_string()),
                    },
                    NamedArg {
                        name: "ignored".to_string(),
                        value: MachineArg::String("changed".to_string()),
                    },
                ],
            )
            .unwrap()
            .0;

        enabled.clear_trace();
        disabled.clear_trace();
        assert_eq!(
            enabled
                .demand_i64("tail_session", vec![enabled_changed, 8])
                .unwrap(),
            enabled_result,
            "{lane:?}"
        );
        assert_eq!(
            disabled
                .demand_i64("tail_session", vec![disabled_changed, 8])
                .unwrap(),
            disabled_result,
            "{lane:?}"
        );
        let enabled_verified = projection_verified_count(&enabled, "tail_session");
        let disabled_verified = projection_verified_count(&disabled, "tail_session");
        assert_eq!(enabled_verified, disabled_verified, "{lane:?}");
        assert_eq!(enabled_verified, vec![1], "{lane:?}");
    }
}

#[test]
fn tail_loop_interp_jit_trace_equal() {
    let mut traces = Vec::new();
    for lane in lanes() {
        let mut machine = load_with_lane(countdown_tail_source(), lane);
        assert_eq!(machine.demand_i64("countdown", vec![9, 0]).unwrap(), 9);
        traces.push((lane, machine.trace().to_vec()));
    }
    assert_lane_traces_equal(&traces);
}

#[test]
fn tail_loop_accumulator_stays_molten() {
    let src = r#"
fn grow(n: Int, acc: Array) -> Array {
    match n {
        0 => acc,
        _ => grow(n - 1, acc.push(n)),
    }
}

pub fn main(n: Int) -> Array {
    grow(n, [0])
}
"#;
    for lane in lanes() {
        let mut machine = load_with_lane(src, lane);
        let result = machine.demand_i64("main", vec![24]).unwrap();
        let RenderedValue::Array { items, .. } = machine.render_result("main", result).unwrap()
        else {
            panic!("main did not render as an Array on {lane:?}");
        };
        assert_eq!(items.len(), 25, "{lane:?}");
        assert_eq!(
            store_alloc_count(&machine),
            1,
            "only the final accumulator should intern on {lane:?}"
        );
    }
}

#[test]
fn non_tail_self_call_stays_invoke() {
    let src = r#"
pub fn sum(n: Int) -> Int {
    match n {
        0 => 0,
        _ => n + sum(n - 1),
    }
}
"#;
    for lane in lanes() {
        let mut machine = load_with_lane(src, lane);
        assert_eq!(machine.demand_i64("sum", vec![6]).unwrap(), 21, "{lane:?}");
        assert_eq!(demanded_count(&machine, "sum"), 7, "{lane:?}");
        assert_eq!(spawned_count(&machine, "sum"), 7, "{lane:?}");
        assert_eq!(completed_count(&machine, "sum"), 7, "{lane:?}");
    }
}

#[test]
fn self_tail_call_inside_match_arm_does_not_spawn_per_iteration() {
    for lane in lanes() {
        let mut machine = load_with_lane(countdown_tail_source(), lane);
        assert_eq!(machine.demand_i64("countdown", vec![20, 0]).unwrap(), 20);
        assert_eq!(spawned_count(&machine, "countdown"), 1, "{lane:?}");
    }
}

#[test]
fn tail_loop_body_invokes_still_demand_children() {
    let src = r#"
fn child(n: Int) -> Int { n * 2 }

pub fn sum_child(n: Int, acc: Int) -> Int {
    match n {
        0 => acc,
        _ => sum_child(n - 1, acc + child(n)),
    }
}
"#;
    for lane in lanes() {
        let mut machine = load_with_lane(src, lane);
        assert_eq!(machine.demand_i64("sum_child", vec![6, 0]).unwrap(), 42);
        assert_eq!(spawned_count(&machine, "sum_child"), 1, "{lane:?}");
        assert_eq!(demanded_count(&machine, "child"), 6, "{lane:?}");
        assert_eq!(spawned_count(&machine, "child"), 6, "{lane:?}");
        assert_eq!(completed_count(&machine, "child"), 6, "{lane:?}");
    }
}

#[test]
fn warm_reload_cross_module_leaf_edit_misses_transitive_users_only() {
    let initial = modules(&[
        (
            "root",
            r#"
use a::bridge;

fn independent() -> Int {
    5
}

pub fn main() -> Int {
    bridge() + independent()
}
"#,
        ),
        (
            "a",
            r#"
fn leaf() -> Int {
    1
}

pub fn bridge() -> Int {
    leaf() + 10
}

fn unused() -> Int {
    100
}
"#,
        ),
    ]);
    let edited = modules(&[
        (
            "root",
            r#"
use a::bridge;

fn independent() -> Int {
    5
}

pub fn main() -> Int {
    bridge() + independent()
}
"#,
        ),
        (
            "a",
            r#"
fn leaf() -> Int {
    2
}

pub fn bridge() -> Int {
    leaf() + 10
}

fn unused() -> Int {
    100
}
"#,
        ),
    ]);
    for lane in lanes() {
        let mut machine = load_modules_with_lane("root", initial.clone(), lane);
        assert_eq!(machine.demand_i64("main", vec![]).unwrap(), 16, "{lane:?}");
        let diff = machine.reload_modules("root", edited.clone()).unwrap();
        assert_eq!(
            diff.changed,
            BTreeSet::from([
                "a::bridge".to_string(),
                "a::leaf".to_string(),
                "main".to_string(),
            ]),
            "{lane:?}"
        );
        assert_eq!(machine.demand_i64("main", vec![]).unwrap(), 17, "{lane:?}");
        assert_eq!(
            spawned_functions(&machine),
            BTreeSet::from([
                "a::bridge".to_string(),
                "a::leaf".to_string(),
                "main".to_string(),
            ]),
            "{lane:?}"
        );
        let hits = memo_hit_functions(&machine);
        assert!(hits.contains("independent"), "{lane:?}: {hits:?}");
        assert!(!hits.contains("a::unused"), "{lane:?}: {hits:?}");
    }
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
fn warm_reload_eval_identity_survives_trivia_and_semantic_edits() {
    let src = include_str!("../../playgrounds/snark/src/bundled/vix/samples/eval.vix");
    for lane in lanes() {
        let mut machine = load_with_lane(src, lane);
        assert_eq!(
            (machine.demand_i64("demo", vec![]).unwrap() as u64),
            42.0f64.to_bits(),
            "{lane:?}"
        );
        assert_eq!(memo_hit_functions(&machine), BTreeSet::new(), "{lane:?}");
        assert!(!spawned_functions(&machine).is_empty(), "{lane:?}");

        let demo_hash = machine.fn_hash("demo").expect("demo hash");
        machine.clear_trace();
        assert_eq!(
            (machine.demand_i64("demo", vec![]).unwrap() as u64),
            42.0f64.to_bits(),
            "{lane:?}"
        );
        assert_eq!(
            machine.trace(),
            &[
                DriveEvent::Demanded { fn_hash: demo_hash },
                DriveEvent::MemoHit { fn_hash: demo_hash },
            ],
            "{lane:?}"
        );

        let reformatted = src
            .replace("fn demo() -> Float {", "fn demo() -> Float {\n    // hi!\n")
            .replace("use vix::Map;", "// preamble\n\nuse vix::Map;");
        let reformatted = load_with_lane(&reformatted, lane);
        assert_eq!(
            machine.fn_hash("demo"),
            reformatted.fn_hash("demo"),
            "{lane:?}"
        );
        assert_eq!(
            machine.fn_hash("eval"),
            reformatted.fn_hash("eval"),
            "{lane:?}"
        );

        let changed = src.replace("Expr::Num(6.0)", "Expr::Num(5.0)");
        let changed = load_with_lane(&changed, lane);
        assert_ne!(machine.fn_hash("demo"), changed.fn_hash("demo"), "{lane:?}");
        assert_eq!(machine.fn_hash("eval"), changed.fn_hash("eval"), "{lane:?}");
    }
}

#[test]
fn warm_reload_trivia_costs_only_root_hit() {
    for lane in lanes() {
        let mut machine = load_with_lane(anti_nix_diamond(), lane);
        assert_eq!(machine.demand_i64("main", vec![]).unwrap(), 37, "{lane:?}");
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
        let diff = machine.reload(&reformatted).unwrap();
        assert!(diff.changed.is_empty(), "{lane:?}: {diff:?}");
        assert_eq!(machine.demand_i64("main", vec![]).unwrap(), 37, "{lane:?}");
        assert_eq!(spawned_functions(&machine), BTreeSet::new(), "{lane:?}");
        assert_eq!(
            memo_hit_functions(&machine),
            BTreeSet::from(["main".to_string()]),
            "{lane:?}"
        );
    }
}

#[test]
fn warm_reload_leaf_edit_misses_exact_blast_radius() {
    for lane in lanes() {
        let mut machine = load_with_lane(anti_nix_diamond(), lane);
        assert_eq!(machine.demand_i64("main", vec![]).unwrap(), 37, "{lane:?}");
        let edited =
            anti_nix_diamond().replace("fn leaf() -> Int {\n    1", "fn leaf() -> Int {\n    2");
        let diff = machine.reload(&edited).unwrap();
        assert_eq!(
            diff.changed,
            BTreeSet::from([
                "leaf".to_string(),
                "left".to_string(),
                "main".to_string(),
                "right".to_string(),
            ]),
            "{lane:?}"
        );
        assert_eq!(machine.demand_i64("main", vec![]).unwrap(), 39, "{lane:?}");
        assert_eq!(
            spawned_functions(&machine),
            BTreeSet::from([
                "leaf".to_string(),
                "left".to_string(),
                "main".to_string(),
                "right".to_string(),
            ]),
            "{lane:?}"
        );
        let hits = memo_hit_functions(&machine);
        assert!(hits.contains("independent"), "{lane:?}: {hits:?}");
        assert!(!hits.contains("never_demanded"), "{lane:?}: {hits:?}");
    }
}

#[test]
fn warm_reload_unused_edit_costs_zero_misses_and_hashes_only_itself() {
    for lane in lanes() {
        let mut machine = load_with_lane(anti_nix_diamond(), lane);
        assert_eq!(machine.demand_i64("main", vec![]).unwrap(), 37, "{lane:?}");
        let before = machine.fn_hashes();
        let edited = anti_nix_diamond().replace(
            "fn never_demanded() -> Int {\n    100",
            "fn never_demanded() -> Int {\n    101",
        );
        let diff = machine.reload(&edited).unwrap();
        assert_eq!(
            diff.changed,
            BTreeSet::from(["never_demanded".to_string()]),
            "{lane:?}"
        );
        for name in ["leaf", "left", "right", "independent", "main"] {
            assert_eq!(
                before.get(name),
                machine.fn_hashes().get(name),
                "{name} should not inherit an unreferenced function edit on {lane:?}"
            );
        }
        assert_ne!(
            before.get("never_demanded"),
            machine.fn_hashes().get("never_demanded"),
            "{lane:?}"
        );
        assert_eq!(machine.demand_i64("main", vec![]).unwrap(), 37, "{lane:?}");
        assert_eq!(spawned_functions(&machine), BTreeSet::new(), "{lane:?}");
        assert_eq!(
            memo_hit_functions(&machine),
            BTreeSet::from(["main".to_string()]),
            "{lane:?}"
        );
    }
}

#[test]
fn warm_reload_type_decl_edit_misses_transitive_users() {
    for lane in lanes() {
        let mut machine = load_with_lane(type_closure_source(), lane);
        assert_eq!(machine.demand_i64("main", vec![]).unwrap(), 8, "{lane:?}");
        let edited = type_closure_source().replace("enum Choice { A, B }", "enum Choice { B, A }");
        let diff = machine.reload(&edited).unwrap();
        assert_eq!(
            diff.changed,
            BTreeSet::from([
                "bridge".to_string(),
                "main".to_string(),
                "typed".to_string()
            ]),
            "{lane:?}"
        );
        assert_eq!(machine.demand_i64("main", vec![]).unwrap(), 8, "{lane:?}");
        assert_eq!(
            spawned_functions(&machine),
            BTreeSet::from([
                "bridge".to_string(),
                "main".to_string(),
                "typed".to_string()
            ]),
            "{lane:?}"
        );
        let hits = memo_hit_functions(&machine);
        assert!(hits.contains("independent"), "{lane:?}: {hits:?}");
    }
}

#[test]
fn pending_entries_resolve_through_reloaded_hash_tables() {
    let src = r#"
fn producer() -> Float { 1.0 }

pub fn make() -> Map<String, Float> {
    let m: Map<String, Float> = {};
    m.insert("x", producer())
}

pub fn touch(m: Map<String, Float>, nonce: Int) -> Float {
    m.get("x").unwrap()
}
"#;
    for lane in lanes() {
        let mut machine = load_with_lane(src, lane);
        let handle = machine.demand_i64("make", vec![]).unwrap();
        assert_eq!(
            (machine.demand_i64("touch", vec![handle, 0]).unwrap() as u64),
            1.0f64.to_bits(),
            "{lane:?}"
        );

        let trivia = src.replace(
            "fn producer() -> Float {",
            "fn producer() -> Float {\n    // hi\n",
        );
        let diff = machine.reload(&trivia).unwrap();
        assert!(diff.changed.is_empty(), "{lane:?}: {diff:?}");
        assert_eq!(
            (machine.demand_i64("touch", vec![handle, 1]).unwrap() as u64),
            1.0f64.to_bits(),
            "{lane:?}"
        );
        assert_eq!(spawned_count(&machine, "producer"), 0, "{lane:?}");
        assert_eq!(memo_hit_count(&machine, "producer"), 1, "{lane:?}");

        let semantic = src.replace(
            "fn producer() -> Float { 1.0 }",
            "fn producer() -> Float { 2.0 }",
        );
        let diff = machine.reload(&semantic).unwrap();
        assert_eq!(
            diff.changed,
            BTreeSet::from(["make".to_string(), "producer".to_string()]),
            "{lane:?}"
        );
        let handle = machine.demand_i64("make", vec![]).unwrap();
        machine.clear_trace();
        assert_eq!(
            (machine.demand_i64("touch", vec![handle, 2]).unwrap() as u64),
            2.0f64.to_bits(),
            "{lane:?}"
        );
        assert_eq!(spawned_count(&machine, "producer"), 1, "{lane:?}");
        assert_eq!(memo_hit_count(&machine, "producer"), 0, "{lane:?}");
    }
}

#[test]
fn semantic_cutoff_soundness_differential_matches_without_comparator() {
    let enabled_src = semantic_cutoff_demo_source("derived__memo_verify_req");
    let disabled_src = semantic_cutoff_demo_source("derived__not_a_memo_verify_req");
    for lane in lanes() {
        let mut enabled = load_with_lane(&enabled_src, lane);
        let mut disabled = load_with_lane(&disabled_src, lane);
        let mut enabled_values = Vec::new();
        let mut disabled_values = Vec::new();
        let mut enabled_observable = Vec::new();
        let mut disabled_observable = Vec::new();
        for req in ["^1.0.0", "^1.2.0", "^1.0.0"] {
            enabled.clear_trace();
            disabled.clear_trace();
            enabled_values.push(call_derived(&mut enabled, req));
            disabled_values.push(call_derived(&mut disabled, req));
            enabled_observable.push(semantic_observable_trace(&enabled));
            disabled_observable.push(semantic_observable_trace(&disabled));
        }
        assert_eq!(enabled_values, disabled_values, "{lane:?}");
        assert_eq!(enabled_observable, disabled_observable, "{lane:?}");
        assert_eq!(
            memo_semantic_hit_count(&enabled, "derived"),
            0,
            "last widened run recomputes on {lane:?}"
        );
    }
}
