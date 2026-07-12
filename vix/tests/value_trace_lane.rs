//! Production certificates for described value-level demand checks.
//!
//! A described-wire trace check (`demanded`, `never_demanded`, `demanded_once`)
//! is an OBSERVER: it holds its operand as an unevaluated wire and reads the
//! frozen completed-run demand log. It must never change what the program
//! executes. The executable demand graph for a test must be identical with the
//! trace checks removed, modulo trace reporting.
//!
//! Rungs 055-056 are certified here: control flow and dead-let elimination make
//! the untaken/unused invocation genuinely not execute (frame evidence: zero
//! frame entries). The remaining rungs 053/054/057/058/059 require the general
//! lazy-demand substrate (real wire arguments, independently demandable
//! aggregate fields, keyed `Array.map` element demands, and shared memoized
//! invocation identity) that is descriptor-independent; until inline frame
//! events and the scheduler demand log agree on what executed, they are held
//! red here rather than certified green by an observer that fabricates the
//! behaviour it reports.

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
        .map(|check| check.identity)
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

/// Rungs 053/054/057/058/059 are held red pending the descriptor-independent
/// lazy-demand substrate. `demanded`/`demanded_once` observe zero because the
/// consumed invocation is not yet a shared memoized demand (053/057/058/059);
/// and 054's `never_demanded(expensive())` currently reports success only
/// because `expensive` executes inline yet is absent from the demand log —
/// exactly the invalid observer/execution disagreement the substrate must
/// close. Frame evidence: `expensive` runs.
#[test]
fn described_wire_rungs_await_the_lazy_demand_substrate() {
    for source in [RUNG_053, RUNG_057, RUNG_058, RUNG_059] {
        assert!(
            !run_source(source)
                .expect("rung runs its value checks")
                .passed(),
            "positive-demand rung stays red until its invocation is a real shared demand",
        );
    }

    // 054's trace check must not be trusted while the field initializer still
    // executes inline: the demand log and the frame trace disagree.
    assert_eq!(
        frame_entries(RUNG_054)
            .get("expensive")
            .copied()
            .unwrap_or(0),
        1,
        "rung 054 still executes the undemanded field initializer inline",
    );
}

/// Higher-order remains independently red at rung 052; the described-wire trace
/// checks do not bypass it.
#[test]
fn rung_052_remains_a_separate_pretrace_boundary() {
    assert!(
        run_source(RUNG_052).is_err(),
        "higher-order remains independently red"
    );
}
