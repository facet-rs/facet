//! Rung 051 forward checkpoints 2 and 4 — deliberately red checkpoint.
//!
//! This file is committed BEFORE the `range where { from, to }` dense-array
//! construct and the molten one-item-append fold shape exist. It pins the exact
//! post-implementation surface the two checkpoints must reach:
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
//! Every assertion targets the post-implementation behaviour and references
//! only already-public API, so the test binary builds and the redness is
//! runtime: `range where` does not parse today, so each `run_source` returns a
//! parse diagnostic and the `.expect(...)` fails at runtime.
//!
//! Checkpoints 3, 5, 6, 7, and 8 (shared value-island extraction and the
//! molten-to-store publication that lets one million-element construction cross
//! the island edge once) are explicitly out of scope here; the seam is pinned
//! by an `#[ignore]`d certificate below rather than guessed at.

use vix::ratchet::run_source;

/// Checkpoint 2 — `range where { from, to }` builds the specified dense
/// `[Int]` in position order, half-open `[from, to)`, driven entirely in-frame.
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
    assert!(report.passed(), "every dense-array check passes: {report:?}");
    assert_eq!(report.plain.checks.len(), 4);
}

/// Checkpoint 2 — an empty half-open range `from == to` is the empty array,
/// which is the natural meaning of `[from, to)`, not a red seam.
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

/// Checkpoint 4 — the strict one-item-append fold over a `range` builds its
/// dense result and its values match the by-hand expectation.
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
    assert!(report.passed(), "every molten-fold check passes: {report:?}");
    assert_eq!(report.plain.checks.len(), 4);
}

/// Checkpoint 4 — a fold whose accumulator escapes beyond the single append
/// base is an arbitrary fold: it must keep the semantic copy path and still
/// produce the correct value.
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
    let report = run_source(SOURCE).expect("arbitrary fold source runs");
    assert!(report.passed(), "the copy-path fold is correct: {report:?}");
}

/// Checkpoints 3/5/6/7/8 — shared value-island extraction and the
/// molten-to-store publication that carries one aggregate across the island
/// edge exactly once. This is the precise remaining seam after checkpoints 2
/// and 4; it is pinned here and left red on purpose.
#[test]
#[ignore = "rung-051 checkpoints 3/5/6/7/8: shared value-island publication seam is not in scope for checkpoints 2 and 4"]
fn shared_value_island_publication_is_the_remaining_red_seam() {
    // The million-element rung witnesses one construction shared by four value
    // checks through scheduler-owned framed publication. Until the extraction
    // registry, opaque resolver, and `realize_value` land, this stays red.
    const SOURCE: &str = r#"
#[test { budget_wall: 5s, budget_rss: 1GB }]
fn molten_accumulator() -> Stream<Check> {
    let n = 1000000;
    let xs = (range where { from: 0, to: n }).fold([], |acc, i| acc + (i * 2));
    yield expect_eq(xs.len(), n);
    yield expect_eq(xs[0], 0);
    yield expect_eq(xs[999999], 1999998);
    yield store_interns_at_most(10);
    yield memo_entries_at_most(10);
}
"#;
    let report = run_source(SOURCE).expect("million-element molten source runs");
    assert!(report.passed(), "the shared-publication rung passes: {report:?}");
}
