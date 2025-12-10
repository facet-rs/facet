//! Build a Layout from a Diff.
//!
//! This module converts a `Diff<'mem, 'facet>` into a `Layout` that can be rendered.
//!
//! # Architecture
//!
//! The build process walks the Diff tree while simultaneously navigating the original
//! `from` and `to` Peek values. This allows us to:
//! - Look up unchanged field values from the original structs
//! - Decide whether to show unchanged fields or collapse them
//!
//! The Diff itself only stores what changed - the original Peeks provide context.

use std::borrow::Cow;

use facet_core::{StructKind, Type, UserType};
use facet_pretty::PrettyPrinter;
use facet_reflect::Peek;
use indextree::{Arena, NodeId};

use super::{
    Attr, ElementChange, FormatArena, FormattedValue, Layout, LayoutNode, group_changed_attrs,
};
use crate::{Diff, ReplaceGroup, Updates, UpdatesGroup, Value};

/// Maximum number of visible items to show before collapsing with "...N more".
const MAX_VISIBLE_ITEMS: usize = 5;

/// Options for building a layout from a diff.
#[derive(Clone, Debug)]
pub struct BuildOptions {
    /// Maximum line width for attribute grouping.
    pub max_line_width: usize,
    /// Maximum number of unchanged fields to show inline.
    /// If more than this many unchanged fields exist, collapse to "N unchanged".
    pub max_unchanged_fields: usize,
    /// Minimum run length to collapse unchanged sequence elements.
    pub collapse_threshold: usize,
}

impl Default for BuildOptions {
    fn default() -> Self {
        Self {
            max_line_width: 80,
            max_unchanged_fields: 5,
            collapse_threshold: 3,
        }
    }
}

/// Build a Layout from a Diff.
///
/// This is the main entry point for converting a diff into a renderable layout.
///
/// # Arguments
///
/// * `diff` - The diff to render
/// * `from` - The original "from" value (for looking up unchanged fields)
/// * `to` - The original "to" value (for looking up unchanged fields)
/// * `opts` - Build options
pub fn build_layout<'mem, 'facet>(
    diff: &Diff<'mem, 'facet>,
    from: Peek<'mem, 'facet>,
    to: Peek<'mem, 'facet>,
    opts: &BuildOptions,
) -> Layout {
    let mut builder = LayoutBuilder::new(opts.clone());
    let root_id = builder.build(diff, Some(from), Some(to));
    builder.finish(root_id)
}

/// Build a Layout from a Diff without original values.
///
/// Use this when you don't have the original Peek values available.
/// Unchanged fields will be collapsed to "N unchanged" instead of showing values.
pub fn build_layout_without_context(diff: &Diff<'_, '_>, opts: &BuildOptions) -> Layout {
    let mut builder = LayoutBuilder::new(opts.clone());
    let root_id = builder.build(diff, None, None);
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

    /// Build the layout from a diff, with optional context Peeks.
    fn build<'mem, 'facet>(
        &mut self,
        diff: &Diff<'mem, 'facet>,
        from: Option<Peek<'mem, 'facet>>,
        to: Option<Peek<'mem, 'facet>>,
    ) -> NodeId {
        self.build_diff(diff, from, to, ElementChange::None)
    }

    /// Build a node from a diff with a given element change type.
    fn build_diff<'mem, 'facet>(
        &mut self,
        diff: &Diff<'mem, 'facet>,
        from: Option<Peek<'mem, 'facet>>,
        to: Option<Peek<'mem, 'facet>>,
        change: ElementChange,
    ) -> NodeId {
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
                from: from_shape,
                to: _to_shape,
                variant,
                value,
            } => {
                // Get type name for the tag
                let tag = from_shape.type_identifier;

                match value {
                    Value::Struct {
                        updates,
                        deletions,
                        insertions,
                        unchanged,
                    } => self.build_struct(
                        tag, *variant, updates, deletions, insertions, unchanged, from, to, change,
                    ),
                    Value::Tuple { updates } => {
                        self.build_tuple(tag, *variant, updates, from, to, change)
                    }
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
    fn build_struct<'mem, 'facet>(
        &mut self,
        tag: &'static str,
        variant: Option<&'static str>,
        updates: &std::collections::HashMap<Cow<'static, str>, Diff<'mem, 'facet>>,
        deletions: &std::collections::HashMap<Cow<'static, str>, Peek<'mem, 'facet>>,
        insertions: &std::collections::HashMap<Cow<'static, str>, Peek<'mem, 'facet>>,
        unchanged: &std::collections::HashSet<Cow<'static, str>>,
        from: Option<Peek<'mem, 'facet>>,
        to: Option<Peek<'mem, 'facet>>,
        change: ElementChange,
    ) -> NodeId {
        let element_tag = tag;

        // If there's a variant, we should indicate it somehow.
        // TODO: LayoutNode::Element should have an optional variant: Option<&'static str>
        if variant.is_some() {
            // For now, just use the tag
        }

        let mut attrs = Vec::new();
        let mut child_nodes = Vec::new();

        // Handle unchanged fields - try to get values from the original Peek
        if !unchanged.is_empty() {
            let unchanged_count = unchanged.len();

            if unchanged_count <= self.opts.max_unchanged_fields {
                // Show unchanged fields with their values (if we have the original Peek)
                if let Some(from_peek) = from {
                    if let Ok(struct_peek) = from_peek.into_struct() {
                        let mut sorted_unchanged: Vec<_> = unchanged.iter().collect();
                        sorted_unchanged.sort();

                        for field_name in sorted_unchanged {
                            if let Ok(field_value) = struct_peek.field_by_name(field_name) {
                                let formatted = self.format_peek(field_value);
                                let name: &'static str = match field_name {
                                    Cow::Borrowed(s) => s,
                                    Cow::Owned(_) => {
                                        // Skip owned field names for now - they come from dynamic values
                                        continue;
                                    }
                                };
                                let attr = Attr::unchanged(name, name.len(), formatted);
                                attrs.push(attr);
                            }
                        }
                    }
                } else {
                    // No original Peek available - add a collapsed placeholder
                    // We'll handle this after building the element
                }
            }
            // If more than max_unchanged_fields, we'll add a collapsed node as a child
        }

        // Process updates - these become changed attributes or nested children
        let mut sorted_updates: Vec<_> = updates.iter().collect();
        sorted_updates.sort_by(|(a, _), (b, _)| a.cmp(b));

        for (field_name, field_diff) in sorted_updates {
            // Navigate into the field in from/to Peeks for nested context
            let field_from = from.and_then(|p| {
                p.into_struct()
                    .ok()
                    .and_then(|s| s.field_by_name(field_name).ok())
            });
            let field_to = to.and_then(|p| {
                p.into_struct()
                    .ok()
                    .and_then(|s| s.field_by_name(field_name).ok())
            });

            match field_diff {
                Diff::Replace { from, to } => {
                    // Scalar replacement - show as changed attribute
                    let old_value = self.format_peek(*from);
                    let new_value = self.format_peek(*to);

                    let name: &'static str = match field_name {
                        Cow::Borrowed(s) => s,
                        Cow::Owned(_) => {
                            // For owned field names, we need to handle them differently
                            // For now, skip - this happens with dynamic values
                            continue;
                        }
                    };

                    let attr = Attr::changed(name, name.len(), old_value, new_value);
                    attrs.push(attr);
                }
                _ => {
                    // Nested diff - build as child element
                    let child =
                        self.build_diff(field_diff, field_from, field_to, ElementChange::None);
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
                Cow::Owned(_) => continue,
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
                Cow::Owned(_) => continue,
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

        // Add collapsed unchanged fields indicator if needed
        let unchanged_count = unchanged.len();
        if unchanged_count > self.opts.max_unchanged_fields
            || (unchanged_count > 0 && from.is_none())
        {
            let collapsed = self.tree.new_node(LayoutNode::collapsed(unchanged_count));
            node.append(collapsed, &mut self.tree);
        }

        node
    }

    /// Build a tuple diff.
    fn build_tuple<'mem, 'facet>(
        &mut self,
        tag: &'static str,
        variant: Option<&'static str>,
        updates: &Updates<'mem, 'facet>,
        _from: Option<Peek<'mem, 'facet>>,
        _to: Option<Peek<'mem, 'facet>>,
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
    ///
    /// This groups consecutive items by their change type (unchanged, deleted, inserted)
    /// and renders them on single lines with optional collapsing for long runs.
    fn build_updates_children(&mut self, parent: NodeId, updates: &Updates<'_, '_>) {
        // First, collect all items with their change types into a flat list
        let mut items: Vec<(Peek<'_, '_>, ElementChange)> = Vec::new();

        let interspersed = &updates.0;

        // Process first update group if present
        if let Some(update_group) = &interspersed.first {
            self.collect_updates_group_items(&mut items, update_group);
        }

        // Process interleaved (unchanged, update) pairs
        for (unchanged_items, update_group) in &interspersed.values {
            // Add unchanged items
            for item in unchanged_items {
                items.push((*item, ElementChange::None));
            }

            self.collect_updates_group_items(&mut items, update_group);
        }

        // Process trailing unchanged items
        if let Some(unchanged_items) = &interspersed.last {
            for item in unchanged_items {
                items.push((*item, ElementChange::None));
            }
        }

        // Now group consecutive items by change type and create nodes
        self.build_grouped_items(parent, &items);
    }

    /// Collect items from an UpdatesGroup into the items list.
    fn collect_updates_group_items<'a, 'mem: 'a, 'facet: 'a>(
        &self,
        items: &mut Vec<(Peek<'mem, 'facet>, ElementChange)>,
        group: &'a UpdatesGroup<'mem, 'facet>,
    ) {
        let interspersed = &group.0;

        // Process first replace group if present
        if let Some(replace) = &interspersed.first {
            self.collect_replace_group_items(items, replace);
        }

        // Process interleaved (diffs, replace) pairs
        // NOTE: Nested diffs (e.g., Vec<Struct> where struct fields changed) are not yet
        // supported in the item grouping. For simple scalar sequences, diffs should be empty.
        for (_diffs, replace) in &interspersed.values {
            self.collect_replace_group_items(items, replace);
        }
    }

    /// Collect items from a ReplaceGroup into the items list.
    fn collect_replace_group_items<'a, 'mem: 'a, 'facet: 'a>(
        &self,
        items: &mut Vec<(Peek<'mem, 'facet>, ElementChange)>,
        group: &'a ReplaceGroup<'mem, 'facet>,
    ) {
        // Add removals as deleted
        for removal in &group.removals {
            items.push((*removal, ElementChange::Deleted));
        }

        // Add additions as inserted
        for addition in &group.additions {
            items.push((*addition, ElementChange::Inserted));
        }
    }

    /// Build grouped items and append to parent.
    fn build_grouped_items(&mut self, parent: NodeId, items: &[(Peek<'_, '_>, ElementChange)]) {
        if items.is_empty() {
            return;
        }

        // Group consecutive items by change type
        let mut i = 0;
        while i < items.len() {
            let current_change = items[i].1;

            // Find the end of this run of same-change items
            let mut run_end = i + 1;
            while run_end < items.len() && items[run_end].1 == current_change {
                run_end += 1;
            }

            let run_len = run_end - i;

            // Decide how to render this run
            let (visible_count, collapsed_count) = if run_len <= MAX_VISIBLE_ITEMS {
                (run_len, None)
            } else {
                // Show first few and last few, collapse middle
                // For simplicity, show first MAX_VISIBLE_ITEMS and collapse the rest
                (MAX_VISIBLE_ITEMS, Some(run_len - MAX_VISIBLE_ITEMS))
            };

            // Format the visible items
            let visible_items: Vec<FormattedValue> = items[i..i + visible_count]
                .iter()
                .map(|(peek, _)| self.format_peek(*peek))
                .collect();

            // Create the item group node
            let node = self.tree.new_node(LayoutNode::item_group(
                visible_items,
                current_change,
                collapsed_count,
            ));
            parent.append(node, &mut self.tree);

            i = run_end;
        }
    }

    /// Format a Peek value into the arena.
    ///
    /// Note: PrettyPrinter quotes strings, but we strip those outer quotes
    /// since our attribute rendering already adds quotes: `attr="value"`.
    fn format_peek(&mut self, peek: Peek<'_, '_>) -> FormattedValue {
        let formatted = self.printer.format_peek(peek);

        // Strip outer quotes from strings to avoid double-quoting in attributes
        // e.g. PrettyPrinter gives us `"hello"`, we want `hello`
        let formatted =
            if formatted.starts_with('"') && formatted.ends_with('"') && formatted.len() >= 2 {
                &formatted[1..formatted.len() - 1]
            } else if formatted.starts_with("r#") && formatted.contains('"') {
                // Handle raw strings like r#"hello"# - strip r#" and "#
                // This is a simplification; proper handling would parse the hash count
                formatted.as_str()
            } else {
                formatted.as_str()
            };

        let (span, width) = self.strings.push_str(formatted);
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

        let layout = build_layout(&diff, peek, peek, &BuildOptions::default());

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

        let layout = build_layout(
            &diff,
            Peek::new(&from),
            Peek::new(&to),
            &BuildOptions::default(),
        );

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

        let layout = build_layout(
            &diff,
            Peek::new(&from),
            Peek::new(&to),
            &BuildOptions::default(),
        );
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
    fn test_build_without_context() {
        let from = 10i32;
        let to = 20i32;
        let diff = Diff::Replace {
            from: Peek::new(&from),
            to: Peek::new(&to),
        };

        let layout = build_layout_without_context(&diff, &BuildOptions::default());
        let output = render_to_string(&layout, &RenderOptions::plain());

        // Should still work without context
        assert!(output.contains("10"));
        assert!(output.contains("20"));
    }

    #[test]
    fn test_build_options_default() {
        let opts = BuildOptions::default();
        assert_eq!(opts.max_line_width, 80);
        assert_eq!(opts.max_unchanged_fields, 5);
        assert_eq!(opts.collapse_threshold, 3);
    }
}
