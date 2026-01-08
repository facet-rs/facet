//! Revision counters used for global invalidation.

use facet::Facet;

// r[revision.type]
// r[revision.monotonic]
/// A monotonically increasing revision counter.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Facet)]
pub struct Revision(pub u64);
