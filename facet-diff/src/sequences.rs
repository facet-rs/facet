//! Sequence diffing using Myers' algorithm.

use facet_diff_core::Updates;
use facet_reflect::Peek;

use crate::diff::{diff_closeness, diff_new_peek};

/// Gets the diff of a sequence by using Myers' algorithm
pub fn diff<'mem, 'facet>(
    a: Vec<Peek<'mem, 'facet>>,
    b: Vec<Peek<'mem, 'facet>>,
) -> Updates<'mem, 'facet> {
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
            if diff_new_peek(a[x], b[y]).is_equal() {
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
        } else if diff_new_peek(a[x - 1], b[y - 1]).is_equal()
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

    updates.flatten_with(|a, b| diff_closeness(&diff_new_peek(a, b)), diff_new_peek);
    updates
}
