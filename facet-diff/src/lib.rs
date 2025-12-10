#![warn(missing_docs)]
#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]

mod diff;
mod sequences;
mod tree;

pub use diff::{
    DiffFormat, DiffOptions, FacetDiff, LeafChange, LeafChangeKind, collect_leaf_changes,
    diff_new_peek, diff_new_peek_with_options, format_diff, format_diff_compact,
    format_diff_compact_plain, format_diff_default,
};
pub use tree::{EditOp, FacetTree, NodeKind, NodeLabel, build_tree, tree_diff};

// Re-export cinereus types for advanced usage
pub use cinereus::{Matching, MatchingConfig};

// Re-export core types from facet-diff-core
pub use facet_diff_core::{
    ChangeKind, Diff, DiffSymbols, DiffTheme, Interspersed, Path, PathSegment, ReplaceGroup,
    Updates, UpdatesGroup, Value,
};

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {}
}
