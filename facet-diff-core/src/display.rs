//! Display implementations for diff types.

use std::fmt::{Display, Write};

use confusables::Confusable;
use facet_pretty::{PrettyPrinter, tokyo_night};
use facet_reflect::Peek;
use owo_colors::OwoColorize;

use crate::{Diff, ReplaceGroup, Updates, UpdatesGroup, Value};

/// Format text for deletions
fn deleted(s: &str) -> String {
    format!("{}", s.color(tokyo_night::DELETION))
}

/// Format text for insertions
fn inserted(s: &str) -> String {
    format!("{}", s.color(tokyo_night::INSERTION))
}

/// Format muted text (unchanged indicators, structural equality)
fn muted(s: &str) -> String {
    format!("{}", s.color(tokyo_night::MUTED))
}

/// Format field name
fn field(s: &str) -> String {
    format!("{}", s.color(tokyo_night::FIELD_NAME))
}

/// Format punctuation as dimmed
fn punct(s: &str) -> String {
    format!("{}", s.color(tokyo_night::COMMENT))
}

struct PadAdapter<'a, 'b: 'a> {
    fmt: &'a mut std::fmt::Formatter<'b>,
    on_newline: bool,
    indent: &'static str,
}

impl<'a, 'b> PadAdapter<'a, 'b> {
    const fn new_indented(fmt: &'a mut std::fmt::Formatter<'b>) -> Self {
        Self {
            fmt,
            on_newline: true,
            indent: "    ",
        }
    }
}

impl<'a, 'b> Write for PadAdapter<'a, 'b> {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        for line in s.split_inclusive('\n') {
            if self.on_newline {
                self.fmt.write_str(self.indent)?;
            }

            self.on_newline = line.ends_with('\n');

            self.fmt.write_str(line)?;
        }

        Ok(())
    }

    fn write_char(&mut self, c: char) -> std::fmt::Result {
        if self.on_newline {
            self.fmt.write_str(self.indent)?;
        }

        self.on_newline = c == '\n';
        self.fmt.write_char(c)
    }
}

/// Simple equality check for display purposes.
/// This is used when rendering ReplaceGroup to check if values are equal.
fn peek_eq<'mem, 'facet>(a: Peek<'mem, 'facet>, b: Peek<'mem, 'facet>) -> bool {
    a.shape().id == b.shape().id && a.shape().is_partial_eq() && a == b
}

impl<'mem, 'facet> Display for Diff<'mem, 'facet> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Diff::Equal { value: _ } => {
                write!(f, "{}", muted("(structurally equal)"))
            }
            Diff::Replace { from, to } => {
                let printer = PrettyPrinter::default()
                    .with_colors(facet_pretty::ColorMode::Never)
                    .with_minimal_option_names(true);

                // Check if both values are strings and visually confusable
                // Note: is_confusable_with is directional - check both directions
                if let (Some(from_str), Some(to_str)) = (from.as_str(), to.as_str())
                    && (from_str.is_confusable_with(to_str) || to_str.is_confusable_with(from_str))
                {
                    // Show the strings with character-level diff
                    // Don't wrap in muted() since the explanation has its own colors
                    write!(
                        f,
                        "{} → {}\n{}",
                        deleted(&printer.format_peek(*from)),
                        inserted(&printer.format_peek(*to)),
                        explain_confusable_differences(from_str, to_str)
                    )?;
                    return Ok(());
                }

                // Show value change inline: old → new
                write!(
                    f,
                    "{} → {}",
                    deleted(&printer.format_peek(*from)),
                    inserted(&printer.format_peek(*to))
                )
            }
            Diff::User {
                from: _,
                to: _,
                variant,
                value,
            } => {
                let printer = PrettyPrinter::default()
                    .with_colors(facet_pretty::ColorMode::Never)
                    .with_minimal_option_names(true);

                // Show variant if present (e.g., "Some" for Option::Some)
                if let Some(variant) = variant {
                    write!(f, "{}", variant.bold())?;
                }

                let has_prefix = variant.is_some();

                match value {
                    Value::Struct {
                        updates,
                        deletions,
                        insertions,
                        unchanged,
                    } => {
                        if updates.is_empty() && deletions.is_empty() && insertions.is_empty() {
                            return write!(f, "{}", muted("(structurally equal)"));
                        }

                        if has_prefix {
                            writeln!(f, " {}", punct("{"))?;
                        } else {
                            writeln!(f, "{}", punct("{"))?;
                        }
                        let mut indent = PadAdapter::new_indented(f);

                        // Show unchanged fields indicator first
                        let unchanged_count = unchanged.len();
                        if unchanged_count > 0 {
                            let label = if unchanged_count == 1 {
                                "field"
                            } else {
                                "fields"
                            };
                            writeln!(
                                indent,
                                "{}",
                                muted(&format!(".. {unchanged_count} unchanged {label}"))
                            )?;
                        }

                        // Sort fields for deterministic output
                        let mut updates: Vec<_> = updates.iter().collect();
                        updates.sort_by(|(a, _), (b, _)| a.cmp(b));
                        for (fld, update) in updates {
                            writeln!(indent, "{}{} {update}", field(fld), punct(":"))?;
                        }

                        let mut deletions: Vec<_> = deletions.iter().collect();
                        deletions.sort_by(|(a, _), (b, _)| a.cmp(b));
                        for (fld, value) in deletions {
                            writeln!(
                                indent,
                                "{} {}{} {}",
                                deleted("-"),
                                field(fld),
                                punct(":"),
                                deleted(&printer.format_peek(*value))
                            )?;
                        }

                        let mut insertions: Vec<_> = insertions.iter().collect();
                        insertions.sort_by(|(a, _), (b, _)| a.cmp(b));
                        for (fld, value) in insertions {
                            writeln!(
                                indent,
                                "{} {}{} {}",
                                inserted("+"),
                                field(fld),
                                punct(":"),
                                inserted(&printer.format_peek(*value))
                            )?;
                        }

                        write!(f, "{}", punct("}"))
                    }
                    Value::Tuple { updates } => {
                        // No changes in tuple
                        if updates.is_empty() {
                            return write!(f, "{}", muted("(structurally equal)"));
                        }
                        // For single-element tuples (like Option::Some), try to be concise
                        if updates.is_single_replace() {
                            if has_prefix {
                                f.write_str(" ")?;
                            }
                            write!(f, "{updates}")
                        } else {
                            f.write_str(if has_prefix { " (\n" } else { "(\n" })?;
                            let mut indent = PadAdapter::new_indented(f);
                            write!(indent, "{updates}")?;
                            f.write_str(")")
                        }
                    }
                }
            }
            Diff::Sequence {
                from: _,
                to: _,
                updates,
            } => {
                if updates.is_empty() {
                    write!(f, "{}", muted("(structurally equal)"))
                } else {
                    writeln!(f, "{}", punct("["))?;
                    let mut indent = PadAdapter::new_indented(f);
                    write!(indent, "{updates}")?;
                    write!(f, "{}", punct("]"))
                }
            }
        }
    }
}

impl<'mem, 'facet> Updates<'mem, 'facet> {
    /// Check if this is a single replace operation (useful for Option::Some)
    pub const fn is_single_replace(&self) -> bool {
        self.0.first.is_some() && self.0.values.is_empty() && self.0.last.is_none()
    }

    /// Check if there are no changes (everything is unchanged)
    pub const fn is_empty(&self) -> bool {
        self.0.first.is_none() && self.0.values.is_empty()
    }
}

impl<'mem, 'facet> Display for Updates<'mem, 'facet> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(update) = &self.0.first {
            update.fmt(f)?;
        }

        for (values, update) in &self.0.values {
            // Collapse kept values into ".. N unchanged items"
            let count = values.len();
            if count > 0 {
                let label = if count == 1 { "item" } else { "items" };
                writeln!(f, "{}", muted(&format!(".. {count} unchanged {label}")))?;
            }
            update.fmt(f)?;
        }

        if let Some(values) = &self.0.last {
            // Collapse trailing kept values
            let count = values.len();
            if count > 0 {
                let label = if count == 1 { "item" } else { "items" };
                writeln!(f, "{}", muted(&format!(".. {count} unchanged {label}")))?;
            }
        }

        Ok(())
    }
}

impl<'mem, 'facet> Display for ReplaceGroup<'mem, 'facet> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let printer = PrettyPrinter::default()
            .with_colors(facet_pretty::ColorMode::Never)
            .with_minimal_option_names(true);

        // If it's a 1-to-1 replacement, check for equality
        if self.removals.len() == 1 && self.additions.len() == 1 {
            let from = self.removals[0];
            let to = self.additions[0];

            if peek_eq(from, to) {
                // Values are equal, show muted
                return writeln!(f, "{}", muted(&printer.format_peek(from)));
            }

            // Show value change inline: old → new
            return writeln!(
                f,
                "{} → {}",
                deleted(&printer.format_peek(from)),
                inserted(&printer.format_peek(to))
            );
        }

        // Otherwise show as - / + lines with consistent indentation
        for remove in &self.removals {
            writeln!(
                f,
                "{}",
                deleted(&format!("- {}", printer.format_peek(*remove)))
            )?;
        }

        for add in &self.additions {
            writeln!(
                f,
                "{}",
                inserted(&format!("+ {}", printer.format_peek(*add)))
            )?;
        }

        Ok(())
    }
}

/// Write a sequence of diffs, collapsing Equal diffs into ".. N unchanged items"
fn write_diff_sequence(
    f: &mut std::fmt::Formatter<'_>,
    diffs: &[Diff<'_, '_>],
) -> std::fmt::Result {
    let mut i = 0;
    while i < diffs.len() {
        // Count consecutive Equal diffs
        let mut equal_count = 0;
        while i + equal_count < diffs.len() {
            if matches!(diffs[i + equal_count], Diff::Equal { .. }) {
                equal_count += 1;
            } else {
                break;
            }
        }

        if equal_count > 0 {
            // Collapse Equal diffs
            let label = if equal_count == 1 { "item" } else { "items" };
            writeln!(
                f,
                "{}",
                muted(&format!(".. {equal_count} unchanged {label}"))
            )?;
            i += equal_count;
        } else {
            // Show the non-Equal diff
            writeln!(f, "{}", diffs[i])?;
            i += 1;
        }
    }
    Ok(())
}

impl<'mem, 'facet> Display for UpdatesGroup<'mem, 'facet> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(update) = &self.0.first {
            update.fmt(f)?;
        }

        for (values, update) in &self.0.values {
            write_diff_sequence(f, values)?;
            update.fmt(f)?;
        }

        if let Some(values) = &self.0.last {
            write_diff_sequence(f, values)?;
        }

        Ok(())
    }
}

/// Format a character for display with its Unicode codepoint.
fn format_char_with_codepoint(c: char) -> String {
    // For printable ASCII characters (except space), show the character directly
    if c.is_ascii_graphic() {
        format!("'{}' (U+{:04X})", c, c as u32)
    } else {
        // For non-printable chars, show the escaped form (codepoint is visible in the escape)
        format!("'\\u{{{:04X}}}'", c as u32)
    }
}

/// Explain the confusable differences between two strings that look identical.
/// Shows character-level differences with full Unicode codepoints.
fn explain_confusable_differences(left: &str, right: &str) -> String {
    use std::fmt::Write;

    // Find character-level differences
    let left_chars: Vec<char> = left.chars().collect();
    let right_chars: Vec<char> = right.chars().collect();

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
        return muted("(strings are identical)");
    }

    writeln!(
        out,
        "{}",
        muted(&format!(
            "(strings are visually confusable but differ in {} position{}):",
            diffs.len(),
            if diffs.len() == 1 { "" } else { "s" }
        ))
    )
    .unwrap();

    for (pos, lc, rc) in &diffs {
        if *lc == '\0' {
            writeln!(
                out,
                "  [{}]: (missing) vs {}",
                pos,
                inserted(&format_char_with_codepoint(*rc))
            )
            .unwrap();
        } else if *rc == '\0' {
            writeln!(
                out,
                "  [{}]: {} vs (missing)",
                pos,
                deleted(&format_char_with_codepoint(*lc))
            )
            .unwrap();
        } else {
            writeln!(
                out,
                "  [{}]: {} vs {}",
                pos,
                deleted(&format_char_with_codepoint(*lc)),
                inserted(&format_char_with_codepoint(*rc))
            )
            .unwrap();
        }
    }

    out.trim_end().to_string()
}
