use std::fmt::{Display, Write};

use facet::TypeNameOpts;
use facet_pretty::PrettyPrinter;

use crate::{
    diff::{Diff, Value},
    sequences::{ReplaceGroup, Updates, UpdatesGroup},
};

struct PadAdapter<'a, 'b: 'a> {
    fmt: &'a mut std::fmt::Formatter<'b>,
    on_newline: bool,
}

impl<'a, 'b> Write for PadAdapter<'a, 'b> {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        for line in s.split_inclusive('\n') {
            if self.on_newline {
                self.fmt.write_str("    ")?;
            }

            self.on_newline = line.ends_with('\n');

            self.fmt.write_str(line)?;
        }

        Ok(())
    }

    fn write_char(&mut self, c: char) -> std::fmt::Result {
        if self.on_newline {
            self.fmt.write_str("    ")?;
        }

        self.on_newline = c == '\n';
        self.fmt.write_char(c)
    }
}

impl<'mem, 'facet> Display for Diff<'mem, 'facet> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Diff::Equal => f.write_str("equal"),
            Diff::Replace { from, to } => {
                let printer = PrettyPrinter::default().with_colors(false);

                if from.shape().id != to.shape().id {
                    f.write_str("\x1b[1m")?;
                    from.type_name(f, TypeNameOpts::infinite())?;
                    f.write_str("\x1b[m => \x1b[1m")?;
                    to.type_name(f, TypeNameOpts::infinite())?;
                    f.write_str(" \x1b[m")?;
                }

                f.write_str("{\n\x1b[31m")?; // Print the next value in red
                //
                let mut indent = PadAdapter {
                    fmt: f,
                    on_newline: true,
                };

                writeln!(indent, "{}\x1b[32m", printer.format_peek(*from))?;
                write!(indent, "{}", printer.format_peek(*to))?;
                f.write_str("\n\x1b[m}") // Reset the colors
            }
            Diff::User {
                from,
                to,
                variant,
                value,
            } => {
                let printer = PrettyPrinter::default().with_colors(false);
                let mut indent = PadAdapter {
                    fmt: f,
                    on_newline: false,
                };

                write!(indent, "\x1b[1m")?;
                from.write_type_name(indent.fmt, TypeNameOpts::infinite())?;

                if let Some(variant) = variant {
                    write!(indent, "\x1b[m::\x1b[1m{variant}")?;
                }

                if from.id != to.id {
                    write!(indent, "\x1b[m => \x1b[1m")?;
                    to.write_type_name(indent.fmt, TypeNameOpts::infinite())?;

                    if let Some(variant) = variant {
                        write!(indent, "\x1b[m::\x1b[1m{variant}")?;
                    }
                }

                match value {
                    Value::Struct {
                        updates,
                        deletions,
                        insertions,
                        unchanged: _,
                    } => {
                        writeln!(indent, "\x1b[m {{")?;
                        for (field, update) in updates {
                            writeln!(indent, "{field}: {update}")?;
                        }

                        for (field, value) in deletions {
                            writeln!(
                                indent,
                                "\x1b[31m{field}: {}\x1b[m",
                                printer.format_peek(*value)
                            )?;
                        }

                        for (field, value) in insertions {
                            writeln!(
                                indent,
                                "\x1b[32m{field}: {}\x1b[m",
                                printer.format_peek(*value)
                            )?;
                        }

                        f.write_str("}")
                    }
                    Value::Tuple { updates } => {
                        writeln!(indent, "\x1b[m (")?;
                        write!(indent, "{updates}")?;
                        f.write_str(")")
                    }
                }
            }
            Diff::Sequence { from, to, updates } => {
                write!(f, "\x1b[1m")?;
                from.write_type_name(f, TypeNameOpts::infinite())?;
                write!(f, "\x1b[m")?;

                if from.id != to.id {
                    write!(f, " => \x1b[1m")?;
                    to.write_type_name(f, TypeNameOpts::infinite())?;
                    write!(f, "\x1b[m")?;
                }

                let mut indent = PadAdapter {
                    fmt: f,
                    on_newline: false,
                };

                writeln!(indent, " [")?;
                write!(indent, "{updates}")?;
                write!(f, "]")
            }
        }
    }
}

impl<'mem, 'facet> Display for Updates<'mem, 'facet> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(update) = &self.0.first {
            update.fmt(f)?;
        }

        let printer = PrettyPrinter::default().with_colors(false);

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

impl<'mem, 'facet> Display for ReplaceGroup<'mem, 'facet> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let printer = PrettyPrinter::default().with_colors(false);

        for remove in &self.removals {
            writeln!(f, "\x1b[31m{}\x1b[m", printer.format_peek(*remove))?;
        }

        for add in &self.additions {
            writeln!(f, "\x1b[32m{}\x1b[m", printer.format_peek(*add))?;
        }

        Ok(())
    }
}
