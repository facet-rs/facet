//! Lint: Redundant param references (e.g., `column $column` → just `column`).

use super::{DiagnosticBuilder, LintContext};
use dibs_query_schema::*;

pub fn lint_redundant_params_in_values(values: &Values, ctx: &mut LintContext<'_>) {
    for (col_name, value_expr) in &values.columns {
        if let Some(ValueExpr::Other {
            tag: None,
            content: Some(Payload::Scalar(s)),
        }) = value_expr
            && let Some(param_name) = s.as_str().strip_prefix('$')
            && param_name == col_name.as_str()
        {
            DiagnosticBuilder::warning("redundant-param")
                .at(s.span)
                .msg(format!(
                    "'{} ${}' can be shortened to just '{}' (implicit @param)",
                    col_name.as_str(),
                    param_name,
                    col_name.as_str()
                ))
                .emit_with_data(ctx.diagnostics, styx_tree::Value::scalar(col_name.as_str()));
        }
    }
}

pub fn lint_redundant_params_in_conflict_update(
    update: &ConflictUpdate,
    ctx: &mut LintContext<'_>,
) {
    for (col_name, update_value) in &update.columns {
        if let Some(UpdateValue::Other {
            tag: None,
            content: Some(Payload::Scalar(s)),
        }) = update_value
            && let Some(param_name) = s.as_str().strip_prefix('$')
            && param_name == col_name.as_str()
        {
            DiagnosticBuilder::warning("redundant-param")
                .at(s.span)
                .msg(format!(
                    "'{} ${}' can be shortened to just '{}' (implicit @param)",
                    col_name.as_str(),
                    param_name,
                    col_name.as_str()
                ))
                .emit_with_data(ctx.diagnostics, styx_tree::Value::scalar(col_name.as_str()));
        }
    }
}

pub fn lint_redundant_params_in_where(where_clause: &Where, ctx: &mut LintContext<'_>) {
    for (col_name, filter) in &where_clause.filters {
        if let FilterValue::EqBare(Some(meta)) = filter
            && let Some(param_name) = meta.as_str().strip_prefix('$')
            && param_name == col_name.as_str()
        {
            DiagnosticBuilder::warning("redundant-param")
                .at(col_name.span)
                .msg(format!(
                    "'{} ${}' can be shortened to just '{}' (implicit @param)",
                    col_name.as_str(),
                    param_name,
                    col_name.as_str()
                ))
                .emit_with_data(ctx.diagnostics, styx_tree::Value::scalar(col_name.as_str()));
        }
    }
}
