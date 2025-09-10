use miette::ReportHandler;

/// A wrapper that formats a miette diagnostic with the graphical report handler
pub struct DiagnosticDisplayWrapper<'a>(pub &'a dyn miette::Diagnostic);

impl<'a> core::fmt::Display for DiagnosticDisplayWrapper<'a> {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        let reporter =
            miette::GraphicalReportHandler::new_themed(miette::GraphicalTheme::unicode_nocolor());
        reporter.debug(self.0, f)?;
        Ok(())
    }
}

/// Do snapshot testing for a miette Diagnostic
#[macro_export]
macro_rules! assert_diag_snapshot {
    ($err:expr) => {
        insta::assert_snapshot!($crate::common::DiagnosticDisplayWrapper(&$err).to_string())
    };
}
