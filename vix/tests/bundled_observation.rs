//! Production certificates for bounded observation of bundled invocations.
//!
//! The cost model may fuse a mapped element or bundle a single-consumer pure call
//! into a direct `WeavyOp::Call`. A described-wire observer retains one
//! realized-demand entry per distinct executed invocation preimage (callee plus
//! framed argument identities) without adding a scheduler edge, so
//! `demanded_once` distinguishes exact preimages while equal preimages share.

use vix::ratchet::run_source;

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
