//! Rung 051 forward checkpoints 3 and 5–8 — shared value-island publication.
//!
//! Checkpoints 2 and 4 initially built the dense `range` array and molten
//! one-item append fold independently per consumer: every `ValueCheck` reading
//! `xs` re-ran the whole construction inside its own island. This preserved red
//! checkpoint now proves the production shape: the million-element `xs` is
//! constructed **once**, published across the island edge through scheduler-owned
//! `realize_value`/`Store::intern_tree`, and shared by its consumers as one
//! `ValueId`.

use vix::compiler::{Compiler, CompilerConfig};
use vixen_runtime::ratchet::{run_source, run_source_with_config};
use vix::runtime::EventKind;
use vix::vir::{Op as VirOp, PartitionedRecipe};

const RUNG_051: &str = include_str!("ratchet/051-molten-accumulator.vix");

/// Count the value-check islands whose own nodes construct the range aggregate
/// (`Op::Range`). The shared-publication invariant is exactly one construction
/// island regardless of the number of consuming checks.
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
    let published = report.plain.values[0].identity.clone();
    assert_eq!(
        report
            .plain
            .checks
            .iter()
            .filter(|check| check.arguments == [published.clone()])
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
fn shared_map_and_set_publish_once_through_ordered_freeze() {
    for source in [
        r#"
#[test]
fn map_shared() -> Stream<Check> {
    let value: Map<String, Int> = %{"k" => 1};
    yield expect_eq(value.len(), 1);
    yield expect(value.has("k"));
}
"#,
        r#"
#[test]
fn set_shared() -> Stream<Check> {
    let value: Set<Int> = %[1, 2];
    yield expect_eq(value.len(), 2);
    yield expect(value.has(1));
}
"#,
    ] {
        let report = run_source(source).expect("ordered aggregate publication runs");
        assert!(report.passed() && report.agrees(), "{report:?}");
        assert_eq!(report.plain.values.len(), 1);
        assert_eq!(report.plain.counters.value_island_spawns, 1);
        assert_eq!(report.plain.counters.successful_aggregate_freezes, 1);
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
        assert_eq!(check.identity, Some(publication.identity.clone()));
        assert_eq!(check.failure, publication.failure);
        assert!(check.failure_context.is_some());
    }
    assert_ne!(
        report.plain.checks[0].failure_context, report.plain.checks[1].failure_context,
        "consumer source context is rebuilt rather than stored with the failure",
    );
}

#[test]
fn active_and_forced_copy_publications_share_identity_and_count_selection() {
    const SOURCE: &str = r#"
#[test]
fn selection() -> Stream<Check> {
    let xs = (range where { from: 0, to: 32 }).fold([], |acc, i| acc + i);
    yield expect_eq(xs.len(), 32);
    yield expect_eq(xs[31], 31);
}
"#;
    let active = run_source(SOURCE).expect("active publication runs");
    let forced = run_source_with_config(
        SOURCE,
        CompilerConfig {
            force_molten_copy: true,
            ..CompilerConfig::default()
        },
    )
    .expect("forced-copy publication runs");
    assert!(active.passed() && forced.passed());
    assert_eq!(
        active.plain.values[0].identity,
        forced.plain.values[0].identity
    );
    assert_eq!(active.plain.counters.active_molten_selections, 1);
    assert_eq!(active.plain.counters.forced_copy_selections, 0);
    assert_eq!(forced.plain.counters.active_molten_selections, 0);
    assert_eq!(forced.plain.counters.forced_copy_selections, 1);
}

#[test]
fn trace_checks_read_the_frozen_publication_snapshot_without_self_counting() {
    const SOURCE: &str = r#"
#[test]
fn counters() -> Stream<Check> {
    let xs = range where { from: 0, to: 4 };
    yield expect_eq(xs.len(), 4);
    yield expect_eq(xs[3], 3);
    yield value_island_spawns_at_most(2);
    yield successful_aggregate_freezes_at_most(1);
    yield active_molten_selections_at_most(1);
    yield forced_copy_selections_at_most(0);
    yield framed_bytes_at_most(32);
    yield peak_molten_bytes_at_most(1024);
    yield peak_molten_nodes_at_most(8);
}
"#;
    let report = run_source(SOURCE).expect("counter trace checks run");
    assert!(report.passed(), "{report:?}");
    assert_eq!(report.plain.counters.scheduler_requests, 3);
    assert_eq!(report.plain.counters.value_island_spawns, 1);
    assert_eq!(report.plain.counters.successful_aggregate_freezes, 1);
}
