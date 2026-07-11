//! Rung 050 harness substrate — deliberately red checkpoint.
//!
//! This file is committed BEFORE the implementation of first-class
//! `TraceCheck` construction/evaluation and enforceable `#[test]` budgets. It
//! proves the two capabilities are absent today and pins the exact target
//! surface the substrate must reach:
//!
//!   1. Trace checks fail at the surface. `scheduler_requests_at_most`,
//!      `memo_entries_at_most`, and `store_interns_at_most` are unknown check
//!      constructors, so a test that yields one does not compile.
//!   2. Budget syntax is absent. `#[test { budget_wall: 5s, budget_rss: 256MB }]`
//!      carries unit-bearing literals the surface cannot lex, so the canonical
//!      rung 050 file does not compile.
//!
//! The rung 050 fixture itself only needs this substrate to *compile*; running
//! its 10M-iteration fused tail loop is a later checkpoint (tail-call fusion is
//! out of scope here). Trace-check *evaluation* is therefore proven on small
//! substrate fixtures whose value checks complete in-process, and budget
//! *enforcement* is proven by the outer watchdog runner in later milestones.
//!
//! Every assertion below targets the post-implementation behaviour, so each
//! test is red until the substrate lands. It references only already-public
//! API so the test binary still builds (the redness is runtime, not a build
//! break).

use vix::compiler::Compiler;
use vix::ratchet::run_source;

const RUNG_050: &str = include_str!("ratchet/050-deep-tail-recursion.vix");

/// The canonical rung 050 file must *compile*: its `#[test { budget_wall: 5s,
/// budget_rss: 256MB }]` attribute parses into typed budget metadata and its
/// `scheduler_requests_at_most`/`memo_entries_at_most` yields are recognised
/// trace-check constructors. (Running the fused 10M-iteration loop is a later
/// rung; compilation does not execute it.)
#[test]
fn rung_050_source_compiles_with_budget_and_trace_checks() {
    let module = Compiler::new()
        .compile(RUNG_050)
        .expect("rung 050 compiles once budgets and trace checks exist");
    assert_eq!(module.tests.len(), 1);
    assert_eq!(module.tests[0].name, "deep_tail_recursion");
}

/// A value check followed by post-run trace checks: the value check is an
/// ordinary demanded island, and each trace check is evaluated after it
/// completes against the frozen completed-run counter snapshot. All three
/// pass, and the trace checks' own evaluation is excluded from the counters
/// they inspect.
#[test]
fn trace_checks_evaluate_against_the_frozen_counter_snapshot() {
    const SOURCE: &str = r#"
#[test]
fn substrate() -> Stream<Check> {
    yield expect_eq(1 + 2, 3);
    yield scheduler_requests_at_most(10);
    yield memo_entries_at_most(10);
    yield store_interns_at_most(10);
}
"#;
    let report = run_source(SOURCE).expect("trace-check substrate runs in-process");
    assert!(report.agrees(), "plain and chaos agree: {report:?}");
    assert!(report.passed(), "value check and trace checks all pass: {report:?}");
    assert_eq!(
        report.plain.checks.len(),
        4,
        "one value check plus three trace checks are all reported",
    );
    assert!(report.plain.checks.iter().all(|check| check.passed));
}

/// A trace budget that the run exceeds is a red check, reported as a failure
/// against the frozen snapshot — never a crash and never a silent pass.
#[test]
fn an_exceeded_trace_budget_is_a_red_check() {
    const SOURCE: &str = r#"
#[test]
fn over_budget() -> Stream<Check> {
    yield expect_eq(1 + 2, 3);
    yield store_interns_at_most(0);
}
"#;
    let report = run_source(SOURCE).expect("over-budget trace substrate compiles and runs");
    assert!(report.agrees(), "plain and chaos agree on the check family: {report:?}");
    assert!(
        !report.passed(),
        "a store-intern budget of 0 is exceeded by the value check's interned constant",
    );
    let trace = report
        .plain
        .checks
        .last()
        .expect("the trace check is reported");
    assert!(!trace.passed, "the exceeded trace budget is red: {trace:?}");
}
