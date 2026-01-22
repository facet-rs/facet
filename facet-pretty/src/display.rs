//! Display trait implementations for pretty-printing Facet types

use core::fmt::{self, Display, Formatter};

use crate::printer::PrettyPrinter;
use facet_core::Facet;

/// Display wrapper for any type that implements Facet.
///
/// The lifetime `'b` is the borrow lifetime (how long we hold the reference),
/// while `'a` is the Facet lifetime (for the type's shape/vtable).
pub struct PrettyDisplay<'a, 'b, T: Facet<'a> + ?Sized> {
    pub(crate) value: &'b T,
    pub(crate) printer: PrettyPrinter,
    pub(crate) _marker: core::marker::PhantomData<&'a ()>,
}

impl<'a, 'b, T: Facet<'a>> Display for PrettyDisplay<'a, 'b, T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.printer.format_to(self.value, f)
    }
}

/// Extension trait for Facet types to easily pretty-print them
pub trait FacetPretty<'a>: Facet<'a> {
    /// Get a displayable wrapper that pretty-prints this value
    fn pretty(&self) -> PrettyDisplay<'a, '_, Self>;

    /// Get a displayable wrapper with custom printer settings
    fn pretty_with(&self, printer: PrettyPrinter) -> PrettyDisplay<'a, '_, Self>;
}

impl<'a, T: Facet<'a>> FacetPretty<'a> for T {
    fn pretty(&self) -> PrettyDisplay<'a, '_, Self> {
        PrettyDisplay {
            value: self,
            printer: PrettyPrinter::new(),
            _marker: core::marker::PhantomData,
        }
    }

    fn pretty_with(&self, printer: PrettyPrinter) -> PrettyDisplay<'a, '_, Self> {
        PrettyDisplay {
            value: self,
            printer,
            _marker: core::marker::PhantomData,
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
        write!(output, "{display}").unwrap();

        // Just check that it contains the field name and doesn't panic
        assert!(output.contains("field"));
    }

    #[test]
    fn test_pretty_with_custom_printer() {
        let test = TestStruct { field: 42 };
        let printer = PrettyPrinter::new().with_colors(false.into());
        let display = test.pretty_with(printer);

        let mut output = String::new();
        write!(output, "{display}").unwrap();

        // Just check that it contains the field name and doesn't panic
        assert!(output.contains("field"));
    }
}
