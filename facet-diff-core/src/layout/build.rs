//! Build a Layout from a Diff.
//!
//! This module converts a `Diff<'mem, 'facet>` into a `Layout` that can be rendered.
//!
//! # Architecture
//!
//! The build process has two phases:
//! 1. **Format phase**: Walk the Diff, format all scalar values into FormatArena
//! 2. **Layout phase**: Build LayoutNode tree, group changed attrs, calculate alignment

use std::borrow::Cow;
use std::collections::HashSet;

use facet_core::{StructKind, Type, UserType};
use facet_pretty::PrettyPrinter;
use facet_reflect::Peek;
use indextree::{Arena, NodeId};

use super::{
    Attr, ElementChange, FormatArena, FormattedValue, Layout, LayoutNode, group_changed_attrs,
};
use crate::{Diff, ReplaceGroup, Updates, UpdatesGroup, Value};

/// Options for building a layout from a diff.
#[derive(Clone, Debug)]
pub struct BuildOptions {
    /// Maximum line width for attribute grouping.
    pub max_line_width: usize,
    /// Number of unchanged siblings to keep as context around changes.
    pub context_lines: usize,
    /// Minimum run length to collapse unchanged elements.
    pub collapse_threshold: usize,
}

impl Default for BuildOptions {
    fn default() -> Self {
        Self {
            max_line_width: 80,
            context_lines: 2,
            collapse_threshold: 3,
        }
    }
}

/// Build a Layout from a Diff.
///
/// This is the main entry point for converting a diff into a renderable layout.
pub fn build_layout(diff: &Diff<'_, '_>, opts: &BuildOptions) -> Layout {
    let mut builder = LayoutBuilder::new(opts.clone());
    let root_id = builder.build(diff);
    builder.finish(root_id)
}

/// Internal builder state.
struct LayoutBuilder {
    /// Arena for formatted strings.
    strings: FormatArena,
    /// Arena for layout nodes.
    tree: Arena<LayoutNode>,
    /// Build options.
    opts: BuildOptions,
    /// Pretty printer for formatting values (no colors for arena storage).
    printer: PrettyPrinter,
}

impl LayoutBuilder {
    fn new(opts: BuildOptions) -> Self {
        Self {
            strings: FormatArena::new(),
            tree: Arena::new(),
            opts,
            printer: PrettyPrinter::default()
                .with_colors(false)
                .with_minimal_option_names(true),
        }
    }

    /// Build the layout from a diff, returning the root node ID.
    fn build(&mut self, diff: &Diff<'_, '_>) -> NodeId {
        self.build_diff(diff, ElementChange::None)
    }

    /// Build a node from a diff with a given element change type.
    fn build_diff(&mut self, diff: &Diff<'_, '_>, change: ElementChange) -> NodeId {
        match diff {
            Diff::Equal { value } => {
                // For equal values, render as unchanged text
                if let Some(peek) = value {
                    self.build_peek(*peek, ElementChange::None)
                } else {
                    // No value available, create a placeholder
                    let (span, width) = self.strings.push_str("(equal)");
                    let value = FormattedValue::new(span, width);
                    self.tree.new_node(LayoutNode::Text {
                        value,
                        change: ElementChange::None,
                    })
                }
            }
            Diff::Replace { from, to } => {
                // Create a container element with deleted and inserted children
                let root = self.tree.new_node(LayoutNode::element("_replace"));

                let from_node = self.build_peek(*from, ElementChange::Deleted);
                let to_node = self.build_peek(*to, ElementChange::Inserted);

                root.append(from_node, &mut self.tree);
                root.append(to_node, &mut self.tree);

                root
            }
            Diff::User {
                from,
                to: _,
                variant,
                value,
            } => {
                // Get type name for the tag
                let tag = from.type_identifier;

                match value {
                    Value::Struct {
                        updates,
                        deletions,
                        insertions,
                        unchanged,
                    } => self.build_struct(
                        tag, *variant, updates, deletions, insertions, unchanged, change,
                    ),
                    Value::Tuple { updates } => self.build_tuple(tag, *variant, updates, change),
                }
            }
            Diff::Sequence {
                from: _,
                to: _,
                updates,
            } => self.build_sequence(updates, change),
        }
    }

    /// Build a node from a Peek value.
    fn build_peek(&mut self, peek: Peek<'_, '_>, change: ElementChange) -> NodeId {
        let shape = peek.shape();

        // Check if this is a struct we can recurse into
        match (shape.def, shape.ty) {
            (_, Type::User(UserType::Struct(ty))) if ty.kind == StructKind::Struct => {
                // Build as element with fields as attributes
                if let Ok(struct_peek) = peek.into_struct() {
                    let tag = shape.type_identifier;
                    let mut attrs = Vec::new();

                    for (i, field) in ty.fields.iter().enumerate() {
                        if let Ok(field_value) = struct_peek.field(i) {
                            let formatted_value = self.format_peek(field_value);
                            let attr = match change {
                                ElementChange::None => {
                                    Attr::unchanged(field.name, field.name.len(), formatted_value)
                                }
                                ElementChange::Deleted => {
                                    Attr::deleted(field.name, field.name.len(), formatted_value)
                                }
                                ElementChange::Inserted => {
                                    Attr::inserted(field.name, field.name.len(), formatted_value)
                                }
                                ElementChange::MovedFrom | ElementChange::MovedTo => {
                                    // For moved elements, show fields as unchanged
                                    Attr::unchanged(field.name, field.name.len(), formatted_value)
                                }
                            };
                            attrs.push(attr);
                        }
                    }

                    let changed_groups = group_changed_attrs(&attrs, self.opts.max_line_width, 0);

                    return self.tree.new_node(LayoutNode::Element {
                        tag,
                        attrs,
                        changed_groups,
                        change,
                    });
                }
            }
            _ => {}
        }

        // Default: format as text
        let formatted = self.format_peek(peek);
        self.tree.new_node(LayoutNode::Text {
            value: formatted,
            change,
        })
    }

    /// Build a struct diff as an element with attributes.
    #[allow(clippy::too_many_arguments)]
    fn build_struct(
        &mut self,
        tag: &'static str,
        variant: Option<&'static str>,
        updates: &std::collections::HashMap<Cow<'static, str>, Diff<'_, '_>>,
        deletions: &std::collections::HashMap<Cow<'static, str>, Peek<'_, '_>>,
        insertions: &std::collections::HashMap<Cow<'static, str>, Peek<'_, '_>>,
        unchanged: &HashSet<Cow<'static, str>>,
        change: ElementChange,
    ) -> NodeId {
        // The LayoutNode::Element requires &'static str for tag.
        // The variant is already Option<&'static str> from the Diff type.
        // We can construct a combined tag, but that requires allocation.
        //
        // For now, we use just the type tag. The variant could be shown
        // as a special attribute or in a future LayoutNode redesign.
        let element_tag = tag;

        // If there's a variant, we should indicate it somehow.
        // Options:
        // 1. Change LayoutNode::Element to have an optional variant field
        // 2. Add variant as a special attribute
        // 3. For now: just use the tag and note variant in a TODO
        if variant.is_some() {
            // TODO: LayoutNode::Element should have an optional variant: Option<&'static str>
            // field to properly render enum variants like `Option::Some { ... }`
        }

        let mut attrs = Vec::new();
        let mut child_nodes = Vec::new();

        // Unchanged fields: we only have their names, not values.
        // The Diff type doesn't store unchanged field values - it only tracks what changed.
        // To render unchanged fields, we'd need to restructure the Diff type or pass
        // the original values separately.
        if !unchanged.is_empty() {
            todo!(
                "Cannot render {} unchanged fields: Diff::User::Value::Struct only stores \
                 field names for unchanged fields, not their values. Either:\n\
                 1. Add unchanged field values to Value::Struct, or\n\
                 2. Pass original from/to Peeks to build_struct, or\n\
                 3. Add a LayoutNode variant for 'N unchanged fields' placeholder",
                unchanged.len()
            );
        }

        // Process updates - these become changed attributes or nested children
        let mut sorted_updates: Vec<_> = updates.iter().collect();
        sorted_updates.sort_by(|(a, _), (b, _)| a.cmp(b));

        for (field_name, field_diff) in sorted_updates {
            match field_diff {
                Diff::Replace { from, to } => {
                    // Scalar replacement - show as changed attribute
                    let old_value = self.format_peek(*from);
                    let new_value = self.format_peek(*to);

                    // field_name is Cow<'static, str> - we need &'static str
                    // If it's Borrowed, we can use it directly. If Owned, we have a problem.
                    let name: &'static str = match field_name {
                        Cow::Borrowed(s) => s,
                        Cow::Owned(_) => {
                            todo!(
                                "Field name '{}' is Cow::Owned - need string interning to get &'static str. \
                                 Consider adding a string arena to LayoutBuilder or changing Attr to use Cow<'static, str>",
                                field_name
                            );
                        }
                    };

                    let attr = Attr::changed(name, name.len(), old_value, new_value);
                    attrs.push(attr);
                }
                _ => {
                    // Nested diff - build as child element
                    let child = self.build_diff(field_diff, ElementChange::None);
                    child_nodes.push(child);
                }
            }
        }

        // Process deletions
        let mut sorted_deletions: Vec<_> = deletions.iter().collect();
        sorted_deletions.sort_by(|(a, _), (b, _)| a.cmp(b));

        for (field_name, value) in sorted_deletions {
            let formatted = self.format_peek(*value);
            let name: &'static str = match field_name {
                Cow::Borrowed(s) => s,
                Cow::Owned(_) => {
                    todo!(
                        "Field name '{}' is Cow::Owned - need string interning",
                        field_name
                    );
                }
            };
            let attr = Attr::deleted(name, name.len(), formatted);
            attrs.push(attr);
        }

        // Process insertions
        let mut sorted_insertions: Vec<_> = insertions.iter().collect();
        sorted_insertions.sort_by(|(a, _), (b, _)| a.cmp(b));

        for (field_name, value) in sorted_insertions {
            let formatted = self.format_peek(*value);
            let name: &'static str = match field_name {
                Cow::Borrowed(s) => s,
                Cow::Owned(_) => {
                    todo!(
                        "Field name '{}' is Cow::Owned - need string interning",
                        field_name
                    );
                }
            };
            let attr = Attr::inserted(name, name.len(), formatted);
            attrs.push(attr);
        }

        // Group changed attributes for alignment
        let changed_groups = group_changed_attrs(&attrs, self.opts.max_line_width, 0);

        // Create the element node
        let node = self.tree.new_node(LayoutNode::Element {
            tag: element_tag,
            attrs,
            changed_groups,
            change,
        });

        // Add children
        for child in child_nodes {
            node.append(child, &mut self.tree);
        }

        node
    }

    /// Build a tuple diff.
    fn build_tuple(
        &mut self,
        tag: &'static str,
        variant: Option<&'static str>,
        updates: &Updates<'_, '_>,
        change: ElementChange,
    ) -> NodeId {
        // Same variant issue as build_struct
        if variant.is_some() {
            // TODO: LayoutNode::Element should support variant display
        }

        // Create element for the tuple
        let node = self.tree.new_node(LayoutNode::Element {
            tag,
            attrs: Vec::new(),
            changed_groups: Vec::new(),
            change,
        });

        // Build children from updates
        self.build_updates_children(node, updates);

        node
    }

    /// Build a sequence diff.
    fn build_sequence(&mut self, updates: &Updates<'_, '_>, change: ElementChange) -> NodeId {
        // Create element for the sequence
        let node = self.tree.new_node(LayoutNode::Element {
            tag: "_seq",
            attrs: Vec::new(),
            changed_groups: Vec::new(),
            change,
        });

        // Build children from updates
        self.build_updates_children(node, updates);

        node
    }

    /// Build children from an Updates structure and append to parent.
    fn build_updates_children(&mut self, parent: NodeId, updates: &Updates<'_, '_>) {
        let interspersed = &updates.0;

        // Process first update group if present
        if let Some(update_group) = &interspersed.first {
            self.build_updates_group_children(parent, update_group);
        }

        // Process interleaved (unchanged, update) pairs
        for (unchanged_items, update_group) in &interspersed.values {
            // Add collapsed or individual unchanged items
            let count = unchanged_items.len();
            if count > self.opts.collapse_threshold {
                // Collapse into a single node
                let collapsed = self.tree.new_node(LayoutNode::collapsed(count));
                parent.append(collapsed, &mut self.tree);
            } else {
                // Show individual unchanged items
                for item in unchanged_items {
                    let child = self.build_peek(*item, ElementChange::None);
                    parent.append(child, &mut self.tree);
                }
            }

            self.build_updates_group_children(parent, update_group);
        }

        // Process trailing unchanged items
        if let Some(unchanged_items) = &interspersed.last {
            let count = unchanged_items.len();
            if count > self.opts.collapse_threshold {
                let collapsed = self.tree.new_node(LayoutNode::collapsed(count));
                parent.append(collapsed, &mut self.tree);
            } else {
                for item in unchanged_items {
                    let child = self.build_peek(*item, ElementChange::None);
                    parent.append(child, &mut self.tree);
                }
            }
        }
    }

    /// Build children from an UpdatesGroup and append to parent.
    fn build_updates_group_children(&mut self, parent: NodeId, group: &UpdatesGroup<'_, '_>) {
        let interspersed = &group.0;

        // Process first replace group if present
        if let Some(replace) = &interspersed.first {
            self.build_replace_group_children(parent, replace);
        }

        // Process interleaved (diffs, replace) pairs
        for (diffs, replace) in &interspersed.values {
            // Add nested diffs
            for diff in diffs {
                let child = self.build_diff(diff, ElementChange::None);
                parent.append(child, &mut self.tree);
            }

            self.build_replace_group_children(parent, replace);
        }

        // Process trailing diffs
        if let Some(diffs) = &interspersed.last {
            for diff in diffs {
                let child = self.build_diff(diff, ElementChange::None);
                parent.append(child, &mut self.tree);
            }
        }
    }

    /// Build children from a ReplaceGroup and append to parent.
    fn build_replace_group_children(&mut self, parent: NodeId, group: &ReplaceGroup<'_, '_>) {
        // Add removals as deleted
        for removal in &group.removals {
            let child = self.build_peek(*removal, ElementChange::Deleted);
            parent.append(child, &mut self.tree);
        }

        // Add additions as inserted
        for addition in &group.additions {
            let child = self.build_peek(*addition, ElementChange::Inserted);
            parent.append(child, &mut self.tree);
        }
    }

    /// Format a Peek value into the arena.
    fn format_peek(&mut self, peek: Peek<'_, '_>) -> FormattedValue {
        let formatted = self.printer.format_peek(peek);
        let (span, width) = self.strings.push_str(&formatted);
        FormattedValue::new(span, width)
    }

    /// Finish building and return the Layout.
    fn finish(self, root: NodeId) -> Layout {
        Layout {
            strings: self.strings,
            tree: self.tree,
            root,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::render::{RenderOptions, render_to_string};

    #[test]
    fn test_build_equal_diff() {
        let value = 42i32;
        let peek = Peek::new(&value);
        let diff = Diff::Equal { value: Some(peek) };

        let layout = build_layout(&diff, &BuildOptions::default());

        // Should produce a single text node
        let root = layout.get(layout.root).unwrap();
        assert!(matches!(root, LayoutNode::Text { .. }));
    }

    #[test]
    fn test_build_replace_diff() {
        let from = 10i32;
        let to = 20i32;
        let diff = Diff::Replace {
            from: Peek::new(&from),
            to: Peek::new(&to),
        };

        let layout = build_layout(&diff, &BuildOptions::default());

        // Should produce an element with two children
        let root = layout.get(layout.root).unwrap();
        assert!(matches!(
            root,
            LayoutNode::Element {
                tag: "_replace",
                ..
            }
        ));

        let children: Vec<_> = layout.children(layout.root).collect();
        assert_eq!(children.len(), 2);
    }

    #[test]
    fn test_build_and_render_replace() {
        let from = 10i32;
        let to = 20i32;
        let diff = Diff::Replace {
            from: Peek::new(&from),
            to: Peek::new(&to),
        };

        let layout = build_layout(&diff, &BuildOptions::default());
        let output = render_to_string(&layout, &RenderOptions::plain());

        // Should contain both values with appropriate markers
        assert!(
            output.contains("10"),
            "output should contain old value: {}",
            output
        );
        assert!(
            output.contains("20"),
            "output should contain new value: {}",
            output
        );
    }

    #[test]
    fn test_build_options_default() {
        let opts = BuildOptions::default();
        assert_eq!(opts.max_line_width, 80);
        assert_eq!(opts.context_lines, 2);
        assert_eq!(opts.collapse_threshold, 3);
    }
}
