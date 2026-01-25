//! Layout tree nodes.

use std::borrow::Cow;

use indextree::{Arena, NodeId};

use super::{Attr, ChangedGroup, FormatArena, FormattedValue};

/// How an element changed (affects its prefix and color).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum ElementChange {
    /// No change to the element itself (may have changed children/attrs)
    #[default]
    None,
    /// Element was deleted (- prefix, red)
    Deleted,
    /// Element was inserted (+ prefix, green)
    Inserted,
    /// Element was moved from this position (← prefix, blue)
    MovedFrom,
    /// Element was moved to this position (→ prefix, blue)
    MovedTo,
}

impl ElementChange {
    /// Get the prefix character for this change type.
    pub const fn prefix(self) -> Option<char> {
        match self {
            Self::None => None,
            Self::Deleted => Some('-'),
            Self::Inserted => Some('+'),
            Self::MovedFrom => Some('←'),
            Self::MovedTo => Some('→'),
        }
    }

    /// Check if this change type uses any prefix.
    pub const fn has_prefix(self) -> bool {
        !matches!(self, Self::None)
    }
}

/// A node in the layout tree.
#[derive(Clone, Debug)]
pub enum LayoutNode {
    /// An element/struct with attributes and children.
    Element {
        /// Element tag name
        tag: Cow<'static, str>,
        /// Field name if this element is a struct field (e.g., "point" for `point: Inner`)
        field_name: Option<&'static str>,
        /// All attributes (unchanged, changed, deleted, inserted)
        attrs: Vec<Attr>,
        /// Changed attributes grouped for -/+ line alignment
        changed_groups: Vec<ChangedGroup>,
        /// How this element itself changed
        change: ElementChange,
    },

    /// A sequence/array with children.
    Sequence {
        /// How this sequence itself changed
        change: ElementChange,
        /// The type name of items in this sequence (e.g., "i32", "Item")
        item_type: &'static str,
        /// Field name if this sequence is a struct field value (e.g., "elements" for `elements: [...]`)
        field_name: Option<&'static str>,
    },

    /// A collapsed run of unchanged siblings.
    Collapsed {
        /// Number of collapsed elements
        count: usize,
    },

    /// Text content.
    Text {
        /// Formatted text value
        value: FormattedValue,
        /// How this text changed
        change: ElementChange,
    },

    /// A group of items rendered on a single line (for sequences).
    /// Used to group consecutive unchanged/deleted/inserted items.
    ItemGroup {
        /// Formatted values for each visible item in the group.
        items: Vec<FormattedValue>,
        /// How these items changed (all have the same change type).
        change: ElementChange,
        /// Number of additional collapsed items (shown as "...N more").
        /// None means all items are visible.
        collapsed_suffix: Option<usize>,
        /// The type name of items (e.g., "i32", "Item") for XML wrapping.
        item_type: &'static str,
    },
}

impl LayoutNode {
    /// Create an element node with no changes.
    pub fn element(tag: impl Into<Cow<'static, str>>) -> Self {
        Self::Element {
            tag: tag.into(),
            field_name: None,
            attrs: Vec::new(),
            changed_groups: Vec::new(),
            change: ElementChange::None,
        }
    }

    /// Create an element node with a specific change type.
    pub fn element_with_change(tag: impl Into<Cow<'static, str>>, change: ElementChange) -> Self {
        Self::Element {
            tag: tag.into(),
            field_name: None,
            attrs: Vec::new(),
            changed_groups: Vec::new(),
            change,
        }
    }

    /// Create a sequence node.
    pub const fn sequence(change: ElementChange, item_type: &'static str) -> Self {
        Self::Sequence {
            change,
            item_type,
            field_name: None,
        }
    }

    /// Create a collapsed node.
    pub const fn collapsed(count: usize) -> Self {
        Self::Collapsed { count }
    }

    /// Create a text node.
    pub const fn text(value: FormattedValue, change: ElementChange) -> Self {
        Self::Text { value, change }
    }

    /// Create an item group node.
    pub const fn item_group(
        items: Vec<FormattedValue>,
        change: ElementChange,
        collapsed_suffix: Option<usize>,
        item_type: &'static str,
    ) -> Self {
        Self::ItemGroup {
            items,
            change,
            collapsed_suffix,
            item_type,
        }
    }

    /// Get the element change type (if applicable).
    pub const fn change(&self) -> ElementChange {
        match self {
            Self::Element { change, .. } => *change,
            Self::Sequence { change, .. } => *change,
            Self::Collapsed { .. } => ElementChange::None,
            Self::Text { change, .. } => *change,
            Self::ItemGroup { change, .. } => *change,
        }
    }

    /// Check if this node has any actual changes to render.
    pub fn has_changes(&self) -> bool {
        match self {
            Self::Element {
                attrs,
                change,
                changed_groups,
                ..
            } => {
                change.has_prefix()
                    || !changed_groups.is_empty()
                    || attrs.iter().any(|a| {
                        matches!(
                            a.status,
                            super::AttrStatus::Changed { .. }
                                | super::AttrStatus::Deleted { .. }
                                | super::AttrStatus::Inserted { .. }
                        )
                    })
            }
            Self::Sequence { change, .. } => change.has_prefix(),
            Self::Collapsed { .. } => false,
            Self::Text { change, .. } => change.has_prefix(),
            Self::ItemGroup { change, .. } => change.has_prefix(),
        }
    }
}

/// The complete layout ready for rendering.
pub struct Layout {
    /// Formatted strings arena
    pub strings: FormatArena,
    /// Tree of layout nodes
    pub tree: Arena<LayoutNode>,
    /// Root node ID
    pub root: NodeId,
}

impl Layout {
    /// Create a new layout with the given root node.
    pub fn new(strings: FormatArena, mut tree: Arena<LayoutNode>, root_node: LayoutNode) -> Self {
        let root = tree.new_node(root_node);
        Self {
            strings,
            tree,
            root,
        }
    }

    /// Get a reference to a node by ID.
    pub fn get(&self, id: NodeId) -> Option<&LayoutNode> {
        self.tree.get(id).map(|n| n.get())
    }

    /// Get a mutable reference to a node by ID.
    pub fn get_mut(&mut self, id: NodeId) -> Option<&mut LayoutNode> {
        self.tree.get_mut(id).map(|n| n.get_mut())
    }

    /// Add a child to a parent node.
    pub fn add_child(&mut self, parent: NodeId, child_node: LayoutNode) -> NodeId {
        let child = self.tree.new_node(child_node);
        parent.append(child, &mut self.tree);
        child
    }

    /// Iterate over children of a node.
    pub fn children(&self, parent: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        parent.children(&self.tree)
    }

    /// Get the string for a span from the arena.
    pub fn get_string(&self, span: super::Span) -> &str {
        self.strings.get(span)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_element_change_prefix() {
        assert_eq!(ElementChange::None.prefix(), None);
        assert_eq!(ElementChange::Deleted.prefix(), Some('-'));
        assert_eq!(ElementChange::Inserted.prefix(), Some('+'));
        assert_eq!(ElementChange::MovedFrom.prefix(), Some('←'));
        assert_eq!(ElementChange::MovedTo.prefix(), Some('→'));
    }

    #[test]
    fn test_layout_tree() {
        let arena = FormatArena::new();
        let tree = Arena::new();

        let mut layout = Layout::new(arena, tree, LayoutNode::element("root"));

        let child1 = layout.add_child(layout.root, LayoutNode::element("child1"));
        let child2 = layout.add_child(layout.root, LayoutNode::element("child2"));

        let children: Vec<_> = layout.children(layout.root).collect();
        assert_eq!(children.len(), 2);
        assert_eq!(children[0], child1);
        assert_eq!(children[1], child2);
    }

    #[test]
    fn test_collapsed_node() {
        let node = LayoutNode::collapsed(5);
        assert!(!node.has_changes());

        if let LayoutNode::Collapsed { count } = node {
            assert_eq!(count, 5);
        } else {
            panic!("expected Collapsed node");
        }
    }
}
