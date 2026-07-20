//! Production certificates for described value-level demand checks.
//!
//! A described-wire trace check (`demanded`, `never_demanded`, `demanded_once`)
//! is an OBSERVER: it holds its operand as an unevaluated wire and reads the
//! frozen completed-run demand log. It must never change what the program
//! executes. The executable demand graph for a test must be identical with the
//! trace checks removed, modulo trace reporting.
//!
//! Rungs 053-059 are certified green here through the lazy-demand substrate:
//! call-site-specialized lazy where-parameters (053), independently-demandable
//! aggregate fields (054), control-flow non-demand with frame evidence (055-056),
//! and — for invocations the cost model fuses or bundles into a direct
//! `WeavyOp::Call` — bounded Production observation of the exact executed
//! preimage (057/058/059). Every observer stays pure: removing the trace checks
//! leaves value results and the executed frame trace byte-identical.

use std::collections::BTreeMap;

use vix::compiler::Compiler;
use vix::ratchet::{RatchetReport, run_source};
use vix::runtime::EventKind;

const RUNG_052: &str = include_str!("ratchet/052-higher-order.vix");
const RUNG_053: &str = include_str!("ratchet/053-args-are-wires.vix");
const RUNG_054: &str = include_str!("ratchet/054-partial-dependency.vix");
const RUNG_055: &str = include_str!("ratchet/055-match-defers.vix");
const RUNG_056: &str = include_str!("ratchet/056-undemanded-is-free.vix");
const RUNG_057: &str = include_str!("ratchet/057-element-independence.vix");
const RUNG_058: &str = include_str!("ratchet/058-memo-within-run.vix");
const RUNG_059: &str = include_str!("ratchet/059-distinct-args-distinct-demands.vix");

/// Frame-entry counts by callee name — direct evidence of what actually
/// executed, independent of any demand log.
fn frame_entries(source: &str) -> BTreeMap<String, usize> {
    let module = Compiler::default()
        .compile(source)
        .expect("rung compiles through the canonical surface");
    let names: BTreeMap<u32, String> = module
        .module
        .functions
        .iter()
        .map(|function| (function.id.0, function.name.clone()))
        .collect();
    let report = run_source(source).expect("rung runs through the production Executable");
    let mut counts = BTreeMap::new();
    for event in &report.plain.events {
        if let EventKind::WeavyFrameEntered { function, .. } = &event.kind {
            *counts
                .entry(names.get(&function.0).cloned().unwrap_or_default())
                .or_insert(0) += 1;
        }
    }
    counts
}

/// Strip the described-wire trace-check yields from a rung, leaving its value
/// checks. The demand graph must be identical with them removed.
fn without_trace_checks(source: &str) -> String {
    source
        .lines()
        .filter(|line| {
            let trimmed = line.trim_start();
            !(trimmed.starts_with("yield demanded(")
                || trimmed.starts_with("yield never_demanded(")
                || trimmed.starts_with("yield demanded_once("))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn value_check_identities(report: &RatchetReport) -> Vec<Option<vix::runtime::ValueId>> {
    report
        .plain
        .checks
        .iter()
        .filter(|check| check.identity.is_some())
        .map(|check| check.identity.clone())
        .collect()
}

fn call_events(report: &RatchetReport) -> Vec<EventKind> {
    report
        .plain
        .events
        .iter()
        .filter(|event| {
            matches!(
                event.kind,
                EventKind::WeavyFrameEntered { .. } | EventKind::WeavyFrameExited { .. }
            )
        })
        .map(|event| event.kind.clone())
        .collect()
}

/// Rungs 055-056 prove unused arm/let values genuinely do not execute — the
/// untaken match arm and the unused `let` binding enter no frame. This is real
/// non-demand, not an observer's omission from a log.
#[test]
fn rungs_055_056_prove_control_flow_non_demand_with_frame_evidence() {
    let report = run_source(RUNG_055).expect("rung 055 runs");
    assert!(report.passed() && report.agrees());
    assert_eq!(
        frame_entries(RUNG_055)
            .get("full_audit")
            .copied()
            .unwrap_or(0),
        0,
        "the untaken match arm never executes",
    );

    let report = run_source(RUNG_056).expect("rung 056 runs");
    assert!(report.passed() && report.agrees());
    assert_eq!(
        frame_entries(RUNG_056)
            .get("landmine")
            .copied()
            .unwrap_or(0),
        0,
        "the unused let is never executed and never faults",
    );
}

/// Metamorphic control: a described-wire trace check is a pure observer.
/// Removing every trace check from a rung must leave the value-check results
/// and the frame/call event trace byte-identical — the observer never creates
/// the behaviour it reports.
#[test]
fn described_wire_checks_do_not_change_execution() {
    for source in [RUNG_055, RUNG_056] {
        let with = run_source(source).expect("rung with trace checks runs");
        let without =
            run_source(&without_trace_checks(source)).expect("rung without trace checks runs");
        assert_eq!(
            value_check_identities(&with),
            value_check_identities(&without),
            "value-check results must not depend on trace checks",
        );
        assert_eq!(
            call_events(&with),
            call_events(&without),
            "the executed call/frame trace must not depend on trace checks",
        );
    }
}

/// Rung 058 is certified green through the lazy-demand substrate: a shared
/// scalar user invocation (`costly(7)` bound twice) is partitioned into one
/// argument island, demanded through `AwaitWire`/force-on-park, and memoized to a
/// single realization. `demanded_once(costly())` therefore observes exactly one
/// realized demand — a real shared memoized demand, not an observer's omission.
/// The observer never fabricates it: removing the trace checks leaves the value
/// results and the executed frame trace byte-identical.
#[test]
fn rung_058_shares_one_memoized_demand_through_the_substrate() {
    let report = run_source(RUNG_058).expect("rung 058 runs");
    assert!(
        report.passed() && report.agrees(),
        "shared scalar invocation memoizes to one demand: {report:?}",
    );

    let with = run_source(RUNG_058).expect("rung 058 with trace checks runs");
    let without = run_source(&without_trace_checks(RUNG_058)).expect("rung 058 without runs");
    assert_eq!(
        value_check_identities(&with),
        value_check_identities(&without),
        "value results must not depend on the described-wire checks",
    );
    assert_eq!(
        call_events(&with),
        call_events(&without),
        "the executed call/frame trace must not depend on the described-wire checks",
    );
}

/// Rung 053 is certified green through call-site-specialized lazy parameters:
/// `pick(true) where { a: cheap(), b: expensive() }` keeps `flag` as a bundled
/// `ArgCopy` frame argument, partitions `a`/`b` into argument islands, and sinks
/// each into the callee's taken control region as an `AwaitWire`. Only the taken
/// arm parks, so `cheap()` is demanded and `expensive()` never runs. The observer
/// never fabricates it: removing the trace checks leaves the value results and the
/// executed call/frame trace byte-identical.
#[test]
fn rung_053_lazy_parameters_demand_only_the_taken_argument() {
    let report = run_source(RUNG_053).expect("rung 053 runs");
    assert!(
        report.passed() && report.agrees(),
        "lazy where-parameters demand only the taken argument: {report:?}",
    );
    assert_eq!(
        frame_entries(RUNG_053)
            .get("expensive")
            .copied()
            .unwrap_or(0),
        0,
        "the untaken lazy argument never executes",
    );
    assert_eq!(
        frame_entries(RUNG_053).get("cheap").copied().unwrap_or(0),
        1,
        "the taken lazy argument is demanded exactly once",
    );

    let with = run_source(RUNG_053).expect("rung 053 with trace checks runs");
    let without = run_source(&without_trace_checks(RUNG_053)).expect("rung 053 without runs");
    assert_eq!(
        value_check_identities(&with),
        value_check_identities(&without),
        "value results must not depend on the described-wire checks",
    );
    assert_eq!(
        call_events(&with),
        call_events(&without),
        "the executed call/frame trace must not depend on the described-wire checks",
    );
}

/// Rung 054 is certified green through independently-demandable aggregate fields:
/// `Point { x: cheap(), y: expensive() }` is never materialized; `p.x` fuses to an
/// `AwaitWire` of `x`'s initializer, so `cheap()` runs and `expensive()` never
/// does. The frame trace and the demand log now agree, and the observer never
/// fabricates it: removing the trace checks leaves value results and the executed
/// frame trace byte-identical.
#[test]
fn rung_054_projects_one_field_without_computing_the_other() {
    let report = run_source(RUNG_054).expect("rung 054 runs");
    assert!(report.passed() && report.agrees(), "{report:?}");
    assert_eq!(
        frame_entries(RUNG_054)
            .get("expensive")
            .copied()
            .unwrap_or(0),
        0,
        "the undemanded field initializer never executes",
    );
    assert_eq!(
        frame_entries(RUNG_054).get("cheap").copied().unwrap_or(0),
        1,
        "the projected field initializer is demanded exactly once",
    );

    let with = run_source(RUNG_054).expect("rung 054 with trace checks runs");
    let without = run_source(&without_trace_checks(RUNG_054)).expect("rung 054 without runs");
    assert_eq!(
        value_check_identities(&with),
        value_check_identities(&without)
    );
    assert_eq!(call_events(&with), call_events(&without));
}

/// Rungs 057 and 059 are certified green through bounded Production observation:
/// the cost model fuses `ys[1]` and bundles single-consumer `costly(1)`/`costly(2)`
/// into direct `WeavyOp::Call`s, and a described-wire observer retains one
/// realized-demand entry per distinct executed invocation preimage (callee plus
/// framed argument identities) without adding a scheduler edge. `demanded_once`
/// distinguishes the exact preimages. The observation is a pure post-run read:
/// removing the trace checks leaves value results and the executed frame trace
/// byte-identical — only the selected observation entries differ.
#[test]
fn rungs_057_059_observe_exact_bundled_preimages() {
    for source in [RUNG_057, RUNG_059] {
        let report = run_source(source).expect("rung runs");
        assert!(
            report.passed() && report.agrees(),
            "bundled invocation preimage is observed exactly: {report:?}",
        );
        let with = run_source(source).expect("rung with trace checks runs");
        let without = run_source(&without_trace_checks(source)).expect("rung without runs");
        assert_eq!(
            value_check_identities(&with),
            value_check_identities(&without),
            "value results must not depend on the described-wire observation",
        );
        assert_eq!(
            call_events(&with),
            call_events(&without),
            "the executed call/frame trace must not depend on the observation",
        );
    }
}

/// Higher-order execution stays green when the described-wire substrate is
/// integrated. Rung 052 has no trace assertions of its own; this certificate
/// guards the shared callable/runtime machinery used immediately below it.
#[test]
fn rung_052_remains_green_across_pretrace_integration() {
    let report = run_source(RUNG_052).expect("rung 052 runs through Executable");
    assert!(report.passed());
    assert!(report.agrees());
}
