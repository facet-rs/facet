//! Attribute types and grouping algorithms.

use std::borrow::Cow;

use super::Span;

/// Type of a formatted value for color selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ValueType {
    /// String type (green-based colors)
    String,
    /// Numeric type (orange-based colors)
    Number,
    /// Boolean type (orange-based colors)
    Boolean,
    /// Null/None type (cyan-based colors)
    Null,
    /// Other/unknown types (use accent color)
    #[default]
    Other,
}

/// A pre-formatted value with its measurements.
#[derive(Copy, Clone, Debug, Default)]
pub struct FormattedValue {
    /// Span into the FormatArena (the formatted, escaped value)
    pub span: Span,
    /// Display width of the value (unicode-aware)
    pub width: usize,
    /// Type of the value for color selection
    pub value_type: ValueType,
}

impl FormattedValue {
    /// Create a new formatted value with unknown type.
    pub const fn new(span: Span, width: usize) -> Self {
        Self {
            span,
            width,
            value_type: ValueType::Other,
        }
    }

    /// Create a new formatted value with a specific type.
    pub const fn with_type(span: Span, width: usize, value_type: ValueType) -> Self {
        Self {
            span,
            width,
            value_type,
        }
    }
}

/// The change status of an attribute.
#[derive(Clone, Debug)]
pub enum AttrStatus {
    /// Unchanged (shown dimmed)
    Unchanged { value: FormattedValue },
    /// Changed (shown as -/+ line pair)
    Changed {
        old: FormattedValue,
        new: FormattedValue,
    },
    /// Deleted (only in old, shown with - prefix)
    Deleted { value: FormattedValue },
    /// Inserted (only in new, shown with + prefix)
    Inserted { value: FormattedValue },
}

/// A single attribute with its formatting info.
#[derive(Clone, Debug)]
pub struct Attr {
    /// Attribute name (can be borrowed for static struct fields or owned for dynamic values)
    pub name: Cow<'static, str>,
    /// Display width of the name
    pub name_width: usize,
    /// Change status with formatted values
    pub status: AttrStatus,
}

impl Attr {
    /// Create an unchanged attribute.
    pub fn unchanged(
        name: impl Into<Cow<'static, str>>,
        name_width: usize,
        value: FormattedValue,
    ) -> Self {
        Self {
            name: name.into(),
            name_width,
            status: AttrStatus::Unchanged { value },
        }
    }

    /// Create a changed attribute.
    pub fn changed(
        name: impl Into<Cow<'static, str>>,
        name_width: usize,
        old: FormattedValue,
        new: FormattedValue,
    ) -> Self {
        Self {
            name: name.into(),
            name_width,
            status: AttrStatus::Changed { old, new },
        }
    }

    /// Create a deleted attribute.
    pub fn deleted(
        name: impl Into<Cow<'static, str>>,
        name_width: usize,
        value: FormattedValue,
    ) -> Self {
        Self {
            name: name.into(),
            name_width,
            status: AttrStatus::Deleted { value },
        }
    }

    /// Create an inserted attribute.
    pub fn inserted(
        name: impl Into<Cow<'static, str>>,
        name_width: usize,
        value: FormattedValue,
    ) -> Self {
        Self {
            name: name.into(),
            name_width,
            status: AttrStatus::Inserted { value },
        }
    }

    /// Check if this attribute is changed (needs -/+ lines).
    pub const fn is_changed(&self) -> bool {
        matches!(self.status, AttrStatus::Changed { .. })
    }

    /// Get the width this attribute takes on a line.
    /// Format: `name="value"` -> name_width + 2 (for =") + value_width + 1 (closing ")
    pub fn line_width(&self) -> usize {
        let value_width = match &self.status {
            AttrStatus::Unchanged { value } => value.width,
            AttrStatus::Changed { old, new } => old.width.max(new.width),
            AttrStatus::Deleted { value } => value.width,
            AttrStatus::Inserted { value } => value.width,
        };
        // name="value"
        // ^^^^^       = name_width
        //      ^^     = ="
        //        ^^^^^ = value_width
        //             ^ = "
        self.name_width + 2 + value_width + 1
    }
}

/// A group of changed attributes that fit on one -/+ line pair.
///
/// Attributes in a group are aligned vertically:
/// ```text
/// - fill="red"   x="10"
/// + fill="blue"  x="20"
/// ```
#[derive(Clone, Debug, Default)]
pub struct ChangedGroup {
    /// Indices into the parent's attrs vec
    pub attr_indices: Vec<usize>,
    /// Max name width in this group (for alignment)
    pub max_name_width: usize,
    /// Max old value width in this group (for alignment)
    pub max_old_width: usize,
    /// Max new value width in this group (for alignment)
    pub max_new_width: usize,
}

impl ChangedGroup {
    /// Create a new empty group.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an attribute to this group, updating max widths.
    pub fn add(&mut self, index: usize, attr: &Attr) {
        self.attr_indices.push(index);
        self.max_name_width = self.max_name_width.max(attr.name_width);

        if let AttrStatus::Changed { old, new } = &attr.status {
            self.max_old_width = self.max_old_width.max(old.width);
            self.max_new_width = self.max_new_width.max(new.width);
        }
    }

    /// Check if this group is empty.
    pub const fn is_empty(&self) -> bool {
        self.attr_indices.is_empty()
    }

    /// Calculate the line width for this group's - line.
    /// Does not include the "- " prefix or leading indent.
    pub fn minus_line_width(&self, attrs: &[Attr]) -> usize {
        if self.attr_indices.is_empty() {
            return 0;
        }

        let mut width = 0;
        for (i, &idx) in self.attr_indices.iter().enumerate() {
            if i > 0 {
                width += 1; // space between attrs
            }
            let attr = &attrs[idx];
            if let AttrStatus::Changed { .. } = &attr.status {
                // name="value" with padding
                // Pad name to max_name_width, pad value to max_old_width
                width += self.max_name_width + 2 + self.max_old_width + 1;
            }
        }
        width
    }

    /// Calculate the line width for this group's + line.
    pub fn plus_line_width(&self, attrs: &[Attr]) -> usize {
        if self.attr_indices.is_empty() {
            return 0;
        }

        let mut width = 0;
        for (i, &idx) in self.attr_indices.iter().enumerate() {
            if i > 0 {
                width += 1; // space between attrs
            }
            let attr = &attrs[idx];
            if let AttrStatus::Changed { .. } = &attr.status {
                // Pad name to max_name_width, pad value to max_new_width
                width += self.max_name_width + 2 + self.max_new_width + 1;
            }
        }
        width
    }
}

/// Group changed attributes into lines that fit within max line width.
///
/// Uses greedy bin-packing: add attributes to the current group until
/// the next one would exceed the width limit, then start a new group.
pub fn group_changed_attrs(
    attrs: &[Attr],
    max_line_width: usize,
    indent_width: usize,
) -> Vec<ChangedGroup> {
    let available_width = max_line_width.saturating_sub(indent_width + 2); // "- " prefix

    let mut groups = Vec::new();
    let mut current = ChangedGroup::new();
    let mut current_width = 0usize;

    for (i, attr) in attrs.iter().enumerate() {
        if !attr.is_changed() {
            continue;
        }

        let attr_width = attr.line_width();
        let needed = if current.is_empty() {
            attr_width
        } else {
            attr_width + 1 // space before
        };

        if current_width + needed > available_width && !current.is_empty() {
            // Start new group
            groups.push(std::mem::take(&mut current));
            current_width = 0;
        }

        let needed = if current.is_empty() {
            attr_width
        } else {
            attr_width + 1
        };
        current_width += needed;
        current.add(i, attr);
    }

    if !current.is_empty() {
        groups.push(current);
    }

    groups
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_changed_attr(name: &'static str, old_width: usize, new_width: usize) -> Attr {
        Attr::changed(
            name,
            name.len(),
            FormattedValue::new(Span::default(), old_width),
            FormattedValue::new(Span::default(), new_width),
        )
    }

    #[test]
    fn test_attr_line_width() {
        // fill="red" -> 4 + 2 + 3 + 1 = 10
        let attr = make_changed_attr("fill", 3, 4);
        assert_eq!(attr.line_width(), 4 + 2 + 4 + 1); // uses max(old, new)
    }

    #[test]
    fn test_group_single_attr() {
        let attrs = vec![make_changed_attr("fill", 3, 4)];
        let groups = group_changed_attrs(&attrs, 80, 0);

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].attr_indices, vec![0]);
    }

    #[test]
    fn test_group_multiple_fit_one_line() {
        // fill="red" x="10" -> fits in 80 chars
        let attrs = vec![
            make_changed_attr("fill", 3, 4),
            make_changed_attr("x", 2, 2),
        ];
        let groups = group_changed_attrs(&attrs, 80, 0);

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].attr_indices, vec![0, 1]);
    }

    #[test]
    fn test_group_overflow_to_second_line() {
        // Very narrow width forces split
        let attrs = vec![
            make_changed_attr("fill", 3, 4),
            make_changed_attr("x", 2, 2),
        ];
        // fill="xxxx" = 4 + 2 + 4 + 1 = 11
        // With "- " prefix = 13
        // x="xx" = 1 + 2 + 2 + 1 = 6
        // Total = 19 + 1 (space) = 20
        let groups = group_changed_attrs(&attrs, 15, 0);

        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].attr_indices, vec![0]);
        assert_eq!(groups[1].attr_indices, vec![1]);
    }

    #[test]
    fn test_group_skips_unchanged() {
        let attrs = vec![
            make_changed_attr("fill", 3, 4),
            Attr::unchanged("x", 1, FormattedValue::new(Span::default(), 2)),
            make_changed_attr("y", 2, 2),
        ];
        let groups = group_changed_attrs(&attrs, 80, 0);

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].attr_indices, vec![0, 2]); // skipped index 1
    }

    #[test]
    fn test_group_max_widths() {
        let attrs = vec![
            make_changed_attr("fill", 3, 4),   // old=3, new=4
            make_changed_attr("stroke", 5, 3), // old=5, new=3
        ];
        let groups = group_changed_attrs(&attrs, 80, 0);

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].max_name_width, 6); // "stroke"
        assert_eq!(groups[0].max_old_width, 5);
        assert_eq!(groups[0].max_new_width, 4);
    }
}
