//! Lint: Empty select block.

use super::{DiagnosticBuilder, LintContext};
use dibs_query_schema::*;

/// Check for empty select block in a query.
pub fn lint_empty_select(select: &Select, ctx: &mut LintContext<'_>) {
    if select.fields.is_empty() {
        DiagnosticBuilder::warning("empty-select")
            .at(select.span)
            .msg("empty select block - query will return no columns")
            .emit(ctx.diagnostics);
    }
}
