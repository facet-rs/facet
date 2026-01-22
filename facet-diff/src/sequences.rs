//! Sequence diffing using Myers' algorithm.

use facet_diff_core::Updates;
use facet_reflect::Peek;
use log::trace;

use crate::diff::{DiffOptions, diff_new_peek_with_options};

/// Maximum size for sequences to use Myers' algorithm.
/// Larger sequences fall back to simple element-by-element comparison
/// to prevent exponential blowup.
const MAX_SEQUENCE_SIZE: usize = 100;

/// Gets the diff of a sequence by using Myers' algorithm
#[allow(dead_code)]
pub fn diff<'mem, 'facet>(
    a: Vec<Peek<'mem, 'facet>>,
    b: Vec<Peek<'mem, 'facet>>,
) -> Updates<'mem, 'facet> {
    diff_with_options(a, b, &DiffOptions::default())
}

/// Gets the diff of a sequence by using Myers' algorithm with options
pub fn diff_with_options<'mem, 'facet>(
    a: Vec<Peek<'mem, 'facet>>,
    b: Vec<Peek<'mem, 'facet>>,
    options: &DiffOptions,
) -> Updates<'mem, 'facet> {
    // Quick check: if lengths match and all elements are structurally equal, return empty
    if a.len() == b.len() {
        let all_equal = a.iter().zip(&b).all(|(a_item, b_item)| {
            diff_new_peek_with_options(*a_item, *b_item, options).is_equal()
        });
        if all_equal {
            return Updates::default();
        }
    }

    // For very large sequences, fall back to simple comparison to avoid
    // exponential blowup in flatten_with
    if a.len() > MAX_SEQUENCE_SIZE || b.len() > MAX_SEQUENCE_SIZE {
        trace!("Using simple_diff fallback (size limit exceeded)");
        return simple_diff_with_options(a, b, options);
    }

    // Moving l-t-r represents removing an element from a
    // Moving t-t-b represents adding an element from b
    //
    // Moving diagonally does both, which has no effect and thus has no cost
    // This can only be done when the items are the same
    //
    let mut mem = vec![vec![0; a.len() + 1]];

    for y in 0..b.len() {
        let mut next = vec![0];
        for x in 0..a.len() {
            let mut v = mem[y][x + 1].min(next[x]) + 1;
            if diff_new_peek_with_options(a[x], b[y], options).is_equal() {
                v = v.min(mem[y][x]);
            }

            next.push(v);
        }

        mem.push(next);
    }

    let mut updates = Updates::default();

    let mut x = a.len();
    let mut y = b.len();
    while x > 0 || y > 0 {
        if y == 0 {
            updates.push_remove(a[x - 1]);
            x -= 1;
        } else if x == 0 {
            updates.push_add(b[y - 1]);
            y -= 1;
        } else if diff_new_peek_with_options(a[x - 1], b[y - 1], options).is_equal()
            && mem[y - 1][x - 1] <= mem[y][x - 1].min(mem[y - 1][x]) + 1
        {
            updates.push_keep(a[x - 1]);
            x -= 1;
            y -= 1;
        } else if mem[y][x - 1] <= mem[y - 1][x] {
            // When costs are equal, prefer removes first to maintain the invariant
            // that within a replace group, all removals come before additions
            updates.push_remove(a[x - 1]);
            x -= 1;
        } else {
            updates.push_add(b[y - 1]);
            y -= 1;
        }
    }

    updates
}

/// Simple fallback diff for large sequences that doesn't use Myers' algorithm.
/// Just treats all differences as removes followed by adds.
#[allow(dead_code)]
fn simple_diff<'mem, 'facet>(
    a: Vec<Peek<'mem, 'facet>>,
    b: Vec<Peek<'mem, 'facet>>,
) -> Updates<'mem, 'facet> {
    simple_diff_with_options(a, b, &DiffOptions::default())
}

fn simple_diff_with_options<'mem, 'facet>(
    a: Vec<Peek<'mem, 'facet>>,
    b: Vec<Peek<'mem, 'facet>>,
    _options: &DiffOptions,
) -> Updates<'mem, 'facet> {
    trace!(
        "simple_diff: creating replace group with {} removals and {} additions",
        a.len(),
        b.len()
    );
    let mut updates = Updates::default();

    // Remove all from a
    for item in a.iter().rev() {
        updates.push_remove(*item);
    }

    // Add all from b
    for item in b.iter().rev() {
        updates.push_add(*item);
    }

    updates
}
