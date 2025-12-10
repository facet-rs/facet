//! Structural sameness checking for Facet types.

use confusables::Confusable;
use facet_core::Facet;
use facet_diff::FacetDiff;
use facet_diff_core::{Diff, Path, PathSegment as DiffPathSegment};
use facet_pretty::PrettyPrinter;
use facet_reflect::{Peek, ScalarType};
use std::borrow::Cow;

/// Options for customizing structural comparison behavior.
///
/// Use the builder pattern to configure options:
///
/// ```
/// use facet_assert::SameOptions;
///
/// let options = SameOptions::new()
///     .float_tolerance(1e-6);
/// ```
#[derive(Debug, Clone, Default)]
pub struct SameOptions {
    /// Tolerance for floating-point comparisons.
    /// If set, two floats are considered equal if their absolute difference
    /// is less than or equal to this value.
    float_tolerance: Option<f64>,
}

impl SameOptions {
    /// Create a new `SameOptions` with default settings (exact comparison).
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the tolerance for floating-point comparisons.
    ///
    /// When set, two `f32` or `f64` values are considered equal if:
    /// `|left - right| <= tolerance`
    ///
    /// # Example
    ///
    /// ```
    /// use facet_assert::{assert_same_with, SameOptions};
    ///
    /// let a = 1.0000001_f64;
    /// let b = 1.0000002_f64;
    ///
    /// // This would fail with exact comparison:
    /// // assert_same!(a, b);
    ///
    /// // But passes with tolerance:
    /// assert_same_with!(a, b, SameOptions::new().float_tolerance(1e-6));
    /// ```
    pub fn float_tolerance(mut self, tolerance: f64) -> Self {
        self.float_tolerance = Some(tolerance);
        self
    }
}

/// Result of checking if two values are structurally the same.
pub enum Sameness {
    /// The values are structurally the same.
    Same,
    /// The values differ - contains a formatted diff.
    Different(String),
    /// Encountered an opaque type that cannot be compared.
    Opaque {
        /// The type name of the opaque type.
        type_name: &'static str,
    },
}

/// Check if two Facet values are structurally the same.
///
/// This does NOT require `PartialEq` - it walks the structure via reflection.
/// Two values are "same" if they have the same structure and values, even if
/// they have different type names.
///
/// Returns [`Sameness::Opaque`] if either value contains an opaque type.
pub fn check_same<'f, T: Facet<'f>, U: Facet<'f>>(left: &T, right: &U) -> Sameness {
    check_same_with(left, right, SameOptions::default())
}

/// Check if two Facet values are structurally the same, with custom options.
///
/// Like [`check_same`], but allows configuring comparison behavior via [`SameOptions`].
///
/// # Example
///
/// ```
/// use facet_assert::{check_same_with, SameOptions, Sameness};
///
/// let a = 1.0000001_f64;
/// let b = 1.0000002_f64;
///
/// // With tolerance, these are considered the same
/// let options = SameOptions::new().float_tolerance(1e-6);
/// assert!(matches!(check_same_with(&a, &b, options), Sameness::Same));
/// ```
pub fn check_same_with<'f, T: Facet<'f>, U: Facet<'f>>(
    left: &T,
    right: &U,
    options: SameOptions,
) -> Sameness {
    // Use facet-diff to compute the diff
    let diff = left.diff(right);

    // Convert the diff to our DiffLine format
    let mut converter = DiffConverter::new(options);
    converter.process_diff(&diff, &Path::default());

    if converter.diffs.is_empty() {
        Sameness::Same
    } else {
        Sameness::Different(converter.into_output())
    }
}

/// Converter from facet-diff's Diff to facet-assert's DiffLine format
struct DiffConverter {
    /// Differences found, stored as lines
    diffs: Vec<DiffLine>,
    /// Comparison options
    options: SameOptions,
}

enum DiffLine {
    Changed {
        path: String,
        left: String,
        right: String,
    },
    OnlyLeft {
        path: String,
        value: String,
    },
    OnlyRight {
        path: String,
        value: String,
    },
}

impl DiffConverter {
    fn new(options: SameOptions) -> Self {
        Self {
            diffs: Vec::new(),
            options,
        }
    }

    fn format_value(peek: Peek<'_, '_>) -> String {
        let printer = PrettyPrinter::default()
            .with_colors(false)
            .with_minimal_option_names(true);
        printer.format_peek(peek).to_string()
    }

    fn format_path(path: &Path) -> String {
        if path.0.is_empty() {
            "root".to_string()
        } else {
            let mut s = String::new();
            for seg in &path.0 {
                match seg {
                    DiffPathSegment::Field(name) => s.push_str(&format!(".{}", name)),
                    DiffPathSegment::Index(i) => s.push_str(&format!("[{}]", i)),
                    DiffPathSegment::Variant(name) => s.push_str(&format!("::{}", name)),
                    DiffPathSegment::Key(k) => s.push_str(&format!("[{:?}]", k)),
                }
            }
            s
        }
    }

    fn record_changed(&mut self, path: String, left: String, right: String) {
        self.diffs.push(DiffLine::Changed { path, left, right });
    }

    fn record_only_left(&mut self, path: String, value: String) {
        self.diffs.push(DiffLine::OnlyLeft { path, value });
    }

    fn record_only_right(&mut self, path: String, value: String) {
        self.diffs.push(DiffLine::OnlyRight { path, value });
    }

    fn process_diff(&mut self, diff: &Diff<'_, '_>, current_path: &Path) {
        match diff {
            Diff::Equal { .. } => {
                // No difference, nothing to record
            }
            Diff::Replace { from, to } => {
                // Check if float tolerance applies
                if self.options.float_tolerance.is_some() {
                    if let (Some(from_f64), Some(to_f64)) =
                        (self.try_extract_float(*from), self.try_extract_float(*to))
                    {
                        if self.floats_equal(from_f64, to_f64) {
                            return; // Equal within tolerance
                        }
                    }
                }

                let path_str = Self::format_path(current_path);
                let left_str = Self::format_value(*from);
                let right_str = Self::format_value(*to);
                self.record_changed(path_str, left_str, right_str);
            }
            Diff::User { value, variant, .. } => {
                // Add variant to path if present
                let mut path = current_path.clone();
                if let Some(variant_name) = variant {
                    path.push(DiffPathSegment::Variant(Cow::Borrowed(*variant_name)));
                }

                self.process_value(value, &path);
            }
            Diff::Sequence { updates, .. } => {
                self.process_updates(updates, current_path);
            }
        }
    }

    fn process_value(&mut self, value: &facet_diff_core::Value<'_, '_>, current_path: &Path) {
        use facet_diff_core::Value;

        match value {
            Value::Tuple { updates } => {
                self.process_updates(updates, current_path);
            }
            Value::Struct {
                updates,
                deletions,
                insertions,
                ..
            } => {
                // Process field updates
                for (field_name, field_diff) in updates {
                    let mut path = current_path.clone();
                    path.push(DiffPathSegment::Field(field_name.clone()));
                    self.process_diff(field_diff, &path);
                }

                // Process deletions
                for (field_name, peek) in deletions {
                    let mut path = current_path.clone();
                    path.push(DiffPathSegment::Field(field_name.clone()));
                    let path_str = Self::format_path(&path);
                    let value_str = Self::format_value(*peek);
                    self.record_only_left(path_str, value_str);
                }

                // Process insertions
                for (field_name, peek) in insertions {
                    let mut path = current_path.clone();
                    path.push(DiffPathSegment::Field(field_name.clone()));
                    let path_str = Self::format_path(&path);
                    let value_str = Self::format_value(*peek);
                    self.record_only_right(path_str, value_str);
                }
            }
        }
    }

    fn process_updates(&mut self, updates: &facet_diff_core::Updates<'_, '_>, current_path: &Path) {
        let mut index = 0;

        // Process first update group if present
        if let Some(update_group) = &updates.0.first {
            self.process_update_group(update_group, current_path, &mut index);
        }

        // Process alternating unchanged values and update groups
        for (unchanged, update_group) in &updates.0.values {
            // Skip unchanged items
            index += unchanged.len();
            // Process the update group (contains replace groups interspersed with diffs)
            self.process_update_group(update_group, current_path, &mut index);
        }

        // Process trailing unchanged items if present
        if let Some(unchanged) = &updates.0.last {
            let _ = index + unchanged.len(); // just for tracking, not used
        }
    }

    fn process_update_group(
        &mut self,
        update_group: &facet_diff_core::UpdatesGroup<'_, '_>,
        current_path: &Path,
        index: &mut usize,
    ) {
        // Process first replace group if present
        if let Some(replace_group) = &update_group.0.first {
            self.process_replace_group(replace_group, current_path, index);
        }

        // Process alternating diffs and replace groups
        for (diffs, replace_group) in &update_group.0.values {
            // Process nested diffs
            for diff in diffs {
                let mut path = current_path.clone();
                path.push(DiffPathSegment::Index(*index));
                self.process_diff(diff, &path);
                *index += 1;
            }
            // Process replace group
            self.process_replace_group(replace_group, current_path, index);
        }

        // Process trailing diffs if present
        if let Some(diffs) = &update_group.0.last {
            for diff in diffs {
                let mut path = current_path.clone();
                path.push(DiffPathSegment::Index(*index));
                self.process_diff(diff, &path);
                *index += 1;
            }
        }
    }

    fn process_replace_group(
        &mut self,
        replace_group: &facet_diff_core::ReplaceGroup<'_, '_>,
        current_path: &Path,
        index: &mut usize,
    ) {
        // If both sides have the same number of items, try to pair them up for comparison
        if replace_group.removals.len() == replace_group.additions.len() {
            for i in 0..replace_group.removals.len() {
                let from = replace_group.removals[i];
                let to = replace_group.additions[i];

                // Check if they're floats within tolerance
                let is_equal_within_tolerance = if self.options.float_tolerance.is_some() {
                    if let (Some(from_f64), Some(to_f64)) =
                        (self.try_extract_float(from), self.try_extract_float(to))
                    {
                        self.floats_equal(from_f64, to_f64)
                    } else {
                        false
                    }
                } else {
                    false
                };

                if !is_equal_within_tolerance {
                    // Record as a change
                    let mut path = current_path.clone();
                    path.push(DiffPathSegment::Index(*index));
                    let path_str = Self::format_path(&path);
                    let left_str = Self::format_value(from);
                    let right_str = Self::format_value(to);
                    self.record_changed(path_str, left_str, right_str);
                }
                *index += 1;
            }
            return;
        }

        // Different lengths - record as separate removals and additions
        // Record all removed items
        for from_peek in &replace_group.removals {
            let mut path = current_path.clone();
            path.push(DiffPathSegment::Index(*index));
            let path_str = Self::format_path(&path);
            let value_str = Self::format_value(*from_peek);
            self.record_only_left(path_str, value_str);
            *index += 1;
        }

        // Record all added items (use the starting index)
        let start_index = *index - replace_group.removals.len();
        for (i, to_peek) in replace_group.additions.iter().enumerate() {
            let mut path = current_path.clone();
            path.push(DiffPathSegment::Index(start_index + i));
            let path_str = Self::format_path(&path);
            let value_str = Self::format_value(*to_peek);
            self.record_only_right(path_str, value_str);
        }

        // Adjust index for net change
        if replace_group.additions.len() > replace_group.removals.len() {
            *index = start_index + replace_group.additions.len();
        } else {
            *index = start_index + replace_group.removals.len();
        }
    }

    /// Compare two f64 values, using tolerance if configured.
    fn floats_equal(&self, left: f64, right: f64) -> bool {
        if let Some(tolerance) = self.options.float_tolerance {
            (left - right).abs() <= tolerance
        } else {
            left == right
        }
    }

    /// Try to extract f64 from a Peek value if it's a float.
    fn try_extract_float(&self, peek: Peek<'_, '_>) -> Option<f64> {
        match peek.scalar_type()? {
            ScalarType::F64 => Some(*peek.get::<f64>().ok()?),
            ScalarType::F32 => Some(*peek.get::<f32>().ok()? as f64),
            _ => None,
        }
    }

    fn into_output(self) -> String {
        use std::fmt::Write;

        let mut out = String::new();

        for diff in self.diffs {
            match diff {
                DiffLine::Changed { path, left, right } => {
                    writeln!(out, "\x1b[1m{path}\x1b[0m:").unwrap();
                    writeln!(out, "  \x1b[31m- {left}\x1b[0m").unwrap();
                    writeln!(out, "  \x1b[32m+ {right}\x1b[0m").unwrap();

                    // Check if the strings are confusable (look identical but differ)
                    // Use the confusables crate for detection, then show character-level diff
                    let left_normalized = left.replace_confusable();
                    let right_normalized = right.replace_confusable();
                    if left_normalized == right_normalized
                        && let Some(explanation) = explain_confusable_differences(&left, &right)
                    {
                        writeln!(out, "  \x1b[33m{}\x1b[0m", explanation).unwrap();
                    }
                }
                DiffLine::OnlyLeft { path, value } => {
                    writeln!(out, "\x1b[1m{path}\x1b[0m (only in left):").unwrap();
                    writeln!(out, "  \x1b[31m- {value}\x1b[0m").unwrap();
                }
                DiffLine::OnlyRight { path, value } => {
                    writeln!(out, "\x1b[1m{path}\x1b[0m (only in right):").unwrap();
                    writeln!(out, "  \x1b[32m+ {value}\x1b[0m").unwrap();
                }
            }
        }

        out
    }
}

/// Format a character for display with its Unicode codepoint and visual representation.
fn format_char_with_codepoint(c: char) -> String {
    // For printable ASCII characters (except space), show the character directly
    if c.is_ascii_graphic() {
        format!("'{}' (U+{:04X})", c, c as u32)
    } else {
        // For everything else, show escaped form with codepoint
        format!("'\\u{{{:04X}}}' (U+{:04X})", c as u32, c as u32)
    }
}

/// Explain the confusable differences between two strings that look identical.
/// Uses the `confusables` crate for detection, then shows character-level diff.
fn explain_confusable_differences(left: &str, right: &str) -> Option<String> {
    // Strings must be different but normalize to the same skeleton
    if left == right {
        return None;
    }

    // Find character-level differences
    let left_chars: Vec<char> = left.chars().collect();
    let right_chars: Vec<char> = right.chars().collect();

    use std::fmt::Write;
    let mut out = String::new();

    // Find all positions where characters differ
    let mut diffs: Vec<(usize, char, char)> = Vec::new();

    let max_len = left_chars.len().max(right_chars.len());
    for i in 0..max_len {
        let lc = left_chars.get(i);
        let rc = right_chars.get(i);

        match (lc, rc) {
            (Some(&l), Some(&r)) if l != r => {
                diffs.push((i, l, r));
            }
            (Some(&l), None) => {
                // Character only in left (will show as deletion)
                diffs.push((i, l, '\0'));
            }
            (None, Some(&r)) => {
                // Character only in right (will show as insertion)
                diffs.push((i, '\0', r));
            }
            _ => {}
        }
    }

    if diffs.is_empty() {
        return None;
    }

    writeln!(
        out,
        "(strings are visually confusable but differ in {} position{}):",
        diffs.len(),
        if diffs.len() == 1 { "" } else { "s" }
    )
    .ok()?;

    for (pos, lc, rc) in &diffs {
        if *lc == '\0' {
            writeln!(
                out,
                "  [{}]: (missing) vs {}",
                pos,
                format_char_with_codepoint(*rc)
            )
            .ok()?;
        } else if *rc == '\0' {
            writeln!(
                out,
                "  [{}]: {} vs (missing)",
                pos,
                format_char_with_codepoint(*lc)
            )
            .ok()?;
        } else {
            writeln!(
                out,
                "  [{}]: {} vs {}",
                pos,
                format_char_with_codepoint(*lc),
                format_char_with_codepoint(*rc)
            )
            .ok()?;
        }
    }

    Some(out.trim_end().to_string())
}
