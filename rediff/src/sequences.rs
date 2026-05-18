//! Sequence diffing using Myers' algorithm.

use crate::{core_sequences::Updates, trace};
use facet_reflect::Peek;

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

    let n = a.len();
    let m = b.len();

    // Structural equality predicate, cached so we compute each pair's
    // (potentially recursive) diff at most once.
    let mut eq_cache = vec![vec![None::<bool>; n]; m];
    let eq = |x: usize, y: usize, cache: &mut Vec<Vec<Option<bool>>>| -> bool {
        if let Some(v) = cache[y][x] {
            return v;
        }
        let v = diff_new_peek_with_options(a[x], b[y], options).is_equal();
        cache[y][x] = Some(v);
        v
    };

    // Classic edit-distance table. Structurally-equal elements take a
    // free diagonal, so every common element (prefix, suffix, and
    // interior LCS) is kept; differing elements become a remove + add
    // pair, which the renderer re-diffs recursively. The first row/
    // column carry the real base cases (deleting / inserting a prefix) —
    // getting these wrong is what made common prefixes churn before.
    let mut d = vec![vec![0usize; n + 1]; m + 1];
    for (x, cell) in d[0].iter_mut().enumerate() {
        *cell = x;
    }
    for (y, row) in d.iter_mut().enumerate() {
        row[0] = y;
    }
    for y in 1..=m {
        for x in 1..=n {
            d[y][x] = if eq(x - 1, y - 1, &mut eq_cache) {
                d[y - 1][x - 1]
            } else {
                1 + d[y - 1][x].min(d[y][x - 1])
            };
        }
    }

    let mut updates = Updates::default();
    let mut x = n;
    let mut y = m;
    while x > 0 || y > 0 {
        if x > 0 && y > 0 && eq(x - 1, y - 1, &mut eq_cache) {
            updates.push_keep(a[x - 1]);
            x -= 1;
            y -= 1;
        } else if x > 0 && (y == 0 || d[y][x - 1] <= d[y - 1][x]) {
            // Deletion. On ties prefer removing first so that within a
            // replace group all removals precede all additions.
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
