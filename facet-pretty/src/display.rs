//! Display trait implementations for pretty-printing Facet types

use core::fmt::{self, Display, Formatter};

use crate::printer::PrettyPrinter;
use facet_core::Facet;

/// Display wrapper for any type that implements Facet
pub struct PrettyDisplay<'a, T: Facet<'a>> {
    pub(crate) value: &'a T,
    pub(crate) printer: PrettyPrinter,
}

impl<'a, T: Facet<'a>> Display for PrettyDisplay<'a, T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.printer.format_to(self.value, f)
    }
}

/// Extension trait for Facet types to easily pretty-print them
pub trait FacetPretty<'a>: Facet<'a> {
    /// Get a displayable wrapper that pretty-prints this value
    fn pretty(&'a self) -> PrettyDisplay<'a, Self>;

    /// Get a displayable wrapper with custom printer settings
    fn pretty_with(&'a self, printer: PrettyPrinter) -> PrettyDisplay<'a, Self>;
}

impl<'a, T: Facet<'a>> FacetPretty<'a> for T {
    fn pretty(&'a self) -> PrettyDisplay<'a, Self> {
        PrettyDisplay {
            value: self,
            printer: PrettyPrinter::default(),
        }
    }

    fn pretty_with(&'a self, printer: PrettyPrinter) -> PrettyDisplay<'a, Self> {
        PrettyDisplay {
            value: self,
            printer,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::fmt::Write;
    use facet::Facet;

    // Use the derive macro from facet
    #[derive(Facet)]
    struct TestStruct {
        field: u32,
    }

    #[test]
    fn test_pretty_display() {
        let test = TestStruct { field: 42 };
        let display = test.pretty();

        let mut output = String::new();
        write!(output, "{}", display).unwrap();

        // Just check that it contains the field name and doesn't panic
        assert!(output.contains("field"));
    }

    #[test]
    fn test_pretty_with_custom_printer() {
        let test = TestStruct { field: 42 };
        let printer = PrettyPrinter::new().with_colors(false);
        let display = test.pretty_with(printer);

        let mut output = String::new();
        write!(output, "{}", display).unwrap();

        // Just check that it contains the field name and doesn't panic
        assert!(output.contains("field"));
    }
}
