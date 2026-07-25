//! Production certificates for bounded observation of bundled invocations.
//!
//! The cost model may fuse a mapped element or bundle a single-consumer pure call
//! into a direct `WeavyOp::Call`. A described-wire observer retains one
//! realized-demand entry per distinct executed invocation preimage (callee plus
//! framed argument identities) without adding a scheduler edge, so
//! `demanded_once` distinguishes exact preimages while equal preimages share.

use vixen_runtime::ratchet::run_source;
use vix::runtime::EventKind;

const COSTLY: &str = "fn costly(n: Int) -> Int { n * 1000 }\n";

/// Distinct arguments are distinct preimages: `demanded_once(costly(1))` and
/// `demanded_once(costly(2))` each match exactly their own bundled invocation and
/// never cross-match, even though both share the callee `costly`.
#[test]
fn distinct_arguments_are_distinct_preimages() {
    let source = format!(
        "{COSTLY}#[test]\nfn t() -> Stream<Check> {{\n    yield expect_eq(costly(1) + costly(2), 3000);\n    yield demanded_once(costly(1));\n    yield demanded_once(costly(2));\n}}\n"
    );
    let report = run_source(&source).expect("runs");
    assert!(report.passed() && report.agrees(), "{report:?}");
}

/// A call-site selector does not match a different argument: `demanded_once`
/// of an argument that never executed observes zero.
#[test]
fn call_site_selector_rejects_a_different_argument() {
    let source = format!(
        "{COSTLY}#[test]\nfn t() -> Stream<Check> {{\n    yield expect_eq(costly(1), 1000);\n    yield never_demanded(costly(2));\n}}\n"
    );
    let report = run_source(&source).expect("runs");
    assert!(report.passed() && report.agrees(), "{report:?}");
}

/// Equal preimages share one realized-demand entry: the same bundled `costly(7)`
/// invocation reached from two check islands is observed once, so
/// `demanded_once(costly(7))` sees exactly one distinct realization.
#[test]
fn equal_preimages_share_one_observation() {
    let source = format!(
        "{COSTLY}#[test]\nfn t() -> Stream<Check> {{\n    yield expect_eq(costly(7), 7000);\n    yield expect_eq(costly(7) + 1, 7001);\n    yield demanded_once(costly(7));\n}}\n"
    );
    let report = run_source(&source).expect("runs");
    assert!(report.passed() && report.agrees(), "{report:?}");
}

/// Two identity-equal exec value roots enter the production frontier together.
/// The second root joins the first demand: one process begins, one
/// realized-demand entry is published, and both root publications receive the
/// same value identity.
#[test]
fn concurrent_roots_join_one_suspended_demand_and_record_one_realization() {
    let source = r#"
#[test]
fn t(echo: Echo) -> Stream<Check> {
    let a = exec echo`"shared"`;
    let b = exec echo`"shared"`;
    yield expect_eq(a.stdout, b.stdout);
    yield ran_processes(1);
}
"#;
    let report = run_source(source).expect("shared exec demand runs");
    assert!(report.passed() && report.agrees());
    for lane in [&report.plain, &report.chaos] {
        assert_eq!(lane.counters.demand_joins, 1);
        assert_eq!(lane.counters.effect_spawns, 1);
        let exec_key = lane
            .events
            .iter()
            .find_map(|event| match &event.kind {
                EventKind::EffectSpawned { key, .. } => Some(*key),
                _ => None,
            })
            .expect("the one exec owner has a demand key");
        assert_eq!(
            lane.events
                .iter()
                .filter(|event| {
                    matches!(
                        event.kind,
                        EventKind::Completed { key, .. } if key == exec_key
                    )
                })
                .count(),
            1,
        );
        assert_eq!(lane.values.len(), 2);
        assert_eq!(lane.values[0].identity, lane.values[1].identity);
        assert_eq!(lane.checks.len(), 2);
        assert!(lane.checks.iter().all(|check| check.passed));
    }
}

/// A binding selector names a let-bound invocation with composite arguments —
/// a preimage no literal spelling can select. Two uses of the binding are one
/// computation; `demanded_once` observes exactly one realization.
#[test]
fn binding_selector_matches_a_composite_preimage() {
    let source = "\
fn build(values: [Int]) -> Int { values.len() * 1000 }
#[test]
fn t() -> Stream<Check> {
    let rows = [1, 2, 3];
    let built = build(rows);
    let a = built;
    let b = built;
    yield expect_eq(a, b);
    yield demanded_once(built);
}
";
    let report = run_source(source).expect("runs");
    assert!(report.passed() && report.agrees(), "{report:?}");
}

/// A binding selector also matches a scalar-literal invocation hoisted as a
/// shared wire island: the memo-path realization and the binding preimage agree
/// on one entry, whether the invocation was bundled or hoisted.
#[test]
fn binding_selector_matches_a_hoisted_scalar_wire() {
    let source = format!(
        "{COSTLY}#[test]\nfn t() -> Stream<Check> {{\n    let a = costly(7);\n    yield expect_eq(a, 7000);\n    yield expect_eq(a + 1, 7001);\n    yield demanded_once(a);\n}}\n"
    );
    let report = run_source(&source).expect("runs");
    assert!(report.passed() && report.agrees(), "{report:?}");
}

/// A held binding is a wire, not a demand: a let-bound invocation no value
/// check ever consumes is never executed, so `never_demanded` passes on it.
#[test]
fn held_binding_is_never_demanded() {
    let source = "\
fn build(values: [Int]) -> Int { values.len() * 1000 }
#[test]
fn t() -> Stream<Check> {
    let held = build([1, 2, 3]);
    yield expect_eq(1, 1);
    yield never_demanded(held);
}
";
    let report = run_source(source).expect("runs");
    assert!(report.passed() && report.agrees(), "{report:?}");
}

/// The composite observation is bounded exactly like the literal one: removing
/// the binding-selector observer leaves scheduler requests and store interns
/// identical, so observing a composite invocation never turns it into a task.
#[test]
fn composite_observation_adds_no_scheduler_edge() {
    let observed = "\
fn build(values: [Int]) -> Int { values.len() * 1000 }
#[test]
fn t() -> Stream<Check> {
    let built = build([1, 2, 3]);
    yield expect_eq(built, 3000);
    yield demanded_once(built);
}
";
    let control = "\
fn build(values: [Int]) -> Int { values.len() * 1000 }
#[test]
fn t() -> Stream<Check> {
    let built = build([1, 2, 3]);
    yield expect_eq(built, 3000);
}
";
    let observed = run_source(observed).expect("runs");
    let control = run_source(control).expect("runs");
    assert!(observed.passed() && observed.agrees(), "{observed:?}");
    assert_eq!(
        observed.plain.counters.scheduler_requests, control.plain.counters.scheduler_requests,
        "the composite observation issues no scheduler request",
    );
    assert_eq!(
        observed.plain.counters.store_interns, control.plain.counters.store_interns,
        "the composite observation interns nothing",
    );
}

/// The observation is bounded and adds no scheduler edge: removing the described
/// wires leaves the scheduler-request count and the store-intern count identical,
/// so a selected observer never turns a bundled call into a task.
#[test]
fn observation_adds_no_scheduler_edge() {
    let with = format!(
        "{COSTLY}#[test]\nfn t() -> Stream<Check> {{\n    yield expect_eq(costly(1) + costly(2), 3000);\n    yield demanded_once(costly(1));\n    yield demanded_once(costly(2));\n}}\n"
    );
    let without = format!(
        "{COSTLY}#[test]\nfn t() -> Stream<Check> {{\n    yield expect_eq(costly(1) + costly(2), 3000);\n}}\n"
    );
    let with = run_source(&with).expect("runs");
    let without = run_source(&without).expect("runs");
    assert_eq!(
        with.plain.counters.scheduler_requests, without.plain.counters.scheduler_requests,
        "the observation issues no scheduler request",
    );
    assert_eq!(
        with.plain.counters.store_interns, without.plain.counters.store_interns,
        "the observation interns nothing",
    );
}
