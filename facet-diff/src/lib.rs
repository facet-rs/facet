#![warn(missing_docs)]
#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]

mod tracing_macros;

mod diff;
mod report;
mod sequences;
mod tree;

pub use diff::{
    DiffFormat, DiffOptions, FacetDiff, LeafChange, LeafChangeKind, collect_leaf_changes,
    diff_new_peek, diff_new_peek_with_options, format_diff, format_diff_compact,
    format_diff_compact_plain, format_diff_default,
};
pub use report::DiffReport;
pub use tree::{
    AttributeChange, EditOp, FacetTree, NodeKind, NodeLabel, NodeRef, SimilarityResult, build_tree,
    compute_element_similarity, elements_are_similar, tree_diff,
};

// Re-export cinereus types for advanced usage
pub use cinereus::{Matching, MatchingConfig};

// Re-export matching stats when feature is enabled
#[cfg(feature = "matching-stats")]
pub use tree::{get_matching_stats, reset_matching_stats};

// Re-export core types from facet-diff-core
pub use facet_diff_core::{
    ChangeKind, Diff, DiffSymbols, DiffTheme, Interspersed, Path, PathSegment, ReplaceGroup,
    Updates, UpdatesGroup, Value,
};

// Re-export layout types for custom rendering
pub use facet_diff_core::layout::{
    AnsiBackend, BuildOptions, ColorBackend, DiffFlavor, JsonFlavor, PlainBackend, RenderOptions,
    RustFlavor, XmlFlavor, build_layout, render_to_string,
};

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {}
}
