//! # Cinereus
//!
//! GumTree-style tree diffing with Chawathe edit script generation.
//!
//! Named after *Phascolarctos cinereus* (the koala), which lives in gum trees.
//!
//! ## Algorithm Overview
//!
//! Cinereus implements a tree diff algorithm based on:
//! - **GumTree** (Falleri et al., ASE 2014) for node matching
//! - **Chawathe algorithm** (1996) for edit script generation
//!
//! The algorithm works in phases:
//!
//! 1. **Top-down matching**: Match identical subtrees by hash (Merkle-tree style)
//! 2. **Bottom-up matching**: Match remaining nodes by structural similarity (Dice coefficient)
//! 3. **Edit script generation**: Produce INSERT, DELETE, UPDATE, MOVE operations
//! 4. **Simplification**: Consolidate redundant operations (e.g., subtree moves)
//!
//! ## Usage
//!
//! ```ignore
//! use cinereus::{Tree, tree_diff};
//!
//! // Build trees from your data structure
//! let tree_a = Tree::build(/* ... */);
//! let tree_b = Tree::build(/* ... */);
//!
//! // Compute the diff
//! let edit_script = tree_diff(&tree_a, &tree_b);
//!
//! for op in edit_script {
//!     println!("{:?}", op);
//! }
//! ```

#![warn(missing_docs)]
#![warn(clippy::std_instead_of_core)]

pub use indextree;

mod chawathe;
/// GumTree matching algorithm
pub mod matching;
mod simplify;
/// Tree representation with properties support
pub mod tree;

pub use chawathe::*;
pub use matching::*;
pub use simplify::*;
pub use tree::*;

use core::hash::Hash;
use facet_core::Facet;

/// Compute a simplified diff between two trees.
///
/// This is the main entry point for tree diffing. It:
/// 1. Computes a matching between nodes using GumTree's two-phase algorithm
/// 2. Generates an edit script using Chawathe's algorithm
/// 3. Simplifies the script to remove redundant operations
///
/// # Example
///
/// ```
/// use cinereus::{Tree, NodeData, diff_trees, MatchingConfig};
///
/// let mut tree_a: Tree<&str, String> = Tree::new(NodeData::new(100, "root"));
/// tree_a.add_child(tree_a.root, NodeData::leaf(1, "leaf", "hello".to_string()));
///
/// let mut tree_b: Tree<&str, String> = Tree::new(NodeData::new(100, "root"));
/// tree_b.add_child(tree_b.root, NodeData::leaf(2, "leaf", "world".to_string()));
///
/// let ops = diff_trees(&tree_a, &tree_b, &MatchingConfig::default());
/// // ops contains the edit operations to transform tree_a into tree_b
/// ```
pub fn diff_trees<'a, K, L, P>(
    tree_a: &'a Tree<K, L, P>,
    tree_b: &'a Tree<K, L, P>,
    config: &MatchingConfig,
) -> Vec<EditOp<K, L, P>>
where
    K: Clone + Eq + Hash + Send + Sync + Facet<'a>,
    L: Clone + Eq + Send + Sync + Facet<'a>,
    P: tree::Properties + Send + Sync,
{
    let (ops, _matching) = diff_trees_with_matching(tree_a, tree_b, config);
    ops
}

/// Like [`diff_trees`], but also returns the node matching.
///
/// This is useful when you need to translate NodeId-based operations
/// into path-based operations, as you need to track which nodes in
/// tree_a correspond to nodes in tree_b.
pub fn diff_trees_with_matching<'a, K, L, P>(
    tree_a: &'a Tree<K, L, P>,
    tree_b: &'a Tree<K, L, P>,
    config: &MatchingConfig,
) -> (Vec<EditOp<K, L, P>>, Matching)
where
    K: Clone + Eq + Hash + Send + Sync + Facet<'a>,
    L: Clone + Eq + Send + Sync + Facet<'a>,
    P: tree::Properties + Send + Sync,
{
    let matching = compute_matching(tree_a, tree_b, config);
    let ops = generate_edit_script(tree_a, tree_b, &matching);
    let ops = simplify_edit_script(ops, tree_a, tree_b);
    (ops, matching)
}
