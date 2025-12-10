#![warn(missing_docs)]
#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]

mod diff;
mod display;
mod sequences;
mod tree;

pub use diff::{Diff, DiffFormat, FacetDiff, LeafChange, LeafChangeKind};
pub use sequences::{Interspersed, ReplaceGroup, Updates, UpdatesGroup};
pub use tree::{EditOp, FacetTree, NodeKind, NodeLabel, Path, PathSegment, build_tree, tree_diff};

// Re-export cinereus types for advanced usage
pub use cinereus::{Matching, MatchingConfig};

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {}
}
