use facet_reflect::Peek;

use crate::Diff;

pub(crate) struct Interspersed<A, B> {
    pub(crate) first: Option<A>,
    pub(crate) values: Vec<(B, A)>,
    pub(crate) last: Option<B>,
}

impl<A, B> Interspersed<A, B> {
    fn front_a(&mut self) -> &mut A
    where
        A: Default,
    {
        self.first.get_or_insert_default()
    }

    fn front_b(&mut self) -> &mut B
    where
        B: Default,
    {
        if let Some(a) = self.first.take() {
            self.values.insert(0, (B::default(), a));
        }

        if let Some((b, _)) = self.values.first_mut() {
            b
        } else {
            self.last.get_or_insert_default()
        }
    }
}

impl<A, B> Default for Interspersed<A, B> {
    fn default() -> Self {
        Self {
            first: Default::default(),
            values: Default::default(),
            last: Default::default(),
        }
    }
}

#[derive(Default)]
pub(crate) struct ReplaceGroup<'mem, 'facet> {
    pub(crate) removals: Vec<Peek<'mem, 'facet>>,
    pub(crate) additions: Vec<Peek<'mem, 'facet>>,
}

impl<'mem, 'facet> ReplaceGroup<'mem, 'facet> {
    fn push_add(&mut self, addition: Peek<'mem, 'facet>) {
        assert!(
            self.removals.is_empty(),
            "We want all blocks of updates to have removals first, then additions, this should follow from our implementation of myers' algorithm"
        );
        self.additions.insert(0, addition);
    }

    fn push_remove(&mut self, removal: Peek<'mem, 'facet>) {
        self.removals.insert(0, removal);
    }
}

#[derive(Default)]
pub(crate) struct UpdatesGroup<'mem, 'facet>(
    pub(crate) Interspersed<ReplaceGroup<'mem, 'facet>, Vec<Diff<'mem, 'facet>>>,
);

impl<'mem, 'facet> UpdatesGroup<'mem, 'facet> {
    fn push_add(&mut self, addition: Peek<'mem, 'facet>) {
        self.0.front_a().push_add(addition);
    }

    fn push_remove(&mut self, removal: Peek<'mem, 'facet>) {
        self.0.front_a().push_remove(removal);
    }

    fn flatten(&mut self) {
        let Some(updates) = self.0.first.take() else {
            return;
        };

        let mut mem = vec![vec![0; updates.removals.len() + 1]];

        for x in 0..updates.removals.len() {
            let mut row = vec![0];

            for y in 0..updates.additions.len() {
                row.push(row.last().copied().unwrap().max(
                    mem[x][y]
                        + Diff::new_peek(updates.removals[x], updates.additions[y]).closeness(),
                ));
            }

            mem.push(row);
        }

        let mut x = updates.removals.len();
        let mut y = updates.additions.len();

        while x > 0 || y > 0 {
            if x == 0 {
                self.push_add(updates.additions[y - 1]);
                y -= 1;
            } else if y == 0 {
                self.push_remove(updates.removals[x - 1]);
                x -= 1;
            } else if mem[x][y - 1] == mem[x][y] {
                self.push_add(updates.additions[y - 1]);
                y -= 1;
            } else {
                let diff = Diff::new_peek(updates.removals[x - 1], updates.additions[y - 1]);
                self.0.front_b().insert(0, diff);

                x -= 1;
                y -= 1;
            }
        }
    }
}

#[derive(Default)]
pub struct Updates<'mem, 'facet>(
    pub(crate) Interspersed<UpdatesGroup<'mem, 'facet>, Vec<Peek<'mem, 'facet>>>,
);

impl<'mem, 'facet> Updates<'mem, 'facet> {
    /// All `push_*` methods on [`Updates`] push from the front, because the myers' algorithm finds updates back to front.
    pub(crate) fn push_add(&mut self, addition: Peek<'mem, 'facet>) {
        self.0.front_a().push_add(addition);
    }

    /// All `push_*` methods on [`Updates`] push from the front, because the myers' algorithm finds updates back to front.
    pub(crate) fn push_remove(&mut self, removal: Peek<'mem, 'facet>) {
        self.0.front_a().push_remove(removal);
    }

    pub(crate) fn closeness(&self) -> usize {
        self.0.values.iter().map(|(x, _)| x.len()).sum::<usize>()
            + self.0.last.as_ref().map(|x| x.len()).unwrap_or_default()
    }

    /// All `push_*` methods on [`Updates`] push from the front, because the myers' algorithm finds updates back to front.
    fn push_keep(&mut self, value: Peek<'mem, 'facet>) {
        self.0.front_b().insert(0, value);
    }

    fn flatten(&mut self) {
        if let Some(update) = &mut self.0.first {
            update.flatten()
        }

        for (_, update) in &mut self.0.values {
            update.flatten()
        }
    }
}

/// Gets the diff of a sequence by using myers' algorithm
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
            if Diff::new_peek(a[x], b[y]).is_equal() {
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
        } else if Diff::new_peek(a[x - 1], b[y - 1]).is_equal()
            && mem[y - 1][x - 1] <= mem[y][x - 1].min(mem[y - 1][x]) + 1
        {
            updates.push_keep(a[x - 1]);
            x -= 1;
            y -= 1;
        } else if mem[y][x - 1] < mem[y - 1][x] {
            updates.push_remove(a[x - 1]);
            x -= 1;
        } else {
            updates.push_add(b[y - 1]);
            y -= 1;
        }
    }

    updates.flatten();
    updates
}
