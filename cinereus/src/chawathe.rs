//! Chawathe edit script generation algorithm.
//!
//! Generates a minimal edit script (INSERT, DELETE, UPDATE, MOVE) from a node matching.
//! Based on "Change Detection in Hierarchically Structured Information" (Chawathe et al., 1996).
//!
//! The algorithm has 5 phases:
//! 1. UPDATE: Change labels of matched nodes where values differ
//! 2. ALIGN: Reorder children to match destination order
//! 3. INSERT: Add nodes that exist only in the destination tree
//! 4. MOVE: Relocate nodes to new parents
//! 5. DELETE: Remove nodes that exist only in the source tree

use facet::Facet;
#[cfg(any(test, feature = "tracing"))]
use tracing::debug;

#[cfg(not(any(test, feature = "tracing")))]
macro_rules! debug {
    ($($arg:tt)*) => {};
}

use crate::matching::Matching;
use crate::tree::{NoProperties, Properties, Tree};
use core::fmt;
use core::hash::Hash;
use facet_pretty::FacetPretty;
use indextree::NodeId;

/// An edit operation in the diff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditOp<K, L, P: Properties = NoProperties> {
    /// Update a node's label (value changed but structure same).
    Update {
        /// The node in tree A that was updated
        node_a: NodeId,
        /// The corresponding node in tree B
        node_b: NodeId,
        /// Old label
        old_label: Option<L>,
        /// New label
        new_label: Option<L>,
    },

    /// Update a single property on a matched node.
    UpdateProperty {
        /// The node in tree A
        node_a: NodeId,
        /// The corresponding node in tree B
        node_b: NodeId,
        /// The property key
        key: P::Key,
        /// Old value (None if property was added)
        old_value: Option<P::Value>,
        /// New value (None if property was removed)
        new_value: Option<P::Value>,
    },

    /// Insert a new node.
    Insert {
        /// The new node in tree B
        node_b: NodeId,
        /// Parent in tree B
        parent_b: NodeId,
        /// Position among siblings (0-indexed)
        position: usize,
        /// The node's kind
        kind: K,
        /// The node's label
        label: Option<L>,
    },

    /// Delete a node.
    Delete {
        /// The node in tree A being deleted
        node_a: NodeId,
    },

    /// Move a node to a new location.
    Move {
        /// The node in tree A
        node_a: NodeId,
        /// The corresponding node in tree B
        node_b: NodeId,
        /// New parent in tree B
        new_parent_b: NodeId,
        /// New position among siblings
        new_position: usize,
    },
}

impl<K: fmt::Debug, L: fmt::Debug, P: Properties> fmt::Display for EditOp<K, L, P>
where
    P::Key: fmt::Debug,
    P::Value: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EditOp::Update { node_a, node_b, .. } => {
                write!(
                    f,
                    "Update(a:{} → b:{})",
                    usize::from(*node_a),
                    usize::from(*node_b)
                )
            }
            EditOp::UpdateProperty {
                node_a,
                key,
                new_value,
                ..
            } => {
                write!(
                    f,
                    "UpdateProp(a:{}, {:?}={:?})",
                    usize::from(*node_a),
                    key,
                    new_value
                )
            }
            EditOp::Insert {
                node_b,
                parent_b,
                position,
                kind,
                ..
            } => {
                write!(
                    f,
                    "Insert(b:{} {:?} @{} under b:{})",
                    usize::from(*node_b),
                    kind,
                    position,
                    usize::from(*parent_b)
                )
            }
            EditOp::Delete { node_a } => {
                write!(f, "Delete(a:{})", usize::from(*node_a))
            }
            EditOp::Move {
                node_a,
                node_b,
                new_parent_b,
                new_position,
            } => {
                write!(
                    f,
                    "Move(a:{} → b:{} @{} under b:{})",
                    usize::from(*node_a),
                    usize::from(*node_b),
                    new_position,
                    usize::from(*new_parent_b)
                )
            }
        }
    }
}

/// Generate an edit script from a matching between two trees.
///
/// The edit script transforms tree A into tree B using INSERT, DELETE, UPDATE, MOVE,
/// and UpdateProperty operations.
pub fn generate_edit_script<'a, K, L, P>(
    tree_a: &'a Tree<K, L, P>,
    tree_b: &'a Tree<K, L, P>,
    matching: &Matching,
) -> Vec<EditOp<K, L, P>>
where
    K: Clone + Eq + Hash + Facet<'a>,
    L: Clone + Eq + Facet<'a>,
    P: Properties,
{
    debug!(matched_pairs = matching.len(), "generate_edit_script start");
    let mut ops = Vec::new();

    // Phase 1: UPDATE - matched nodes where hash differs (content changed)
    // Note: We compare hashes, not labels, since labels may contain paths
    // which differ even when content is identical
    for (a_id, b_id) in matching.pairs() {
        let a_data = tree_a.get(a_id);
        let b_data = tree_b.get(b_id);

        if a_data.hash != b_data.hash {
            debug!(
                a = usize::from(a_id),
                b = usize::from(b_id),
                a_hash = a_data.hash,
                b_hash = b_data.hash,
                "emit UPDATE"
            );
            ops.push(EditOp::Update {
                node_a: a_id,
                node_b: b_id,
                old_label: a_data.label.clone(),
                new_label: b_data.label.clone(),
            });
        }

        // Phase 1b: Property changes - diff properties for matched nodes
        for change in a_data.properties.diff(&b_data.properties) {
            debug!(
                a = usize::from(a_id),
                b = usize::from(b_id),
                "emit UpdateProperty"
            );
            ops.push(EditOp::UpdateProperty {
                node_a: a_id,
                node_b: b_id,
                key: change.key,
                old_value: change.old_value,
                new_value: change.new_value,
            });
        }
    }

    // Phase 2 & 3: INSERT - nodes in B that are not matched
    // Process in breadth-first order so parents are inserted before children
    for b_id in tree_b.iter() {
        if !matching.contains_b(b_id) {
            let b_data = tree_b.get(b_id);
            let parent_b = tree_b.parent(b_id);

            if let Some(parent_b) = parent_b {
                let pos = tree_b.position(b_id);
                let parent_b_data = tree_b.get(parent_b);
                debug!(
                    b = usize::from(b_id),
                    b_kind = %b_data.kind.pretty(),
                    b_label = %b_data.label.pretty(),
                    parent = usize::from(parent_b),
                    parent_kind = %parent_b_data.kind.pretty(),
                    parent_label = %parent_b_data.label.pretty(),
                    pos,
                    "emit INSERT"
                );
                ops.push(EditOp::Insert {
                    node_b: b_id,
                    parent_b,
                    position: pos,
                    kind: b_data.kind.clone(),
                    label: b_data.label.clone(),
                });
            }
            // Root insertion is a special case - usually trees have matching roots
        }
    }

    // Phase 4: MOVE - matched nodes where parent or position changed
    for (a_id, b_id) in matching.pairs() {
        // Skip root
        let Some(parent_a) = tree_a.parent(a_id) else {
            continue;
        };
        let Some(parent_b) = tree_b.parent(b_id) else {
            continue;
        };

        // Check if parent changed
        let parent_match = matching.get_b(parent_a);
        let parent_changed = parent_match != Some(parent_b);

        // Check if position among siblings changed
        let pos_a = tree_a.position(a_id);
        let pos_b = tree_b.position(b_id);
        let position_changed = pos_a != pos_b;

        if parent_changed || position_changed {
            debug!(
                a = usize::from(a_id),
                b = usize::from(b_id),
                parent_changed,
                pos_a,
                pos_b,
                "emit MOVE"
            );
            ops.push(EditOp::Move {
                node_a: a_id,
                node_b: b_id,
                new_parent_b: parent_b,
                new_position: pos_b,
            });
        }
    }

    // Phase 5: DELETE - nodes in A that are not matched
    // Process in post-order so children are deleted before parents
    for a_id in tree_a.post_order() {
        if !matching.contains_a(a_id) {
            let a_data = tree_a.get(a_id);
            debug!(
                a = usize::from(a_id),
                a_kind = %a_data.kind.pretty(),
                a_label = %a_data.label.pretty(),
                "emit DELETE"
            );
            ops.push(EditOp::Delete { node_a: a_id });
        }
    }

    debug!(total_ops = ops.len(), "generate_edit_script done");
    ops
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::matching::MatchingConfig;
    use crate::matching::compute_matching;
    use crate::tree::{NodeData, PropertyChange};
    use facet::Facet;
    use facet_testhelpers::test;

    #[test]
    fn test_no_changes() {
        let mut tree_a: Tree<&str, String> = Tree::new(NodeData::new(100, "root"));
        tree_a.add_child(tree_a.root, NodeData::leaf(1, "leaf", "a".to_string()));

        let mut tree_b: Tree<&str, String> = Tree::new(NodeData::new(100, "root"));
        tree_b.add_child(tree_b.root, NodeData::leaf(1, "leaf", "a".to_string()));

        let matching = compute_matching(&tree_a, &tree_b, &MatchingConfig::default());
        let ops = generate_edit_script(&tree_a, &tree_b, &matching);

        assert!(ops.is_empty(), "Identical trees should have no edits");
    }

    #[test]
    fn test_update() {
        let mut tree_a: Tree<&str, String> = Tree::new(NodeData::new(100, "root"));
        tree_a.add_child(tree_a.root, NodeData::leaf(1, "leaf", "old".to_string()));

        let mut tree_b: Tree<&str, String> = Tree::new(NodeData::new(100, "root"));
        tree_b.add_child(tree_b.root, NodeData::leaf(2, "leaf", "new".to_string()));

        let matching = compute_matching(&tree_a, &tree_b, &MatchingConfig::default());
        let ops = generate_edit_script(&tree_a, &tree_b, &matching);

        // Should have an update operation
        let updates: Vec<_> = ops
            .iter()
            .filter(|op| matches!(op, EditOp::Update { .. }))
            .collect();
        assert!(!updates.is_empty(), "Should have update operation");
    }

    #[test]
    fn test_insert() {
        let tree_a: Tree<&str, String> = Tree::new(NodeData::new(100, "root"));

        let mut tree_b: Tree<&str, String> = Tree::new(NodeData::new(100, "root"));
        tree_b.add_child(tree_b.root, NodeData::leaf(1, "leaf", "new".to_string()));

        let matching = compute_matching(&tree_a, &tree_b, &MatchingConfig::default());
        let ops = generate_edit_script(&tree_a, &tree_b, &matching);

        let inserts: Vec<_> = ops
            .iter()
            .filter(|op| matches!(op, EditOp::Insert { .. }))
            .collect();
        assert_eq!(inserts.len(), 1, "Should have one insert operation");
    }

    #[test]
    fn test_delete() {
        let mut tree_a: Tree<&str, String> = Tree::new(NodeData::new(100, "root"));
        tree_a.add_child(tree_a.root, NodeData::leaf(1, "leaf", "old".to_string()));

        let tree_b: Tree<&str, String> = Tree::new(NodeData::new(100, "root"));

        let matching = compute_matching(&tree_a, &tree_b, &MatchingConfig::default());
        let ops = generate_edit_script(&tree_a, &tree_b, &matching);

        let deletes: Vec<_> = ops
            .iter()
            .filter(|op| matches!(op, EditOp::Delete { .. }))
            .collect();
        assert_eq!(deletes.len(), 1, "Should have one delete operation");
    }

    #[test]
    fn test_swap_two_siblings() {
        // Tree A: root -> [child_a at pos 0, child_b at pos 1]
        // Tree B: root -> [child_b at pos 0, child_a at pos 1]
        // This tests the swap scenario to understand Move semantics

        // Root hashes must differ (otherwise top-down recursively matches children BY POSITION).
        // With min_height=0, leaves are included in top-down and matched by hash.
        let mut tree_a: Tree<&str, &str> = Tree::new(NodeData::new(100, "root"));
        let child_a = tree_a.add_child(tree_a.root, NodeData::leaf(1, "leaf", "A"));
        let child_b = tree_a.add_child(tree_a.root, NodeData::leaf(2, "leaf", "B"));

        let mut tree_b: Tree<&str, &str> = Tree::new(NodeData::new(200, "root")); // Different root hash!
        // Swap order: B first, then A
        let child_b2 = tree_b.add_child(tree_b.root, NodeData::leaf(2, "leaf", "B"));
        let child_a2 = tree_b.add_child(tree_b.root, NodeData::leaf(1, "leaf", "A"));

        let config = MatchingConfig {
            min_height: 0, // Include leaves in top-down matching
            ..Default::default()
        };
        let matching = compute_matching(&tree_a, &tree_b, &config);

        // Debug: print tree structure and matching
        debug!(?tree_a.root, "tree_a root");
        debug!(
            ?child_a,
            hash = tree_a.get(child_a).hash,
            pos = tree_a.position(child_a),
            "tree_a child_a"
        );
        debug!(
            ?child_b,
            hash = tree_a.get(child_b).hash,
            pos = tree_a.position(child_b),
            "tree_a child_b"
        );
        debug!(?tree_b.root, "tree_b root");
        debug!(
            ?child_b2,
            hash = tree_b.get(child_b2).hash,
            pos = tree_b.position(child_b2),
            "tree_b child_b2"
        );
        debug!(
            ?child_a2,
            hash = tree_b.get(child_a2).hash,
            pos = tree_b.position(child_a2),
            "tree_b child_a2"
        );
        for (a, b) in matching.pairs() {
            debug!(?a, ?b, "matching pair");
        }

        // Verify matching is correct
        assert_eq!(
            matching.get_b(child_a),
            Some(child_a2),
            "child_a should match child_a2"
        );
        assert_eq!(
            matching.get_b(child_b),
            Some(child_b2),
            "child_b should match child_b2"
        );

        // Verify positions in original trees
        assert_eq!(tree_a.position(child_a), 0, "child_a at pos 0 in tree_a");
        assert_eq!(tree_a.position(child_b), 1, "child_b at pos 1 in tree_a");
        assert_eq!(tree_b.position(child_a2), 1, "child_a2 at pos 1 in tree_b");
        assert_eq!(tree_b.position(child_b2), 0, "child_b2 at pos 0 in tree_b");

        let ops = generate_edit_script(&tree_a, &tree_b, &matching);

        for op in &ops {
            debug!(?op, "edit script op");
        }

        // Filter move operations
        let moves: Vec<_> = ops
            .iter()
            .filter_map(|op| match op {
                EditOp::Move {
                    node_a,
                    node_b,
                    new_parent_b,
                    new_position,
                } => Some((*node_a, *node_b, *new_parent_b, *new_position)),
                _ => None,
            })
            .collect();

        // Key question: What does cinereus emit for a swap?
        // - Move for child_a: was at pos 0, should be at pos 1
        // - Move for child_b: was at pos 1, should be at pos 0
        //
        // The new_position field comes from tree_b.position(b_id), which is the
        // FINAL position in the target tree, not an intermediate position.

        assert_eq!(moves.len(), 2, "Should have two move operations for a swap");

        // Find move for child_a (hash 1)
        let move_a = moves.iter().find(|(a, _, _, _)| *a == child_a);
        assert!(move_a.is_some(), "Should have move for child_a");
        let (_, _, _, new_pos_a) = move_a.unwrap();
        assert_eq!(*new_pos_a, 1, "child_a should move to position 1");

        // Find move for child_b (hash 2)
        let move_b = moves.iter().find(|(a, _, _, _)| *a == child_b);
        assert!(move_b.is_some(), "Should have move for child_b");
        let (_, _, _, new_pos_b) = move_b.unwrap();
        assert_eq!(*new_pos_b, 0, "child_b should move to position 0");
    }

    /// Test demonstrating the problem with modeling attributes as children.
    ///
    /// When attributes are modeled as child nodes, nodes with identical values
    /// (like Option::None) get cross-matched regardless of their field names.
    ///
    /// Example: `attrs.onscroll: None` matches `attrs.oncontextmenu: None`
    /// because they have the same hash.
    #[test]
    fn test_attribute_cross_matching_problem() {
        // Model a Div element with two None attributes as children
        // This simulates how facet-diff currently builds trees
        //
        // Tree A: Div
        //   ├── id: None (hash = 0, representing Option::None)
        //   └── class: None (hash = 0, same hash!)
        //
        // Tree B: Div
        //   ├── id: "foo" (hash = 123)
        //   └── class: None (hash = 0)
        //
        // CURRENT BEHAVIOR: id:None might match class:None (same hash)
        // DESIRED BEHAVIOR: id:None should match id:"foo" (same field)

        let mut tree_a: Tree<&str, &str> = Tree::new(NodeData::new(100, "div"));
        let id_a = tree_a.add_child(tree_a.root, NodeData::leaf(0, "option", "None")); // id: None
        let class_a = tree_a.add_child(tree_a.root, NodeData::leaf(0, "option", "None")); // class: None

        let mut tree_b: Tree<&str, &str> = Tree::new(NodeData::new(200, "div"));
        let id_b = tree_b.add_child(tree_b.root, NodeData::leaf(123, "option", "foo")); // id: "foo"
        let class_b = tree_b.add_child(tree_b.root, NodeData::leaf(0, "option", "None")); // class: None

        let config = MatchingConfig {
            min_height: 0,
            ..Default::default()
        };
        let matching = compute_matching(&tree_a, &tree_b, &config);

        // Log what got matched
        debug!("id_a={:?}, class_a={:?}", id_a, class_a);
        debug!("id_b={:?}, class_b={:?}", id_b, class_b);
        for (a, b) in matching.pairs() {
            debug!("matched: {:?} -> {:?}", a, b);
        }

        // CURRENT (BROKEN) BEHAVIOR:
        // - id_a (None) matches class_b (None) because same hash
        // - class_a (None) is orphaned or matches something random
        // - id_b ("foo") is unmatched → Insert
        // - One of the Nones is unmatched → Delete
        //
        // This results in Insert + Delete instead of Update for the id field!

        // Check what actually got matched
        let id_a_match = matching.get_b(id_a);
        let class_a_match = matching.get_b(class_a);

        debug!("id_a matched to: {:?}", id_a_match);
        debug!("class_a matched to: {:?}", class_a_match);

        // The problem: with identical hashes, we can't guarantee correct matching
        // One of these assertions will likely fail or show cross-matching:
        //
        // DESIRED: id_a should match id_b (same logical field)
        // DESIRED: class_a should match class_b (same logical field)
        //
        // But without field name information in the hash, cinereus can't know this.

        // For now, just document that the current behavior is problematic
        let ops = generate_edit_script(&tree_a, &tree_b, &matching);

        debug!("Edit ops:");
        for op in &ops {
            debug!("  {:?}", op);
        }

        // Count the ops - with cross-matching we get Insert+Delete instead of Update
        let updates = ops
            .iter()
            .filter(|op| matches!(op, EditOp::Update { .. }))
            .count();
        let inserts = ops
            .iter()
            .filter(|op| matches!(op, EditOp::Insert { .. }))
            .count();
        let deletes = ops
            .iter()
            .filter(|op| matches!(op, EditOp::Delete { .. }))
            .count();

        debug!(
            "updates={}, inserts={}, deletes={}",
            updates, inserts, deletes
        );

        // IDEAL: 1 update (id: None -> "foo"), 0 inserts, 0 deletes
        // ACTUAL: likely 1 insert, 1 delete, maybe 1 update
        // This test documents the problem - it may pass or fail depending on
        // which None gets matched to which.
    }

    /// Test properties implementation for HTML-like attributes
    #[derive(Debug, Clone, PartialEq, Eq, Facet)]
    struct HtmlAttrs {
        id: Option<String>,
        class: Option<String>,
    }

    impl HtmlAttrs {
        fn new() -> Self {
            Self {
                id: None,
                class: None,
            }
        }

        fn with_id(mut self, id: &str) -> Self {
            self.id = Some(id.to_string());
            self
        }

        fn with_class(mut self, class: &str) -> Self {
            self.class = Some(class.to_string());
            self
        }
    }

    impl Properties for HtmlAttrs {
        type Key = &'static str;
        type Value = String;

        fn similarity(&self, other: &Self) -> f64 {
            let mut matches = 0;
            let mut total = 0;

            // Compare id
            if self.id.is_some() || other.id.is_some() {
                total += 1;
                if self.id == other.id {
                    matches += 1;
                }
            }

            // Compare class
            if self.class.is_some() || other.class.is_some() {
                total += 1;
                if self.class == other.class {
                    matches += 1;
                }
            }

            if total == 0 {
                1.0
            } else {
                matches as f64 / total as f64
            }
        }

        fn diff(&self, other: &Self) -> Vec<PropertyChange<Self::Key, Self::Value>> {
            let mut changes = vec![];

            if self.id != other.id {
                changes.push(PropertyChange {
                    key: "id",
                    old_value: self.id.clone(),
                    new_value: other.id.clone(),
                });
            }

            if self.class != other.class {
                changes.push(PropertyChange {
                    key: "class",
                    old_value: self.class.clone(),
                    new_value: other.class.clone(),
                });
            }

            changes
        }

        fn is_empty(&self) -> bool {
            self.id.is_none() && self.class.is_none()
        }
    }

    #[test]
    fn test_properties_emit_update_property_ops() {
        // Tree A: root -> div (id="foo", class=None)
        let mut tree_a: Tree<&str, String, HtmlAttrs> =
            Tree::new(NodeData::with_properties(0, "root", HtmlAttrs::new()));
        let div_a = tree_a.add_child(
            tree_a.root,
            NodeData::with_properties(1, "div", HtmlAttrs::new().with_id("foo")),
        );

        // Tree B: root -> div (id="bar", class="container")
        // Same structure, different properties
        let mut tree_b: Tree<&str, String, HtmlAttrs> =
            Tree::new(NodeData::with_properties(0, "root", HtmlAttrs::new()));
        let div_b = tree_b.add_child(
            tree_b.root,
            NodeData::with_properties(
                2, // Different hash (properties differ)
                "div",
                HtmlAttrs::new().with_id("bar").with_class("container"),
            ),
        );

        // Match trees
        let matching = compute_matching(&tree_a, &tree_b, &MatchingConfig::default());

        // The divs should match (same kind, same position)
        assert!(matching.contains_a(div_a), "div_a should be matched");
        assert_eq!(
            matching.get_b(div_a),
            Some(div_b),
            "div_a should match div_b"
        );

        // Generate edit script
        let ops = generate_edit_script(&tree_a, &tree_b, &matching);

        // Should get UpdateProperty ops, NOT Insert+Delete
        let update_property_ops: Vec<_> = ops
            .iter()
            .filter(|op| matches!(op, EditOp::UpdateProperty { .. }))
            .collect();

        let insert_ops: Vec<_> = ops
            .iter()
            .filter(|op| matches!(op, EditOp::Insert { .. }))
            .collect();

        let delete_ops: Vec<_> = ops
            .iter()
            .filter(|op| matches!(op, EditOp::Delete { .. }))
            .collect();

        debug!("All ops: {:#?}", ops);

        // We should have 2 UpdateProperty ops (id changed, class added)
        assert_eq!(
            update_property_ops.len(),
            2,
            "Expected 2 UpdateProperty ops for id and class changes, got {:?}",
            update_property_ops
        );

        // We should NOT have Insert or Delete ops
        assert!(
            insert_ops.is_empty(),
            "Should not have Insert ops, got {:?}",
            insert_ops
        );
        assert!(
            delete_ops.is_empty(),
            "Should not have Delete ops, got {:?}",
            delete_ops
        );

        // Verify the specific property changes
        let id_change = update_property_ops
            .iter()
            .find(|op| matches!(op, EditOp::UpdateProperty { key: "id", .. }));
        assert!(id_change.is_some(), "Should have UpdateProperty for 'id'");

        let class_change = update_property_ops
            .iter()
            .find(|op| matches!(op, EditOp::UpdateProperty { key: "class", .. }));
        assert!(
            class_change.is_some(),
            "Should have UpdateProperty for 'class'"
        );

        // Verify the values
        if let Some(EditOp::UpdateProperty {
            old_value,
            new_value,
            ..
        }) = id_change
        {
            assert_eq!(old_value, &Some("foo".to_string()));
            assert_eq!(new_value, &Some("bar".to_string()));
        }

        if let Some(EditOp::UpdateProperty {
            old_value,
            new_value,
            ..
        }) = class_change
        {
            assert_eq!(old_value, &None);
            assert_eq!(new_value, &Some("container".to_string()));
        }
    }

    #[test]
    fn test_properties_no_cross_matching() {
        // This test verifies that we don't have the cross-matching problem
        // when properties are NOT tree children.
        //
        // The old approach modeled attributes as tree children:
        //   div -> [id: None, class: None, onclick: None, ...]
        //
        // This caused None values to cross-match (they all hash the same).
        //
        // With properties, each attribute stays with its key, so
        // id=None in tree_a maps to id=Some("x") in tree_b correctly.

        // Tree A: root -> div (id=None, class=None)
        let mut tree_a: Tree<&str, String, HtmlAttrs> =
            Tree::new(NodeData::with_properties(0, "root", HtmlAttrs::new()));
        let _div_a = tree_a.add_child(
            tree_a.root,
            NodeData::with_properties(1, "div", HtmlAttrs::new()), // Both None
        );

        // Tree B: root -> div (id="myid", class=None)
        let mut tree_b: Tree<&str, String, HtmlAttrs> =
            Tree::new(NodeData::with_properties(0, "root", HtmlAttrs::new()));
        let _div_b = tree_b.add_child(
            tree_b.root,
            NodeData::with_properties(2, "div", HtmlAttrs::new().with_id("myid")),
        );

        let matching = compute_matching(&tree_a, &tree_b, &MatchingConfig::default());
        let ops = generate_edit_script(&tree_a, &tree_b, &matching);

        // Should get exactly 1 UpdateProperty op for id
        let update_property_ops: Vec<_> = ops
            .iter()
            .filter(|op| matches!(op, EditOp::UpdateProperty { .. }))
            .collect();

        assert_eq!(
            update_property_ops.len(),
            1,
            "Expected 1 UpdateProperty op for id, got {:?}",
            update_property_ops
        );

        // class stayed None, so no change for it
        let class_change = update_property_ops
            .iter()
            .find(|op| matches!(op, EditOp::UpdateProperty { key: "class", .. }));
        assert!(
            class_change.is_none(),
            "Should NOT have UpdateProperty for 'class' since it didn't change"
        );
    }
}
