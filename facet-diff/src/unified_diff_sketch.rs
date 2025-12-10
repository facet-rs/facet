//! SKETCH: Unified diff API that combines tree structure with value diffs.
//!
//! This is a design sketch, not working code yet.
//!
//! The idea: tree diff is the foundation (understands moves, structure),
//! augmented with value diffs for the actual changes.

use std::borrow::Cow;

use facet_reflect::Peek;

/// A path segment describing how to reach a child.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PathSegment {
    Field(Cow<'static, str>),
    Index(usize),
    Key(Cow<'static, str>),
    Variant(Cow<'static, str>),
}

/// A path from root to a node.
#[derive(Debug, Clone, PartialEq, Eq, Default, Hash)]
pub struct Path(pub Vec<PathSegment>);

// ============================================================================
// The unified diff result
// ============================================================================

/// The result of diffing two values.
///
/// This is a list of edit operations that describe how to transform
/// the old value into the new value.
pub struct UnifiedDiff<'mem, 'facet> {
    /// The edit operations, in a sensible order for display.
    pub ops: Vec<EditOp<'mem, 'facet>>,
}

/// A single edit operation in the diff.
#[derive(Debug)]
pub enum EditOp<'mem, 'facet> {
    /// A node was updated (same position, content changed).
    Update {
        path: Path,
        /// The detailed changes within this node
        changes: Vec<LeafChange<'mem, 'facet>>,
    },

    /// A node was inserted (exists in new, not in old).
    Insert {
        path: Path,
        /// The inserted value
        value: Peek<'mem, 'facet>,
    },

    /// A node was deleted (exists in old, not in new).
    Delete {
        path: Path,
        /// The deleted value
        value: Peek<'mem, 'facet>,
    },

    /// A node was moved from one location to another.
    Move {
        old_path: Path,
        new_path: Path,
        /// The value that moved (from old tree)
        value: Peek<'mem, 'facet>,
        /// If the value also changed during the move, the changes
        changes: Option<Vec<LeafChange<'mem, 'facet>>>,
    },
}

/// A leaf-level change (scalar value changed).
#[derive(Debug)]
pub struct LeafChange<'mem, 'facet> {
    /// Path relative to the parent EditOp's path
    pub relative_path: Path,
    /// What kind of change
    pub kind: LeafChangeKind<'mem, 'facet>,
}

/// The kind of leaf change.
#[derive(Debug)]
pub enum LeafChangeKind<'mem, 'facet> {
    /// A scalar value was replaced
    Replace {
        from: Peek<'mem, 'facet>,
        to: Peek<'mem, 'facet>,
    },
    /// A field/element was deleted
    Delete { value: Peek<'mem, 'facet> },
    /// A field/element was inserted
    Insert { value: Peek<'mem, 'facet> },
}

// ============================================================================
// Example output for SVG diff
// ============================================================================

/*

Diffing two SVGs where:
- viewBox changed
- A rect's fill changed
- A circle moved from children[1] to children[3]
- A path was deleted
- A group was inserted

UnifiedDiff {
    ops: [
        // Attribute change at root level
        Update {
            path: [],
            changes: [
                LeafChange {
                    relative_path: [Field("view_box")],
                    kind: Replace {
                        from: "0 0 100 100",
                        to: "0 0 200 200",
                    },
                },
            ],
        },

        // Nested change in first child
        Update {
            path: [Field("children"), Index(0)],
            changes: [
                LeafChange {
                    relative_path: [Field("fill")],
                    kind: Replace {
                        from: "red",
                        to: "blue",
                    },
                },
            ],
        },

        // Circle moved (and maybe changed too)
        Move {
            old_path: [Field("children"), Index(1)],
            new_path: [Field("children"), Index(3)],
            value: <Circle>,
            changes: Some([
                LeafChange {
                    relative_path: [Field("r")],
                    kind: Replace { from: "25", to: "30" },
                },
            ]),
        },

        // Path was deleted
        Delete {
            path: [Field("children"), Index(2)],
            value: <Path>,
        },

        // Group was inserted
        Insert {
            path: [Field("children"), Index(2)],
            value: <Group>,
        },
    ],
}

*/

// ============================================================================
// The API
// ============================================================================

/// Trait for computing unified diffs.
pub trait UnifiedDiffExt<'facet> {
    /// Compute a unified diff between self and other.
    fn unified_diff<'a>(&'a self, other: &'a impl facet::Facet<'facet>) -> UnifiedDiff<'a, 'facet>;
}

// Implementation would:
// 1. Build trees for both values (like current tree.rs)
// 2. Run cinereus diff to get structural ops (moves, inserts, deletes, updates)
// 3. For each Update/Move, compute the value diff to get LeafChanges
// 4. Return UnifiedDiff with all ops

// ============================================================================
// Display: this is where we'd plug in format-specific rendering
// ============================================================================

/// Trait for rendering diffs in different formats.
pub trait DiffRenderer {
    type Output;

    /// Called for each update operation
    fn render_update(&mut self, path: &Path, changes: &[LeafChange]) -> Self::Output;

    /// Called for each insert operation
    fn render_insert(&mut self, path: &Path, value: Peek) -> Self::Output;

    /// Called for each delete operation
    fn render_delete(&mut self, path: &Path, value: Peek) -> Self::Output;

    /// Called for each move operation
    fn render_move(
        &mut self,
        old_path: &Path,
        new_path: &Path,
        value: Peek,
        changes: Option<&[LeafChange]>,
    ) -> Self::Output;
}

// Then we'd have:
// - TerminalRenderer (current colored output)
// - XmlDiffRenderer (renders as XML with +/- lines)
// - JsonDiffRenderer
// - etc.
