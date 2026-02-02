//! Lint: Unused parameter declarations.

use super::{DiagnosticBuilder, LintContext};
use dibs_query_schema::*;

/// Collect param refs from a Where clause.
fn collect_param_refs_from_where(where_clause: &Where) -> Vec<String> {
    let mut refs = Vec::new();
    for (col_name, filter) in &where_clause.filters {
        match filter {
            FilterValue::EqBare(Some(meta)) => {
                if let Some(param) = meta.as_str().strip_prefix('$') {
                    refs.push(param.to_string());
                }
            }
            FilterValue::EqBare(None) => {
                // Shorthand: {column} means {column $column}
                refs.push(col_name.as_str().to_string());
            }
            FilterValue::Eq(args)
            | FilterValue::Ilike(args)
            | FilterValue::Like(args)
            | FilterValue::Gt(args)
            | FilterValue::Lt(args)
            | FilterValue::Gte(args)
            | FilterValue::Lte(args)
            | FilterValue::Ne(args)
            | FilterValue::In(args)
            | FilterValue::JsonGet(args)
            | FilterValue::JsonGetText(args)
            | FilterValue::Contains(args)
            | FilterValue::KeyExists(args) => {
                for arg in args {
                    if let Some(param) = arg.as_str().strip_prefix('$') {
                        refs.push(param.to_string());
                    }
                }
            }
            FilterValue::Null | FilterValue::NotNull => {}
        }
    }
    refs
}

/// Collect param refs from a Values clause.
fn collect_param_refs_from_values(values: &Values) -> Vec<String> {
    let mut refs = Vec::new();
    for (col_name, value_expr) in &values.columns {
        match value_expr {
            None => {
                // Shorthand: column with no value means implicit $column
                refs.push(col_name.value.to_string());
            }
            Some(ValueExpr::Default) => {}
            Some(ValueExpr::Other { tag: _, content }) => {
                if let Some(payload) = content {
                    collect_param_refs_from_payload(payload, &mut refs);
                }
            }
        }
    }
    refs
}

/// Collect param refs from ConflictUpdate.
fn collect_param_refs_from_conflict_update(update: &ConflictUpdate) -> Vec<String> {
    let mut refs = Vec::new();
    for (col_name, update_value) in &update.columns {
        match update_value {
            None => {
                refs.push(col_name.value.to_string());
            }
            Some(UpdateValue::Default) => {}
            Some(UpdateValue::Other { tag: _, content }) => {
                if let Some(payload) = content {
                    collect_param_refs_from_payload(payload, &mut refs);
                }
            }
        }
    }
    refs
}

/// Collect param refs from Payload.
fn collect_param_refs_from_payload(payload: &Payload, refs: &mut Vec<String>) {
    match payload {
        Payload::Scalar(s) => {
            if let Some(param) = s.as_str().strip_prefix('$') {
                refs.push(param.to_string());
            }
        }
        Payload::Seq(items) => {
            for item in items {
                if let ValueExpr::Other {
                    tag: _,
                    content: Some(p),
                } = item
                {
                    collect_param_refs_from_payload(p, refs);
                }
            }
        }
    }
}

pub fn lint_unused_params_query(query: &Select, ctx: &mut LintContext<'_>) {
    let Some(params) = &query.params else { return };

    let mut used = Vec::new();
    if let Some(where_clause) = &query.where_clause {
        used.extend(collect_param_refs_from_where(where_clause));
    }
    if let Some(limit) = &query.limit
        && let Some(param) = limit.as_str().strip_prefix('$')
    {
        used.push(param.to_string());
    }
    if let Some(offset) = &query.offset
        && let Some(param) = offset.as_str().strip_prefix('$')
    {
        used.push(param.to_string());
    }

    for (param_name, _param_type) in &params.params {
        if !used.iter().any(|u| u == param_name.as_str()) {
            DiagnosticBuilder::warning("unused-param")
                .at(param_name.span)
                .msg(format!(
                    "param '{}' is declared but never used",
                    param_name.as_str()
                ))
                .emit(ctx.diagnostics);
        }
    }
}

pub fn lint_unused_params_insert(insert: &Insert, ctx: &mut LintContext<'_>) {
    let Some(params) = &insert.params else { return };

    let used = collect_param_refs_from_values(&insert.values);

    for (param_name, _param_type) in &params.params {
        if !used.iter().any(|u| u == param_name.as_str()) {
            DiagnosticBuilder::warning("unused-param")
                .at(param_name.span)
                .msg(format!(
                    "param '{}' is declared but never used",
                    param_name.as_str()
                ))
                .emit(ctx.diagnostics);
        }
    }
}

pub fn lint_unused_params_insert_many(insert_many: &InsertMany, ctx: &mut LintContext<'_>) {
    let Some(params) = &insert_many.params else {
        return;
    };

    let used = collect_param_refs_from_values(&insert_many.values);

    for (param_name, _param_type) in &params.params {
        if !used.iter().any(|u| u == param_name.as_str()) {
            DiagnosticBuilder::warning("unused-param")
                .at(param_name.span)
                .msg(format!(
                    "param '{}' is declared but never used",
                    param_name.as_str()
                ))
                .emit(ctx.diagnostics);
        }
    }
}

pub fn lint_unused_params_upsert(upsert: &Upsert, ctx: &mut LintContext<'_>) {
    let Some(params) = &upsert.params else { return };

    let mut used = collect_param_refs_from_values(&upsert.values);
    used.extend(collect_param_refs_from_conflict_update(
        &upsert.on_conflict.update,
    ));

    for (param_name, _param_type) in &params.params {
        if !used.iter().any(|u| u == param_name.as_str()) {
            DiagnosticBuilder::warning("unused-param")
                .at(param_name.span)
                .msg(format!(
                    "param '{}' is declared but never used",
                    param_name.as_str()
                ))
                .emit(ctx.diagnostics);
        }
    }
}

pub fn lint_unused_params_upsert_many(upsert_many: &UpsertMany, ctx: &mut LintContext<'_>) {
    let Some(params) = &upsert_many.params else {
        return;
    };

    let mut used = collect_param_refs_from_values(&upsert_many.values);
    used.extend(collect_param_refs_from_conflict_update(
        &upsert_many.on_conflict.update,
    ));

    for (param_name, _param_type) in &params.params {
        if !used.iter().any(|u| u == param_name.as_str()) {
            DiagnosticBuilder::warning("unused-param")
                .at(param_name.span)
                .msg(format!(
                    "param '{}' is declared but never used",
                    param_name.as_str()
                ))
                .emit(ctx.diagnostics);
        }
    }
}

pub fn lint_unused_params_update(update: &Update, ctx: &mut LintContext<'_>) {
    let Some(params) = &update.params else { return };

    let mut used = collect_param_refs_from_values(&update.set);
    if let Some(where_clause) = &update.where_clause {
        used.extend(collect_param_refs_from_where(where_clause));
    }

    for (param_name, _param_type) in &params.params {
        if !used.iter().any(|u| u == param_name.as_str()) {
            DiagnosticBuilder::warning("unused-param")
                .at(param_name.span)
                .msg(format!(
                    "param '{}' is declared but never used",
                    param_name.as_str()
                ))
                .emit(ctx.diagnostics);
        }
    }
}

pub fn lint_unused_params_delete(delete: &Delete, ctx: &mut LintContext<'_>) {
    let Some(params) = &delete.params else { return };

    let used = delete
        .where_clause
        .as_ref()
        .map(collect_param_refs_from_where)
        .unwrap_or_default();

    for (param_name, _param_type) in &params.params {
        if !used.iter().any(|u| u == param_name.as_str()) {
            DiagnosticBuilder::warning("unused-param")
                .at(param_name.span)
                .msg(format!(
                    "param '{}' is declared but never used",
                    param_name.as_str()
                ))
                .emit(ctx.diagnostics);
        }
    }
}
