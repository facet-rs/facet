//! Lint: Mutations without WHERE clause.

use super::{DiagnosticBuilder, LintContext};
use dibs_query_schema::*;

pub fn lint_update_without_where(update: &Update, ctx: &mut LintContext<'_>) {
    if update.where_clause.is_none() {
        DiagnosticBuilder::error("mutation-without-where")
            .at(update.table.span)
            .msg("@update without 'where' affects all rows - add 'where' or 'all true'")
            .emit(ctx.diagnostics);
    }
}

pub fn lint_delete_without_where(delete: &Delete, ctx: &mut LintContext<'_>) {
    if delete.where_clause.is_none() {
        DiagnosticBuilder::error("mutation-without-where")
            .at(delete.from.span)
            .msg("@delete without 'where' affects all rows - add 'where' or 'all true'")
            .emit(ctx.diagnostics);
    }
}
