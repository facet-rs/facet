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

use std::path::Path;

use vix::budget::{BudgetOutcome, Workload, run_under_budget};
use vix::compiler::Compiler;
use vix::ratchet::run_source;
use vix::vir::Budget;

const RUNG_050: &str = include_str!("ratchet/050-deep-tail-recursion.vix");

/// The compiled outer budget-enforcing child process.
const CHILD_EXE: &str = env!("CARGO_BIN_EXE_vix-budget-child");

/// Compile a single-test source and return its typed `#[test]` budget.
fn budget_of(source: &str) -> Budget {
    let module = Compiler::new().compile(source).expect("source compiles");
    assert_eq!(module.tests.len(), 1, "fixture declares exactly one test");
    module.tests[0].metadata.budget
}

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

    // The budget attribute parses into typed metadata: 5s wall, 256 MiB RSS.
    let budget = module.tests[0].metadata.budget;
    assert_eq!(budget.wall_ns, Some(5 * 1_000_000_000));
    assert_eq!(budget.rss_bytes, Some(256 * 1024 * 1024));
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
    assert!(
        report.passed(),
        "value check and trace checks all pass: {report:?}"
    );
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
    assert!(
        report.agrees(),
        "plain and chaos agree on the check family: {report:?}"
    );
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

/// The trace verdict is decided by the post-run counter snapshot: a bound at the
/// observed intern count passes, one below it goes red. This pins the trace
/// check to the actual completed-run counters, not to a fabricated number.
#[test]
fn trace_verdict_tracks_the_post_run_counter_snapshot() {
    const VALUE_ONLY: &str = r#"
#[test]
fn value_only() -> Stream<Check> {
    yield expect_eq("a" + "b", "ab");
}
"#;
    let baseline = run_source(VALUE_ONLY).expect("value-only baseline runs");
    let interns = baseline.plain.counters.store_interns;
    assert!(interns >= 1, "the interned string constant is counted");

    let at_bound = format!(
        "#[test]\nfn at_bound() -> Stream<Check> {{\n    yield expect_eq(\"a\" + \"b\", \"ab\");\n    yield store_interns_at_most({interns});\n}}\n"
    );
    let below_bound = format!(
        "#[test]\nfn below_bound() -> Stream<Check> {{\n    yield expect_eq(\"a\" + \"b\", \"ab\");\n    yield store_interns_at_most({});\n}}\n",
        interns - 1
    );

    let at = run_source(&at_bound).expect("at-bound source runs");
    assert!(
        at.passed(),
        "a bound at the observed intern count passes: {at:?}"
    );

    let below = run_source(&below_bound).expect("below-bound source runs");
    assert!(
        !below.passed(),
        "a bound below the observed intern count is red: {below:?}"
    );
}

/// Adding trace checks changes no counter they inspect: a trace check demands
/// nothing, interns nothing, issues no scheduler request, and stands up no memo
/// entry. The value-only and value-plus-trace runs produce identical counters.
#[test]
fn trace_reporting_is_excluded_from_the_counters_it_inspects() {
    const VALUE_ONLY: &str = r#"
#[test]
fn value_only() -> Stream<Check> {
    yield expect_eq(("x" + "y") + "z", "xyz");
}
"#;
    const WITH_TRACE: &str = r#"
#[test]
fn with_trace() -> Stream<Check> {
    yield expect_eq(("x" + "y") + "z", "xyz");
    yield scheduler_requests_at_most(1000);
    yield memo_entries_at_most(1000);
    yield store_interns_at_most(1000);
}
"#;
    let value_only = run_source(VALUE_ONLY).expect("value-only runs");
    let with_trace = run_source(WITH_TRACE).expect("value-plus-trace runs");

    assert!(
        with_trace.passed(),
        "the generous trace budgets pass: {with_trace:?}"
    );
    assert_eq!(
        value_only.plain.counters.scheduler_requests, with_trace.plain.counters.scheduler_requests,
        "trace checks issue no scheduler request",
    );
    assert_eq!(
        value_only.plain.counters.store_interns, with_trace.plain.counters.store_interns,
        "trace checks intern nothing",
    );
    assert_eq!(
        value_only.plain.counters.memo_misses, with_trace.plain.counters.memo_misses,
        "trace checks stand up no memo entry",
    );
    // The one value check is reported in both; the trace run adds exactly three
    // more reports, none of which perturbed the counters above.
    assert_eq!(value_only.plain.checks.len(), 1);
    assert_eq!(with_trace.plain.checks.len(), 4);
}

/// A trace check owned by an untaken control arm publishes nothing and is never
/// evaluated. If the untaken `None`-arm `scheduler_requests_at_most(0)` fired,
/// it would go red (the taken value check makes at least one scheduler request);
/// the test passing proves the untaken trace site does nothing.
#[test]
fn an_untaken_conditional_trace_site_does_nothing() {
    const SOURCE: &str = r#"
#[test]
fn untaken_trace() -> Stream<Check> {
    let xs = [1, 2, 3];
    yield match xs.split_last() {
        Some((last, rest)) => {
            yield expect_eq(last, 3);
        },
        None => scheduler_requests_at_most(0),
    };
    yield expect(true);
}
"#;
    let report = run_source(SOURCE).expect("conditional trace-site source runs");
    assert!(report.agrees(), "plain and chaos agree: {report:?}");
    assert!(
        report.passed(),
        "the taken checks pass and the untaken trace site is silent: {report:?}"
    );
    assert_eq!(
        report.plain.checks.len(),
        2,
        "the taken Some value check and the unconditional check; no None-arm trace phantom",
    );
}

/// A within-budget production certificate proven through the *outer* enforcement
/// path: a real child runs `run_source` and reports that every check passed,
/// staying under a generous wall and RSS budget read from typed metadata.
#[test]
fn within_budget_child_passes_through_the_outer_path() {
    const SOURCE: &str = r#"
#[test { budget_wall: 5s, budget_rss: 256MB }]
fn within() -> Stream<Check> {
    yield expect_eq(1 + 2, 3);
    yield store_interns_at_most(1000);
}
"#;
    let budget = budget_of(SOURCE);
    let outcome = run_under_budget(
        Path::new(CHILD_EXE),
        &budget,
        &Workload::RunSource {
            source: SOURCE.to_owned(),
        },
    );
    assert!(
        outcome.passed(),
        "the within-budget production run passes through the child: {outcome:?}",
    );
    assert!(
        matches!(
            outcome,
            BudgetOutcome::Within {
                report: vix::budget::ChildReport::RanSource { passed: true }
            }
        ),
        "the outcome is an in-budget successful source run: {outcome:?}",
    );
}

/// The wall watchdog terminates a runaway native loop the language cannot
/// interrupt from the inside. The budget is read from typed metadata; the
/// workload is a deterministic infinite spin.
#[test]
fn over_wall_child_is_killed() {
    const SOURCE: &str = r#"
#[test { budget_wall: 200ms }]
fn tight_wall() -> Stream<Check> {
    yield expect(true);
}
"#;
    let budget = budget_of(SOURCE);
    let outcome = run_under_budget(Path::new(CHILD_EXE), &budget, &Workload::SpinForever);
    assert!(
        matches!(outcome, BudgetOutcome::OverWall { .. }),
        "a runaway native loop is killed over the wall budget: {outcome:?}",
    );
    assert!(!outcome.passed(), "an over-wall run is red: {outcome:?}");
}

/// The resident-set watchdog terminates an over-memory child. On a platform
/// without sound cross-process RSS observation, the runner reports a typed seam
/// rather than a silently unenforced budget — either way the run is red.
#[test]
fn over_rss_child_is_killed_or_reports_the_platform_seam() {
    const SOURCE: &str = r#"
#[test { budget_rss: 128MB }]
fn tight_rss() -> Stream<Check> {
    yield expect(true);
}
"#;
    let budget = budget_of(SOURCE);
    let outcome = run_under_budget(
        Path::new(CHILD_EXE),
        &budget,
        &Workload::GrowResident {
            target_bytes: 512 * 1024 * 1024,
        },
    );
    match &outcome {
        BudgetOutcome::OverRss {
            budget_bytes,
            observed_bytes,
        } => {
            assert_eq!(*budget_bytes, 128 * 1024 * 1024);
            assert!(
                *observed_bytes > *budget_bytes,
                "the child was killed after exceeding the RSS ceiling: {outcome:?}",
            );
        }
        BudgetOutcome::RssEnforcementUnsupported { .. } => {
            // Acceptable only on a platform this runner cannot soundly observe.
            // macOS and Linux must enforce RSS, never report the seam.
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            panic!("macOS and Linux must enforce RSS, not report the seam: {outcome:?}");
        }
        other => panic!("expected an RSS kill or the typed platform seam, got {other:?}"),
    }
    assert!(!outcome.passed(), "an over-RSS run is red: {outcome:?}");
}
