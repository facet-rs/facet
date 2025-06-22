use facet_reflect::Peek;

use crate::Diff;

#[derive(Default)]
pub struct UpdatesGroup<'mem, 'facet> {
    pub(crate) removals: Vec<Peek<'mem, 'facet, 'static>>,
    pub(crate) additions: Vec<Peek<'mem, 'facet, 'static>>,

    /// For now, this is only used when there is a single removal and addition
    pub(crate) updates: Option<Box<Diff<'mem, 'facet>>>,
}

impl<'mem, 'facet> UpdatesGroup<'mem, 'facet> {
    fn push_add(&mut self, addition: Peek<'mem, 'facet, 'static>) {
        assert!(
            self.removals.is_empty(),
            "We want all blocks of updates to have removals first, then additions, this should follow from our implementation of myers' algorithm"
        );
        self.additions.insert(0, addition);
    }

    fn push_remove(&mut self, removal: Peek<'mem, 'facet, 'static>) {
        self.removals.insert(0, removal);
    }

    fn flatten(&mut self) {
        if self.removals.len() == 1 && self.additions.len() == 1 {
            let from = self.removals.pop().unwrap();
            let to = self.additions.pop().unwrap();

            let diff = Diff::new_peek(from, to);

            self.updates = Some(Box::new(diff));
        }
    }
}

#[derive(Default)]
pub struct Updates<'mem, 'facet> {
    pub(crate) first: Option<UpdatesGroup<'mem, 'facet>>,
    pub(crate) values: Vec<(Vec<Peek<'mem, 'facet, 'static>>, UpdatesGroup<'mem, 'facet>)>,
    pub(crate) last: Option<Vec<Peek<'mem, 'facet, 'static>>>,
}

impl<'mem, 'facet> Updates<'mem, 'facet> {
    /// All `push_*` methods on [`Updates`] push from the front, because the myers' algorithm finds updates back to front.
    pub(crate) fn push_add(&mut self, addition: Peek<'mem, 'facet, 'static>) {
        self.first.get_or_insert_default().push_add(addition);
    }

    /// All `push_*` methods on [`Updates`] push from the front, because the myers' algorithm finds updates back to front.
    pub(crate) fn push_remove(&mut self, removal: Peek<'mem, 'facet, 'static>) {
        self.first.get_or_insert_default().push_remove(removal);
    }

    /// All `push_*` methods on [`Updates`] push from the front, because the myers' algorithm finds updates back to front.
    fn push_keep(&mut self, value: Peek<'mem, 'facet, 'static>) {
        if let Some(update) = self.first.take() {
            self.values.insert(0, (vec![value], update));
        } else if let Some((values, _)) = self.values.first_mut() {
            values.insert(0, value);
        } else {
            self.last.get_or_insert_default().insert(0, value);
        }
    }

    fn flatten(&mut self) {
        if let Some(update) = &mut self.first {
            update.flatten()
        }

        for (_, update) in &mut self.values {
            update.flatten()
        }
    }
}

/// Gets the diff of a sequence by using myers' algorithm
pub fn diff<'mem, 'facet>(
    a: Vec<Peek<'mem, 'facet, 'static>>,
    b: Vec<Peek<'mem, 'facet, 'static>>,
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
        } else if x == 1 {
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
