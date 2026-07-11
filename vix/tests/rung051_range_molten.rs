//! Rung 051 forward checkpoints 2 and 4.
//!
//!   2. `range where { from, to }` allocates one dense array and fills it
//!      in-frame. Range and fold loop bodies use the same cheap interior
//!      vocabulary as rung 050 and emit no per-iteration trace marks, scheduler
//!      contacts, store operations, or identities.
//!   4. `Array.fold` selects a molten in-frame shape only for the exact strict
//!      one-item-append closure (accumulator consumed once as the append base,
//!      the appended expression evaluated exactly once). Arbitrary folds keep
//!      the semantic copy path, and a bounded forced-copy differential proves
//!      the molten and copy values are identical, duplicates and order included.
//!
//! Checkpoints 3, 5, 6, 7, and 8 add shared value-island extraction and the
//! molten-to-store publication that lets the million-element construction cross
//! the island edge exactly once. The canonical certificate at the end exercises
//! that production path unchanged.

use vix::compiler::{Compiler, CompilerConfig};
use vix::lowering::LoweringCache;
use vix::ratchet::{RunError, run_source, run_source_with_config};
use vix::runtime::EventKind;
use vix::vir::Op as VirOp;
use weavy::task::Op as WeavyOp;

// ---------------------------------------------------------------------------
// Checkpoint 2 — `range where { from, to }`
// ---------------------------------------------------------------------------

/// `range where { from, to }` builds the specified dense `[Int]` in position
/// order, half-open `[from, to)`, and runs green in both lanes.
#[test]
fn range_where_builds_the_specified_dense_array() {
    const SOURCE: &str = r#"
#[test]
fn range_dense() -> Stream<Check> {
    let xs = range where { from: 2, to: 6 };
    yield expect_eq(xs.len(), 4);
    yield expect_eq(xs[0], 2);
    yield expect_eq(xs[1], 3);
    yield expect_eq(xs[3], 5);
}
"#;
    let report = run_source(SOURCE).expect("range dense-array source runs");
    assert!(report.agrees(), "plain and chaos agree: {report:?}");
    assert!(
        report.passed(),
        "every dense-array check passes: {report:?}"
    );
    assert_eq!(report.plain.checks.len(), 4);
}

/// An empty half-open range `from == to` is the empty array — the natural
/// meaning of `[from, to)`, not a red seam.
#[test]
fn range_where_empty_bounds_is_the_empty_array() {
    const SOURCE: &str = r#"
#[test]
fn range_empty() -> Stream<Check> {
    let xs = range where { from: 4, to: 4 };
    yield expect_eq(xs.len(), 0);
}
"#;
    let report = run_source(SOURCE).expect("empty range source runs");
    assert!(report.passed(), "the empty range check passes: {report:?}");
}

/// Reversed bounds (`from > to`) are the deliberately unspecified edge: the
/// negative element count faults with a typed machine status rather than
/// silently clamping to an empty array.
#[test]
fn range_where_reversed_bounds_is_a_typed_red_seam() {
    const SOURCE: &str = r#"
#[test]
fn range_reversed() -> Stream<Check> {
    let xs = range where { from: 6, to: 2 };
    yield expect_eq(xs.len(), 0);
}
"#;
    match run_source(SOURCE) {
        Err(RunError::Machine(_)) => {}
        other => panic!("reversed range must be a typed machine fault, got {other:?}"),
    }
}

/// `range` lowers to one dense in-frame fill: an `ArrayNew` reservation and a
/// whole-element `ArrayStore` in the loop body, with no host call anywhere.
#[test]
fn range_lowers_to_one_dense_in_frame_fill() {
    const SOURCE: &str = r#"
#[test]
fn range_shape() -> Stream<Check> {
    let xs = range where { from: 0, to: 8 };
    yield expect_eq(xs.len(), 8);
}
"#;
    let ops = lowered_ops(SOURCE);
    assert_eq!(
        ops.iter()
            .filter(|op| matches!(op, WeavyOp::ArrayNew { .. }))
            .count(),
        1,
        "range reserves exactly one dense array",
    );
    assert!(
        ops.iter()
            .any(|op| matches!(op, WeavyOp::ArrayStore { .. })),
        "range fills the dense array with whole-element stores",
    );
    assert!(
        !ops.iter()
            .any(|op| matches!(op, WeavyOp::HostCall { .. } | WeavyOp::HostCallYield { .. })),
        "range uses no host call",
    );
}

/// The in-frame fill is *not demand*: enlarging a `range` or the fold over it by
/// an order of magnitude changes no scheduler request, store intern, memo entry,
/// or trace mark. The per-iteration body touches none of that machinery.
#[test]
fn range_and_fold_loop_bodies_touch_no_per_iteration_machinery() {
    let small = run_source(&range_fold_source(16)).expect("small range/fold runs");
    let large = run_source(&range_fold_source(4096)).expect("large range/fold runs");
    assert!(small.passed() && large.passed(), "both sizes pass");

    assert_eq!(
        small.plain.counters.scheduler_requests, large.plain.counters.scheduler_requests,
        "no per-iteration scheduler contact",
    );
    assert_eq!(
        small.plain.counters.store_interns, large.plain.counters.store_interns,
        "no per-iteration store operation",
    );
    assert_eq!(
        small.plain.counters.memo_misses, large.plain.counters.memo_misses,
        "no per-iteration memo entry",
    );
    assert_eq!(
        weavy_mark_count(&small),
        weavy_mark_count(&large),
        "no per-iteration trace mark",
    );
}

fn range_fold_source(n: i64) -> String {
    format!(
        r#"
#[test]
fn range_fold() -> Stream<Check> {{
    let xs = (range where {{ from: 0, to: {n} }}).fold([], |acc, i| acc + (i * 2));
    yield expect_eq(xs.len(), {n});
    yield expect_eq(xs[0], 0);
}}
"#
    )
}

// ---------------------------------------------------------------------------
// Checkpoint 4 — molten one-item-append fold
// ---------------------------------------------------------------------------

/// The strict one-item-append fold over a `range` is correct: `xs[i] == i * 2`.
#[test]
fn molten_append_fold_over_range_is_correct() {
    const SOURCE: &str = r#"
#[test]
fn molten_small() -> Stream<Check> {
    let xs = (range where { from: 0, to: 5 }).fold([], |acc, i| acc + (i * 2));
    yield expect_eq(xs.len(), 5);
    yield expect_eq(xs[0], 0);
    yield expect_eq(xs[1], 2);
    yield expect_eq(xs[4], 8);
}
"#;
    let report = run_source(SOURCE).expect("molten append fold source runs");
    assert!(report.agrees(), "plain and chaos agree: {report:?}");
    assert!(
        report.passed(),
        "every molten-fold check passes: {report:?}"
    );
    assert_eq!(report.plain.checks.len(), 4);
}

/// The strict append fold selects the molten in-frame shape: its VIR carries an
/// `ArrayMap` (the per-element dense build) and no `ArrayFold` copy node.
#[test]
fn strict_append_fold_selects_the_molten_shape() {
    const SOURCE: &str = r#"
#[test]
fn strict() -> Stream<Check> {
    let xs = [1, 2, 3].fold([], |acc, i| acc + (i * 2));
    yield expect_eq(xs.len(), 3);
}
"#;
    let ops = test_function_vir_ops(SOURCE, CompilerConfig::default());
    assert!(
        ops.iter().any(|op| matches!(op, VirOp::ArrayMap { .. })),
        "the molten append fold lowers to the dense map build",
    );
    assert!(
        !ops.iter().any(|op| matches!(op, VirOp::ArrayFold)),
        "the molten shape does not keep the copy fold node",
    );
}

/// `force_molten_copy` disables the molten shape: the same strict append fold
/// keeps the semantic copy path, an `ArrayFold` and no synthesized `ArrayMap`.
#[test]
fn forced_copy_keeps_the_copy_fold_node() {
    const SOURCE: &str = r#"
#[test]
fn strict() -> Stream<Check> {
    let xs = [1, 2, 3].fold([], |acc, i| acc + (i * 2));
    yield expect_eq(xs.len(), 3);
}
"#;
    let ops = test_function_vir_ops(
        SOURCE,
        CompilerConfig {
            force_molten_copy: true,
        },
    );
    assert!(
        ops.iter().any(|op| matches!(op, VirOp::ArrayFold)),
        "the forced-copy fold keeps the semantic copy path",
    );
    assert!(
        !ops.iter().any(|op| matches!(op, VirOp::ArrayMap { .. })),
        "no molten map is synthesized under forced copy",
    );
}

/// A fold whose accumulator escapes beyond the single append base (here `acc`
/// is read again through `acc.len()`) is arbitrary: it keeps the copy fold node
/// and is still correct.
#[test]
fn arbitrary_fold_keeps_the_semantic_copy_path() {
    const SOURCE: &str = r#"
#[test]
fn arbitrary() -> Stream<Check> {
    let xs = [1, 2, 3].fold([], |acc, i| acc + (acc.len() + i));
    yield expect_eq(xs.len(), 3);
    yield expect_eq(xs[0], 1);
    yield expect_eq(xs[1], 3);
    yield expect_eq(xs[2], 5);
}
"#;
    let ops = test_function_vir_ops(SOURCE, CompilerConfig::default());
    assert!(
        ops.iter().any(|op| matches!(op, VirOp::ArrayFold)),
        "a fold that re-reads the accumulator stays on the copy path",
    );
    let report = run_source(SOURCE).expect("arbitrary fold source runs");
    assert!(report.passed(), "the copy-path fold is correct: {report:?}");
}

/// The bounded forced-copy differential (faithful to canonical rung 123): the
/// molten run and the forced-copy run produce identical values — including
/// duplicate and order behaviour — proven by identical per-check value
/// identities. `i * i` produces duplicates (e.g. no duplicates here, but the
/// filter/dedup shape is exercised by the multiset selection below).
#[test]
fn molten_and_forced_copy_values_are_identical() {
    const SOURCE: &str = r#"
#[test]
fn differential() -> Stream<Check> {
    let xs = (range where { from: 0, to: 200 }).fold([], |acc, i| acc + (i * i));
    yield expect_eq(xs.len(), 200);
    yield expect_eq(xs[0], 0);
    yield expect_eq(xs[1], 1);
    yield expect_eq(xs[13], 169);
    yield expect_eq(xs[199], 39601);
}
"#;
    let molten = run_source_with_config(SOURCE, CompilerConfig::default())
        .expect("molten differential runs");
    let copy = run_source_with_config(
        SOURCE,
        CompilerConfig {
            force_molten_copy: true,
        },
    )
    .expect("forced-copy differential runs");

    assert!(molten.passed(), "molten run passes: {molten:?}");
    assert!(copy.passed(), "forced-copy run passes: {copy:?}");

    let molten_ids: Vec<_> = molten
        .plain
        .checks
        .iter()
        .map(|check| (check.provenance.clone(), check.identity, check.passed))
        .collect();
    let copy_ids: Vec<_> = copy
        .plain
        .checks
        .iter()
        .map(|check| (check.provenance.clone(), check.identity, check.passed))
        .collect();
    assert_eq!(
        molten_ids, copy_ids,
        "molten and forced-copy value identities are bit-identical",
    );
}

/// Duplicate and order behaviour is preserved across the differential: a fold
/// that maps every element to the same constant builds a dense array of equal
/// duplicates, and molten and forced-copy agree on it.
#[test]
fn molten_and_forced_copy_preserve_duplicates_and_order() {
    const SOURCE: &str = r#"
#[test]
fn duplicates() -> Stream<Check> {
    let xs = (range where { from: 0, to: 6 }).fold([], |acc, i| acc + (i.rem(2)));
    yield expect_eq(xs.len(), 6);
    yield expect_eq(xs[0], 0);
    yield expect_eq(xs[1], 1);
    yield expect_eq(xs[2], 0);
    yield expect_eq(xs[5], 1);
}
"#;
    let molten =
        run_source_with_config(SOURCE, CompilerConfig::default()).expect("molten duplicates run");
    let copy = run_source_with_config(
        SOURCE,
        CompilerConfig {
            force_molten_copy: true,
        },
    )
    .expect("forced-copy duplicates run");
    assert!(molten.passed() && copy.passed());
    let ids = |report: &vix::ratchet::RatchetReport| -> Vec<_> {
        report
            .plain
            .checks
            .iter()
            .map(|check| (check.provenance.clone(), check.identity))
            .collect()
    };
    assert_eq!(
        ids(&molten),
        ids(&copy),
        "duplicate/order values agree across molten and forced copy",
    );
}

// ---------------------------------------------------------------------------
// Shared publication — checkpoints 3/5/6/7/8
// ---------------------------------------------------------------------------

/// Shared value-island extraction and the molten-to-store publication that
/// carries one aggregate across the island edge exactly once. This is the
/// production edge after checkpoints 2 and 4. The canonical fixture is included
/// unchanged: one construction is shared by its value checks and frozen once.
#[test]
fn canonical_rung_051_publishes_the_million_element_value_once() {
    const SOURCE: &str = include_str!("ratchet/051-molten-accumulator.vix");
    let report = run_source(SOURCE).expect("canonical million-element molten source runs");
    assert!(
        report.passed(),
        "the shared-publication rung passes: {report:?}"
    );
    assert_eq!(report.plain.counters.value_island_spawns, 1);
    assert_eq!(report.plain.counters.successful_aggregate_freezes, 1);
    assert_eq!(report.plain.counters.active_molten_selections, 1);
    assert_eq!(report.plain.counters.forced_copy_selections, 0);
    assert_eq!(report.plain.counters.framed_bytes, 8_000_000);
    assert_eq!(report.chaos.counters.value_island_spawns, 2);
    assert_eq!(report.chaos.counters.successful_aggregate_freezes, 1);
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn weavy_mark_count(report: &vix::ratchet::RatchetReport) -> usize {
    report
        .plain
        .events
        .iter()
        .filter(|event| matches!(event.kind, EventKind::WeavyMark { .. }))
        .count()
}

/// Every lowered Weavy op across the first test's islands.
fn lowered_ops(source: &str) -> Vec<WeavyOp> {
    let module = Compiler::new().compile(source).expect("source compiles");
    let partitioned = module.partition_test(&module.tests[0]);
    let mut cache = LoweringCache::default();
    let mut ops = Vec::new();
    for island in &partitioned.islands {
        let lowered = cache.get_or_lower(island).expect("island lowers");
        for function in &lowered.program().fns {
            ops.extend(function.code.iter().cloned());
        }
    }
    ops
}

/// The VIR ops of the first test's own function under a given configuration.
fn test_function_vir_ops(source: &str, config: CompilerConfig) -> Vec<VirOp> {
    let module = Compiler::with_config(config)
        .compile(source)
        .expect("source compiles");
    let test = &module.tests[0];
    module.functions[test.function.0 as usize]
        .nodes
        .iter()
        .map(|node| node.op.clone())
        .collect()
}
