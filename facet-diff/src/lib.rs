#![warn(missing_docs)]
#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]

mod diff;
mod display;
mod sequences;
mod tree;

pub use diff::Diff;
pub use diff::FacetDiff;
pub use tree::{
    EditOp, Matching, NodeId, NodeKind, Path, PathSegment, Tree, TreeBuilder, tree_diff,
};

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {}
}
