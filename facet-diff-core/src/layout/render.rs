//! Layout rendering to output.

use std::fmt::{self, Write};

use super::backend::{AnsiBackend, ColorBackend, PlainBackend, SemanticColor};
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
pub fn render<W: Write, B: ColorBackend>(
    layout: &Layout,
    w: &mut W,
    opts: &RenderOptions<B>,
) -> fmt::Result {
    render_node(layout, w, layout.root, 0, opts)
}

/// Render a layout to a String.
pub fn render_to_string<B: ColorBackend>(layout: &Layout, opts: &RenderOptions<B>) -> String {
    let mut out = String::new();
    render(layout, &mut out, opts).expect("writing to String cannot fail");
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

fn render_node<W: Write, B: ColorBackend>(
    layout: &Layout,
    w: &mut W,
    node_id: indextree::NodeId,
    depth: usize,
    opts: &RenderOptions<B>,
) -> fmt::Result {
    let node = layout.get(node_id).expect("node exists");

    match node {
        LayoutNode::Element {
            tag,
            attrs,
            changed_groups,
            change,
        } => {
            let tag = *tag;
            let change = *change;
            let attrs = attrs.clone();
            let changed_groups = changed_groups.clone();

            render_element(
                layout,
                w,
                node_id,
                depth,
                opts,
                tag,
                &attrs,
                &changed_groups,
                change,
            )
        }

        LayoutNode::Collapsed { count } => {
            let count = *count;
            write_indent(w, depth, opts)?;
            let comment = format!("<!-- {} unchanged -->", count);
            opts.backend
                .write_styled(w, &comment, SemanticColor::Unchanged)?;
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
        } => {
            let items = items.clone();
            let change = *change;
            let collapsed_suffix = *collapsed_suffix;

            write_indent(w, depth, opts)?;
            if let Some(prefix) = change.prefix() {
                opts.backend
                    .write_prefix(w, prefix, element_change_to_semantic(change))?;
                write!(w, " ")?;
            }

            // Render items space-separated
            let semantic = element_change_to_semantic(change);
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    write!(w, " ")?;
                }
                let text = layout.get_string(item.span);
                opts.backend.write_styled(w, text, semantic)?;
            }

            // Render collapsed suffix if present
            if let Some(count) = collapsed_suffix {
                let suffix = format!(" ...{} more", count);
                opts.backend
                    .write_styled(w, &suffix, SemanticColor::Unchanged)?;
            }

            writeln!(w)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn render_element<W: Write, B: ColorBackend>(
    layout: &Layout,
    w: &mut W,
    node_id: indextree::NodeId,
    depth: usize,
    opts: &RenderOptions<B>,
    tag: &str,
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

    // Opening tag line
    write_indent(w, depth, opts)?;
    if let Some(prefix) = change.prefix() {
        opts.backend
            .write_prefix(w, prefix, element_change_to_semantic(change))?;
        write!(w, " ")?;
    }

    opts.backend
        .write_styled(w, &format!("<{}", tag), tag_color)?;

    if has_attr_changes {
        // Multi-line attribute format
        writeln!(w)?;

        // Render changed groups as -/+ line pairs
        for group in changed_groups {
            render_changed_group(layout, w, depth + 1, opts, attrs, group)?;
        }

        // Render deleted attributes
        for (i, attr) in attrs.iter().enumerate() {
            if let AttrStatus::Deleted { value } = &attr.status {
                // Skip if already in a changed group
                if changed_groups.iter().any(|g| g.attr_indices.contains(&i)) {
                    continue;
                }
                write_indent(w, depth + 1, opts)?;
                opts.backend.write_prefix(w, '-', SemanticColor::Deleted)?;
                write!(w, " ")?;
                render_attr_deleted(layout, w, opts, attr.name, value)?;
                writeln!(w)?;
            }
        }

        // Render inserted attributes
        for (i, attr) in attrs.iter().enumerate() {
            if let AttrStatus::Inserted { value } = &attr.status {
                if changed_groups.iter().any(|g| g.attr_indices.contains(&i)) {
                    continue;
                }
                write_indent(w, depth + 1, opts)?;
                opts.backend.write_prefix(w, '+', SemanticColor::Inserted)?;
                write!(w, " ")?;
                render_attr_inserted(layout, w, opts, attr.name, value)?;
                writeln!(w)?;
            }
        }

        // Render unchanged attributes on one line (dimmed)
        let unchanged: Vec<_> = attrs
            .iter()
            .filter(|a| matches!(a.status, AttrStatus::Unchanged { .. }))
            .collect();
        if !unchanged.is_empty() {
            write_indent(w, depth + 1, opts)?;
            write!(w, "  ")?; // align with -/+ lines
            for (i, attr) in unchanged.iter().enumerate() {
                if i > 0 {
                    write!(w, " ")?;
                }
                if let AttrStatus::Unchanged { value } = &attr.status {
                    render_attr_unchanged(layout, w, opts, attr.name, value)?;
                }
            }
            writeln!(w)?;
        }

        // Closing bracket
        write_indent(w, depth, opts)?;
        if has_children {
            opts.backend.write_styled(w, ">", tag_color)?;
            writeln!(w)?;
        } else {
            opts.backend.write_styled(w, "/>", tag_color)?;
            writeln!(w)?;
        }
    } else {
        // Inline attributes (no changes)
        for attr in attrs {
            write!(w, " ")?;
            if let AttrStatus::Unchanged { value } = &attr.status {
                render_attr_unchanged(layout, w, opts, attr.name, value)?;
            }
        }

        if has_children {
            opts.backend.write_styled(w, ">", tag_color)?;
            writeln!(w)?;
        } else {
            opts.backend.write_styled(w, "/>", tag_color)?;
            writeln!(w)?;
        }
    }

    // Children
    for child_id in children {
        render_node(layout, w, child_id, depth + 1, opts)?;
    }

    // Closing tag
    if has_children {
        write_indent(w, depth, opts)?;
        if let Some(prefix) = change.prefix() {
            opts.backend
                .write_prefix(w, prefix, element_change_to_semantic(change))?;
            write!(w, " ")?;
        }
        opts.backend
            .write_styled(w, &format!("</{}>", tag), tag_color)?;
        writeln!(w)?;
    }

    Ok(())
}

fn render_changed_group<W: Write, B: ColorBackend>(
    layout: &Layout,
    w: &mut W,
    depth: usize,
    opts: &RenderOptions<B>,
    attrs: &[super::Attr],
    group: &ChangedGroup,
) -> fmt::Result {
    // Minus line
    write_indent(w, depth, opts)?;
    opts.backend.write_prefix(w, '-', SemanticColor::Deleted)?;
    write!(w, " ")?;

    // For alignment, we need to pad both - and + lines to the same width
    let max_value_width = group.max_old_width.max(group.max_new_width);

    for (i, &idx) in group.attr_indices.iter().enumerate() {
        if i > 0 {
            write!(w, " ")?;
        }
        let attr = &attrs[idx];
        if let AttrStatus::Changed { old, .. } = &attr.status {
            let name_padding = group.max_name_width.saturating_sub(attr.name_width);
            write!(w, "{}=\"", attr.name)?;
            let old_str = layout.get_string(old.span);
            opts.backend
                .write_styled(w, old_str, SemanticColor::Deleted)?;
            write!(w, "\"")?;
            // Pad value for column alignment
            let value_padding = max_value_width.saturating_sub(old.width) + name_padding;
            for _ in 0..value_padding {
                write!(w, " ")?;
            }
        }
    }
    writeln!(w)?;

    // Plus line
    write_indent(w, depth, opts)?;
    opts.backend.write_prefix(w, '+', SemanticColor::Inserted)?;
    write!(w, " ")?;

    for (i, &idx) in group.attr_indices.iter().enumerate() {
        if i > 0 {
            write!(w, " ")?;
        }
        let attr = &attrs[idx];
        if let AttrStatus::Changed { new, .. } = &attr.status {
            let name_padding = group.max_name_width.saturating_sub(attr.name_width);
            write!(w, "{}=\"", attr.name)?;
            let new_str = layout.get_string(new.span);
            opts.backend
                .write_styled(w, new_str, SemanticColor::Inserted)?;
            write!(w, "\"")?;
            // Pad for column alignment
            let value_padding = max_value_width.saturating_sub(new.width) + name_padding;
            for _ in 0..value_padding {
                write!(w, " ")?;
            }
        }
    }
    writeln!(w)?;

    Ok(())
}

fn render_attr_unchanged<W: Write, B: ColorBackend>(
    layout: &Layout,
    w: &mut W,
    opts: &RenderOptions<B>,
    name: &str,
    value: &super::FormattedValue,
) -> fmt::Result {
    let value_str = layout.get_string(value.span);
    let formatted = format!("{}=\"{}\"", name, value_str);
    opts.backend
        .write_styled(w, &formatted, SemanticColor::Unchanged)
}

fn render_attr_deleted<W: Write, B: ColorBackend>(
    layout: &Layout,
    w: &mut W,
    opts: &RenderOptions<B>,
    name: &str,
    value: &super::FormattedValue,
) -> fmt::Result {
    let value_str = layout.get_string(value.span);
    // Key stays white, value and quotes are colored
    write!(w, "{}=", name)?;
    let with_quotes = format!("\"{}\"", value_str);
    opts.backend
        .write_styled(w, &with_quotes, SemanticColor::Deleted)
}

fn render_attr_inserted<W: Write, B: ColorBackend>(
    layout: &Layout,
    w: &mut W,
    opts: &RenderOptions<B>,
    name: &str,
    value: &super::FormattedValue,
) -> fmt::Result {
    let value_str = layout.get_string(value.span);
    // Key stays white, value and quotes are colored
    write!(w, "{}=", name)?;
    let with_quotes = format!("\"{}\"", value_str);
    opts.backend
        .write_styled(w, &with_quotes, SemanticColor::Inserted)
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

#[cfg(test)]
mod tests {
    use indextree::Arena;

    use super::*;
    use crate::layout::{Attr, FormatArena, FormattedValue, Layout, LayoutNode};

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
        let output = render_to_string(&layout, &opts);

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
        let output = render_to_string(&layout, &opts);

        assert!(output.contains("<!-- 5 unchanged -->"));
    }

    #[test]
    fn test_render_with_children() {
        let mut strings = FormatArena::new();
        let mut tree = Arena::new();

        // Parent element
        let parent = tree.new_node(LayoutNode::Element {
            tag: "svg",
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
        let output = render_to_string(&layout, &opts);

        assert!(output.contains("<svg>"));
        assert!(output.contains("</svg>"));
        assert!(output.contains("<rect"));
    }

    #[test]
    fn test_ansi_backend_produces_escapes() {
        let layout = make_test_layout();
        let opts = RenderOptions::default();
        let output = render_to_string(&layout, &opts);

        // Should contain ANSI escape codes
        assert!(
            output.contains("\x1b["),
            "output should contain ANSI escapes"
        );
    }
}
