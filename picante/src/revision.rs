use facet::Facet;

/// A monotonically increasing revision counter.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Facet)]
pub struct Revision(pub u64);
