use std::fmt::{Display, Write};

use facet::TypeNameOpts;
use facet_pretty::PrettyPrinter;

use crate::diff::Diff;

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
            Diff::Struct { from, to, updates } => {
                let mut indent = PadAdapter {
                    fmt: f,
                    on_newline: false,
                };

                write!(indent, "\x1b[1m")?;
                from.write_type_name(indent.fmt, TypeNameOpts::infinite())?;

                if from.id != to.id {
                    write!(indent, "\x1b[m => \x1b[1m")?;
                    to.write_type_name(indent.fmt, TypeNameOpts::infinite())?;
                }

                writeln!(indent, "\x1b[m {{")?;
                for (field, update) in updates {
                    writeln!(indent, "{field}: {update}")?;
                }

                f.write_str("}")
            }
        }
    }
}
