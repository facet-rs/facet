//! Lint: Redundant param references (e.g., `column $column` → just `column`).

use super::{DiagnosticBuilder, LintContext};
use dibs_query_schema::*;

/// Create diagnostic data for redundant-param with the span to delete.
fn make_redundant_param_data(name: &str, delete_start: u32, delete_end: u32) -> styx_tree::Value {
    use styx_tree::{Entry, Object, Payload, Value};
    Value {
        tag: None,
        payload: Some(Payload::Object(Object {
            entries: vec![
                Entry {
                    key: Value::scalar("name"),
                    value: Value::scalar(name),
                    doc_comment: None,
                },
                Entry {
                    key: Value::scalar("delete_start"),
                    value: Value::scalar(delete_start.to_string()),
                    doc_comment: None,
                },
                Entry {
                    key: Value::scalar("delete_end"),
                    value: Value::scalar(delete_end.to_string()),
                    doc_comment: None,
                },
            ],
            span: None,
        })),
        span: None,
    }
}

pub fn lint_redundant_params_in_values(values: &Values, ctx: &mut LintContext<'_>) {
    for (col_name, value_expr) in &values.columns {
        if let Some(ValueExpr::Other {
            tag: None,
            content: Some(Payload::Scalar(s)),
        }) = value_expr
            && let Some(param_name) = s.as_str().strip_prefix('$')
            && param_name == col_name.as_str()
        {
            // Calculate span to delete: from end of column name to end of $param
            // This deletes " $handle" leaving just "handle"
            let delete_start = col_name.span.offset + col_name.span.len;
            let delete_end = s.span.offset + s.span.len;

            DiagnosticBuilder::warning("redundant-param")
                .at(s.span) // Highlight the $param that will be deleted
                .msg(format!(
                    "'{} ${}' can be shortened to just '{}' (implicit @param)",
                    col_name.as_str(),
                    param_name,
                    col_name.as_str()
                ))
                .emit_with_data(
                    ctx.diagnostics,
                    make_redundant_param_data(col_name.as_str(), delete_start, delete_end),
                );
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
            // Calculate span to delete: from end of column name to end of $param
            // This deletes " $handle" leaving just "handle"
            let delete_start = col_name.span.offset + col_name.span.len;
            let delete_end = s.span.offset + s.span.len;

            DiagnosticBuilder::warning("redundant-param")
                .at(s.span) // Highlight the $param that will be deleted
                .msg(format!(
                    "'{} ${}' can be shortened to just '{}' (implicit @param)",
                    col_name.as_str(),
                    param_name,
                    col_name.as_str()
                ))
                .emit_with_data(
                    ctx.diagnostics,
                    make_redundant_param_data(col_name.as_str(), delete_start, delete_end),
                );
        }
    }
}

pub fn lint_redundant_params_in_where(where_clause: &Where, ctx: &mut LintContext<'_>) {
    for (col_name, filter) in &where_clause.filters {
        if let FilterValue::EqBare(Some(meta)) = filter
            && let Some(param_name) = meta.as_str().strip_prefix('$')
            && param_name == col_name.as_str()
        {
            // Calculate span to delete: from end of column name to end of $param
            // This deletes " $user_id" leaving just "user_id"
            let delete_start = col_name.span.offset + col_name.span.len;
            let delete_end = meta.span.offset + meta.span.len;

            DiagnosticBuilder::warning("redundant-param")
                .at(meta.span) // Highlight the $param that will be deleted
                .msg(format!(
                    "'{} ${}' can be shortened to just '{}' (implicit @param)",
                    col_name.as_str(),
                    param_name,
                    col_name.as_str()
                ))
                .emit_with_data(
                    ctx.diagnostics,
                    make_redundant_param_data(col_name.as_str(), delete_start, delete_end),
                );
        }
    }
}
