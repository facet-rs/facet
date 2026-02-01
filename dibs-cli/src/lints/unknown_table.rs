//! Lint: Unknown table references.

use super::{DiagnosticBuilder, LintContext};
use dibs_query_schema::*;

pub fn lint_unknown_table_query(query: &Query, ctx: &mut LintContext<'_>) {
    let Some(from) = &query.from else { return };
    let name = from.as_str();
    if ctx.find_table(name).is_none() {
        DiagnosticBuilder::error("unknown-table")
            .at(from.span)
            .msg(format!("Unknown table '{name}'"))
            .emit(ctx.diagnostics);
    }
}

pub fn lint_unknown_table_insert(insert: &Insert, ctx: &mut LintContext<'_>) {
    let name = insert.into.as_str();
    if ctx.find_table(name).is_none() {
        DiagnosticBuilder::error("unknown-table")
            .at(insert.into.span)
            .msg(format!("Unknown table '{name}'"))
            .emit(ctx.diagnostics);
    }
}

pub fn lint_unknown_table_insert_many(insert_many: &InsertMany, ctx: &mut LintContext<'_>) {
    let name = insert_many.into.as_str();
    if ctx.find_table(name).is_none() {
        DiagnosticBuilder::error("unknown-table")
            .at(insert_many.into.span)
            .msg(format!("Unknown table '{name}'"))
            .emit(ctx.diagnostics);
    }
}

pub fn lint_unknown_table_update(update: &Update, ctx: &mut LintContext<'_>) {
    let name = update.table.as_str();
    if ctx.find_table(name).is_none() {
        DiagnosticBuilder::error("unknown-table")
            .at(update.table.span)
            .msg(format!("Unknown table '{name}'"))
            .emit(ctx.diagnostics);
    }
}

pub fn lint_unknown_table_delete(delete: &Delete, ctx: &mut LintContext<'_>) {
    let name = delete.from.as_str();
    if ctx.find_table(name).is_none() {
        DiagnosticBuilder::error("unknown-table")
            .at(delete.from.span)
            .msg(format!("Unknown table '{name}'"))
            .emit(ctx.diagnostics);
    }
}

pub fn lint_unknown_table_upsert(upsert: &Upsert, ctx: &mut LintContext<'_>) {
    let name = upsert.into.as_str();
    if ctx.find_table(name).is_none() {
        DiagnosticBuilder::error("unknown-table")
            .at(upsert.into.span)
            .msg(format!("Unknown table '{name}'"))
            .emit(ctx.diagnostics);
    }
}

pub fn lint_unknown_table_upsert_many(upsert_many: &UpsertMany, ctx: &mut LintContext<'_>) {
    let name = upsert_many.into.as_str();
    if ctx.find_table(name).is_none() {
        DiagnosticBuilder::error("unknown-table")
            .at(upsert_many.into.span)
            .msg(format!("Unknown table '{name}'"))
            .emit(ctx.diagnostics);
    }
}
