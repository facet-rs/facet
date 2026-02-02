//! Lint: Soft delete issues.

use super::{DiagnosticBuilder, LintContext};
use dibs_query_schema::*;

/// Check if a where clause filters on deleted_at.
fn where_filters_deleted_at(where_clause: &Where) -> bool {
    where_clause
        .filters
        .iter()
        .any(|(col, _)| col.as_str() == "deleted_at")
}

pub fn lint_missing_deleted_at_filter(query: &Select, ctx: &mut LintContext<'_>) {
    let Some(from) = &query.from else { return };
    let Some(table) = ctx.find_table(from.as_str()) else {
        return;
    };
    let has_deleted_at = table.columns.iter().any(|c| c.name == "deleted_at");
    if !has_deleted_at {
        return;
    }

    let filters_deleted_at = query
        .where_clause
        .as_ref()
        .is_some_and(where_filters_deleted_at);

    if !filters_deleted_at {
        DiagnosticBuilder::warning("missing-deleted-at-filter")
            .at(from.span)
            .msg(format!(
                "query on '{}' doesn't filter 'deleted_at' - consider adding 'deleted_at @null'",
                from.as_str()
            ))
            .emit(ctx.diagnostics);
    }
}

pub fn lint_hard_delete_on_soft_delete_table(delete: &Delete, ctx: &mut LintContext<'_>) {
    let Some(table) = ctx.find_table(delete.from.as_str()) else {
        return;
    };
    let has_deleted_at = table.columns.iter().any(|c| c.name == "deleted_at");

    if has_deleted_at {
        DiagnosticBuilder::warning("hard-delete-on-soft-delete-table")
            .at(delete.from.span)
            .msg("@delete on table with 'deleted_at' column - consider soft delete with @update instead")
            .emit(ctx.diagnostics);
    }
}
