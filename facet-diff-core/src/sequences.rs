//! Sequence diff types.
//!
//! These types represent the result of sequence diffing using Myers' algorithm.

use facet_reflect::Peek;

use crate::Diff;

/// An interspersed sequence of A and B values.
/// Pattern: [A?, (B, A)*, B?]
pub struct Interspersed<A, B> {
    /// The first A value (if any)
    pub first: Option<A>,
    /// Pairs of (B, A) values
    pub values: Vec<(B, A)>,
    /// The trailing B value (if any)
    pub last: Option<B>,
}

impl<A, B> Interspersed<A, B> {
    /// Get or insert a default at the front for the A type
    pub fn front_a(&mut self) -> &mut A
    where
        A: Default,
    {
        self.first.get_or_insert_default()
    }

    /// Get or insert a default at the front for the B type
    pub fn front_b(&mut self) -> &mut B
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

/// A group of values being replaced (removals paired with additions).
#[derive(Default)]
pub struct ReplaceGroup<'mem, 'facet> {
    /// The values being removed
    pub removals: Vec<Peek<'mem, 'facet>>,
    /// The values being added
    pub additions: Vec<Peek<'mem, 'facet>>,
}

impl<'mem, 'facet> ReplaceGroup<'mem, 'facet> {
    /// Push an addition at the front
    pub fn push_add(&mut self, addition: Peek<'mem, 'facet>) {
        // Note: The Myers algorithm backtracks and may interleave adds/removes
        // when costs are equal. We handle this by just collecting both.
        self.additions.insert(0, addition);
    }

    /// Push a removal at the front
    pub fn push_remove(&mut self, removal: Peek<'mem, 'facet>) {
        self.removals.insert(0, removal);
    }
}

/// A group of updates containing replace groups interspersed with nested diffs.
#[derive(Default)]
pub struct UpdatesGroup<'mem, 'facet>(
    /// The interspersed structure of replace groups and diffs
    pub Interspersed<ReplaceGroup<'mem, 'facet>, Vec<Diff<'mem, 'facet>>>,
);

impl<'mem, 'facet> UpdatesGroup<'mem, 'facet> {
    /// Push an addition
    pub fn push_add(&mut self, addition: Peek<'mem, 'facet>) {
        self.0.front_a().push_add(addition);
    }

    /// Push a removal
    pub fn push_remove(&mut self, removal: Peek<'mem, 'facet>) {
        self.0.front_a().push_remove(removal);
    }
}

/// Sequence updates: update groups interspersed with unchanged items.
#[derive(Default)]
pub struct Updates<'mem, 'facet>(
    /// The interspersed structure
    pub Interspersed<UpdatesGroup<'mem, 'facet>, Vec<Peek<'mem, 'facet>>>,
);

impl<'mem, 'facet> Updates<'mem, 'facet> {
    /// Push an addition at the front
    ///
    /// All `push_*` methods push from the front, because Myers' algorithm
    /// finds updates back to front.
    pub fn push_add(&mut self, addition: Peek<'mem, 'facet>) {
        self.0.front_a().push_add(addition);
    }

    /// Push a removal at the front
    ///
    /// All `push_*` methods push from the front, because Myers' algorithm
    /// finds updates back to front.
    pub fn push_remove(&mut self, removal: Peek<'mem, 'facet>) {
        self.0.front_a().push_remove(removal);
    }

    /// Returns a measure of how similar the sequences are (higher = more similar)
    pub fn closeness(&self) -> usize {
        self.0.values.iter().map(|(x, _)| x.len()).sum::<usize>()
            + self.0.last.as_ref().map(|x| x.len()).unwrap_or_default()
    }

    /// Push a kept value at the front
    ///
    /// All `push_*` methods push from the front, because Myers' algorithm
    /// finds updates back to front.
    pub fn push_keep(&mut self, value: Peek<'mem, 'facet>) {
        self.0.front_b().insert(0, value);
    }
}
