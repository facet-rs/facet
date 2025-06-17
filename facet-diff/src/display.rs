use facet_pretty::PrettyPrinter;

use crate::diff::Diff;

impl<'mem, 'facet, 'shape> std::fmt::Display for Diff<'mem, 'facet, 'shape> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Diff::Equal => f.write_str("equal"),
            Diff::Replace { from, to } => {
                let printer = PrettyPrinter::default().with_colors(false);

                f.write_str("\x1b[31m")?;
                f.write_str(&printer.format_peek(*from))?;
                f.write_str("\n\x1b[32m")?;
                f.write_str(&printer.format_peek(*to))?;
                f.write_str("\n\x1b[m")
            }
        }
    }
}
