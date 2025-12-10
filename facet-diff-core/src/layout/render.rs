//! Layout rendering to output.

use std::fmt::{self, Write};

use super::backend::{AnsiBackend, ColorBackend, PlainBackend, SemanticColor};
use super::flavor::DiffFlavor;
use super::{AttrStatus, ChangedGroup, ElementChange, Layout, LayoutNode};
use crate::DiffSymbols;

/// Options for rendering a layout.
#[derive(Clone, Debug)]
pub struct RenderOptions<B: ColorBackend> {
    /// Symbols to use for diff markers.
    pub symbols: DiffSymbols,
    /// Color backend for styling output.
    pub backend: B,
    /// Indentation string (default: 2 spaces).
    pub indent: &'static str,
}

impl Default for RenderOptions<AnsiBackend> {
    fn default() -> Self {
        Self {
            symbols: DiffSymbols::default(),
            backend: AnsiBackend::default(),
            indent: "  ",
        }
    }
}

impl RenderOptions<PlainBackend> {
    /// Create options with plain backend (no colors).
    pub fn plain() -> Self {
        Self {
            symbols: DiffSymbols::default(),
            backend: PlainBackend,
            indent: "  ",
        }
    }
}

impl<B: ColorBackend> RenderOptions<B> {
    /// Create options with a custom backend.
    pub fn with_backend(backend: B) -> Self {
        Self {
            symbols: DiffSymbols::default(),
            backend,
            indent: "  ",
        }
    }
}

/// Render a layout to a writer.
///
/// Starts at depth 1 to provide a gutter for change prefixes (- / +).
pub fn render<W: Write, B: ColorBackend, F: DiffFlavor>(
    layout: &Layout,
    w: &mut W,
    opts: &RenderOptions<B>,
    flavor: &F,
) -> fmt::Result {
    render_node(layout, w, layout.root, 1, opts, flavor)
}

/// Render a layout to a String.
pub fn render_to_string<B: ColorBackend, F: DiffFlavor>(
    layout: &Layout,
    opts: &RenderOptions<B>,
    flavor: &F,
) -> String {
    let mut out = String::new();
    render(layout, &mut out, opts, flavor).expect("writing to String cannot fail");
    out
}

fn element_change_to_semantic(change: ElementChange) -> SemanticColor {
    match change {
        ElementChange::None => SemanticColor::Unchanged,
        ElementChange::Deleted => SemanticColor::Deleted,
        ElementChange::Inserted => SemanticColor::Inserted,
        ElementChange::MovedFrom | ElementChange::MovedTo => SemanticColor::Moved,
    }
}

fn render_node<W: Write, B: ColorBackend, F: DiffFlavor>(
    layout: &Layout,
    w: &mut W,
    node_id: indextree::NodeId,
    depth: usize,
    opts: &RenderOptions<B>,
    flavor: &F,
) -> fmt::Result {
    let node = layout.get(node_id).expect("node exists");

    match node {
        LayoutNode::Element {
            tag,
            field_name,
            attrs,
            changed_groups,
            change,
        } => {
            let tag = *tag;
            let field_name = *field_name;
            let change = *change;
            let attrs = attrs.clone();
            let changed_groups = changed_groups.clone();

            render_element(
                layout,
                w,
                node_id,
                depth,
                opts,
                flavor,
                tag,
                field_name,
                &attrs,
                &changed_groups,
                change,
            )
        }

        LayoutNode::Sequence {
            change,
            item_type,
            field_name,
        } => {
            let change = *change;
            let item_type = *item_type;
            let field_name = *field_name;
            render_sequence(
                layout, w, node_id, depth, opts, flavor, change, item_type, field_name,
            )
        }

        LayoutNode::Collapsed { count } => {
            let count = *count;
            write_indent(w, depth, opts)?;
            let comment = flavor.comment(&format!("{} unchanged", count));
            opts.backend
                .write_styled(w, &comment, SemanticColor::Comment)?;
            writeln!(w)
        }

        LayoutNode::Text { value, change } => {
            let text = layout.get_string(value.span);
            let change = *change;

            write_indent(w, depth, opts)?;
            if let Some(prefix) = change.prefix() {
                opts.backend
                    .write_prefix(w, prefix, element_change_to_semantic(change))?;
                write!(w, " ")?;
            }

            opts.backend
                .write_styled(w, text, element_change_to_semantic(change))?;
            writeln!(w)
        }

        LayoutNode::ItemGroup {
            items,
            change,
            collapsed_suffix,
            item_type,
        } => {
            let items = items.clone();
            let change = *change;
            let collapsed_suffix = *collapsed_suffix;
            let item_type = *item_type;

            // For changed items, the prefix eats into the indent (goes in the "gutter")
            if let Some(prefix) = change.prefix() {
                // Write indent minus 2 chars, then prefix + space
                write_indent_minus_prefix(w, depth, opts)?;
                opts.backend
                    .write_prefix(w, prefix, element_change_to_semantic(change))?;
                write!(w, " ")?;
            } else {
                write_indent(w, depth, opts)?;
            }

            // Render items with flavor separator and optional wrapping
            let semantic = element_change_to_semantic(change);
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    write!(w, "{}", flavor.item_separator())?;
                }
                let raw_value = layout.get_string(item.span);
                let formatted = flavor.format_seq_item(item_type, raw_value);
                opts.backend.write_styled(w, &formatted, semantic)?;
            }

            // Render collapsed suffix if present
            if let Some(count) = collapsed_suffix {
                let suffix = flavor.comment(&format!("{} more", count));
                write!(w, " ")?;
                opts.backend
                    .write_styled(w, &suffix, SemanticColor::Comment)?;
            }

            writeln!(w)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn render_element<W: Write, B: ColorBackend, F: DiffFlavor>(
    layout: &Layout,
    w: &mut W,
    node_id: indextree::NodeId,
    depth: usize,
    opts: &RenderOptions<B>,
    flavor: &F,
    tag: &str,
    field_name: Option<&str>,
    attrs: &[super::Attr],
    changed_groups: &[ChangedGroup],
    change: ElementChange,
) -> fmt::Result {
    let has_attr_changes = !changed_groups.is_empty()
        || attrs.iter().any(|a| {
            matches!(
                a.status,
                AttrStatus::Deleted { .. } | AttrStatus::Inserted { .. }
            )
        });

    let children: Vec<_> = layout.children(node_id).collect();
    let has_children = !children.is_empty();

    let tag_color = match change {
        ElementChange::None => SemanticColor::Structure,
        ElementChange::Deleted => SemanticColor::Deleted,
        ElementChange::Inserted => SemanticColor::Inserted,
        ElementChange::MovedFrom | ElementChange::MovedTo => SemanticColor::Moved,
    };

    // Opening tag/struct
    write_indent(w, depth, opts)?;
    if let Some(prefix) = change.prefix() {
        opts.backend
            .write_prefix(w, prefix, element_change_to_semantic(change))?;
        write!(w, " ")?;
    }

    // Render field name prefix if this element is a struct field (e.g., "point: " for Rust)
    // Uses format_child_open which handles the difference between:
    // - Rust/JSON: `field_name: `
    // - XML: `` (empty - nested elements don't use attribute syntax)
    if let Some(name) = field_name {
        let prefix = flavor.format_child_open(name);
        if !prefix.is_empty() {
            opts.backend
                .write_styled(w, &prefix, SemanticColor::Unchanged)?;
        }
    }

    let open = flavor.struct_open(tag);
    opts.backend.write_styled(w, &open, tag_color)?;

    // Render type comment in muted color if present
    if let Some(comment) = flavor.type_comment(tag) {
        write!(w, " ")?;
        opts.backend
            .write_styled(w, &comment, SemanticColor::Comment)?;
    }

    if has_attr_changes {
        // Multi-line attribute format
        writeln!(w)?;

        // Render changed groups as -/+ line pairs
        for group in changed_groups {
            render_changed_group(layout, w, depth + 1, opts, flavor, attrs, group)?;
        }

        // Render deleted attributes (prefix uses indent gutter)
        for (i, attr) in attrs.iter().enumerate() {
            if let AttrStatus::Deleted { value } = &attr.status {
                // Skip if already in a changed group
                if changed_groups.iter().any(|g| g.attr_indices.contains(&i)) {
                    continue;
                }
                write_indent_minus_prefix(w, depth + 1, opts)?;
                opts.backend.write_prefix(w, '-', SemanticColor::Deleted)?;
                write!(w, " ")?;
                render_attr_deleted(layout, w, opts, flavor, &attr.name, value)?;
                // Trailing comma (muted)
                opts.backend.write_styled(
                    w,
                    flavor.trailing_separator(),
                    SemanticColor::Comment,
                )?;
                writeln!(w)?;
            }
        }

        // Render inserted attributes (prefix uses indent gutter)
        for (i, attr) in attrs.iter().enumerate() {
            if let AttrStatus::Inserted { value } = &attr.status {
                if changed_groups.iter().any(|g| g.attr_indices.contains(&i)) {
                    continue;
                }
                write_indent_minus_prefix(w, depth + 1, opts)?;
                opts.backend.write_prefix(w, '+', SemanticColor::Inserted)?;
                write!(w, " ")?;
                render_attr_inserted(layout, w, opts, flavor, &attr.name, value)?;
                // Trailing comma (muted)
                opts.backend.write_styled(
                    w,
                    flavor.trailing_separator(),
                    SemanticColor::Comment,
                )?;
                writeln!(w)?;
            }
        }

        // Render unchanged attributes on one line
        let unchanged: Vec<_> = attrs
            .iter()
            .filter(|a| matches!(a.status, AttrStatus::Unchanged { .. }))
            .collect();
        if !unchanged.is_empty() {
            write_indent(w, depth + 1, opts)?;
            for (i, attr) in unchanged.iter().enumerate() {
                if i > 0 {
                    write!(w, "{}", flavor.field_separator())?;
                }
                if let AttrStatus::Unchanged { value } = &attr.status {
                    render_attr_unchanged(layout, w, opts, flavor, &attr.name, value)?;
                }
            }
            // Trailing comma (muted)
            opts.backend
                .write_styled(w, flavor.trailing_separator(), SemanticColor::Comment)?;
            writeln!(w)?;
        }

        // Closing bracket
        write_indent(w, depth, opts)?;
        if has_children {
            let open_close = flavor.struct_open_close();
            opts.backend.write_styled(w, open_close, tag_color)?;
        } else {
            let close = flavor.struct_close(tag, true);
            opts.backend.write_styled(w, &close, tag_color)?;
        }
        writeln!(w)?;
    } else if has_children && !attrs.is_empty() {
        // Unchanged attributes with children: put attrs on their own lines
        writeln!(w)?;
        for attr in attrs.iter() {
            write_indent(w, depth + 1, opts)?;
            if let AttrStatus::Unchanged { value } = &attr.status {
                render_attr_unchanged(layout, w, opts, flavor, &attr.name, value)?;
            }
            // Trailing comma (muted)
            opts.backend
                .write_styled(w, flavor.trailing_separator(), SemanticColor::Comment)?;
            writeln!(w)?;
        }
        // Close the opening (e.g., ">" for XML) - only if non-empty
        let open_close = flavor.struct_open_close();
        if !open_close.is_empty() {
            write_indent(w, depth, opts)?;
            opts.backend.write_styled(w, open_close, tag_color)?;
            writeln!(w)?;
        }
    } else {
        // Inline attributes (no changes, no children) or no attrs
        for (i, attr) in attrs.iter().enumerate() {
            if i > 0 {
                write!(w, "{}", flavor.field_separator())?;
            } else {
                write!(w, " ")?;
            }
            if let AttrStatus::Unchanged { value } = &attr.status {
                render_attr_unchanged(layout, w, opts, flavor, &attr.name, value)?;
            }
        }

        if has_children {
            // Close the opening tag (e.g., ">" for XML)
            let open_close = flavor.struct_open_close();
            opts.backend.write_styled(w, open_close, tag_color)?;
        } else {
            // Self-closing
            let close = flavor.struct_close(tag, true);
            opts.backend.write_styled(w, &close, tag_color)?;
        }
        writeln!(w)?;
    }

    // Children
    for child_id in children {
        render_node(layout, w, child_id, depth + 1, opts, flavor)?;
    }

    // Closing tag (if we have children, we already printed opening part above)
    if has_children {
        write_indent(w, depth, opts)?;
        if let Some(prefix) = change.prefix() {
            opts.backend
                .write_prefix(w, prefix, element_change_to_semantic(change))?;
            write!(w, " ")?;
        }
        let close = flavor.struct_close(tag, false);
        opts.backend.write_styled(w, &close, tag_color)?;
        writeln!(w)?;
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn render_sequence<W: Write, B: ColorBackend, F: DiffFlavor>(
    layout: &Layout,
    w: &mut W,
    node_id: indextree::NodeId,
    depth: usize,
    opts: &RenderOptions<B>,
    flavor: &F,
    change: ElementChange,
    _item_type: &str, // Item type available for future use (items use it via ItemGroup)
    field_name: Option<&str>,
) -> fmt::Result {
    let children: Vec<_> = layout.children(node_id).collect();

    let tag_color = match change {
        ElementChange::None => SemanticColor::Structure,
        ElementChange::Deleted => SemanticColor::Deleted,
        ElementChange::Inserted => SemanticColor::Inserted,
        ElementChange::MovedFrom | ElementChange::MovedTo => SemanticColor::Moved,
    };

    // Empty sequences: render on single line
    if children.is_empty() {
        // Always render empty sequences with field name (e.g., "elements: []")
        // Only skip if unchanged AND no field name
        if change == ElementChange::None && field_name.is_none() {
            return Ok(());
        }

        write_indent(w, depth, opts)?;
        if let Some(prefix) = change.prefix() {
            opts.backend
                .write_prefix(w, prefix, element_change_to_semantic(change))?;
            write!(w, " ")?;
        }

        // Open and close with optional field name
        if let Some(name) = field_name {
            let open = flavor.format_seq_field_open(name);
            let close = flavor.format_seq_field_close(name);
            opts.backend.write_styled(w, &open, tag_color)?;
            opts.backend.write_styled(w, &close, tag_color)?;
        } else {
            let open = flavor.seq_open();
            let close = flavor.seq_close();
            opts.backend.write_styled(w, &open, tag_color)?;
            opts.backend.write_styled(w, &close, tag_color)?;
        }

        // Trailing comma for fields
        if field_name.is_some() {
            opts.backend
                .write_styled(w, flavor.trailing_separator(), SemanticColor::Comment)?;
        }
        writeln!(w)?;
        return Ok(());
    }

    // Opening bracket with optional field name
    write_indent(w, depth, opts)?;
    if let Some(prefix) = change.prefix() {
        opts.backend
            .write_prefix(w, prefix, element_change_to_semantic(change))?;
        write!(w, " ")?;
    }

    // Open with optional field name
    if let Some(name) = field_name {
        let open = flavor.format_seq_field_open(name);
        opts.backend.write_styled(w, &open, tag_color)?;
    } else {
        let open = flavor.seq_open();
        opts.backend.write_styled(w, &open, tag_color)?;
    }
    writeln!(w)?;

    // Children
    for child_id in children {
        render_node(layout, w, child_id, depth + 1, opts, flavor)?;
    }

    // Closing bracket
    write_indent(w, depth, opts)?;
    if let Some(prefix) = change.prefix() {
        opts.backend
            .write_prefix(w, prefix, element_change_to_semantic(change))?;
        write!(w, " ")?;
    }

    // Close with optional field name
    if let Some(name) = field_name {
        let close = flavor.format_seq_field_close(name);
        opts.backend.write_styled(w, &close, tag_color)?;
    } else {
        let close = flavor.seq_close();
        opts.backend.write_styled(w, &close, tag_color)?;
    }

    // Trailing comma for fields
    if field_name.is_some() {
        opts.backend
            .write_styled(w, flavor.trailing_separator(), SemanticColor::Comment)?;
    }
    writeln!(w)?;

    Ok(())
}

fn render_changed_group<W: Write, B: ColorBackend, F: DiffFlavor>(
    layout: &Layout,
    w: &mut W,
    depth: usize,
    opts: &RenderOptions<B>,
    flavor: &F,
    attrs: &[super::Attr],
    group: &ChangedGroup,
) -> fmt::Result {
    // Minus line (prefix uses indent gutter)
    write_indent_minus_prefix(w, depth, opts)?;
    opts.backend.write_prefix(w, '-', SemanticColor::Deleted)?;
    write!(w, " ")?;

    let last_idx = group.attr_indices.len().saturating_sub(1);
    for (i, &idx) in group.attr_indices.iter().enumerate() {
        if i > 0 {
            write!(w, "{}", flavor.field_separator())?;
        }
        let attr = &attrs[idx];
        if let AttrStatus::Changed { old, new } = &attr.status {
            // Each field padded to max of its own old/new value width
            let field_max_width = old.width.max(new.width);
            write!(w, "{}", flavor.format_field_prefix(&attr.name))?;
            let old_str = layout.get_string(old.span);
            opts.backend
                .write_styled(w, old_str, SemanticColor::Deleted)?;
            write!(w, "{}", flavor.format_field_suffix())?;
            // Pad to align with the + line's value (only between fields, not at end)
            if i < last_idx {
                let value_padding = field_max_width.saturating_sub(old.width);
                for _ in 0..value_padding {
                    write!(w, " ")?;
                }
            }
        }
    }
    writeln!(w)?;

    // Plus line (prefix uses indent gutter)
    write_indent_minus_prefix(w, depth, opts)?;
    opts.backend.write_prefix(w, '+', SemanticColor::Inserted)?;
    write!(w, " ")?;

    for (i, &idx) in group.attr_indices.iter().enumerate() {
        if i > 0 {
            write!(w, "{}", flavor.field_separator())?;
        }
        let attr = &attrs[idx];
        if let AttrStatus::Changed { old, new } = &attr.status {
            // Each field padded to max of its own old/new value width
            let field_max_width = old.width.max(new.width);
            write!(w, "{}", flavor.format_field_prefix(&attr.name))?;
            let new_str = layout.get_string(new.span);
            opts.backend
                .write_styled(w, new_str, SemanticColor::Inserted)?;
            write!(w, "{}", flavor.format_field_suffix())?;
            // Pad to align with the - line's value (only between fields, not at end)
            if i < last_idx {
                let value_padding = field_max_width.saturating_sub(new.width);
                for _ in 0..value_padding {
                    write!(w, " ")?;
                }
            }
        }
    }
    writeln!(w)?;

    Ok(())
}

fn render_attr_unchanged<W: Write, B: ColorBackend, F: DiffFlavor>(
    layout: &Layout,
    w: &mut W,
    opts: &RenderOptions<B>,
    flavor: &F,
    name: &str,
    value: &super::FormattedValue,
) -> fmt::Result {
    let value_str = layout.get_string(value.span);
    let formatted = flavor.format_field(name, value_str);
    opts.backend
        .write_styled(w, &formatted, SemanticColor::Unchanged)
}

fn render_attr_deleted<W: Write, B: ColorBackend, F: DiffFlavor>(
    layout: &Layout,
    w: &mut W,
    opts: &RenderOptions<B>,
    flavor: &F,
    name: &str,
    value: &super::FormattedValue,
) -> fmt::Result {
    let value_str = layout.get_string(value.span);
    // Entire field is colored red for deleted
    let formatted = flavor.format_field(name, value_str);
    opts.backend
        .write_styled(w, &formatted, SemanticColor::Deleted)
}

fn render_attr_inserted<W: Write, B: ColorBackend, F: DiffFlavor>(
    layout: &Layout,
    w: &mut W,
    opts: &RenderOptions<B>,
    flavor: &F,
    name: &str,
    value: &super::FormattedValue,
) -> fmt::Result {
    let value_str = layout.get_string(value.span);
    // Entire field is colored green for inserted
    let formatted = flavor.format_field(name, value_str);
    opts.backend
        .write_styled(w, &formatted, SemanticColor::Inserted)
}

fn write_indent<W: Write, B: ColorBackend>(
    w: &mut W,
    depth: usize,
    opts: &RenderOptions<B>,
) -> fmt::Result {
    for _ in 0..depth {
        write!(w, "{}", opts.indent)?;
    }
    Ok(())
}

/// Write indent minus 2 characters for the prefix gutter.
/// The "- " or "+ " prefix will occupy those 2 characters.
fn write_indent_minus_prefix<W: Write, B: ColorBackend>(
    w: &mut W,
    depth: usize,
    opts: &RenderOptions<B>,
) -> fmt::Result {
    let total_indent = depth * opts.indent.len();
    let gutter_indent = total_indent.saturating_sub(2);
    for _ in 0..gutter_indent {
        write!(w, " ")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use indextree::Arena;

    use super::*;
    use crate::layout::{Attr, FormatArena, FormattedValue, Layout, LayoutNode, XmlFlavor};

    fn make_test_layout() -> Layout {
        let mut strings = FormatArena::new();
        let tree = Arena::new();

        // Create a simple element with one changed attribute
        let (red_span, red_width) = strings.push_str("red");
        let (blue_span, blue_width) = strings.push_str("blue");

        let fill_attr = Attr::changed(
            "fill",
            4,
            FormattedValue::new(red_span, red_width),
            FormattedValue::new(blue_span, blue_width),
        );

        let attrs = vec![fill_attr];
        let changed_groups = super::super::group_changed_attrs(&attrs, 80, 0);

        let root = LayoutNode::Element {
            tag: "rect",
            field_name: None,
            attrs,
            changed_groups,
            change: ElementChange::None,
        };

        Layout::new(strings, tree, root)
    }

    #[test]
    fn test_render_simple_change() {
        let layout = make_test_layout();
        let opts = RenderOptions::plain();
        let output = render_to_string(&layout, &opts, &XmlFlavor);

        assert!(output.contains("<rect"));
        assert!(output.contains("- fill=\"red\""));
        assert!(output.contains("+ fill=\"blue\""));
        assert!(output.contains("/>"));
    }

    #[test]
    fn test_render_collapsed() {
        let strings = FormatArena::new();
        let tree = Arena::new();

        let root = LayoutNode::collapsed(5);
        let layout = Layout::new(strings, tree, root);

        let opts = RenderOptions::plain();
        let output = render_to_string(&layout, &opts, &XmlFlavor);

        assert!(output.contains("<!-- 5 unchanged -->"));
    }

    #[test]
    fn test_render_with_children() {
        let mut strings = FormatArena::new();
        let mut tree = Arena::new();

        // Parent element
        let parent = tree.new_node(LayoutNode::Element {
            tag: "svg",
            field_name: None,
            attrs: vec![],
            changed_groups: vec![],
            change: ElementChange::None,
        });

        // Child element with change
        let (red_span, red_width) = strings.push_str("red");
        let (blue_span, blue_width) = strings.push_str("blue");

        let fill_attr = Attr::changed(
            "fill",
            4,
            FormattedValue::new(red_span, red_width),
            FormattedValue::new(blue_span, blue_width),
        );
        let attrs = vec![fill_attr];
        let changed_groups = super::super::group_changed_attrs(&attrs, 80, 0);

        let child = tree.new_node(LayoutNode::Element {
            tag: "rect",
            field_name: None,
            attrs,
            changed_groups,
            change: ElementChange::None,
        });

        parent.append(child, &mut tree);

        let layout = Layout {
            strings,
            tree,
            root: parent,
        };

        let opts = RenderOptions::plain();
        let output = render_to_string(&layout, &opts, &XmlFlavor);

        assert!(output.contains("<svg>"));
        assert!(output.contains("</svg>"));
        assert!(output.contains("<rect"));
    }

    #[test]
    fn test_ansi_backend_produces_escapes() {
        let layout = make_test_layout();
        let opts = RenderOptions::default();
        let output = render_to_string(&layout, &opts, &XmlFlavor);

        // Should contain ANSI escape codes
        assert!(
            output.contains("\x1b["),
            "output should contain ANSI escapes"
        );
    }
}
