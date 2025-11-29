use std::fmt::{Display, Write};

use facet_pretty::PrettyPrinter;
use owo_colors::OwoColorize;

use crate::{
    diff::{Diff, Value},
    sequences::{ReplaceGroup, Updates, UpdatesGroup},
};

struct PadAdapter<'a, 'b: 'a> {
    fmt: &'a mut std::fmt::Formatter<'b>,
    on_newline: bool,
    indent: &'static str,
}

impl<'a, 'b> PadAdapter<'a, 'b> {
    fn new_indented(fmt: &'a mut std::fmt::Formatter<'b>) -> Self {
        Self {
            fmt,
            on_newline: true,
            indent: "  ",
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

impl<'mem, 'facet> Display for Diff<'mem, 'facet> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Diff::Equal { value } => {
                if let Some(peek) = value {
                    let printer = PrettyPrinter::default()
                        .with_colors(false)
                        .with_minimal_option_names(true);
                    write!(f, "{}", printer.format_peek(*peek))
                } else {
                    f.write_str("(no changes)")
                }
            }
            Diff::Replace { from, to } => {
                let printer = PrettyPrinter::default()
                    .with_colors(false)
                    .with_minimal_option_names(true);

                // Show value change inline: old → new
                write!(
                    f,
                    "{} → {}",
                    printer.format_peek(*from).red(),
                    printer.format_peek(*to).green()
                )
            }
            Diff::User {
                from: _,
                to: _,
                variant,
                value,
            } => {
                let printer = PrettyPrinter::default()
                    .with_colors(false)
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
                        unchanged: _,
                    } => {
                        if updates.is_empty() && deletions.is_empty() && insertions.is_empty() {
                            return f.write_str(if has_prefix {
                                " (no changes)"
                            } else {
                                "(no changes)"
                            });
                        }

                        f.write_str(if has_prefix { " {\n" } else { "{\n" })?;
                        let mut indent = PadAdapter::new_indented(f);

                        // Sort fields for deterministic output
                        let mut updates: Vec<_> = updates.iter().collect();
                        updates.sort_by(|(a, _), (b, _)| a.cmp(b));
                        for (field, update) in updates {
                            writeln!(indent, "{field}: {update}")?;
                        }

                        let mut deletions: Vec<_> = deletions.iter().collect();
                        deletions.sort_by(|(a, _), (b, _)| a.cmp(b));
                        for (field, value) in deletions {
                            writeln!(
                                indent,
                                "{}",
                                format_args!("- {field}: {}", printer.format_peek(*value)).red()
                            )?;
                        }

                        let mut insertions: Vec<_> = insertions.iter().collect();
                        insertions.sort_by(|(a, _), (b, _)| a.cmp(b));
                        for (field, value) in insertions {
                            writeln!(
                                indent,
                                "{}",
                                format_args!("+ {field}: {}", printer.format_peek(*value)).green()
                            )?;
                        }

                        f.write_str("}")
                    }
                    Value::Tuple { updates } => {
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
                f.write_str("[\n")?;
                let mut indent = PadAdapter::new_indented(f);
                write!(indent, "{updates}")?;
                f.write_str("]")
            }
        }
    }
}

impl<'mem, 'facet> Updates<'mem, 'facet> {
    /// Check if this is a single replace operation (useful for Option::Some)
    fn is_single_replace(&self) -> bool {
        self.0.first.is_some() && self.0.values.is_empty() && self.0.last.is_none()
    }
}

impl<'mem, 'facet> Display for Updates<'mem, 'facet> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let printer = PrettyPrinter::default()
            .with_colors(false)
            .with_minimal_option_names(true);

        if let Some(update) = &self.0.first {
            update.fmt(f)?;
        }

        for (values, update) in &self.0.values {
            for value in values {
                writeln!(f, "{}", printer.format_peek(*value))?;
            }
            update.fmt(f)?;
        }

        if let Some(values) = &self.0.last {
            for value in values {
                writeln!(f, "{}", printer.format_peek(*value))?;
            }
        }

        Ok(())
    }
}

impl<'mem, 'facet> Display for ReplaceGroup<'mem, 'facet> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let printer = PrettyPrinter::default()
            .with_colors(false)
            .with_minimal_option_names(true);

        // If it's a 1-to-1 replacement, check for nested diff or equality
        if self.removals.len() == 1 && self.additions.len() == 1 {
            let from = self.removals[0];
            let to = self.additions[0];
            let diff = Diff::new_peek(from, to);

            match &diff {
                Diff::Equal { .. } => {
                    // Values are equal, just show the value
                    return writeln!(f, "{}", printer.format_peek(from));
                }
                Diff::Replace { .. } => {
                    // Fall through to - / + display below
                }
                _ => {
                    // Has nested structure, show the diff
                    return writeln!(f, "{diff}");
                }
            }
        }

        // Otherwise show as - / + lines with consistent indentation
        for remove in &self.removals {
            writeln!(
                f,
                "{}",
                format_args!("- {}", printer.format_peek(*remove)).red()
            )?;
        }

        for add in &self.additions {
            writeln!(
                f,
                "{}",
                format_args!("+ {}", printer.format_peek(*add)).green()
            )?;
        }

        Ok(())
    }
}

impl<'mem, 'facet> Display for UpdatesGroup<'mem, 'facet> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(update) = &self.0.first {
            update.fmt(f)?;
        }

        for (values, update) in &self.0.values {
            for value in values {
                writeln!(f, "{value}")?;
            }
            update.fmt(f)?;
        }

        if let Some(values) = &self.0.last {
            for value in values {
                writeln!(f, "{value}")?;
            }
        }

        Ok(())
    }
}
