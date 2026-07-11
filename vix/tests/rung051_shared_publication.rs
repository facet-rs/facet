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
use vix::vir::{Op as VirOp, PartitionedRecipe};

const RUNG_051: &str = include_str!("ratchet/051-molten-accumulator.vix");

/// Count the value-check islands whose own nodes construct the range aggregate
/// (`Op::Range`). Under checkpoints 2 and 4 each consumer recomputes it; the
/// shared-publication target is exactly one construction island.
fn islands_constructing_the_range(source: &str) -> usize {
    let module = Compiler::new().compile(source).expect("source compiles");
    let partitioned = module.partition_test(&module.tests[0]);
    partitioned
        .islands
        .iter()
        .filter(|island| island.nodes.iter().any(|node| matches!(node.op, VirOp::Range)))
        .count()
}

/// The canonical rung still compiles and keeps its four value checks plus two
/// trace checks; this checkpoint changes publication, never the rung.
#[test]
fn canonical_rung_051_structure_is_unchanged() {
    let module = Compiler::new().compile(RUNG_051).expect("rung 051 compiles");
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
    assert_eq!(value_sites, 4, "four value checks (len, sample, xs[0], xs[last])");
    assert_eq!(trace_sites, 2, "two trace checks (store interns, memo entries)");
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
