//! Lint: Pagination issues (limit, offset, first, order-by).

use super::{DiagnosticBuilder, LintContext};
use dibs_query_schema::*;

pub fn lint_pagination_query(query: &Select, ctx: &mut LintContext<'_>) {
    let has_order_by = query.order_by.is_some();

    // offset without limit
    if let Some(offset) = &query.offset
        && query.limit.is_none()
    {
        DiagnosticBuilder::warning("offset-without-limit")
            .at(offset.span)
            .msg("'offset' without 'limit' is unusual - did you forget 'limit'?")
            .emit(ctx.diagnostics);
    }

    // limit without order-by
    if let Some(limit) = &query.limit
        && !has_order_by
    {
        DiagnosticBuilder::warning("limit-without-order-by")
            .at(limit.span)
            .msg("'limit' without 'order-by' returns arbitrary rows")
            .emit(ctx.diagnostics);
    }

    // first without order-by
    if let Some(first) = &query.first
        && first.get()
        && !has_order_by
    {
        DiagnosticBuilder::warning("first-without-order-by")
            .at(first.span)
            .msg("'first' without 'order-by' returns arbitrary row")
            .emit(ctx.diagnostics);
    }

    // large offset warning
    if let Some(offset) = &query.offset
        && let Ok(n) = offset.as_str().parse::<i64>()
        && n > 1000
    {
        DiagnosticBuilder::warning("large-offset")
            .at(offset.span)
            .msg(format!(
                "large offset ({}) may cause performance issues - consider cursor-based pagination",
                n
            ))
            .emit(ctx.diagnostics);
    }
}
