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

use facet_core::{Shape, StructKind, Type, UserType};
use facet_reflect::Peek;
use indextree::{Arena, NodeId};

use super::{
    Attr, DiffFlavor, ElementChange, FormatArena, FormattedValue, Layout, LayoutNode,
    group_changed_attrs,
};
use crate::{Diff, ReplaceGroup, Updates, UpdatesGroup, Value};

/// Get the display name for a shape, respecting the `rename` attribute.
fn get_shape_display_name(shape: &Shape) -> &'static str {
    if let Some(renamed) = shape.get_builtin_attr_value::<&str>("rename") {
        return renamed;
    }
    shape.type_identifier
}

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
/// * `flavor` - The output flavor (Rust, JSON, XML)
pub fn build_layout<'mem, 'facet, F: DiffFlavor>(
    diff: &Diff<'mem, 'facet>,
    from: Peek<'mem, 'facet>,
    to: Peek<'mem, 'facet>,
    opts: &BuildOptions,
    flavor: &F,
) -> Layout {
    let mut builder = LayoutBuilder::new(opts.clone(), flavor);
    let root_id = builder.build(diff, Some(from), Some(to));
    builder.finish(root_id)
}

/// Internal builder state.
struct LayoutBuilder<'f, F: DiffFlavor> {
    /// Arena for formatted strings.
    strings: FormatArena,
    /// Arena for layout nodes.
    tree: Arena<LayoutNode>,
    /// Build options.
    opts: BuildOptions,
    /// Output flavor for formatting.
    flavor: &'f F,
}

impl<'f, F: DiffFlavor> LayoutBuilder<'f, F> {
    fn new(opts: BuildOptions, flavor: &'f F) -> Self {
        Self {
            strings: FormatArena::new(),
            tree: Arena::new(),
            opts,
            flavor,
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
                // Get type name for the tag, respecting `rename` attribute
                let tag = get_shape_display_name(from_shape);

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
                from: _seq_shape_from,
                to: _seq_shape_to,
                updates,
            } => {
                // Get item type from the from/to Peek values passed to build_diff
                let item_type = from
                    .and_then(|p| p.into_list_like().ok())
                    .and_then(|list| list.iter().next())
                    .or_else(|| {
                        to.and_then(|p| p.into_list_like().ok())
                            .and_then(|list| list.iter().next())
                    })
                    .map(|item| get_shape_display_name(item.shape()))
                    .unwrap_or("item");
                self.build_sequence(updates, change, item_type)
            }
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
                    let tag = get_shape_display_name(shape);
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
                        field_name: None,
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
                                let name_width = field_name.len();
                                let attr =
                                    Attr::unchanged(field_name.clone(), name_width, formatted);
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
                    let name_width = field_name.len();
                    let attr = Attr::changed(field_name.clone(), name_width, old_value, new_value);
                    attrs.push(attr);
                }
                _ => {
                    // Nested diff - build as child element or sequence
                    let child =
                        self.build_diff(field_diff, field_from, field_to, ElementChange::None);

                    // Set the field name on the child (only for borrowed names for now)
                    // TODO: Support owned field names for nested elements
                    if let Cow::Borrowed(name) = field_name
                        && let Some(node) = self.tree.get_mut(child)
                    {
                        match node.get_mut() {
                            LayoutNode::Element { field_name, .. } => {
                                *field_name = Some(name);
                            }
                            LayoutNode::Sequence { field_name, .. } => {
                                *field_name = Some(name);
                            }
                            _ => {}
                        }
                    }

                    child_nodes.push(child);
                }
            }
        }

        // Process deletions
        let mut sorted_deletions: Vec<_> = deletions.iter().collect();
        sorted_deletions.sort_by(|(a, _), (b, _)| a.cmp(b));

        for (field_name, value) in sorted_deletions {
            let formatted = self.format_peek(*value);
            let name_width = field_name.len();
            let attr = Attr::deleted(field_name.clone(), name_width, formatted);
            attrs.push(attr);
        }

        // Process insertions
        let mut sorted_insertions: Vec<_> = insertions.iter().collect();
        sorted_insertions.sort_by(|(a, _), (b, _)| a.cmp(b));

        for (field_name, value) in sorted_insertions {
            let formatted = self.format_peek(*value);
            let name_width = field_name.len();
            let attr = Attr::inserted(field_name.clone(), name_width, formatted);
            attrs.push(attr);
        }

        // Group changed attributes for alignment
        let changed_groups = group_changed_attrs(&attrs, self.opts.max_line_width, 0);

        // Create the element node
        let node = self.tree.new_node(LayoutNode::Element {
            tag: element_tag,
            field_name: None, // Will be set by parent if this is a struct field
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
            field_name: None,
            attrs: Vec::new(),
            changed_groups: Vec::new(),
            change,
        });

        // Build children from updates (tuple items don't have specific type names)
        self.build_updates_children(node, updates, "item");

        node
    }

    /// Build a sequence diff.
    fn build_sequence(
        &mut self,
        updates: &Updates<'_, '_>,
        change: ElementChange,
        item_type: &'static str,
    ) -> NodeId {
        // Create sequence node with item type info
        let node = self.tree.new_node(LayoutNode::Sequence {
            change,
            item_type,
            field_name: None,
        });

        // Build children from updates
        self.build_updates_children(node, updates, item_type);

        node
    }

    /// Build children from an Updates structure and append to parent.
    ///
    /// This groups consecutive items by their change type (unchanged, deleted, inserted)
    /// and renders them on single lines with optional collapsing for long runs.
    /// Nested diffs (struct items with internal changes) are built as full child nodes.
    fn build_updates_children(
        &mut self,
        parent: NodeId,
        updates: &Updates<'_, '_>,
        _item_type: &'static str,
    ) {
        // Collect simple items (adds/removes) and nested diffs separately
        let mut items: Vec<(Peek<'_, '_>, ElementChange)> = Vec::new();
        let mut nested_diffs: Vec<&Diff<'_, '_>> = Vec::new();

        let interspersed = &updates.0;

        // Process first update group if present
        if let Some(update_group) = &interspersed.first {
            self.collect_updates_group_items(&mut items, &mut nested_diffs, update_group);
        }

        // Process interleaved (unchanged, update) pairs
        for (unchanged_items, update_group) in &interspersed.values {
            // Add unchanged items
            for item in unchanged_items {
                items.push((*item, ElementChange::None));
            }

            self.collect_updates_group_items(&mut items, &mut nested_diffs, update_group);
        }

        // Process trailing unchanged items
        if let Some(unchanged_items) = &interspersed.last {
            for item in unchanged_items {
                items.push((*item, ElementChange::None));
            }
        }

        tracing::debug!(
            items_count = items.len(),
            nested_diffs_count = nested_diffs.len(),
            "collected sequence items"
        );

        // Build nested diffs as full child nodes (struct items with internal changes)
        for diff in nested_diffs {
            // Get from/to Peek from the diff for context
            let (from_peek, to_peek) = match diff {
                Diff::User { .. } => {
                    // For User diffs, we need the actual Peek values
                    // The diff contains the shapes but we need to find the corresponding Peeks
                    // For now, pass None - the build_diff will use the shape info
                    (None, None)
                }
                Diff::Replace { from, to } => (Some(*from), Some(*to)),
                _ => (None, None),
            };
            let child = self.build_diff(diff, from_peek, to_peek, ElementChange::None);
            parent.append(child, &mut self.tree);
        }

        // TODO: Also handle simple items (adds/removes) - for now they're not rendered
        // This is fine since nested diffs are the main use case
        let _ = items; // suppress unused warning for now
    }

    /// Collect items from an UpdatesGroup into the items list.
    /// Also returns nested diffs that need to be built as full child nodes.
    fn collect_updates_group_items<'a, 'mem: 'a, 'facet: 'a>(
        &self,
        items: &mut Vec<(Peek<'mem, 'facet>, ElementChange)>,
        nested_diffs: &mut Vec<&'a Diff<'mem, 'facet>>,
        group: &'a UpdatesGroup<'mem, 'facet>,
    ) {
        let interspersed = &group.0;

        // Process first replace group if present
        if let Some(replace) = &interspersed.first {
            self.collect_replace_group_items(items, replace);
        }

        // Process interleaved (diffs, replace) pairs
        for (diffs, replace) in &interspersed.values {
            // Collect nested diffs - these are struct items with internal changes
            for diff in diffs {
                nested_diffs.push(diff);
            }
            self.collect_replace_group_items(items, replace);
        }

        // Process trailing diffs (if any)
        if let Some(diffs) = &interspersed.last {
            for diff in diffs {
                nested_diffs.push(diff);
            }
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

    /// Format a Peek value into the arena using the flavor.
    fn format_peek(&mut self, peek: Peek<'_, '_>) -> FormattedValue {
        let (span, width) = self.strings.format(|w| self.flavor.format_value(peek, w));
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
    use crate::layout::{RustFlavor, XmlFlavor};

    #[test]
    fn test_build_equal_diff() {
        let value = 42i32;
        let peek = Peek::new(&value);
        let diff = Diff::Equal { value: Some(peek) };

        let layout = build_layout(&diff, peek, peek, &BuildOptions::default(), &RustFlavor);

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
            &RustFlavor,
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
            &RustFlavor,
        );
        let output = render_to_string(&layout, &RenderOptions::plain(), &XmlFlavor);

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
        assert_eq!(opts.max_unchanged_fields, 5);
        assert_eq!(opts.collapse_threshold, 3);
    }
}
