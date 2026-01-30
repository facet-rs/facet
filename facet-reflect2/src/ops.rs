//! Operations for partial value construction.

use facet_core::{PtrConst, Shape};

/// A value to move into the destination.
pub struct Move {
    pub ptr: PtrConst,
    pub shape: &'static Shape,
}

/// Build a value incrementally.
pub struct Build {
    pub len_hint: Option<usize>,
}

/// How to fill a value.
pub enum Source {
    /// Move a complete value from ptr into destination.
    Move(Move),
    /// Build incrementally - pushes a frame.
    Build(Build),
    /// Use the type's default value.
    Default,
}

/// An operation on a Partial.
pub enum Op<'a> {
    /// Set a value at a path relative to the current frame.
    Set { path: &'a [usize], source: Source },
}
