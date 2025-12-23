//! Layout rendering to output.

use std::fmt::{self, Write};

use super::backend::{AnsiBackend, ColorBackend, PlainBackend, SemanticColor};
use super::flavor::DiffFlavor;
use super::{AttrStatus, ChangedGroup, ElementChange, Layout, LayoutNode, ValueType};
use crate::DiffSymbols;

/// Syntax element type for context-aware coloring.
#[derive(Clone, Copy)]
#[allow(dead_code)]
enum SyntaxElement {
    Key,
    Structure,
    Comment,
}

/// Get the appropriate semantic color for a syntax element in a given context.
fn syntax_color(base: SyntaxElement, context: ElementChange) -> SemanticColor {
    match (base, context) {
        (SyntaxElement::Key, ElementChange::Deleted) => SemanticColor::DeletedKey,
        (SyntaxElement::Key, ElementChange::Inserted) => SemanticColor::InsertedKey,
        (SyntaxElement::Key, _) => SemanticColor::Key,

        (SyntaxElement::Structure, ElementChange::Deleted) => SemanticColor::DeletedStructure,
        (SyntaxElement::Structure, ElementChange::Inserted) => SemanticColor::InsertedStructure,
        (SyntaxElement::Structure, _) => SemanticColor::Structure,

        (SyntaxElement::Comment, ElementChange::Deleted) => SemanticColor::DeletedComment,
        (SyntaxElement::Comment, ElementChange::Inserted) => SemanticColor::InsertedComment,
        (SyntaxElement::Comment, _) => SemanticColor::Comment,
    }
}

/// Get the appropriate semantic color for a value based on its type and context.
fn value_color(value_type: ValueType, context: ElementChange) -> SemanticColor {
    match (value_type, context) {
        (ValueType::String, ElementChange::Deleted) => SemanticColor::DeletedString,
        (ValueType::String, ElementChange::Inserted) => SemanticColor::InsertedString,
        (ValueType::String, _) => SemanticColor::String,

        (ValueType::Number, ElementChange::Deleted) => SemanticColor::DeletedNumber,
        (ValueType::Number, ElementChange::Inserted) => SemanticColor::InsertedNumber,
        (ValueType::Number, _) => SemanticColor::Number,

        (ValueType::Boolean, ElementChange::Deleted) => SemanticColor::DeletedBoolean,
        (ValueType::Boolean, ElementChange::Inserted) => SemanticColor::InsertedBoolean,
        (ValueType::Boolean, _) => SemanticColor::Boolean,

        (ValueType::Null, ElementChange::Deleted) => SemanticColor::DeletedNull,
        (ValueType::Null, ElementChange::Inserted) => SemanticColor::InsertedNull,
        (ValueType::Null, _) => SemanticColor::Null,

        // Other/unknown types use accent colors
        (ValueType::Other, ElementChange::Deleted) => SemanticColor::Deleted,
        (ValueType::Other, ElementChange::Inserted) => SemanticColor::Inserted,
        (ValueType::Other, ElementChange::MovedFrom)
        | (ValueType::Other, ElementChange::MovedTo) => SemanticColor::Moved,
        (ValueType::Other, ElementChange::None) => SemanticColor::Unchanged,
    }
}

/// Get semantic color for highlight background (changed values).
fn value_color_highlight(value_type: ValueType, context: ElementChange) -> SemanticColor {
    match (value_type, context) {
        (ValueType::String, ElementChange::Deleted) => SemanticColor::DeletedString,
        (ValueType::String, ElementChange::Inserted) => SemanticColor::InsertedString,

        (ValueType::Number, ElementChange::Deleted) => SemanticColor::DeletedNumber,
        (ValueType::Number, ElementChange::Inserted) => SemanticColor::InsertedNumber,

        (ValueType::Boolean, ElementChange::Deleted) => SemanticColor::DeletedBoolean,
        (ValueType::Boolean, ElementChange::Inserted) => SemanticColor::InsertedBoolean,

        (ValueType::Null, ElementChange::Deleted) => SemanticColor::DeletedNull,
        (ValueType::Null, ElementChange::Inserted) => SemanticColor::InsertedNull,

        // Highlight uses generic highlights for Other/unchanged
        (_, ElementChange::Deleted) => SemanticColor::DeletedHighlight,
        (_, ElementChange::Inserted) => SemanticColor::InsertedHighlight,
        (_, ElementChange::MovedFrom) | (_, ElementChange::MovedTo) => {
            SemanticColor::MovedHighlight
        }
        _ => SemanticColor::Unchanged,
    }
}

/// Information for inline element diff rendering.
/// When all attributes fit on one line, we render the full element on each -/+ line.
struct InlineElementInfo {
    /// Width of each attr slot (padded to max of old/new values)
    slot_widths: Vec<usize>,
}

impl InlineElementInfo {
    /// Calculate inline element info if all attrs fit on one line.
    /// Returns None if the element is not suitable for inline rendering.
    fn calculate<F: DiffFlavor>(
        attrs: &[super::Attr],
        tag: &str,
        flavor: &F,
        max_line_width: usize,
        indent_width: usize,
    ) -> Option<Self> {
        if attrs.is_empty() {
            return None;
        }

        let mut slot_widths = Vec::with_capacity(attrs.len());
        let mut total_width = 0usize;

        // struct_open (e.g., "<Point" or "Point {")
        total_width += flavor.struct_open(tag).len();

        for (i, attr) in attrs.iter().enumerate() {
            // Space or separator before attr
            if i > 0 {
                total_width += flavor.field_separator().len();
            } else {
                total_width += 1; // space after opening
            }

            // Calculate slot width for this attr (max of old/new/both)
            let slot_width = match &attr.status {
                AttrStatus::Unchanged { value } => {
                    // name="value" -> prefix + value + suffix
                    flavor.format_field_prefix(&attr.name).len()
                        + value.width
                        + flavor.format_field_suffix().len()
                }
                AttrStatus::Changed { old, new } => {
                    let max_val = old.width.max(new.width);
                    flavor.format_field_prefix(&attr.name).len()
                        + max_val
                        + flavor.format_field_suffix().len()
                }
                AttrStatus::Deleted { value } => {
                    flavor.format_field_prefix(&attr.name).len()
                        + value.width
                        + flavor.format_field_suffix().len()
                }
                AttrStatus::Inserted { value } => {
                    flavor.format_field_prefix(&attr.name).len()
                        + value.width
                        + flavor.format_field_suffix().len()
                }
            };

            slot_widths.push(slot_width);
            total_width += slot_width;
        }

        // struct_close (e.g., "/>" or "}")
        total_width += 1; // space before close for XML
        total_width += flavor.struct_close(tag, true).len();

        // Check if it fits (account for "- " prefix and indent)
        let available = max_line_width.saturating_sub(indent_width + 2);
        if total_width > available {
            return None;
        }

        Some(Self { slot_widths })
    }
}

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
            indent: "    ",
        }
    }
}

impl RenderOptions<PlainBackend> {
    /// Create options with plain backend (no colors).
    pub fn plain() -> Self {
        Self {
            symbols: DiffSymbols::default(),
            backend: PlainBackend,
            indent: "    ",
        }
    }
}

impl<B: ColorBackend> RenderOptions<B> {
    /// Create options with a custom backend.
    pub fn with_backend(backend: B) -> Self {
        Self {
            symbols: DiffSymbols::default(),
            backend,
            indent: "    ",
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
            let tag = tag.as_ref();
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

            let semantic = value_color(value.value_type, change);
            opts.backend.write_styled(w, text, semantic)?;
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
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    write!(w, "{}", flavor.item_separator())?;
                }
                let raw_value = layout.get_string(item.span);
                let formatted = flavor.format_seq_item(item_type, raw_value);
                let semantic = value_color(item.value_type, change);
                opts.backend.write_styled(w, &formatted, semantic)?;
            }

            // Render collapsed suffix if present (context-aware)
            if let Some(count) = collapsed_suffix {
                let suffix = flavor.comment(&format!("{} more", count));
                write!(w, " ")?;
                opts.backend.write_styled(
                    w,
                    &suffix,
                    syntax_color(SyntaxElement::Comment, change),
                )?;
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
    // Handle transparent elements - render children only without wrapper
    // This is used for Option types which should not create XML elements
    if tag == "_transparent" {
        for child_id in layout.children(node_id) {
            render_node(layout, w, child_id, depth, opts, flavor)?;
        }
        return Ok(());
    }

    // Check what kinds of attribute changes we have
    let has_changed_attrs = !changed_groups.is_empty();
    let has_deleted_attrs = attrs
        .iter()
        .any(|a| matches!(a.status, AttrStatus::Deleted { .. }));
    let has_inserted_attrs = attrs
        .iter()
        .any(|a| matches!(a.status, AttrStatus::Inserted { .. }));

    // Pure insertion: all non-unchanged attrs are Inserted (no Changed or Deleted)
    // These should render as a single + line, not ← → pairs with ∅ placeholders
    let is_pure_insertion = has_inserted_attrs && !has_changed_attrs && !has_deleted_attrs;

    // Pure deletion: all non-unchanged attrs are Deleted (no Changed or Inserted)
    // These should render as a single - line, not ← → pairs with ∅ placeholders
    let is_pure_deletion = has_deleted_attrs && !has_changed_attrs && !has_inserted_attrs;

    let has_attr_changes = has_changed_attrs || has_deleted_attrs || has_inserted_attrs;

    let children: Vec<_> = layout.children(node_id).collect();
    let has_children = !children.is_empty();

    // Check if we can render as inline element diff (all attrs on one -/+ line pair)
    // This is only viable when:
    // 1. There are attribute changes (otherwise no need for -/+ lines)
    // 2. No children (self-closing element)
    // 3. All attrs fit on one line
    // 4. NOT a pure insertion/deletion (those should use single line with +/- prefix)
    if has_attr_changes && !has_children && !is_pure_insertion && !is_pure_deletion {
        let indent_width = depth * opts.indent.len();
        if let Some(info) = InlineElementInfo::calculate(attrs, tag, flavor, 80, indent_width) {
            return render_inline_element(
                layout, w, depth, opts, flavor, tag, field_name, attrs, &info,
            );
        }
    }

    let tag_color = match change {
        ElementChange::None => SemanticColor::Structure,
        ElementChange::Deleted => SemanticColor::DeletedStructure,
        ElementChange::Inserted => SemanticColor::InsertedStructure,
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

    // Render type comment in muted color if present (context-aware)
    if let Some(comment) = flavor.type_comment(tag) {
        write!(w, " ")?;
        opts.backend
            .write_styled(w, &comment, syntax_color(SyntaxElement::Comment, change))?;
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
                // Trailing comma (no highlight background)
                opts.backend.write_styled(
                    w,
                    flavor.trailing_separator(),
                    SemanticColor::Whitespace,
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
                // Trailing comma (no highlight background)
                opts.backend.write_styled(
                    w,
                    flavor.trailing_separator(),
                    SemanticColor::Whitespace,
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
            // Trailing comma (no background)
            opts.backend
                .write_styled(w, flavor.trailing_separator(), SemanticColor::Whitespace)?;
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
            // Trailing comma (no background)
            opts.backend
                .write_styled(w, flavor.trailing_separator(), SemanticColor::Whitespace)?;
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

/// Render an element with all attrs on one line per -/+ row.
/// This is used when all attrs fit on a single line for a more compact diff.
#[allow(clippy::too_many_arguments)]
fn render_inline_element<W: Write, B: ColorBackend, F: DiffFlavor>(
    layout: &Layout,
    w: &mut W,
    depth: usize,
    opts: &RenderOptions<B>,
    flavor: &F,
    tag: &str,
    field_name: Option<&str>,
    attrs: &[super::Attr],
    info: &InlineElementInfo,
) -> fmt::Result {
    // Render field name prefix if present (for nested struct fields)
    let field_prefix = field_name.map(|name| flavor.format_child_open(name));
    let open = flavor.struct_open(tag);
    let close = flavor.struct_close(tag, true);

    // --- Before line (old values) ---
    // Line background applies to structural parts, highlight background to changed values
    // Use ← for "changed from" (vs - for "deleted entirely")
    write_indent_minus_prefix(w, depth, opts)?;
    opts.backend.write_prefix(w, '←', SemanticColor::Deleted)?;
    opts.backend.write_styled(w, " ", SemanticColor::Deleted)?;

    // Field name prefix (line bg)
    if let Some(ref prefix) = field_prefix
        && !prefix.is_empty()
    {
        opts.backend
            .write_styled(w, prefix, SemanticColor::Deleted)?;
    }

    // Opening tag (line bg, with deleted context blending)
    opts.backend
        .write_styled(w, &open, SemanticColor::DeletedStructure)?;

    // Attributes (old values or spaces for inserted)
    for (i, (attr, &slot_width)) in attrs.iter().zip(info.slot_widths.iter()).enumerate() {
        if i > 0 {
            opts.backend
                .write_styled(w, flavor.field_separator(), SemanticColor::Whitespace)?;
        } else {
            opts.backend.write_styled(w, " ", SemanticColor::Deleted)?;
        }

        let written = match &attr.status {
            AttrStatus::Unchanged { value } => {
                // Unchanged: context-aware colors for structural elements
                opts.backend.write_styled(
                    w,
                    &flavor.format_field_prefix(&attr.name),
                    SemanticColor::DeletedKey,
                )?;
                let val = layout.get_string(value.span);
                let color = value_color(value.value_type, ElementChange::Deleted);
                opts.backend.write_styled(w, val, color)?;
                opts.backend.write_styled(
                    w,
                    flavor.format_field_suffix(),
                    SemanticColor::DeletedStructure,
                )?;
                flavor.format_field_prefix(&attr.name).len()
                    + value.width
                    + flavor.format_field_suffix().len()
            }
            AttrStatus::Changed { old, .. } => {
                // Changed: context-aware key color, highlight bg for value only
                opts.backend.write_styled(
                    w,
                    &flavor.format_field_prefix(&attr.name),
                    SemanticColor::DeletedKey,
                )?;
                let val = layout.get_string(old.span);
                let color = value_color_highlight(old.value_type, ElementChange::Deleted);
                opts.backend.write_styled(w, val, color)?;
                opts.backend.write_styled(
                    w,
                    flavor.format_field_suffix(),
                    SemanticColor::DeletedStructure,
                )?;
                flavor.format_field_prefix(&attr.name).len()
                    + old.width
                    + flavor.format_field_suffix().len()
            }
            AttrStatus::Deleted { value } => {
                // Deleted entirely: highlight bg for key AND value
                opts.backend.write_styled(
                    w,
                    &flavor.format_field_prefix(&attr.name),
                    SemanticColor::DeletedHighlight,
                )?;
                let val = layout.get_string(value.span);
                let color = value_color_highlight(value.value_type, ElementChange::Deleted);
                opts.backend.write_styled(w, val, color)?;
                opts.backend.write_styled(
                    w,
                    flavor.format_field_suffix(),
                    SemanticColor::DeletedHighlight,
                )?;
                flavor.format_field_prefix(&attr.name).len()
                    + value.width
                    + flavor.format_field_suffix().len()
            }
            AttrStatus::Inserted { .. } => {
                // Empty slot on minus line - show ∅ placeholder
                opts.backend.write_styled(w, "∅", SemanticColor::Deleted)?;
                1 // ∅ is 1 char wide
            }
        };

        // Pad to slot width (line bg)
        let padding = slot_width.saturating_sub(written);
        if padding > 0 {
            let spaces: String = " ".repeat(padding);
            opts.backend
                .write_styled(w, &spaces, SemanticColor::Whitespace)?;
        }
    }

    // Closing (line bg, with deleted context blending)
    opts.backend.write_styled(w, " ", SemanticColor::Deleted)?;
    opts.backend
        .write_styled(w, &close, SemanticColor::DeletedStructure)?;
    writeln!(w)?;

    // --- After line (new values) ---
    // Use → for "changed to" (vs + for "inserted entirely")
    write_indent_minus_prefix(w, depth, opts)?;
    opts.backend.write_prefix(w, '→', SemanticColor::Inserted)?;
    opts.backend.write_styled(w, " ", SemanticColor::Inserted)?;

    // Field name prefix (line bg)
    if let Some(ref prefix) = field_prefix
        && !prefix.is_empty()
    {
        opts.backend
            .write_styled(w, prefix, SemanticColor::Inserted)?;
    }

    // Opening tag (line bg, with inserted context blending)
    opts.backend
        .write_styled(w, &open, SemanticColor::InsertedStructure)?;

    // Attributes (new values or spaces for deleted)
    for (i, (attr, &slot_width)) in attrs.iter().zip(info.slot_widths.iter()).enumerate() {
        if i > 0 {
            opts.backend
                .write_styled(w, flavor.field_separator(), SemanticColor::Whitespace)?;
        } else {
            opts.backend.write_styled(w, " ", SemanticColor::Inserted)?;
        }

        let written = match &attr.status {
            AttrStatus::Unchanged { value } => {
                // Unchanged: context-aware colors for structural elements
                opts.backend.write_styled(
                    w,
                    &flavor.format_field_prefix(&attr.name),
                    SemanticColor::InsertedKey,
                )?;
                let val = layout.get_string(value.span);
                let color = value_color(value.value_type, ElementChange::Inserted);
                opts.backend.write_styled(w, val, color)?;
                opts.backend.write_styled(
                    w,
                    flavor.format_field_suffix(),
                    SemanticColor::InsertedStructure,
                )?;
                flavor.format_field_prefix(&attr.name).len()
                    + value.width
                    + flavor.format_field_suffix().len()
            }
            AttrStatus::Changed { new, .. } => {
                // Changed: context-aware key color, highlight bg for value only
                opts.backend.write_styled(
                    w,
                    &flavor.format_field_prefix(&attr.name),
                    SemanticColor::InsertedKey,
                )?;
                let val = layout.get_string(new.span);
                let color = value_color_highlight(new.value_type, ElementChange::Inserted);
                opts.backend.write_styled(w, val, color)?;
                opts.backend.write_styled(
                    w,
                    flavor.format_field_suffix(),
                    SemanticColor::InsertedStructure,
                )?;
                flavor.format_field_prefix(&attr.name).len()
                    + new.width
                    + flavor.format_field_suffix().len()
            }
            AttrStatus::Deleted { .. } => {
                // Empty slot on plus line - show ∅ placeholder
                opts.backend.write_styled(w, "∅", SemanticColor::Inserted)?;
                1 // ∅ is 1 char wide
            }
            AttrStatus::Inserted { value } => {
                // Inserted entirely: highlight bg for key AND value
                opts.backend.write_styled(
                    w,
                    &flavor.format_field_prefix(&attr.name),
                    SemanticColor::InsertedHighlight,
                )?;
                let val = layout.get_string(value.span);
                let color = value_color_highlight(value.value_type, ElementChange::Inserted);
                opts.backend.write_styled(w, val, color)?;
                opts.backend.write_styled(
                    w,
                    flavor.format_field_suffix(),
                    SemanticColor::InsertedHighlight,
                )?;
                flavor.format_field_prefix(&attr.name).len()
                    + value.width
                    + flavor.format_field_suffix().len()
            }
        };

        // Pad to slot width (no background for spaces)
        let padding = slot_width.saturating_sub(written);
        if padding > 0 {
            let spaces: String = " ".repeat(padding);
            opts.backend
                .write_styled(w, &spaces, SemanticColor::Whitespace)?;
        }
    }

    // Closing (line bg, with inserted context blending)
    opts.backend.write_styled(w, " ", SemanticColor::Inserted)?;
    opts.backend
        .write_styled(w, &close, SemanticColor::InsertedStructure)?;
    writeln!(w)?;

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
        ElementChange::Deleted => SemanticColor::DeletedStructure,
        ElementChange::Inserted => SemanticColor::InsertedStructure,
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

        // Trailing comma for fields (context-aware)
        if field_name.is_some() {
            opts.backend
                .write_styled(w, flavor.trailing_separator(), SemanticColor::Whitespace)?;
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

    // Trailing comma for fields (context-aware)
    if field_name.is_some() {
        opts.backend
            .write_styled(w, flavor.trailing_separator(), SemanticColor::Whitespace)?;
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
    // Before line - use ← for "changed from" (prefix uses indent gutter)
    write_indent_minus_prefix(w, depth, opts)?;
    opts.backend.write_prefix(w, '←', SemanticColor::Deleted)?;
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
            // Use context-aware key color for field prefix (line bg)
            opts.backend.write_styled(
                w,
                &flavor.format_field_prefix(&attr.name),
                SemanticColor::DeletedKey,
            )?;
            // Changed value uses highlight background for contrast
            let old_str = layout.get_string(old.span);
            let color = value_color_highlight(old.value_type, ElementChange::Deleted);
            opts.backend.write_styled(w, old_str, color)?;
            // Use context-aware structure color for field suffix (line bg)
            opts.backend.write_styled(
                w,
                flavor.format_field_suffix(),
                SemanticColor::DeletedStructure,
            )?;
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

    // After line - use → for "changed to" (prefix uses indent gutter)
    write_indent_minus_prefix(w, depth, opts)?;
    opts.backend.write_prefix(w, '→', SemanticColor::Inserted)?;
    write!(w, " ")?;

    for (i, &idx) in group.attr_indices.iter().enumerate() {
        if i > 0 {
            write!(w, "{}", flavor.field_separator())?;
        }
        let attr = &attrs[idx];
        if let AttrStatus::Changed { old, new } = &attr.status {
            // Each field padded to max of its own old/new value width
            let field_max_width = old.width.max(new.width);
            // Use context-aware key color for field prefix (line bg)
            opts.backend.write_styled(
                w,
                &flavor.format_field_prefix(&attr.name),
                SemanticColor::InsertedKey,
            )?;
            // Changed value uses highlight background for contrast
            let new_str = layout.get_string(new.span);
            let color = value_color_highlight(new.value_type, ElementChange::Inserted);
            opts.backend.write_styled(w, new_str, color)?;
            // Use context-aware structure color for field suffix (line bg)
            opts.backend.write_styled(
                w,
                flavor.format_field_suffix(),
                SemanticColor::InsertedStructure,
            )?;
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
    // Entire field uses highlight background for deleted (better contrast)
    let formatted = flavor.format_field(name, value_str);
    opts.backend
        .write_styled(w, &formatted, SemanticColor::DeletedHighlight)
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
    // Entire field uses highlight background for inserted (better contrast)
    let formatted = flavor.format_field(name, value_str);
    opts.backend
        .write_styled(w, &formatted, SemanticColor::InsertedHighlight)
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
    use std::borrow::Cow;

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
            tag: Cow::Borrowed("rect"),
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

        // With inline element diff, the format uses ← / → for changed state:
        // ← <rect fill="red"  />
        // → <rect fill="blue" />
        assert!(output.contains("← <rect fill=\"red\""));
        assert!(output.contains("→ <rect fill=\"blue\""));
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
            tag: Cow::Borrowed("svg"),
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
            tag: Cow::Borrowed("rect"),
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
