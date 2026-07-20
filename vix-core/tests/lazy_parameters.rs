//! Production certificates for call-site-specialized lazy where-parameters.
//!
//! A call whose callee has inline-word wire parameters keeps its strict
//! arguments as bundled `ArgCopy` frame bindings and demands each non-strict
//! argument inside the callee through force-on-park, only where the taken control
//! region consumes it. These certificates exercise both branches, the absence of
//! an untaken failing argument, the attribution of a selected failing argument,
//! that distinct call sites cannot cross-wire, and that an ordinary strict call
//! stays a bundled direct `WeavyOp::Call`.

use std::collections::BTreeMap;

use vix::compiler::Compiler;
use vix::lowering::LoweringCache;
use vixen_runtime::ratchet::run_source;
use vix::runtime::{EventKind, FailureValue};
use weavy::task::Op as WeavyOp;

/// Frame-entry counts by callee name — direct evidence of what executed.
fn frame_entries(source: &str) -> BTreeMap<String, usize> {
    let module = Compiler::default().compile(source).expect("compiles");
    let names: BTreeMap<u32, String> = module
        .module
        .functions
        .iter()
        .map(|function| (function.id.0, function.name.clone()))
        .collect();
    let report = run_source(source).expect("runs");
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

const PICK: &str = r#"
fn pick(flag: Bool) where { a: Int, b: Int } -> Int {
    if flag { a } else { b }
}
fn cheap() -> Int { 1 }
fn expensive() -> Int { 999 }
"#;

/// The taken branch selects `a`: `cheap` runs, `expensive` never does.
#[test]
fn true_branch_demands_only_the_first_argument() {
    let source = format!(
        "{PICK}\n#[test]\nfn t() -> Stream<Check> {{\n    yield expect_eq(pick(true) where {{ a: cheap(), b: expensive() }}, 1);\n    yield demanded(cheap());\n    yield never_demanded(expensive());\n}}\n"
    );
    let report = run_source(&source).expect("runs");
    assert!(report.passed() && report.agrees(), "{report:?}");
    let frames = frame_entries(&source);
    assert_eq!(frames.get("cheap").copied().unwrap_or(0), 1);
    assert_eq!(frames.get("expensive").copied().unwrap_or(0), 0);
}

/// The `false` branch mirrors it: `expensive` runs, `cheap` never does.
#[test]
fn false_branch_demands_only_the_second_argument() {
    let source = format!(
        "{PICK}\n#[test]\nfn t() -> Stream<Check> {{\n    yield expect_eq(pick(false) where {{ a: cheap(), b: expensive() }}, 999);\n    yield demanded(expensive());\n    yield never_demanded(cheap());\n}}\n"
    );
    let report = run_source(&source).expect("runs");
    assert!(report.passed() && report.agrees(), "{report:?}");
    let frames = frame_entries(&source);
    assert_eq!(frames.get("expensive").copied().unwrap_or(0), 1);
    assert_eq!(frames.get("cheap").copied().unwrap_or(0), 0);
}

/// An untaken failing argument (`1 / 0`) is never forced: the run succeeds with
/// no failure because the division is on the branch control never selects.
#[test]
fn untaken_failing_argument_is_never_forced() {
    let source = format!(
        "{PICK}\n#[test]\nfn t() -> Stream<Check> {{\n    yield expect_eq(pick(false) where {{ a: 1 / 0, b: 7 }}, 7);\n}}\n"
    );
    let report = run_source(&source).expect("runs");
    assert!(report.passed() && report.agrees(), "{report:?}");
    assert!(
        report
            .plain
            .checks
            .iter()
            .all(|check| check.failure.is_none()),
        "no failure when the failing argument is never selected: {report:?}",
    );
}

/// A selected failing argument propagates `DivisionByZero` with its authored
/// source site to the parent demand.
#[test]
fn selected_failing_argument_propagates_typed_failure_with_attribution() {
    let source = format!(
        "{PICK}\n#[test]\nfn t() -> Stream<Check> {{\n    yield expect_eq(pick(true) where {{ a: 1 / 0, b: 7 }}, 999);\n}}\n"
    );
    let report =
        run_source(&source).expect("a failing argument is a language failure, not a crash");
    let failed = report
        .plain
        .checks
        .iter()
        .find(|check| check.failure.is_some())
        .expect("the selected division-by-zero argument fails the check");
    assert!(
        matches!(failed.failure, Some(FailureValue::DivisionByZero { .. })),
        "typed failure, got {:?}",
        failed.failure,
    );
    assert!(
        failed.failure_context.is_some(),
        "the propagated failure retains its authored source site",
    );
}

/// Two lazy call sites to the same callee, with swapped arguments, cannot
/// cross-wire: each demands exactly its own selected argument.
#[test]
fn distinct_call_sites_cannot_cross_wire() {
    let source = format!(
        "{PICK}\nfn other() -> Int {{ 5 }}\n#[test]\nfn t() -> Stream<Check> {{\n    yield expect_eq(pick(true) where {{ a: cheap(), b: expensive() }}, 1);\n    yield expect_eq(pick(false) where {{ a: other(), b: expensive() }}, 999);\n    yield demanded(cheap());\n    yield demanded(expensive());\n    yield never_demanded(other());\n}}\n"
    );
    let report = run_source(&source).expect("runs");
    assert!(report.passed() && report.agrees(), "{report:?}");
    let frames = frame_entries(&source);
    // First site (true) forces cheap; second site (false) forces expensive.
    // `other` is the first site's untaken slot at the second site — never forced.
    assert_eq!(frames.get("cheap").copied().unwrap_or(0), 1);
    assert_eq!(frames.get("expensive").copied().unwrap_or(0), 1);
    assert_eq!(frames.get("other").copied().unwrap_or(0), 0);
}

const POINT: &str = r#"
struct Point { x: Int, y: Int }
fn cheap() -> Int { 41 }
fn expensive() -> Int { 999 }
"#;

/// Projecting one field of a projection-only local record demands only that
/// field's initializer; the other field's invocation never runs.
#[test]
fn projecting_one_field_never_computes_the_other() {
    let source = format!(
        "{POINT}\n#[test]\nfn t() -> Stream<Check> {{\n    let p = Point {{ x: cheap(), y: expensive() }};\n    yield expect_eq(p.x + 1, 42);\n    yield never_demanded(expensive());\n    yield demanded(cheap());\n}}\n"
    );
    let report = run_source(&source).expect("runs");
    assert!(report.passed() && report.agrees(), "{report:?}");
    let frames = frame_entries(&source);
    assert_eq!(frames.get("cheap").copied().unwrap_or(0), 1);
    assert_eq!(frames.get("expensive").copied().unwrap_or(0), 0);
}

/// Demanding the other field forces the other field: `p.y` computes `expensive`
/// and leaves `cheap` — never projected here — uncomputed.
#[test]
fn demanding_the_other_field_forces_only_that_field() {
    let source = format!(
        "{POINT}\n#[test]\nfn t() -> Stream<Check> {{\n    let p = Point {{ x: cheap(), y: expensive() }};\n    yield expect_eq(p.y, 999);\n    yield demanded(expensive());\n    yield never_demanded(cheap());\n}}\n"
    );
    let report = run_source(&source).expect("runs");
    assert!(report.passed() && report.agrees(), "{report:?}");
    let frames = frame_entries(&source);
    assert_eq!(frames.get("expensive").copied().unwrap_or(0), 1);
    assert_eq!(frames.get("cheap").copied().unwrap_or(0), 0);
}

/// A strict call (no wire parameters) stays a bundled direct `WeavyOp::Call`:
/// the partition extracts no argument island and the check island lowers a
/// direct call, exactly as rung 004 requires.
#[test]
fn strict_call_stays_bundled_direct_call() {
    const SOURCE: &str = r#"
fn double(n: Int) -> Int { n * 2 }
#[test]
fn t() -> Stream<Check> {
    yield expect_eq(double(21), 42);
}
"#;
    let module = Compiler::default().compile(SOURCE).expect("compiles");
    let partitioned = module.module.partition_test(&module.module.tests[0]);
    assert!(
        partitioned.wire_islands.is_empty(),
        "a strict call is not partitioned into an argument island",
    );
    let mut cache = LoweringCache::default();
    let lowered = cache
        .get_or_lower(&partitioned.islands[0])
        .expect("check island lowers");
    let has_direct_call = lowered.program().fns.iter().any(|function| {
        function
            .code
            .iter()
            .any(|op| matches!(op, WeavyOp::Call { .. }))
    });
    let has_await = lowered.program().fns.iter().any(|function| {
        function
            .code
            .iter()
            .any(|op| matches!(op, WeavyOp::Await { .. }))
    });
    assert!(
        has_direct_call,
        "double(21) lowers to a bundled direct call"
    );
    assert!(!has_await, "a strict call issues no AwaitWire");
    assert!(run_source(SOURCE).expect("runs").passed());
}
