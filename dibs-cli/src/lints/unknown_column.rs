//! Lint: Unknown column references.

use super::{DiagnosticBuilder, LintContext};
use dibs_proto::TableInfo;
use dibs_query_schema::*;

pub fn lint_unknown_columns_select(
    select: &SelectFields,
    table: &TableInfo,
    ctx: &mut LintContext<'_>,
) {
    for (col_name, field_def) in &select.fields {
        if matches!(field_def, Some(FieldDef::Rel(_))) {
            continue;
        }
        if !table.columns.iter().any(|c| c.name == col_name.as_str()) {
            let available = table
                .columns
                .iter()
                .map(|c| c.name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            DiagnosticBuilder::error("unknown-column")
                .at(col_name.span)
                .msg(format!(
                    "Unknown column '{}' in table '{}'. Available columns: {}",
                    col_name.as_str(),
                    table.name,
                    available
                ))
                .emit(ctx.diagnostics);
        }
    }
}

pub fn lint_unknown_columns_where(
    where_clause: &Where,
    table: &TableInfo,
    ctx: &mut LintContext<'_>,
) {
    for (col_name, _filter) in &where_clause.filters {
        if !table.columns.iter().any(|c| c.name == col_name.as_str()) {
            let available = table
                .columns
                .iter()
                .map(|c| c.name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            DiagnosticBuilder::error("unknown-column")
                .at(col_name.span)
                .msg(format!(
                    "Unknown column '{}' in table '{}'. Available columns: {}",
                    col_name.as_str(),
                    table.name,
                    available
                ))
                .emit(ctx.diagnostics);
        }
    }
}

pub fn lint_unknown_columns_order_by(
    order_by: &OrderBy,
    table: &TableInfo,
    ctx: &mut LintContext<'_>,
) {
    for (col_name, _dir) in &order_by.columns {
        if !table.columns.iter().any(|c| c.name == col_name.as_str()) {
            let available = table
                .columns
                .iter()
                .map(|c| c.name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            DiagnosticBuilder::error("unknown-column")
                .at(col_name.span)
                .msg(format!(
                    "Unknown column '{}' in table '{}'. Available columns: {}",
                    col_name.as_str(),
                    table.name,
                    available
                ))
                .emit(ctx.diagnostics);
        }
    }
}

pub fn lint_unknown_columns_values(values: &Values, table: &TableInfo, ctx: &mut LintContext<'_>) {
    for (col_name, _value_expr) in &values.columns {
        if !table.columns.iter().any(|c| c.name == col_name.as_str()) {
            let available = table
                .columns
                .iter()
                .map(|c| c.name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            DiagnosticBuilder::error("unknown-column")
                .at(col_name.span)
                .msg(format!(
                    "Unknown column '{}' in table '{}'. Available columns: {}",
                    col_name.as_str(),
                    table.name,
                    available
                ))
                .emit(ctx.diagnostics);
        }
    }
}
