use facet_core::Facet;
use facet_reflect::Peek;

pub enum Diff<'mem, 'facet, 'shape> {
    Equal,
    Replace {
        from: Peek<'mem, 'facet, 'shape>,
        to: Peek<'mem, 'facet, 'shape>,
    },
}

pub trait FacetDiff<'f>: Facet<'f> {
    fn diff<'a, U: Facet<'f>>(&'a self, other: &'a U) -> Diff<'a, 'f, 'static>;
}

impl<'f, T: Facet<'f>> FacetDiff<'f> for T {
    fn diff<'a, U: Facet<'f>>(&'a self, other: &'a U) -> Diff<'a, 'f, 'static> {
        let from = Peek::new(self);
        let to = Peek::new(other);

        if from.shape().id == to.shape().id && from.shape().is_partial_eq() && from == to {
            return Diff::Equal;
        }

        Diff::Replace { from, to }
    }
}
