//! Rung 051 forward checkpoints 3 and 5–8 — shared value-island publication.
//!
//! Checkpoints 2 and 4 build the dense `range` array and the molten one-item
//! append fold correctly, but *independently per consumer*: every `ValueCheck`
//! that reads `xs` re-runs the whole construction inside its own island. The
//! production-shaped rung requires the million-element `xs` to be constructed
//! **once**, published across the island edge through scheduler-owned
//! `realize_value`/`Store::intern_tree`, and shared by its consumers as one
//! `ValueId`.
//!
//! This file is committed as the deliberately red production-path checkpoint:
//! it partitions the *unchanged* canonical rung 051 and asserts the target
//! post-implementation shape — one shared `xs` construction island — which is
//! currently false because each consumer recomputes it.

use vix::compiler::Compiler;
use vix::ratchet::{RunError, run_source};
use vix::runtime::EventKind;
use vix::vir::{Op as VirOp, PartitionedRecipe};

const RUNG_051: &str = include_str!("ratchet/051-molten-accumulator.vix");

/// Count the value-check islands whose own nodes construct the range aggregate
/// (`Op::Range`). Under checkpoints 2 and 4 each consumer recomputes it; the
/// shared-publication target is exactly one construction island.
fn islands_constructing_the_range(source: &str) -> usize {
    let module = Compiler::new().compile(source).expect("source compiles");
    let partitioned = module.partition_test(&module.tests[0]);
    partitioned
        .values
        .iter()
        .map(|value| &value.island)
        .chain(partitioned.islands.iter())
        .filter(|island| {
            island
                .nodes
                .iter()
                .any(|node| matches!(node.op, VirOp::Range))
        })
        .count()
}

/// The canonical rung still compiles and keeps its four value checks plus two
/// trace checks; this checkpoint changes publication, never the rung.
#[test]
fn canonical_rung_051_structure_is_unchanged() {
    let module = Compiler::new()
        .compile(RUNG_051)
        .expect("rung 051 compiles");
    let partitioned = module.partition_test(&module.tests[0]);
    let value_sites = partitioned
        .sites
        .iter()
        .filter(|site| matches!(site.recipe, PartitionedRecipe::Value { .. }))
        .count();
    let trace_sites = partitioned
        .sites
        .iter()
        .filter(|site| matches!(site.recipe, PartitionedRecipe::Trace(_)))
        .count();
    assert_eq!(
        value_sites, 4,
        "four value checks (len, sample, xs[0], xs[last])"
    );
    assert_eq!(
        trace_sites, 2,
        "two trace checks (store interns, memo entries)"
    );
}

/// The red production-path boundary: the million-element `xs` must be
/// constructed by exactly one shared value island, not recomputed by each of
/// its consumers. Today `Op::Range` appears in three consumer islands
/// (`xs.len()`, `xs[0]`, `xs[999999]`); the `sample` check constructs its own
/// small array and does not read `xs`.
#[test]
fn canonical_rung_051_constructs_the_shared_aggregate_once() {
    assert_eq!(
        islands_constructing_the_range(RUNG_051),
        1,
        "xs must be built once in a shared value island, not recomputed per consumer",
    );
}

#[test]
fn three_consumers_share_one_publication_identity_and_constant_machinery() {
    let report = run_source(RUNG_051).expect("canonical rung runs");
    assert!(report.passed() && report.agrees());
    assert_eq!(report.plain.values.len(), 1);
    assert_eq!(report.plain.values, report.chaos.values);
    let published = report.plain.values[0].identity;
    assert_eq!(
        report
            .plain
            .checks
            .iter()
            .filter(|check| check.arguments == [published])
            .count(),
        3,
        "all three xs consumers bind the same ValueId through their demand preimage",
    );
    assert_eq!(report.plain.counters.value_island_spawns, 1);
    assert_eq!(report.plain.counters.successful_aggregate_freezes, 1);
    assert_eq!(report.plain.counters.scheduler_requests, 5);
    assert!(report.plain.counters.store_interns <= 10);
    let marks = report
        .plain
        .events
        .iter()
        .filter(|event| matches!(event.kind, EventKind::WeavyMark { .. }))
        .count();
    assert!(
        marks < 128,
        "million elements do not produce per-element trace events: observed {marks}",
    );
    let mut provenance = report
        .plain
        .checks
        .iter()
        .map(|check| check.provenance.clone())
        .collect::<Vec<_>>();
    provenance.sort();
    provenance.dedup();
    assert_eq!(provenance.len(), report.plain.checks.len());
}

#[test]
fn nested_referents_freeze_by_value_identity() {
    const SOURCE: &str = r#"
#[test]
fn nested() -> Stream<Check> {
    let xs = ["alpha", "beta"];
    yield expect_eq(xs.len(), 2);
    yield expect_eq(xs[0], "alpha");
    yield expect_eq(xs[1], "beta");
}
"#;
    let report = run_source(SOURCE).expect("nested handle array runs");
    assert!(report.passed() && report.agrees(), "{report:?}");
    assert_eq!(report.plain.values.len(), 1);
    assert_eq!(
        report.plain.values[0].identity,
        report.chaos.values[0].identity
    );
    assert_eq!(report.plain.counters.successful_aggregate_freezes, 1);
}

#[test]
fn shared_map_and_set_require_the_ordered_freeze_capability() {
    for (source, kind) in [
        (
            r#"
#[test]
fn map_shared() -> Stream<Check> {
    let value: Map<String, Int> = %{"k" => 1};
    yield expect_eq(value.len(), 1);
    yield expect(value.has("k"));
}
"#,
            "Map",
        ),
        (
            r#"
#[test]
fn set_shared() -> Stream<Check> {
    let value: Set<Int> = %[1, 2];
    yield expect_eq(value.len(), 2);
    yield expect(value.has(1));
}
"#,
            "Set",
        ),
    ] {
        let RunError::Diagnostics(diagnostics) = run_source(source).expect_err("must be typed red")
        else {
            unreachable!()
        };
        assert!(diagnostics.entries[0].message().contains(kind));
        assert!(diagnostics.entries[0].message().contains("rung-138"));
    }
}

#[test]
fn shared_value_failure_identity_propagates_with_consumer_context() {
    const SOURCE: &str = r#"
#[test]
fn failure() -> Stream<Check> {
    let seed = [1];
    let xs = [seed[9]];
    yield expect_eq(xs.len(), 1);
    yield expect_eq(xs[0], 1);
}
"#;
    let report = run_source(SOURCE).expect("language failure remains a report value");
    assert!(report.agrees());
    let publication = &report.plain.values[0];
    assert!(publication.failure.is_some());
    assert_eq!(report.plain.checks.len(), 2);
    for check in &report.plain.checks {
        assert_eq!(check.identity, Some(publication.identity));
        assert_eq!(check.failure, publication.failure);
        assert!(check.failure_context.is_some());
    }
    assert_ne!(
        report.plain.checks[0].failure_context, report.plain.checks[1].failure_context,
        "consumer source context is rebuilt rather than stored with the failure",
    );
}
