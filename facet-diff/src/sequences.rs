use facet_reflect::Peek;

#[derive(Debug)]
pub enum Update<'mem, 'facet> {
    Add(Peek<'mem, 'facet, 'static>),
    Remove(Peek<'mem, 'facet, 'static>),
    Keep(Peek<'mem, 'facet, 'static>),
}

/// Gets the diff of a sequence by using myers' algorithm
pub fn diff<'mem, 'facet>(
    a: Vec<Peek<'mem, 'facet, 'static>>,
    b: Vec<Peek<'mem, 'facet, 'static>>,
) -> Vec<Update<'mem, 'facet>> {
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
            if a[x] == b[y] {
                v = v.min(mem[y][x]);
            }

            next.push(v);
        }

        mem.push(next);
    }

    let mut updates = vec![];

    let mut x = a.len();
    let mut y = b.len();
    while x > 0 || y > 0 {
        if y == 0 {
            updates.push(Update::Remove(a[x - 1]));
            x -= 1;
        } else if x == 1 {
            updates.push(Update::Add(b[y - 1]));
            y -= 1;
        } else if a[x - 1] == b[y - 1] && mem[y - 1][x - 1] <= mem[y][x - 1].min(mem[y - 1][x]) + 1
        {
            updates.push(Update::Keep(a[x - 1]));
            x -= 1;
            y -= 1;
        } else if mem[y][x - 1] < mem[y - 1][x] {
            updates.push(Update::Remove(a[x - 1]));
            x -= 1;
        } else {
            updates.push(Update::Add(b[y - 1]));
            y -= 1;
        }
    }

    updates.reverse();
    updates
}
